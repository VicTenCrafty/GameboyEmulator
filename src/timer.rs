pub struct Timer {
    pub div: u16,  // Internal divider counter (16-bit, but only upper 8 bits exposed)
    pub tima: u8,  // Timer counter (0xFF05)
    pub tma: u8,   // Timer modulo (0xFF06)
    pub tac: u8,   // Timer control (0xFF07)

    div_cycles: u32,
    tima_cycles: u32,
}

impl Timer {
    pub fn new() -> Self {
        Timer {
            div: 0,
            tima: 0,
            tma: 0,
            tac: 0,
            div_cycles: 0,
            tima_cycles: 0,
        }
    }

    pub fn step(&mut self, cycles: u32) -> bool {
        // Update DIV register (increments at 16384 Hz = every 256 cycles)
        self.div_cycles += cycles;
        while self.div_cycles >= 256 {
            self.div = self.div.wrapping_add(1);
            self.div_cycles -= 256;
        }

        // Check if timer is enabled
        if (self.tac & 0x04) == 0 {
            return false;
        }

        // Update TIMA based on frequency
        let frequency = match self.tac & 0x03 {
            0 => 1024,  // 4096 Hz
            1 => 16,    // 262144 Hz
            2 => 64,    // 65536 Hz
            3 => 256,   // 16384 Hz
            _ => 1024,
        };

        self.tima_cycles += cycles;
        let mut interrupt = false;

        while self.tima_cycles >= frequency {
            self.tima_cycles -= frequency;

            if self.tima == 0xFF {
                // Timer overflow - trigger interrupt
                self.tima = self.tma;
                interrupt = true;
            } else {
                self.tima = self.tima.wrapping_add(1);
            }
        }

        interrupt
    }

    pub fn read_div(&self) -> u8 {
        (self.div >> 8) as u8
    }

    pub fn write_div(&mut self) {
        self.div = 0;
        self.div_cycles = 0;
    }

    pub fn read_tima(&self) -> u8 {
        self.tima
    }

    pub fn write_tima(&mut self, value: u8) {
        self.tima = value;
    }

    pub fn read_tma(&self) -> u8 {
        self.tma
    }

    pub fn write_tma(&mut self, value: u8) {
        self.tma = value;
    }

    pub fn read_tac(&self) -> u8 {
        self.tac | 0xF8 // Unused bits read as 1
    }

    pub fn write_tac(&mut self, value: u8) {
        self.tac = value & 0x07;
    }
}