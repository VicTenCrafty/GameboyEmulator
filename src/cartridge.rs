use std::fs::File;
use std::io::Read;

#[derive(Clone, Copy, PartialEq, Debug)]
enum CartridgeType {
    RomOnly,
    Mbc1,
    Mbc2,
    Mbc3,
    Mbc5,
}

#[derive(Clone, Copy)]
enum BankMode {
    Rom, // 16Mbit ROM/8KByte RAM mode
    Ram, // 4Mbit ROM/32KByte RAM mode
}

pub struct Cartridge {
    rom: Vec<u8>,
    ram: Vec<u8>,
    cart_type: CartridgeType,
    bank: u8,           // Combined bank register
    bank_mode: BankMode,
    ram_enabled: bool,
    // MBC3 RTC registers
    rtc_register: u8,
    rtc_latched: bool,
    // MBC5 registers
    rom_bank_low: u8,   // MBC5: lower 8 bits of ROM bank
    rom_bank_high: u8,  // MBC5: 9th bit of ROM bank
    ram_bank: u8,       // MBC5: RAM bank (4 bits)
    // Save file support
    save_path: Option<String>,
    #[allow(dead_code)]
    has_battery: bool,
}

impl Cartridge {
    pub fn load(path: &str) -> Result<Self, std::io::Error> {
        let mut file = File::open(path)?;
        let mut rom = Vec::new();
        file.read_to_end(&mut rom)?;

        println!("Loaded ROM: {} bytes", rom.len());

        // Determine cartridge type
        let cart_type_byte = if rom.len() >= 0x148 { rom[0x147] } else { 0 };
        let (cart_type, has_battery) = match cart_type_byte {
            0x00 => (CartridgeType::RomOnly, false),
            0x01 => (CartridgeType::Mbc1, false),
            0x02 => (CartridgeType::Mbc1, false),
            0x03 => (CartridgeType::Mbc1, true),
            0x05 => (CartridgeType::Mbc2, false),
            0x06 => (CartridgeType::Mbc2, true),
            0x0F => (CartridgeType::Mbc3, true),
            0x10 => (CartridgeType::Mbc3, true),
            0x11 => (CartridgeType::Mbc3, false),
            0x12 => (CartridgeType::Mbc3, false),
            0x13 => (CartridgeType::Mbc3, true),
            0x19 => (CartridgeType::Mbc5, false),
            0x1A => (CartridgeType::Mbc5, false),
            0x1B => (CartridgeType::Mbc5, true),
            0x1C => (CartridgeType::Mbc5, false),
            0x1D => (CartridgeType::Mbc5, false),
            0x1E => (CartridgeType::Mbc5, true),
            _ => {
                println!("Warning: Unsupported cartridge type 0x{:02X}, defaulting to MBC1", cart_type_byte);
                (CartridgeType::Mbc1, false)
            }
        };

        // Print cartridge header info
        if rom.len() >= 0x150 {
            let title_bytes = &rom[0x134..0x144];
            let title = String::from_utf8_lossy(title_bytes).trim_matches('\0').to_string();
            println!("Title: {}", title);
            println!("Cartridge type: 0x{:02X} ({:?})", cart_type_byte, cart_type);

            let rom_size = rom[0x148];
            println!("ROM size: 0x{:02X}", rom_size);
        }

        // Initialize RAM based on cartridge type and RAM size byte
        let ram_size_byte = if rom.len() >= 0x149 { rom[0x149] } else { 0 };
        let ram_size = match ram_size_byte {
            0x00 => 0,
            0x01 => 0x800,      // 2KB (unused)
            0x02 => 0x2000,     // 8KB
            0x03 => 0x8000,     // 32KB (4 banks)
            0x04 => 0x20000,    // 128KB (16 banks)
            0x05 => 0x10000,    // 64KB (8 banks)
            _ => {
                if cart_type == CartridgeType::Mbc2 {
                    512 // MBC2 has built-in 512x4 bits RAM
                } else {
                    0
                }
            }
        };
        let mut ram = vec![0; ram_size];

        // Generate save file path
        let save_path = if has_battery && ram_size > 0 {
            let save_file = path.replace(".gb", ".sav").replace(".gbc", ".sav");
            Some(save_file)
        } else {
            None
        };

        // Load saved RAM if exists
        if let Some(ref save_file) = save_path {
            if let Ok(mut file) = File::open(save_file) {
                let _ = file.read_to_end(&mut ram);
                println!("Loaded save file: {}", save_file);
            }
        }

        Ok(Cartridge {
            rom,
            ram,
            cart_type,
            bank: 0x01, // Start with bank 1
            bank_mode: BankMode::Rom,
            ram_enabled: false,
            rtc_register: 0,
            rtc_latched: false,
            rom_bank_low: 0x01,
            rom_bank_high: 0x00,
            ram_bank: 0x00,
            save_path,
            has_battery,
        })
    }

    pub fn save(&self) {
        if let Some(ref save_file) = self.save_path {
            if let Ok(mut file) = File::create(save_file) {
                use std::io::Write;
                let _ = file.write_all(&self.ram);
                println!("Saved to: {}", save_file);
            }
        }
    }

    fn rom_bank(&self) -> usize {
        if self.cart_type == CartridgeType::Mbc5 {
            // MBC5 uses 9-bit ROM bank (0-511)
            let bank = ((self.rom_bank_high as usize & 0x01) << 8) | (self.rom_bank_low as usize);
            return bank;
        }

        let n = match self.bank_mode {
            BankMode::Rom => self.bank & 0x7F, // Use all 7 bits
            BankMode::Ram => self.bank & 0x1F, // Use only lower 5 bits
        };
        let bank = n as usize;
        if bank == 0 { 1 } else { bank } // Bank 0 is mapped to bank 1
    }

    fn ram_bank(&self) -> usize {
        if self.cart_type == CartridgeType::Mbc5 {
            return (self.ram_bank & 0x0F) as usize;
        }

        let n = match self.bank_mode {
            BankMode::Rom => 0x00,                    // Always bank 0
            BankMode::Ram => (self.bank & 0x60) >> 5, // Upper 2 bits
        };
        n as usize
    }

    pub fn read_rom(&self, address: u16) -> u8 {
        let addr = match address {
            0x0000..=0x3FFF => {
                // Bank 0 (or high ROM bank in RAM mode)
                let bank = match self.bank_mode {
                    BankMode::Rom => 0,
                    BankMode::Ram => ((self.bank & 0x60) >> 5) as usize,
                };
                (bank * 0x4000) + (address as usize)
            }
            0x4000..=0x7FFF => {
                // Switchable ROM bank
                let bank = self.rom_bank();
                (bank * 0x4000) + ((address - 0x4000) as usize)
            }
            _ => return 0xFF,
        };

        if addr < self.rom.len() {
            self.rom[addr]
        } else {
            0xFF
        }
    }

    pub fn read_ram(&self, address: u16) -> u8 {
        if !self.ram_enabled {
            return 0xFF;
        }

        // MBC2 has special RAM handling
        if self.cart_type == CartridgeType::Mbc2 {
            let addr = (address - 0xA000) as usize & 0x1FF; // Only 512 addresses
            if addr < self.ram.len() {
                return self.ram[addr] & 0x0F; // Only lower 4 bits
            } else {
                return 0xFF;
            }
        }

        // MBC3 RTC register read
        if self.cart_type == CartridgeType::Mbc3 && self.rtc_register >= 0x08 && self.rtc_register <= 0x0C {
            // Return dummy RTC values (not implemented)
            return 0;
        }

        let bank = self.ram_bank();
        let addr = (bank * 0x2000) + ((address - 0xA000) as usize);

        if addr < self.ram.len() {
            self.ram[addr]
        } else {
            0xFF
        }
    }

    pub fn write_ram(&mut self, address: u16, value: u8) {
        if !self.ram_enabled {
            return;
        }

        // MBC2 has special RAM handling
        if self.cart_type == CartridgeType::Mbc2 {
            let addr = (address - 0xA000) as usize & 0x1FF; // Only 512 addresses
            if addr < self.ram.len() {
                self.ram[addr] = value & 0x0F; // Only lower 4 bits
            }
            return;
        }

        // MBC3 RTC register write (not implemented, just ignore)
        if self.cart_type == CartridgeType::Mbc3 && self.rtc_register >= 0x08 && self.rtc_register <= 0x0C {
            return;
        }

        let bank = self.ram_bank();
        let addr = (bank * 0x2000) + ((address - 0xA000) as usize);

        if addr < self.ram.len() {
            self.ram[addr] = value;
        }
    }

    pub fn write_rom(&mut self, address: u16, value: u8) {
        match self.cart_type {
            CartridgeType::RomOnly => {}

            CartridgeType::Mbc1 => {
                match address {
                    0x0000..=0x1FFF => {
                        // RAM Enable
                        self.ram_enabled = (value & 0x0F) == 0x0A;
                    }
                    0x2000..=0x3FFF => {
                        // ROM Bank Number (lower 5 bits)
                        let lower = value & 0x1F;
                        self.bank = (self.bank & 0x60) | lower;
                    }
                    0x4000..=0x5FFF => {
                        // RAM Bank Number or Upper Bits of ROM Bank Number (upper 2 bits)
                        let upper = (value & 0x03) << 5;
                        self.bank = (self.bank & 0x1F) | upper;
                    }
                    0x6000..=0x7FFF => {
                        // Banking Mode Select
                        self.bank_mode = if (value & 0x01) != 0 {
                            BankMode::Ram
                        } else {
                            BankMode::Rom
                        };
                    }
                    _ => {}
                }
            }

            CartridgeType::Mbc2 => {
                match address {
                    0x0000..=0x1FFF => {
                        // RAM Enable (only if bit 8 of address is 0)
                        if (address & 0x0100) == 0 {
                            self.ram_enabled = (value & 0x0F) == 0x0A;
                        }
                    }
                    0x2000..=0x3FFF => {
                        // ROM Bank Number (only if bit 8 of address is 1)
                        if (address & 0x0100) != 0 {
                            self.bank = value & 0x0F; // Only 4 bits for MBC2
                        }
                    }
                    _ => {}
                }
            }

            CartridgeType::Mbc3 => {
                match address {
                    0x0000..=0x1FFF => {
                        // RAM and Timer Enable
                        self.ram_enabled = (value & 0x0F) == 0x0A;
                    }
                    0x2000..=0x3FFF => {
                        // ROM Bank Number (7 bits)
                        self.bank = value & 0x7F;
                        if self.bank == 0 {
                            self.bank = 1;
                        }
                    }
                    0x4000..=0x5FFF => {
                        // RAM Bank Number or RTC Register Select
                        if value <= 0x03 {
                            // RAM bank
                            self.bank = (self.bank & 0x7F) | ((value & 0x03) << 5);
                        } else if value >= 0x08 && value <= 0x0C {
                            // RTC register
                            self.rtc_register = value;
                        }
                    }
                    0x6000..=0x7FFF => {
                        // Latch Clock Data
                        if value == 0x01 {
                            self.rtc_latched = true;
                        } else if value == 0x00 {
                            self.rtc_latched = false;
                        }
                    }
                    _ => {}
                }
            }

            CartridgeType::Mbc5 => {
                match address {
                    0x0000..=0x1FFF => {
                        // RAM Enable
                        self.ram_enabled = (value & 0x0F) == 0x0A;
                    }
                    0x2000..=0x2FFF => {
                        // ROM Bank Number (lower 8 bits)
                        self.rom_bank_low = value;
                    }
                    0x3000..=0x3FFF => {
                        // ROM Bank Number (9th bit)
                        self.rom_bank_high = value & 0x01;
                    }
                    0x4000..=0x5FFF => {
                        // RAM Bank Number (4 bits)
                        self.ram_bank = value & 0x0F;
                    }
                    _ => {}
                }
            }
        }
    }
}
