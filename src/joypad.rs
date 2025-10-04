pub struct Joypad {
    pub select_button: bool, // Bit 5 - select button keys
    pub select_dpad: bool,   // Bit 4 - select direction keys

    // D-pad
    pub down: bool,
    pub up: bool,
    pub left: bool,
    pub right: bool,

    // Buttons
    pub start: bool,
    pub select: bool,
    pub b: bool,
    pub a: bool,

    // Interrupt tracking
    prev_state: u8,
    pub interrupt_requested: bool,
}

impl Joypad {
    pub fn new() -> Self {
        Joypad {
            select_button: false,
            select_dpad: false,
            down: false,
            up: false,
            left: false,
            right: false,
            start: false,
            select: false,
            b: false,
            a: false,
            prev_state: 0xFF,
            interrupt_requested: false,
        }
    }

    pub fn read(&self) -> u8 {
        let mut result = 0xCF; // Bits 6-7 always 1, bits 0-3 default high

        // When bit 5 is clear, read button keys (Start, Select, B, A)
        if self.select_button {
            result &= 0xEF; // Clear bit 5
            if self.start {
                result &= 0xF7; // Clear bit 3 - Start
            }
            if self.select {
                result &= 0xFB; // Clear bit 2 - Select
            }
            if self.b {
                result &= 0xFD; // Clear bit 1 - B
            }
            if self.a {
                result &= 0xFE; // Clear bit 0 - A
            }
        }

        // When bit 4 is clear, read direction keys (Down, Up, Left, Right)
        if self.select_dpad {
            result &= 0xDF; // Clear bit 4
            if self.down {
                result &= 0xF7; // Clear bit 3 - Down
            }
            if self.up {
                result &= 0xFB; // Clear bit 2 - Up
            }
            if self.left {
                result &= 0xFD; // Clear bit 1 - Left
            }
            if self.right {
                result &= 0xFE; // Clear bit 0 - Right
            }
        }

        result
    }

    pub fn write(&mut self, value: u8) {
        self.select_button = (value & 0x20) == 0;
        self.select_dpad = (value & 0x10) == 0;
    }

    // Check for state change and request interrupt if button pressed
    fn check_interrupt(&mut self, new_state: u8) {
        // Interrupt triggered on high-to-low transition (button press)
        // Bits are active low (0 = pressed)
        let changed = self.prev_state & !new_state;
        if changed & 0x0F != 0 {
            self.interrupt_requested = true;
        }
        self.prev_state = new_state;
    }

    // D-pad controls
    pub fn set_up(&mut self, pressed: bool) {
        self.up = pressed;
        self.check_interrupt(self.read());
    }

    pub fn set_down(&mut self, pressed: bool) {
        self.down = pressed;
        self.check_interrupt(self.read());
    }

    pub fn set_left(&mut self, pressed: bool) {
        self.left = pressed;
        self.check_interrupt(self.read());
    }

    pub fn set_right(&mut self, pressed: bool) {
        self.right = pressed;
        self.check_interrupt(self.read());
    }

    // Button controls
    pub fn set_a(&mut self, pressed: bool) {
        self.a = pressed;
        self.check_interrupt(self.read());
    }

    pub fn set_b(&mut self, pressed: bool) {
        self.b = pressed;
        self.check_interrupt(self.read());
    }

    pub fn set_start(&mut self, pressed: bool) {
        self.start = pressed;
        self.check_interrupt(self.read());
    }

    pub fn set_select(&mut self, pressed: bool) {
        self.select = pressed;
        self.check_interrupt(self.read());
    }
}
