pub const SCREEN_WIDTH: usize = 160;
pub const SCREEN_HEIGHT: usize = 144;

pub struct Ppu {
    pub vram: [[u8; 0x2000]; 2], // 16KB VRAM (2 banks for GBC)
    pub oam: [u8; 0xA0],         // Object Attribute Memory (sprites)
    pub framebuffer: [u32; SCREEN_WIDTH * SCREEN_HEIGHT],

    // LCD Control registers
    pub lcdc: u8,  // 0xFF40
    pub stat: u8,  // 0xFF41
    pub scy: u8,   // 0xFF42 - Scroll Y
    pub scx: u8,   // 0xFF43 - Scroll X
    pub ly: u8,    // 0xFF44 - Current scanline
    pub lyc: u8,   // 0xFF45
    pub bgp: u8,   // 0xFF47 - BG palette (DMG)
    pub obp0: u8,  // 0xFF48 - OBJ palette 0 (DMG)
    pub obp1: u8,  // 0xFF49 - OBJ palette 1 (DMG)
    pub wy: u8,    // 0xFF4A - Window Y
    pub wx: u8,    // 0xFF4B - Window X

    // GBC-specific registers
    pub vram_bank: u8,           // 0xFF4F - VRAM bank select (0-1)
    pub bcps: u8,                // 0xFF68 - BG Color Palette Spec
    pub bcpd: [u8; 64],          // BG Color Palette Data (8 palettes × 4 colors × 2 bytes)
    pub ocps: u8,                // 0xFF6A - OBJ Color Palette Spec
    pub ocpd: [u8; 64],          // OBJ Color Palette Data (8 palettes × 4 colors × 2 bytes)
    pub is_gbc: bool,

    dots: u32, // Dot counter for timing (0-455 per scanline)
    pub frame_ready: bool,
    pub stat_interrupt: bool, // Set when STAT interrupt should fire

    // Priority buffer: stores (bg_color_num) for sprite priority checks
    bg_priority: [u8; SCREEN_WIDTH],

    // Window internal line counter
    window_line: u8,
}

impl Ppu {
    fn default_gbc_palette() -> [u8; 64] {
        let mut palette = [0u8; 64];
        // Initialize palette 0 with test colors
        // RGB555 format: 0BBBBBGGGGGRRRRR
        let test_colors = [
            (31, 31, 31), // White
            (31, 0, 0),   // Red
            (0, 31, 0),   // Green
            (0, 0, 31),   // Blue
        ];

        // Palette 0: test colors
        for (col_idx, &(r, g, b)) in test_colors.iter().enumerate() {
            let base = col_idx * 2;
            let color15 = (r & 0x1F) | ((g & 0x1F) << 5) | ((b & 0x1F) << 10);
            palette[base] = (color15 & 0xFF) as u8;
            palette[base + 1] = ((color15 >> 8) & 0xFF) as u8;
        }

        // Palettes 1-7: grayscale
        let gray_colors = [
            (31, 31, 31), // White
            (21, 21, 21), // Light gray
            (10, 10, 10), // Dark gray
            (0, 0, 0),    // Black
        ];
        for pal in 1..8 {
            for (col_idx, &(r, g, b)) in gray_colors.iter().enumerate() {
                let base = pal * 8 + col_idx * 2;
                let color15 = (r & 0x1F) | ((g & 0x1F) << 5) | ((b & 0x1F) << 10);
                palette[base] = (color15 & 0xFF) as u8;
                palette[base + 1] = ((color15 >> 8) & 0xFF) as u8;
            }
        }
        palette
    }

    pub fn new(is_gbc: bool) -> Self {
        let default_color = if is_gbc { 0xFFFFFF } else { 0x9BBC0F };
        Ppu {
            vram: [[0; 0x2000]; 2],
            oam: [0xFF; 0xA0], // Initialize OAM to 0xFF (invalid sprites)
            framebuffer: [default_color; SCREEN_WIDTH * SCREEN_HEIGHT],
            lcdc: 0x91, // Post-boot ROM value
            stat: 0x85, // Post-boot value (varies)
            scy: 0,
            scx: 0,
            ly: 0,
            lyc: 0,
            bgp: 0xFC,
            obp0: 0xFF,
            obp1: 0xFF,
            wy: 0,
            wx: 0,
            vram_bank: if is_gbc { 0xFE } else { 0 }, // Post-boot: 0xFE for GBC
            bcps: if is_gbc { 0xC8 } else { 0 },
            bcpd: Self::default_gbc_palette(),
            ocps: if is_gbc { 0xD0 } else { 0 },
            ocpd: Self::default_gbc_palette(),
            is_gbc,
            dots: 0,
            frame_ready: false,
            stat_interrupt: false,
            bg_priority: [0; SCREEN_WIDTH],
            window_line: 0,
        }
    }

    pub fn step(&mut self, cycles: u32) {
        self.stat_interrupt = false;

        // If LCD is disabled, don't process
        if (self.lcdc & 0x80) == 0 {
            // LCD off - ly should be 0, mode should be 0
            self.ly = 0;
            self.stat = self.stat & 0xFC;
            self.dots = 0;
            return;
        }

        // Process cycles in smaller chunks for better accuracy
        let chunks = (cycles - 1) / 80 + 1;
        for i in 0..chunks {
            let dots_to_add = if i == chunks - 1 {
                cycles % 80
            } else {
                80
            };

            if dots_to_add == 0 {
                continue;
            }

            self.dots += dots_to_add;
            let old_mode = self.stat & 0x03;

            match old_mode {
                // Mode 2: OAM search (0-79 dots)
                2 => {
                    if self.dots >= 80 {
                        self.stat = (self.stat & 0xFC) | 3; // Enter mode 3
                    }
                }
                // Mode 3: Pixel transfer (80-251 dots)
                3 => {
                    if self.dots >= 252 {
                        self.stat = (self.stat & 0xFC) | 0; // Enter HBlank
                        self.render_scanline();

                        // HBlank interrupt (STAT bit 3)
                        if (self.stat & 0x08) != 0 {
                            self.stat_interrupt = true;
                        }
                    }
                }
                // Mode 0: HBlank (252-455 dots)
                0 => {
                    if self.dots >= 456 {
                        self.dots -= 456;
                        self.ly += 1;

                        // Check LY=LYC coincidence
                        let lyc_match = self.ly == self.lyc;
                        if lyc_match {
                            self.stat |= 0x04; // Set coincidence flag
                            // LYC interrupt (STAT bit 6)
                            if (self.stat & 0x40) != 0 {
                                self.stat_interrupt = true;
                            }
                        } else {
                            self.stat &= !0x04; // Clear coincidence flag
                        }

                        if self.ly == 144 {
                            // Enter VBlank
                            self.stat = (self.stat & 0xFC) | 1;
                            self.frame_ready = true;
                            self.window_line = 0; // Reset window line counter at start of VBlank

                            // VBlank STAT interrupt (STAT bit 4)
                            if (self.stat & 0x10) != 0 {
                                self.stat_interrupt = true;
                            }
                        } else {
                            self.stat = (self.stat & 0xFC) | 2; // Back to OAM search

                            // OAM interrupt (STAT bit 5)
                            if (self.stat & 0x20) != 0 {
                                self.stat_interrupt = true;
                            }
                        }
                    }
                }
                // Mode 1: VBlank (lines 144-153)
                1 => {
                    if self.dots >= 456 {
                        self.dots -= 456;
                        self.ly += 1;

                        // Check LY=LYC coincidence
                        let lyc_match = self.ly == self.lyc;
                        if lyc_match {
                            self.stat |= 0x04; // Set coincidence flag
                            // LYC interrupt (STAT bit 6)
                            if (self.stat & 0x40) != 0 {
                                self.stat_interrupt = true;
                            }
                        } else {
                            self.stat &= !0x04; // Clear coincidence flag
                        }

                        if self.ly > 153 {
                            self.ly = 0;
                            self.stat = (self.stat & 0xFC) | 2; // Back to OAM search

                            // OAM interrupt (STAT bit 5)
                            if (self.stat & 0x20) != 0 {
                                self.stat_interrupt = true;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn render_scanline(&mut self) {
        if (self.lcdc & 0x80) == 0 {
            return; // LCD off
        }

        let y = self.ly as usize;
        if y >= SCREEN_HEIGHT {
            return;
        }

        // Clear priority buffer for this scanline
        self.bg_priority = [0; SCREEN_WIDTH];

        // Render background/window (unified)
        if (self.lcdc & 0x01) != 0 {
            self.render_bg_window(y);
        }

        // Render sprites
        if (self.lcdc & 0x02) != 0 {
            self.render_sprites(y);
        }
    }

    fn render_bg_window(&mut self, y: usize) {
        // Check if window is enabled and visible on this scanline
        let window_enabled = (self.lcdc & 0x20) != 0 && self.wy <= self.ly;
        let wx_offset = self.wx.saturating_sub(7); // Window X is offset by 7

        let mut window_rendered = false;

        for x in 0..SCREEN_WIDTH {
            // Determine if we're rendering window or background
            let in_window = window_enabled && (x as u8) >= wx_offset;

            let (pixel_x, pixel_y, tile_map_base) = if in_window {
                window_rendered = true;
                // Window rendering - use internal line counter
                let win_x = (x as u8).wrapping_sub(wx_offset);
                let win_y = self.window_line;
                let tile_map = if (self.lcdc & 0x40) != 0 { 0x1C00 } else { 0x1800 };
                (win_x, win_y, tile_map)
            } else {
                // Background rendering
                let bg_x = self.scx.wrapping_add(x as u8);
                let bg_y = self.scy.wrapping_add(y as u8);
                let tile_map = if (self.lcdc & 0x08) != 0 { 0x1C00 } else { 0x1800 };
                (bg_x, bg_y, tile_map)
            };

            // Calculate tile position
            let tile_x = ((pixel_x as u16 / 8) & 31) as u16;
            let tile_y = ((pixel_y as u16 / 8) & 31) as u16;
            let pixel_x_in_tile = (pixel_x % 8) as u16;
            let pixel_y_in_tile = (pixel_y % 8) as u16;

            // Get tile number from tile map
            let tile_map_addr = tile_map_base + (tile_y * 32) + tile_x;
            if tile_map_addr >= 0x2000 {
                continue;
            }
            let tile_num = self.vram[0][tile_map_addr as usize];

            // GBC: Read attributes from VRAM bank 1
            let (palette_num, flip_x, flip_y, _bg_priority) = if self.is_gbc {
                let attr = self.vram[1][tile_map_addr as usize];
                let pal = attr & 0x07;
                let flip_x = (attr & 0x20) != 0;
                let flip_y = (attr & 0x40) != 0;
                let priority = (attr & 0x80) != 0;
                (pal, flip_x, flip_y, priority)
            } else {
                (0, false, false, false)
            };

            // Tile data address (signed vs unsigned addressing)
            // LCDC bit 4 = 1: unsigned mode, tiles at $8000-$8FFF (VRAM 0x0000-0x0FFF)
            // LCDC bit 4 = 0: signed mode, tiles at $8800-$97FF, base at $9000 (VRAM 0x1000)
            let tile_addr = if (self.lcdc & 0x10) != 0 {
                // Unsigned mode: tile 0 at VRAM 0x0000
                (tile_num as u16) * 16
            } else {
                // Signed mode: tile 0 at VRAM 0x1000 ($9000)
                let offset = (tile_num as i8 as i32) * 16;
                (0x1000i32 + offset) as u16
            };

            if (tile_addr + pixel_y_in_tile * 2 + 1) as usize >= 0x2000 {
                continue;
            }

            // Read tile data (use correct VRAM bank for GBC)
            let tile_vram_bank = if self.is_gbc && ((self.vram[1][tile_map_addr as usize] & 0x08) != 0) { 1 } else { 0 };

            let mut line = pixel_y_in_tile;
            if flip_y {
                line = 7 - line;
            }

            let byte1 = self.vram[tile_vram_bank][(tile_addr + line * 2) as usize];
            let byte2 = self.vram[tile_vram_bank][(tile_addr + line * 2 + 1) as usize];

            let mut bit = 7 - pixel_x_in_tile;
            if flip_x {
                bit = pixel_x_in_tile;
            }

            let color_bit_1 = (byte1 >> bit) & 1;
            let color_bit_2 = (byte2 >> bit) & 1;
            let color_num = (color_bit_2 << 1) | color_bit_1;

            // Store color number for sprite priority
            self.bg_priority[x] = color_num;

            let color = if self.is_gbc {
                self.get_gbc_bg_color(color_num, palette_num)
            } else {
                self.get_bg_color(color_num)
            };
            self.framebuffer[y * SCREEN_WIDTH + x] = color;
        }

        // Increment window line counter if window was rendered on this scanline
        if window_rendered {
            self.window_line = self.window_line.wrapping_add(1);
        }
    }

    fn render_sprites(&mut self, y: usize) {
        let sprite_height = if (self.lcdc & 0x04) != 0 { 16 } else { 8 };

        // Collect visible sprites on this scanline
        let mut visible_sprites = Vec::new();
        for sprite_idx in 0..40 {
            let oam_addr = sprite_idx * 4;
            let sprite_y_raw = self.oam[oam_addr];
            let sprite_x_raw = self.oam[oam_addr + 1];

            // Skip completely invalid/hidden sprites
            // Games typically hide sprites at Y=0 or Y>=160
            if sprite_y_raw == 0 {
                continue;
            }

            // Convert to screen coordinates (Y - 16)
            let sprite_y = sprite_y_raw as i16 - 16;

            // Check if scanline intersects with this sprite
            let y_i16 = y as i16;
            if y_i16 >= sprite_y && y_i16 < sprite_y + sprite_height as i16 {
                // Only add if X is potentially visible (0 is used to hide)
                if sprite_x_raw > 0 && sprite_x_raw < 168 {
                    visible_sprites.push((sprite_idx, sprite_x_raw)); // (index, x position)
                }
            }
        }

        // Limit to 10 sprites per scanline (hardware limitation)
        visible_sprites.truncate(10);

        // Sort sprites by X coordinate (descending), then by OAM index (ascending)
        // This ensures sprites with lower X are drawn last (on top)
        visible_sprites.sort_by(|a, b| {
            match b.1.cmp(&a.1) {
                std::cmp::Ordering::Equal => a.0.cmp(&b.0), // Same X: lower OAM index wins
                other => other // Different X: higher X first (will be drawn first/behind)
            }
        });

        // Render sprites - those drawn later appear on top
        for (sprite_idx, _) in visible_sprites.iter() {
            let oam_addr = sprite_idx * 4;
            let sprite_y_raw = self.oam[oam_addr];
            let sprite_x_raw = self.oam[oam_addr + 1];
            let tile_num = self.oam[oam_addr + 2];
            let attributes = self.oam[oam_addr + 3];

            let palette = if (attributes & 0x10) != 0 { self.obp1 } else { self.obp0 };
            let flip_y = (attributes & 0x40) != 0;
            let flip_x = (attributes & 0x20) != 0;
            let priority = (attributes & 0x80) != 0; // Priority flag: 1 = behind BG colors 1-3

            // GBC: Extract palette number and VRAM bank
            let (gbc_palette, gbc_vram_bank) = if self.is_gbc {
                let pal = attributes & 0x07;
                let bank = if (attributes & 0x08) != 0 { 1 } else { 0 };
                (pal, bank)
            } else {
                (0, 0)
            };

            // Convert to screen coordinates
            let sprite_y = sprite_y_raw as i16 - 16;
            let sprite_x = sprite_x_raw as i16 - 8;

            // Calculate which line of the sprite we're rendering
            let mut line = (y as i16 - sprite_y) as u16;
            if flip_y {
                line = (sprite_height as u16 - 1) - line;
            }

            // Handle 8x16 sprites
            let tile_addr = if sprite_height == 16 {
                // In 8x16 mode, bit 0 is ignored, line 0-7 uses tile_num & 0xFE, line 8-15 uses tile_num | 0x01
                let actual_tile = if line < 8 {
                    tile_num & 0xFE
                } else {
                    tile_num | 0x01
                };
                let tile_line = line % 8;
                (actual_tile as u16 * 16) + (tile_line * 2)
            } else {
                (tile_num as u16 * 16) + (line * 2)
            };

            if (tile_addr + 1) as usize >= 0x2000 {
                continue;
            }

            let byte1 = self.vram[gbc_vram_bank][tile_addr as usize];
            let byte2 = self.vram[gbc_vram_bank][(tile_addr + 1) as usize];

            for x in 0..8 {
                let pixel_x = sprite_x + x as i16;

                // Skip if off screen
                if pixel_x < 0 || pixel_x >= 160 {
                    continue;
                }

                let bit = if flip_x { x } else { 7 - x };
                let color_bit_1 = (byte1 >> bit) & 1;
                let color_bit_2 = (byte2 >> bit) & 1;
                let color_num = (color_bit_2 << 1) | color_bit_1;

                if color_num == 0 {
                    continue; // Transparent
                }

                // Check sprite-to-BG priority
                let bg_color = self.bg_priority[pixel_x as usize];

                // Priority logic:
                // - If sprite priority flag is set (1) AND BG color is not 0, sprite is behind BG
                // - If sprite priority flag is clear (0), sprite is always on top
                // - BG color 0 is always transparent (sprite shows through)
                if priority && bg_color != 0 {
                    continue; // Sprite is behind non-transparent background
                }

                let color = if self.is_gbc {
                    self.get_gbc_sprite_color(color_num, gbc_palette)
                } else {
                    self.get_sprite_color(color_num, palette)
                };
                self.framebuffer[y * SCREEN_WIDTH + pixel_x as usize] = color;
            }
        }
    }

    fn get_bg_color(&self, color_num: u8) -> u32 {
        let palette_color = (self.bgp >> (color_num * 2)) & 0x03;
        // Classic Game Boy green palette (0RGB format)
        match palette_color {
            0 => 0x9BBC0F, // Lightest
            1 => 0x8BAC0F, // Light
            2 => 0x306230, // Dark
            3 => 0x0F380F, // Darkest
            _ => 0x9BBC0F,
        }
    }

    fn get_sprite_color(&self, color_num: u8, palette: u8) -> u32 {
        let palette_color = (palette >> (color_num * 2)) & 0x03;
        match palette_color {
            0 => 0x9BBC0F,
            1 => 0x8BAC0F,
            2 => 0x306230,
            3 => 0x0F380F,
            _ => 0x9BBC0F,
        }
    }

    fn get_gbc_bg_color(&self, color_num: u8, palette_num: u8) -> u32 {
        // Each palette is 8 bytes (4 colors × 2 bytes per color)
        let palette_base = ((palette_num & 0x07) as usize) * 8;
        let color_offset = ((color_num & 0x03) as usize) * 2;
        let addr = palette_base + color_offset;

        // Safety check
        if addr + 1 >= 64 {
            return 0xFFFFFF; // White fallback
        }

        // Read 16-bit color (little-endian)
        let low = self.bcpd[addr] as u16;
        let high = self.bcpd[addr + 1] as u16;
        let color15 = low | (high << 8);

        self.convert_gbc_color(color15)
    }

    fn get_gbc_sprite_color(&self, color_num: u8, palette_num: u8) -> u32 {
        // Each palette is 8 bytes (4 colors × 2 bytes per color)
        let palette_base = ((palette_num & 0x07) as usize) * 8;
        let color_offset = ((color_num & 0x03) as usize) * 2;
        let addr = palette_base + color_offset;

        // Safety check
        if addr + 1 >= 64 {
            return 0xFFFFFF; // White fallback
        }

        // Read 16-bit color (little-endian)
        let low = self.ocpd[addr] as u16;
        let high = self.ocpd[addr + 1] as u16;
        let color15 = low | (high << 8);

        self.convert_gbc_color(color15)
    }

    fn convert_gbc_color(&self, color15: u16) -> u32 {
        // GBC uses 15-bit RGB555 format: 0BBBBBGGGGGRRRRR
        let r = (color15 & 0x1F) as u32;
        let g = ((color15 >> 5) & 0x1F) as u32;
        let b = ((color15 >> 10) & 0x1F) as u32;

        // Convert from 5-bit to 8-bit
        // Shift left by 3 and copy top 3 bits to bottom for full range
        let r8 = (r << 3) | (r >> 2);
        let g8 = (g << 3) | (g >> 2);
        let b8 = (b << 3) | (b >> 2);

        // minifb expects 0RGB format
        (r8 << 16) | (g8 << 8) | b8
    }

    pub fn read_vram(&self, addr: u16) -> u8 {
        let bank = if self.is_gbc { (self.vram_bank & 0x01) as usize } else { 0 };
        self.vram[bank][(addr - 0x8000) as usize]
    }

    pub fn write_vram(&mut self, addr: u16, value: u8) {
        let bank = if self.is_gbc { (self.vram_bank & 0x01) as usize } else { 0 };
        self.vram[bank][(addr - 0x8000) as usize] = value;
    }

    pub fn read_oam(&self, addr: u16) -> u8 {
        self.oam[(addr - 0xFE00) as usize]
    }

    pub fn write_oam(&mut self, addr: u16, value: u8) {
        self.oam[(addr - 0xFE00) as usize] = value;
    }
}
