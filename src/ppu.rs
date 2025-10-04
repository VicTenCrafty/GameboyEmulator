pub const SCREEN_WIDTH: usize = 160;
pub const SCREEN_HEIGHT: usize = 144;

pub struct Ppu {
    pub vram: [u8; 0x2000],      // 8KB VRAM
    pub oam: [u8; 0xA0],         // Object Attribute Memory (sprites)
    pub framebuffer: [u32; SCREEN_WIDTH * SCREEN_HEIGHT],

    // LCD Control registers
    pub lcdc: u8,  // 0xFF40
    pub stat: u8,  // 0xFF41
    pub scy: u8,   // 0xFF42 - Scroll Y
    pub scx: u8,   // 0xFF43 - Scroll X
    pub ly: u8,    // 0xFF44 - Current scanline
    pub lyc: u8,   // 0xFF45
    pub bgp: u8,   // 0xFF47 - BG palette
    pub obp0: u8,  // 0xFF48 - OBJ palette 0
    pub obp1: u8,  // 0xFF49 - OBJ palette 1
    pub wy: u8,    // 0xFF4A - Window Y
    pub wx: u8,    // 0xFF4B - Window X

    dots: u32, // Dot counter for timing (0-455 per scanline)
    pub frame_ready: bool,
    pub stat_interrupt: bool, // Set when STAT interrupt should fire

    // Priority buffer: stores (bg_color_num) for sprite priority checks
    bg_priority: [u8; SCREEN_WIDTH],

    // Window internal line counter
    window_line: u8,
}

impl Ppu {
    pub fn new() -> Self {
        Ppu {
            vram: [0; 0x2000],
            oam: [0xFF; 0xA0], // Initialize OAM to 0xFF (invalid sprites)
            framebuffer: [0x9BBC0F; SCREEN_WIDTH * SCREEN_HEIGHT], // Game Boy green
            lcdc: 0x91,
            stat: 0x02, // Start in mode 2 (OAM search)
            scy: 0,
            scx: 0,
            ly: 0,
            lyc: 0,
            bgp: 0xFC,
            obp0: 0xFF,
            obp1: 0xFF,
            wy: 0,
            wx: 0,
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
            let tile_num = self.vram[tile_map_addr as usize];

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

            // Read tile data
            let byte1 = self.vram[(tile_addr + pixel_y_in_tile * 2) as usize];
            let byte2 = self.vram[(tile_addr + pixel_y_in_tile * 2 + 1) as usize];

            let bit = 7 - pixel_x_in_tile;
            let color_bit_1 = (byte1 >> bit) & 1;
            let color_bit_2 = (byte2 >> bit) & 1;
            let color_num = (color_bit_2 << 1) | color_bit_1;

            // Store color number for sprite priority
            self.bg_priority[x] = color_num;

            let color = self.get_bg_color(color_num);
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

            if (tile_addr + 1) as usize >= self.vram.len() {
                continue;
            }

            let byte1 = self.vram[tile_addr as usize];
            let byte2 = self.vram[(tile_addr + 1) as usize];

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

                let color = self.get_sprite_color(color_num, palette);
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

    pub fn read_vram(&self, addr: u16) -> u8 {
        self.vram[(addr - 0x8000) as usize]
    }

    pub fn write_vram(&mut self, addr: u16, value: u8) {
        self.vram[(addr - 0x8000) as usize] = value;
    }

    pub fn read_oam(&self, addr: u16) -> u8 {
        self.oam[(addr - 0xFE00) as usize]
    }

    pub fn write_oam(&mut self, addr: u16, value: u8) {
        self.oam[(addr - 0xFE00) as usize] = value;
    }
}
