use crate::cartridge::Cartridge;
use crate::ppu::Ppu;
use crate::joypad::Joypad;
use crate::timer::Timer;
use crate::apu::Apu;

const WRAM_SIZE: usize = 0x2000; // 8KB work RAM
const HRAM_SIZE: usize = 0x7F;   // High RAM

pub struct Mmu {
    pub cartridge: Cartridge,
    pub ppu: Ppu,
    pub joypad: Joypad,
    pub timer: Timer,
    pub apu: Apu,
    wram: [u8; WRAM_SIZE],
    hram: [u8; HRAM_SIZE],
    pub ie: u8, // Interrupt enable register
    pub if_reg: u8, // Interrupt flag register (0xFF0F)
}

impl Mmu {
    pub fn new(cartridge: Cartridge) -> Self {
        Mmu {
            cartridge,
            ppu: Ppu::new(),
            joypad: Joypad::new(),
            timer: Timer::new(),
            apu: Apu::new(),
            wram: [0; WRAM_SIZE],
            hram: [0; HRAM_SIZE],
            ie: 0,
            if_reg: 0,
        }
    }

    pub fn step(&mut self, cycles: u32) {
        // Step timer and check for interrupt
        if self.timer.step(cycles) {
            self.if_reg |= 0x04; // Timer interrupt
        }

        // Step APU
        self.apu.step(cycles);

        // DMA is handled instantly when triggered (in write_io)
        // No need to step it here
    }

    fn do_dma(&mut self, source: u16) {
        // DMA transfers 160 bytes from source to OAM instantly
        // In reality this takes 160 M-cycles, but we do it atomically
        let base = source << 8;
        for i in 0..0xA0 {
            let source_addr = base + i;

            // Read from source
            let value = match source_addr {
                0x0000..=0x7FFF => self.cartridge.read_rom(source_addr),
                0x8000..=0x9FFF => self.ppu.read_vram(source_addr),
                0xA000..=0xBFFF => self.cartridge.read_ram(source_addr),
                0xC000..=0xDFFF => self.wram[(source_addr - 0xC000) as usize],
                0xE000..=0xFDFF => self.wram[(source_addr - 0xE000) as usize],
                _ => 0xFF,
            };

            // Write to OAM
            self.ppu.write_oam(0xFE00 + i, value);
        }
    }

    pub fn read_byte(&self, address: u16) -> u8 {
        match address {
            0x0000..=0x7FFF => self.cartridge.read_rom(address), // ROM
            0x8000..=0x9FFF => self.ppu.read_vram(address), // VRAM
            0xA000..=0xBFFF => self.cartridge.read_ram(address), // External RAM
            0xC000..=0xDFFF => self.wram[(address - 0xC000) as usize],
            0xE000..=0xFDFF => self.wram[(address - 0xE000) as usize], // Echo RAM
            0xFE00..=0xFE9F => self.ppu.read_oam(address), // OAM
            0xFEA0..=0xFEFF => 0, // Unusable
            0xFF00..=0xFF7F => self.read_io(address), // I/O registers
            0xFF80..=0xFFFE => self.hram[(address - 0xFF80) as usize],
            0xFFFF => self.ie,
        }
    }

    pub fn write_byte(&mut self, address: u16, value: u8) {
        match address {
            0x0000..=0x7FFF => self.cartridge.write_rom(address, value), // ROM bank switching
            0x8000..=0x9FFF => self.ppu.write_vram(address, value), // VRAM
            0xA000..=0xBFFF => self.cartridge.write_ram(address, value), // External RAM
            0xC000..=0xDFFF => self.wram[(address - 0xC000) as usize] = value,
            0xE000..=0xFDFF => self.wram[(address - 0xE000) as usize] = value,
            0xFE00..=0xFE9F => self.ppu.write_oam(address, value), // OAM
            0xFEA0..=0xFEFF => {}, // Unusable
            0xFF00..=0xFF7F => self.write_io(address, value),
            0xFF80..=0xFFFE => self.hram[(address - 0xFF80) as usize] = value,
            0xFFFF => self.ie = value,
        }
    }

    fn read_io(&self, address: u16) -> u8 {
        match address {
            0xFF00 => self.joypad.read(),
            0xFF01 => 0xFF, // Serial data (not implemented)
            0xFF02 => 0x7E, // Serial control (not implemented, bit 7=0)
            0xFF04 => self.timer.read_div(),
            0xFF05 => self.timer.read_tima(),
            0xFF06 => self.timer.read_tma(),
            0xFF07 => self.timer.read_tac(),
            0xFF0F => self.if_reg,
            0xFF40 => self.ppu.lcdc,
            0xFF41 => self.ppu.stat,
            0xFF42 => self.ppu.scy,
            0xFF43 => self.ppu.scx,
            0xFF44 => self.ppu.ly,
            0xFF45 => self.ppu.lyc,
            0xFF46 => 0xFF, // DMA register (write-only)
            0xFF47 => self.ppu.bgp,
            0xFF48 => self.ppu.obp0,
            0xFF49 => self.ppu.obp1,
            0xFF4A => self.ppu.wy,
            0xFF4B => self.ppu.wx,

            // APU registers
            0xFF10..=0xFF26 => self.apu.read_register(address),
            0xFF30..=0xFF3F => self.apu.read_register(address),

            _ => 0xFF,
        }
    }

    fn write_io(&mut self, address: u16, value: u8) {
        match address {
            0xFF00 => self.joypad.write(value),
            0xFF01 => {}, // Serial data (not implemented)
            0xFF02 => {}, // Serial control (not implemented)
            0xFF04 => self.timer.write_div(),
            0xFF05 => self.timer.write_tima(value),
            0xFF06 => self.timer.write_tma(value),
            0xFF07 => self.timer.write_tac(value),
            0xFF0F => self.if_reg = value & 0x1F, // Only lower 5 bits writable
            0xFF40 => self.ppu.lcdc = value,
            0xFF41 => self.ppu.stat = (value & 0xF8) | (self.ppu.stat & 0x07), // Only bits 3-6 writable
            0xFF42 => self.ppu.scy = value,
            0xFF43 => self.ppu.scx = value,
            0xFF44 => {}, // LY is read-only
            0xFF45 => self.ppu.lyc = value,
            0xFF46 => {
                // DMA transfer - copies 160 bytes from XX00-XX9F to OAM (FE00-FE9F)
                // This happens instantly (atomically)
                self.do_dma(value as u16);
            }
            0xFF47 => self.ppu.bgp = value,
            0xFF48 => self.ppu.obp0 = value,
            0xFF49 => self.ppu.obp1 = value,
            0xFF4A => self.ppu.wy = value,
            0xFF4B => self.ppu.wx = value,

            // APU registers
            0xFF10..=0xFF26 => self.apu.write_register(address, value),
            0xFF30..=0xFF3F => self.apu.write_register(address, value),

            _ => {}
        }
    }
}
