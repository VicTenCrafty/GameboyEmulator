pub struct Registers {
    pub a: u8,
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,
    pub f: u8, // Flags register
    pub sp: u16, // Stack pointer
    pub pc: u16, // Program counter
}

impl Registers {
    pub fn new() -> Self {
        Registers {
            a: 0x01,
            b: 0x00,
            c: 0x13,
            d: 0x00,
            e: 0xD8,
            h: 0x01,
            l: 0x4D,
            f: 0xB0,
            sp: 0xFFFE,
            pc: 0x0100,
        }
    }

    // 16-bit register pairs
    pub fn af(&self) -> u16 {
        ((self.a as u16) << 8) | (self.f as u16)
    }

    pub fn bc(&self) -> u16 {
        ((self.b as u16) << 8) | (self.c as u16)
    }

    pub fn de(&self) -> u16 {
        ((self.d as u16) << 8) | (self.e as u16)
    }

    pub fn hl(&self) -> u16 {
        ((self.h as u16) << 8) | (self.l as u16)
    }

    pub fn set_bc(&mut self, value: u16) {
        self.b = (value >> 8) as u8;
        self.c = value as u8;
    }

    pub fn set_de(&mut self, value: u16) {
        self.d = (value >> 8) as u8;
        self.e = value as u8;
    }

    pub fn set_hl(&mut self, value: u16) {
        self.h = (value >> 8) as u8;
        self.l = value as u8;
    }

    // Flag operations
    pub fn get_flag(&self, flag: Flag) -> bool {
        (self.f & (flag as u8)) != 0
    }

    pub fn set_flag(&mut self, flag: Flag, value: bool) {
        if value {
            self.f |= flag as u8;
        } else {
            self.f &= !(flag as u8);
        }
    }
}

#[repr(u8)]
pub enum Flag {
    Zero = 0b1000_0000,
    Subtract = 0b0100_0000,
    HalfCarry = 0b0010_0000,
    Carry = 0b0001_0000,
}

pub struct Cpu {
    pub registers: Registers,
    pub halted: bool,
    pub ime: bool, // Interrupt Master Enable
    ime_scheduled: bool, // EI takes effect after next instruction
}

impl Cpu {
    pub fn new() -> Self {
        Cpu {
            registers: Registers::new(),
            halted: false,
            ime: false,
            ime_scheduled: false,
        }
    }

    pub fn new_gbc() -> Self {
        let mut cpu = Cpu::new();
        // GBC boot register values
        cpu.registers.a = 0x11; // GBC detection value
        cpu.registers.b = 0x00;
        cpu.registers.c = 0x00; // CGB mode (not in DMG compatibility)
        cpu.registers.d = 0xFF;
        cpu.registers.e = 0x56;
        cpu.registers.h = 0x00;
        cpu.registers.l = 0x0D;
        cpu
    }

    pub fn step(&mut self, mmu: &mut crate::mmu::Mmu) -> u32 {
        // Handle scheduled IME enable (EI takes effect after next instruction)
        if self.ime_scheduled {
            self.ime = true;
            self.ime_scheduled = false;
        }

        // Check for interrupts
        let interrupt_flag = mmu.read_byte(0xFF0F);
        let interrupt_enable = mmu.ie;
        let triggered = interrupt_flag & interrupt_enable;

        if triggered != 0 {
            self.halted = false; // Wake from HALT

            if self.ime {
                self.ime = false; // Disable interrupts

                // Handle interrupts in priority order
                let (vector, bit) = if (triggered & 0x01) != 0 {
                    (0x0040, 0) // VBlank
                } else if (triggered & 0x02) != 0 {
                    (0x0048, 1) // LCD STAT
                } else if (triggered & 0x04) != 0 {
                    (0x0050, 2) // Timer
                } else if (triggered & 0x08) != 0 {
                    (0x0058, 3) // Serial
                } else if (triggered & 0x10) != 0 {
                    (0x0060, 4) // Joypad
                } else {
                    (0x0040, 0)
                };

                mmu.write_byte(0xFF0F, interrupt_flag & !(1 << bit));
                self.push_stack(mmu, self.registers.pc);
                self.registers.pc = vector;
                return 20;
            }
        }

        if self.halted {
            return 4;
        }

        let opcode = mmu.read_byte(self.registers.pc);
        self.registers.pc = self.registers.pc.wrapping_add(1);

        self.execute(opcode, mmu)
    }

    fn execute(&mut self, opcode: u8, mmu: &mut crate::mmu::Mmu) -> u32 {
        match opcode {
            // 8-bit loads
            0x06 => { let v = self.read_byte_pc(mmu); self.registers.b = v; 8 } // LD B, n
            0x0E => { let v = self.read_byte_pc(mmu); self.registers.c = v; 8 } // LD C, n
            0x16 => { let v = self.read_byte_pc(mmu); self.registers.d = v; 8 } // LD D, n
            0x1E => { let v = self.read_byte_pc(mmu); self.registers.e = v; 8 } // LD E, n
            0x26 => { let v = self.read_byte_pc(mmu); self.registers.h = v; 8 } // LD H, n
            0x2E => { let v = self.read_byte_pc(mmu); self.registers.l = v; 8 } // LD L, n
            0x3E => { let v = self.read_byte_pc(mmu); self.registers.a = v; 8 } // LD A, n

            0x40 => { self.registers.b = self.registers.b; 4 } // LD B, B
            0x41 => { self.registers.b = self.registers.c; 4 } // LD B, C
            0x42 => { self.registers.b = self.registers.d; 4 } // LD B, D
            0x43 => { self.registers.b = self.registers.e; 4 } // LD B, E
            0x44 => { self.registers.b = self.registers.h; 4 } // LD B, H
            0x45 => { self.registers.b = self.registers.l; 4 } // LD B, L
            0x47 => { self.registers.b = self.registers.a; 4 } // LD B, A
            0x48 => { self.registers.c = self.registers.b; 4 } // LD C, B
            0x49 => { self.registers.c = self.registers.c; 4 } // LD C, C
            0x4A => { self.registers.c = self.registers.d; 4 } // LD C, D
            0x4B => { self.registers.c = self.registers.e; 4 } // LD C, E
            0x4C => { self.registers.c = self.registers.h; 4 } // LD C, H
            0x4D => { self.registers.c = self.registers.l; 4 } // LD C, L
            0x4F => { self.registers.c = self.registers.a; 4 } // LD C, A
            0x50 => { self.registers.d = self.registers.b; 4 } // LD D, B
            0x51 => { self.registers.d = self.registers.c; 4 } // LD D, C
            0x52 => { self.registers.d = self.registers.d; 4 } // LD D, D
            0x53 => { self.registers.d = self.registers.e; 4 } // LD D, E
            0x54 => { self.registers.d = self.registers.h; 4 } // LD D, H
            0x55 => { self.registers.d = self.registers.l; 4 } // LD D, L
            0x57 => { self.registers.d = self.registers.a; 4 } // LD D, A
            0x58 => { self.registers.e = self.registers.b; 4 } // LD E, B
            0x59 => { self.registers.e = self.registers.c; 4 } // LD E, C
            0x5A => { self.registers.e = self.registers.d; 4 } // LD E, D
            0x5B => { self.registers.e = self.registers.e; 4 } // LD E, E
            0x5C => { self.registers.e = self.registers.h; 4 } // LD E, H
            0x5D => { self.registers.e = self.registers.l; 4 } // LD E, L
            0x5F => { self.registers.e = self.registers.a; 4 } // LD E, A
            0x60 => { self.registers.h = self.registers.b; 4 } // LD H, B
            0x61 => { self.registers.h = self.registers.c; 4 } // LD H, C
            0x62 => { self.registers.h = self.registers.d; 4 } // LD H, D
            0x63 => { self.registers.h = self.registers.e; 4 } // LD H, E
            0x64 => { self.registers.h = self.registers.h; 4 } // LD H, H
            0x65 => { self.registers.h = self.registers.l; 4 } // LD H, L
            0x67 => { self.registers.h = self.registers.a; 4 } // LD H, A
            0x68 => { self.registers.l = self.registers.b; 4 } // LD L, B
            0x69 => { self.registers.l = self.registers.c; 4 } // LD L, C
            0x6A => { self.registers.l = self.registers.d; 4 } // LD L, D
            0x6B => { self.registers.l = self.registers.e; 4 } // LD L, E
            0x6C => { self.registers.l = self.registers.h; 4 } // LD L, H
            0x6D => { self.registers.l = self.registers.l; 4 } // LD L, L
            0x6F => { self.registers.l = self.registers.a; 4 } // LD L, A
            0x78 => { self.registers.a = self.registers.b; 4 } // LD A, B
            0x79 => { self.registers.a = self.registers.c; 4 } // LD A, C
            0x7A => { self.registers.a = self.registers.d; 4 } // LD A, D
            0x7B => { self.registers.a = self.registers.e; 4 } // LD A, E
            0x7C => { self.registers.a = self.registers.h; 4 } // LD A, H
            0x7D => { self.registers.a = self.registers.l; 4 } // LD A, L
            0x7F => { self.registers.a = self.registers.a; 4 } // LD A, A

            0x02 => { let addr = self.registers.bc(); mmu.write_byte(addr, self.registers.a); 8 } // LD (BC), A
            0x12 => { let addr = self.registers.de(); mmu.write_byte(addr, self.registers.a); 8 } // LD (DE), A
            0x0A => { let addr = self.registers.bc(); self.registers.a = mmu.read_byte(addr); 8 } // LD A, (BC)
            0x1A => { let addr = self.registers.de(); self.registers.a = mmu.read_byte(addr); 8 } // LD A, (DE)

            0x36 => { let v = self.read_byte_pc(mmu); let addr = self.registers.hl(); mmu.write_byte(addr, v); 12 } // LD (HL), n
            0x46 => { let addr = self.registers.hl(); self.registers.b = mmu.read_byte(addr); 8 } // LD B, (HL)
            0x4E => { let addr = self.registers.hl(); self.registers.c = mmu.read_byte(addr); 8 } // LD C, (HL)
            0x56 => { let addr = self.registers.hl(); self.registers.d = mmu.read_byte(addr); 8 } // LD D, (HL)
            0x5E => { let addr = self.registers.hl(); self.registers.e = mmu.read_byte(addr); 8 } // LD E, (HL)
            0x66 => { let addr = self.registers.hl(); self.registers.h = mmu.read_byte(addr); 8 } // LD H, (HL)
            0x6E => { let addr = self.registers.hl(); self.registers.l = mmu.read_byte(addr); 8 } // LD L, (HL)
            0x7E => { let addr = self.registers.hl(); self.registers.a = mmu.read_byte(addr); 8 } // LD A, (HL)
            0x70 => { let addr = self.registers.hl(); mmu.write_byte(addr, self.registers.b); 8 } // LD (HL), B
            0x71 => { let addr = self.registers.hl(); mmu.write_byte(addr, self.registers.c); 8 } // LD (HL), C
            0x72 => { let addr = self.registers.hl(); mmu.write_byte(addr, self.registers.d); 8 } // LD (HL), D
            0x73 => { let addr = self.registers.hl(); mmu.write_byte(addr, self.registers.e); 8 } // LD (HL), E
            0x74 => { let addr = self.registers.hl(); mmu.write_byte(addr, self.registers.h); 8 } // LD (HL), H
            0x75 => { let addr = self.registers.hl(); mmu.write_byte(addr, self.registers.l); 8 } // LD (HL), L
            0x77 => { let addr = self.registers.hl(); mmu.write_byte(addr, self.registers.a); 8 } // LD (HL), A

            // 16-bit loads
            0x01 => { let v = self.read_word_pc(mmu); self.registers.set_bc(v); 12 } // LD BC, nn
            0x11 => { let v = self.read_word_pc(mmu); self.registers.set_de(v); 12 } // LD DE, nn
            0x21 => { let v = self.read_word_pc(mmu); self.registers.set_hl(v); 12 } // LD HL, nn
            0x31 => { let v = self.read_word_pc(mmu); self.registers.sp = v; 12 } // LD SP, nn

            // INC/DEC
            0x03 => { let v = self.registers.bc().wrapping_add(1); self.registers.set_bc(v); 8 } // INC BC
            0x13 => { let v = self.registers.de().wrapping_add(1); self.registers.set_de(v); 8 } // INC DE
            0x23 => { let v = self.registers.hl().wrapping_add(1); self.registers.set_hl(v); 8 } // INC HL
            0x33 => { self.registers.sp = self.registers.sp.wrapping_add(1); 8 } // INC SP
            0x0B => { let v = self.registers.bc().wrapping_sub(1); self.registers.set_bc(v); 8 } // DEC BC
            0x1B => { let v = self.registers.de().wrapping_sub(1); self.registers.set_de(v); 8 } // DEC DE
            0x2B => { let v = self.registers.hl().wrapping_sub(1); self.registers.set_hl(v); 8 } // DEC HL
            0x3B => { self.registers.sp = self.registers.sp.wrapping_sub(1); 8 } // DEC SP

            0x04 => { self.registers.b = self.inc(self.registers.b); 4 } // INC B
            0x14 => { self.registers.d = self.inc(self.registers.d); 4 } // INC D
            0x24 => { self.registers.h = self.inc(self.registers.h); 4 } // INC H
            0x0C => { self.registers.c = self.inc(self.registers.c); 4 } // INC C
            0x1C => { self.registers.e = self.inc(self.registers.e); 4 } // INC E
            0x2C => { self.registers.l = self.inc(self.registers.l); 4 } // INC L
            0x3C => { self.registers.a = self.inc(self.registers.a); 4 } // INC A
            0x34 => { let addr = self.registers.hl(); let v = self.inc(mmu.read_byte(addr)); mmu.write_byte(addr, v); 12 } // INC (HL)

            0x05 => { self.registers.b = self.dec(self.registers.b); 4 } // DEC B
            0x15 => { self.registers.d = self.dec(self.registers.d); 4 } // DEC D
            0x25 => { self.registers.h = self.dec(self.registers.h); 4 } // DEC H
            0x0D => { self.registers.c = self.dec(self.registers.c); 4 } // DEC C
            0x1D => { self.registers.e = self.dec(self.registers.e); 4 } // DEC E
            0x2D => { self.registers.l = self.dec(self.registers.l); 4 } // DEC L
            0x3D => { self.registers.a = self.dec(self.registers.a); 4 } // DEC A
            0x35 => { let addr = self.registers.hl(); let v = self.dec(mmu.read_byte(addr)); mmu.write_byte(addr, v); 12 } // DEC (HL)

            // Jumps
            0xC3 => { let addr = self.read_word_pc(mmu); self.registers.pc = addr; 16 } // JP nn
            0xC2 => { let addr = self.read_word_pc(mmu); if !self.registers.get_flag(Flag::Zero) { self.registers.pc = addr; 16 } else { 12 } } // JP NZ, nn
            0xCA => { let addr = self.read_word_pc(mmu); if self.registers.get_flag(Flag::Zero) { self.registers.pc = addr; 16 } else { 12 } } // JP Z, nn
            0xD2 => { let addr = self.read_word_pc(mmu); if !self.registers.get_flag(Flag::Carry) { self.registers.pc = addr; 16 } else { 12 } } // JP NC, nn
            0xDA => { let addr = self.read_word_pc(mmu); if self.registers.get_flag(Flag::Carry) { self.registers.pc = addr; 16 } else { 12 } } // JP C, nn
            0xE9 => { self.registers.pc = self.registers.hl(); 4 } // JP (HL)
            0x18 => { let offset = self.read_byte_pc(mmu) as i8; self.registers.pc = self.registers.pc.wrapping_add(offset as u16); 12 } // JR n
            0x20 => { let offset = self.read_byte_pc(mmu) as i8; if !self.registers.get_flag(Flag::Zero) { self.registers.pc = self.registers.pc.wrapping_add(offset as u16); 12 } else { 8 } } // JR NZ, n
            0x28 => { let offset = self.read_byte_pc(mmu) as i8; if self.registers.get_flag(Flag::Zero) { self.registers.pc = self.registers.pc.wrapping_add(offset as u16); 12 } else { 8 } } // JR Z, n
            0x30 => { let offset = self.read_byte_pc(mmu) as i8; if !self.registers.get_flag(Flag::Carry) { self.registers.pc = self.registers.pc.wrapping_add(offset as u16); 12 } else { 8 } } // JR NC, n
            0x38 => { let offset = self.read_byte_pc(mmu) as i8; if self.registers.get_flag(Flag::Carry) { self.registers.pc = self.registers.pc.wrapping_add(offset as u16); 12 } else { 8 } } // JR C, n

            // Calls & Returns
            0xCD => { let addr = self.read_word_pc(mmu); self.push_stack(mmu, self.registers.pc); self.registers.pc = addr; 24 } // CALL nn
            0xC4 => { let addr = self.read_word_pc(mmu); if !self.registers.get_flag(Flag::Zero) { self.push_stack(mmu, self.registers.pc); self.registers.pc = addr; 24 } else { 12 } } // CALL NZ, nn
            0xCC => { let addr = self.read_word_pc(mmu); if self.registers.get_flag(Flag::Zero) { self.push_stack(mmu, self.registers.pc); self.registers.pc = addr; 24 } else { 12 } } // CALL Z, nn
            0xD4 => { let addr = self.read_word_pc(mmu); if !self.registers.get_flag(Flag::Carry) { self.push_stack(mmu, self.registers.pc); self.registers.pc = addr; 24 } else { 12 } } // CALL NC, nn
            0xDC => { let addr = self.read_word_pc(mmu); if self.registers.get_flag(Flag::Carry) { self.push_stack(mmu, self.registers.pc); self.registers.pc = addr; 24 } else { 12 } } // CALL C, nn
            0xC9 => { self.registers.pc = self.pop_stack(mmu); 16 } // RET
            0xC0 => { if !self.registers.get_flag(Flag::Zero) { self.registers.pc = self.pop_stack(mmu); 20 } else { 8 } } // RET NZ
            0xC8 => { if self.registers.get_flag(Flag::Zero) { self.registers.pc = self.pop_stack(mmu); 20 } else { 8 } } // RET Z
            0xD0 => { if !self.registers.get_flag(Flag::Carry) { self.registers.pc = self.pop_stack(mmu); 20 } else { 8 } } // RET NC
            0xD8 => { if self.registers.get_flag(Flag::Carry) { self.registers.pc = self.pop_stack(mmu); 20 } else { 8 } } // RET C
            0xD9 => { self.registers.pc = self.pop_stack(mmu); self.ime = true; 16 } // RETI

            // Stack operations
            0xC5 => { let v = self.registers.bc(); self.push_stack(mmu, v); 16 } // PUSH BC
            0xD5 => { let v = self.registers.de(); self.push_stack(mmu, v); 16 } // PUSH DE
            0xE5 => { let v = self.registers.hl(); self.push_stack(mmu, v); 16 } // PUSH HL
            0xF5 => { let v = self.registers.af(); self.push_stack(mmu, v); 16 } // PUSH AF
            0xC1 => { let v = self.pop_stack(mmu); self.registers.set_bc(v); 12 } // POP BC
            0xD1 => { let v = self.pop_stack(mmu); self.registers.set_de(v); 12 } // POP DE
            0xE1 => { let v = self.pop_stack(mmu); self.registers.set_hl(v); 12 } // POP HL
            0xF1 => { let v = self.pop_stack(mmu); self.registers.a = (v >> 8) as u8; self.registers.f = (v & 0xF0) as u8; 12 } // POP AF

            // ALU operations
            0x87 => { self.add(self.registers.a); 4 } // ADD A, A
            0x80 => { self.add(self.registers.b); 4 } // ADD A, B
            0x81 => { self.add(self.registers.c); 4 } // ADD A, C
            0x82 => { self.add(self.registers.d); 4 } // ADD A, D
            0x83 => { self.add(self.registers.e); 4 } // ADD A, E
            0x84 => { self.add(self.registers.h); 4 } // ADD A, H
            0x85 => { self.add(self.registers.l); 4 } // ADD A, L
            0x86 => { let v = mmu.read_byte(self.registers.hl()); self.add(v); 8 } // ADD A, (HL)
            0xC6 => { let v = self.read_byte_pc(mmu); self.add(v); 8 } // ADD A, n

            0x09 => { self.add_hl(self.registers.bc()); 8 } // ADD HL, BC
            0x19 => { self.add_hl(self.registers.de()); 8 } // ADD HL, DE
            0x29 => { let hl = self.registers.hl(); self.add_hl(hl); 8 } // ADD HL, HL
            0x39 => { self.add_hl(self.registers.sp); 8 } // ADD HL, SP
            0xE8 => { let v = self.read_byte_pc(mmu) as i8; self.add_sp(v); 16 } // ADD SP, n

            0x8F => { self.adc(self.registers.a); 4 } // ADC A, A
            0x88 => { self.adc(self.registers.b); 4 } // ADC A, B
            0x89 => { self.adc(self.registers.c); 4 } // ADC A, C
            0x8A => { self.adc(self.registers.d); 4 } // ADC A, D
            0x8B => { self.adc(self.registers.e); 4 } // ADC A, E
            0x8C => { self.adc(self.registers.h); 4 } // ADC A, H
            0x8D => { self.adc(self.registers.l); 4 } // ADC A, L
            0x8E => { let v = mmu.read_byte(self.registers.hl()); self.adc(v); 8 } // ADC A, (HL)
            0xCE => { let v = self.read_byte_pc(mmu); self.adc(v); 8 } // ADC A, n

            0x97 => { self.sub(self.registers.a); 4 } // SUB A
            0x90 => { self.sub(self.registers.b); 4 } // SUB B
            0x91 => { self.sub(self.registers.c); 4 } // SUB C
            0x92 => { self.sub(self.registers.d); 4 } // SUB D
            0x93 => { self.sub(self.registers.e); 4 } // SUB E
            0x94 => { self.sub(self.registers.h); 4 } // SUB H
            0x95 => { self.sub(self.registers.l); 4 } // SUB L
            0x96 => { let v = mmu.read_byte(self.registers.hl()); self.sub(v); 8 } // SUB (HL)
            0xD6 => { let v = self.read_byte_pc(mmu); self.sub(v); 8 } // SUB n
            0x9F => { self.sbc(self.registers.a); 4 } // SBC A, A
            0x98 => { self.sbc(self.registers.b); 4 } // SBC A, B
            0x99 => { self.sbc(self.registers.c); 4 } // SBC A, C
            0x9A => { self.sbc(self.registers.d); 4 } // SBC A, D
            0x9B => { self.sbc(self.registers.e); 4 } // SBC A, E
            0x9C => { self.sbc(self.registers.h); 4 } // SBC A, H
            0x9D => { self.sbc(self.registers.l); 4 } // SBC A, L
            0x9E => { let v = mmu.read_byte(self.registers.hl()); self.sbc(v); 8 } // SBC A, (HL)
            0xDE => { let v = self.read_byte_pc(mmu); self.sbc(v); 8 } // SBC A, n

            0xA7 => { self.and(self.registers.a); 4 } // AND A
            0xA0 => { self.and(self.registers.b); 4 } // AND B
            0xA1 => { self.and(self.registers.c); 4 } // AND C
            0xA2 => { self.and(self.registers.d); 4 } // AND D
            0xA3 => { self.and(self.registers.e); 4 } // AND E
            0xA4 => { self.and(self.registers.h); 4 } // AND H
            0xA5 => { self.and(self.registers.l); 4 } // AND L
            0xA6 => { let v = mmu.read_byte(self.registers.hl()); self.and(v); 8 } // AND (HL)
            0xE6 => { let v = self.read_byte_pc(mmu); self.and(v); 8 } // AND n

            0xB7 => { self.or(self.registers.a); 4 } // OR A
            0xB0 => { self.or(self.registers.b); 4 } // OR B
            0xB1 => { self.or(self.registers.c); 4 } // OR C
            0xB2 => { self.or(self.registers.d); 4 } // OR D
            0xB3 => { self.or(self.registers.e); 4 } // OR E
            0xB4 => { self.or(self.registers.h); 4 } // OR H
            0xB5 => { self.or(self.registers.l); 4 } // OR L
            0xB6 => { let v = mmu.read_byte(self.registers.hl()); self.or(v); 8 } // OR (HL)
            0xF6 => { let v = self.read_byte_pc(mmu); self.or(v); 8 } // OR n

            0xAF => { self.xor(self.registers.a); 4 } // XOR A
            0xA8 => { self.xor(self.registers.b); 4 } // XOR B
            0xA9 => { self.xor(self.registers.c); 4 } // XOR C
            0xAA => { self.xor(self.registers.d); 4 } // XOR D
            0xAB => { self.xor(self.registers.e); 4 } // XOR E
            0xAC => { self.xor(self.registers.h); 4 } // XOR H
            0xAD => { self.xor(self.registers.l); 4 } // XOR L
            0xAE => { let v = mmu.read_byte(self.registers.hl()); self.xor(v); 8 } // XOR (HL)
            0xEE => { let v = self.read_byte_pc(mmu); self.xor(v); 8 } // XOR n

            0xBF => { self.cp(self.registers.a); 4 } // CP A
            0xB8 => { self.cp(self.registers.b); 4 } // CP B
            0xB9 => { self.cp(self.registers.c); 4 } // CP C
            0xBA => { self.cp(self.registers.d); 4 } // CP D
            0xBB => { self.cp(self.registers.e); 4 } // CP E
            0xBC => { self.cp(self.registers.h); 4 } // CP H
            0xBD => { self.cp(self.registers.l); 4 } // CP L
            0xBE => { let v = mmu.read_byte(self.registers.hl()); self.cp(v); 8 } // CP (HL)
            0xFE => { let v = self.read_byte_pc(mmu); self.cp(v); 8 } // CP n

            // Memory operations
            0x22 => { let addr = self.registers.hl(); mmu.write_byte(addr, self.registers.a); self.registers.set_hl(addr.wrapping_add(1)); 8 } // LD (HL+), A
            0x32 => { let addr = self.registers.hl(); mmu.write_byte(addr, self.registers.a); self.registers.set_hl(addr.wrapping_sub(1)); 8 } // LD (HL-), A
            0x2A => { let addr = self.registers.hl(); self.registers.a = mmu.read_byte(addr); self.registers.set_hl(addr.wrapping_add(1)); 8 } // LD A, (HL+)
            0x3A => { let addr = self.registers.hl(); self.registers.a = mmu.read_byte(addr); self.registers.set_hl(addr.wrapping_sub(1)); 8 } // LD A, (HL-)

            0xE0 => { let offset = self.read_byte_pc(mmu); mmu.write_byte(0xFF00 + offset as u16, self.registers.a); 12 } // LDH (n), A
            0xF0 => { let offset = self.read_byte_pc(mmu); self.registers.a = mmu.read_byte(0xFF00 + offset as u16); 12 } // LDH A, (n)
            0xE2 => { mmu.write_byte(0xFF00 + self.registers.c as u16, self.registers.a); 8 } // LD (C), A
            0xF2 => { self.registers.a = mmu.read_byte(0xFF00 + self.registers.c as u16); 8 } // LD A, (C)
            0xEA => { let addr = self.read_word_pc(mmu); mmu.write_byte(addr, self.registers.a); 16 } // LD (nn), A
            0xFA => { let addr = self.read_word_pc(mmu); self.registers.a = mmu.read_byte(addr); 16 } // LD A, (nn)

            // Misc
            0x00 => 4, // NOP
            0x10 => {
                // STOP - Halts CPU and LCD until button press
                // Read and discard the next byte (always 0x00)
                self.read_byte_pc(mmu);

                // On GBC with KEY1 bit 0 set, this performs speed switching
                // Otherwise, it acts like HALT (stops until interrupt)
                let key1 = mmu.read_byte(0xFF4D);
                if (key1 & 0x01) != 0 {
                    // Speed switch requested - toggle speed and clear bit 0
                    mmu.write_byte(0xFF4D, key1 ^ 0x80);
                }

                // STOP always halts like HALT
                self.halted = true;
                4
            }
            0x76 => { self.halted = true; 4 } // HALT
            0xF3 => { self.ime = false; self.ime_scheduled = false; 4 } // DI
            0xFB => { self.ime_scheduled = true; 4 } // EI (takes effect after next instruction)
            0x17 => { self.rl(true, false); 4 } // RLA
            0x1F => { self.rr(true, false); 4 } // RRA
            0x07 => { self.rlc(true, false); 4 } // RLCA
            0x0F => { self.rrc(true, false); 4 } // RRCA
            0x27 => { self.daa(); 4 } // DAA
            0x2F => { self.registers.a = !self.registers.a; self.registers.set_flag(Flag::Subtract, true); self.registers.set_flag(Flag::HalfCarry, true); 4 } // CPL
            0x3F => { let c = self.registers.get_flag(Flag::Carry); self.registers.set_flag(Flag::Subtract, false); self.registers.set_flag(Flag::HalfCarry, false); self.registers.set_flag(Flag::Carry, !c); 4 } // CCF
            0x37 => { self.registers.set_flag(Flag::Subtract, false); self.registers.set_flag(Flag::HalfCarry, false); self.registers.set_flag(Flag::Carry, true); 4 } // SCF

            // RST
            0xC7 => { self.push_stack(mmu, self.registers.pc); self.registers.pc = 0x00; 16 } // RST 00
            0xCF => { self.push_stack(mmu, self.registers.pc); self.registers.pc = 0x08; 16 } // RST 08
            0xD7 => { self.push_stack(mmu, self.registers.pc); self.registers.pc = 0x10; 16 } // RST 10
            0xDF => { self.push_stack(mmu, self.registers.pc); self.registers.pc = 0x18; 16 } // RST 18
            0xE7 => { self.push_stack(mmu, self.registers.pc); self.registers.pc = 0x20; 16 } // RST 20
            0xEF => { self.push_stack(mmu, self.registers.pc); self.registers.pc = 0x28; 16 } // RST 28
            0xF7 => { self.push_stack(mmu, self.registers.pc); self.registers.pc = 0x30; 16 } // RST 30
            0xFF => { self.push_stack(mmu, self.registers.pc); self.registers.pc = 0x38; 16 } // RST 38

            0xF9 => { self.registers.sp = self.registers.hl(); 8 } // LD SP, HL
            0x08 => { let addr = self.read_word_pc(mmu); mmu.write_byte(addr, self.registers.sp as u8); mmu.write_byte(addr + 1, (self.registers.sp >> 8) as u8); 20 } // LD (nn), SP
            0xF8 => { let v = self.read_byte_pc(mmu) as i8; let result = self.registers.sp.wrapping_add(v as u16); self.registers.set_flag(Flag::Zero, false); self.registers.set_flag(Flag::Subtract, false); self.registers.set_flag(Flag::HalfCarry, ((self.registers.sp & 0x0F) + ((v as u16) & 0x0F)) > 0x0F); self.registers.set_flag(Flag::Carry, ((self.registers.sp & 0xFF) + ((v as u16) & 0xFF)) > 0xFF); self.registers.set_hl(result); 12 } // LD HL, SP+n

            0xCB => self.execute_cb(mmu),

            _ => {
                println!("Unknown opcode: 0x{:02X} at PC: 0x{:04X}", opcode, self.registers.pc - 1);
                4
            }
        }
    }

    fn execute_cb(&mut self, mmu: &mut crate::mmu::Mmu) -> u32 {
        let opcode = self.read_byte_pc(mmu);
        match opcode {
            // RLC - Rotate left with carry
            0x00 => { self.registers.b = self.rlc_reg(self.registers.b); 8 }
            0x01 => { self.registers.c = self.rlc_reg(self.registers.c); 8 }
            0x02 => { self.registers.d = self.rlc_reg(self.registers.d); 8 }
            0x03 => { self.registers.e = self.rlc_reg(self.registers.e); 8 }
            0x04 => { self.registers.h = self.rlc_reg(self.registers.h); 8 }
            0x05 => { self.registers.l = self.rlc_reg(self.registers.l); 8 }
            0x06 => { let addr = self.registers.hl(); let v = self.rlc_reg(mmu.read_byte(addr)); mmu.write_byte(addr, v); 16 }
            0x07 => { self.registers.a = self.rlc_reg(self.registers.a); 8 }

            // RRC - Rotate right with carry
            0x08 => { self.registers.b = self.rrc_reg(self.registers.b); 8 }
            0x09 => { self.registers.c = self.rrc_reg(self.registers.c); 8 }
            0x0A => { self.registers.d = self.rrc_reg(self.registers.d); 8 }
            0x0B => { self.registers.e = self.rrc_reg(self.registers.e); 8 }
            0x0C => { self.registers.h = self.rrc_reg(self.registers.h); 8 }
            0x0D => { self.registers.l = self.rrc_reg(self.registers.l); 8 }
            0x0E => { let addr = self.registers.hl(); let v = self.rrc_reg(mmu.read_byte(addr)); mmu.write_byte(addr, v); 16 }
            0x0F => { self.registers.a = self.rrc_reg(self.registers.a); 8 }

            // RL - Rotate left through carry
            0x10 => { self.registers.b = self.rl_reg_full(self.registers.b); 8 }
            0x11 => { self.registers.c = self.rl_reg_full(self.registers.c); 8 }
            0x12 => { self.registers.d = self.rl_reg_full(self.registers.d); 8 }
            0x13 => { self.registers.e = self.rl_reg_full(self.registers.e); 8 }
            0x14 => { self.registers.h = self.rl_reg_full(self.registers.h); 8 }
            0x15 => { self.registers.l = self.rl_reg_full(self.registers.l); 8 }
            0x16 => { let addr = self.registers.hl(); let v = self.rl_reg_full(mmu.read_byte(addr)); mmu.write_byte(addr, v); 16 }
            0x17 => { self.registers.a = self.rl_reg_full(self.registers.a); 8 }

            // RR - Rotate right through carry
            0x18 => { self.registers.b = self.rr_reg_full(self.registers.b); 8 }
            0x19 => { self.registers.c = self.rr_reg_full(self.registers.c); 8 }
            0x1A => { self.registers.d = self.rr_reg_full(self.registers.d); 8 }
            0x1B => { self.registers.e = self.rr_reg_full(self.registers.e); 8 }
            0x1C => { self.registers.h = self.rr_reg_full(self.registers.h); 8 }
            0x1D => { self.registers.l = self.rr_reg_full(self.registers.l); 8 }
            0x1E => { let addr = self.registers.hl(); let v = self.rr_reg_full(mmu.read_byte(addr)); mmu.write_byte(addr, v); 16 }
            0x1F => { self.registers.a = self.rr_reg_full(self.registers.a); 8 }

            // SLA - Shift left arithmetic
            0x20 => { self.registers.b = self.sla(self.registers.b); 8 }
            0x21 => { self.registers.c = self.sla(self.registers.c); 8 }
            0x22 => { self.registers.d = self.sla(self.registers.d); 8 }
            0x23 => { self.registers.e = self.sla(self.registers.e); 8 }
            0x24 => { self.registers.h = self.sla(self.registers.h); 8 }
            0x25 => { self.registers.l = self.sla(self.registers.l); 8 }
            0x26 => { let addr = self.registers.hl(); let v = self.sla(mmu.read_byte(addr)); mmu.write_byte(addr, v); 16 }
            0x27 => { self.registers.a = self.sla(self.registers.a); 8 }

            // SRA - Shift right arithmetic
            0x28 => { self.registers.b = self.sra(self.registers.b); 8 }
            0x29 => { self.registers.c = self.sra(self.registers.c); 8 }
            0x2A => { self.registers.d = self.sra(self.registers.d); 8 }
            0x2B => { self.registers.e = self.sra(self.registers.e); 8 }
            0x2C => { self.registers.h = self.sra(self.registers.h); 8 }
            0x2D => { self.registers.l = self.sra(self.registers.l); 8 }
            0x2E => { let addr = self.registers.hl(); let v = self.sra(mmu.read_byte(addr)); mmu.write_byte(addr, v); 16 }
            0x2F => { self.registers.a = self.sra(self.registers.a); 8 }

            // SWAP
            0x30 => { self.registers.b = self.swap(self.registers.b); 8 }
            0x31 => { self.registers.c = self.swap(self.registers.c); 8 }
            0x32 => { self.registers.d = self.swap(self.registers.d); 8 }
            0x33 => { self.registers.e = self.swap(self.registers.e); 8 }
            0x34 => { self.registers.h = self.swap(self.registers.h); 8 }
            0x35 => { self.registers.l = self.swap(self.registers.l); 8 }
            0x36 => { let addr = self.registers.hl(); let v = self.swap(mmu.read_byte(addr)); mmu.write_byte(addr, v); 16 }
            0x37 => { self.registers.a = self.swap(self.registers.a); 8 }

            // SRL - Shift right logical
            0x38 => { self.registers.b = self.srl(self.registers.b); 8 }
            0x39 => { self.registers.c = self.srl(self.registers.c); 8 }
            0x3A => { self.registers.d = self.srl(self.registers.d); 8 }
            0x3B => { self.registers.e = self.srl(self.registers.e); 8 }
            0x3C => { self.registers.h = self.srl(self.registers.h); 8 }
            0x3D => { self.registers.l = self.srl(self.registers.l); 8 }
            0x3E => { let addr = self.registers.hl(); let v = self.srl(mmu.read_byte(addr)); mmu.write_byte(addr, v); 16 }
            0x3F => { self.registers.a = self.srl(self.registers.a); 8 }

            // BIT - Test bit
            0x40..=0x7F => {
                let bit = (opcode >> 3) & 0x07;
                let reg = opcode & 0x07;
                let value = match reg {
                    0 => self.registers.b,
                    1 => self.registers.c,
                    2 => self.registers.d,
                    3 => self.registers.e,
                    4 => self.registers.h,
                    5 => self.registers.l,
                    6 => mmu.read_byte(self.registers.hl()),
                    7 => self.registers.a,
                    _ => 0,
                };
                self.bit(bit, value);
                if reg == 6 { 12 } else { 8 }
            }

            // RES - Reset bit
            0x80..=0xBF => {
                let bit = (opcode >> 3) & 0x07;
                let reg = opcode & 0x07;
                let mask = !(1 << bit);
                match reg {
                    0 => { self.registers.b &= mask; 8 }
                    1 => { self.registers.c &= mask; 8 }
                    2 => { self.registers.d &= mask; 8 }
                    3 => { self.registers.e &= mask; 8 }
                    4 => { self.registers.h &= mask; 8 }
                    5 => { self.registers.l &= mask; 8 }
                    6 => { let addr = self.registers.hl(); let v = mmu.read_byte(addr) & mask; mmu.write_byte(addr, v); 16 }
                    7 => { self.registers.a &= mask; 8 }
                    _ => 8,
                }
            }

            // SET - Set bit
            0xC0..=0xFF => {
                let bit = (opcode >> 3) & 0x07;
                let reg = opcode & 0x07;
                let mask = 1 << bit;
                match reg {
                    0 => { self.registers.b |= mask; 8 }
                    1 => { self.registers.c |= mask; 8 }
                    2 => { self.registers.d |= mask; 8 }
                    3 => { self.registers.e |= mask; 8 }
                    4 => { self.registers.h |= mask; 8 }
                    5 => { self.registers.l |= mask; 8 }
                    6 => { let addr = self.registers.hl(); let v = mmu.read_byte(addr) | mask; mmu.write_byte(addr, v); 16 }
                    7 => { self.registers.a |= mask; 8 }
                    _ => 8,
                }
            }
        }
    }

    // Helper methods
    fn read_byte_pc(&mut self, mmu: &mut crate::mmu::Mmu) -> u8 {
        let byte = mmu.read_byte(self.registers.pc);
        self.registers.pc = self.registers.pc.wrapping_add(1);
        byte
    }

    fn read_word_pc(&mut self, mmu: &mut crate::mmu::Mmu) -> u16 {
        let low = self.read_byte_pc(mmu) as u16;
        let high = self.read_byte_pc(mmu) as u16;
        (high << 8) | low
    }

    fn push_stack(&mut self, mmu: &mut crate::mmu::Mmu, value: u16) {
        self.registers.sp = self.registers.sp.wrapping_sub(1);
        mmu.write_byte(self.registers.sp, (value >> 8) as u8);
        self.registers.sp = self.registers.sp.wrapping_sub(1);
        mmu.write_byte(self.registers.sp, value as u8);
    }

    fn pop_stack(&mut self, mmu: &mut crate::mmu::Mmu) -> u16 {
        let low = mmu.read_byte(self.registers.sp) as u16;
        self.registers.sp = self.registers.sp.wrapping_add(1);
        let high = mmu.read_byte(self.registers.sp) as u16;
        self.registers.sp = self.registers.sp.wrapping_add(1);
        (high << 8) | low
    }

    fn inc(&mut self, value: u8) -> u8 {
        let result = value.wrapping_add(1);
        self.registers.set_flag(Flag::Zero, result == 0);
        self.registers.set_flag(Flag::Subtract, false);
        self.registers.set_flag(Flag::HalfCarry, (value & 0x0F) + 1 > 0x0F);
        result
    }

    fn dec(&mut self, value: u8) -> u8 {
        let result = value.wrapping_sub(1);
        self.registers.set_flag(Flag::Zero, result == 0);
        self.registers.set_flag(Flag::Subtract, true);
        self.registers.set_flag(Flag::HalfCarry, (value & 0x0F) == 0);
        result
    }

    fn add(&mut self, value: u8) {
        let a = self.registers.a;
        let result = a.wrapping_add(value);
        self.registers.set_flag(Flag::Zero, result == 0);
        self.registers.set_flag(Flag::Subtract, false);
        self.registers.set_flag(Flag::HalfCarry, (a & 0x0F) + (value & 0x0F) > 0x0F);
        self.registers.set_flag(Flag::Carry, (a as u16) + (value as u16) > 0xFF);
        self.registers.a = result;
    }

    fn adc(&mut self, value: u8) {
        let a = self.registers.a;
        let carry = if self.registers.get_flag(Flag::Carry) { 1 } else { 0 };
        let result = a.wrapping_add(value).wrapping_add(carry);
        self.registers.set_flag(Flag::Zero, result == 0);
        self.registers.set_flag(Flag::Subtract, false);
        self.registers.set_flag(Flag::HalfCarry, (a & 0x0F) + (value & 0x0F) + carry > 0x0F);
        self.registers.set_flag(Flag::Carry, (a as u16) + (value as u16) + (carry as u16) > 0xFF);
        self.registers.a = result;
    }

    fn sub(&mut self, value: u8) {
        let a = self.registers.a;
        let result = a.wrapping_sub(value);
        self.registers.set_flag(Flag::Zero, result == 0);
        self.registers.set_flag(Flag::Subtract, true);
        self.registers.set_flag(Flag::HalfCarry, (a & 0x0F) < (value & 0x0F));
        self.registers.set_flag(Flag::Carry, a < value);
        self.registers.a = result;
    }

    fn and(&mut self, value: u8) {
        self.registers.a &= value;
        self.registers.set_flag(Flag::Zero, self.registers.a == 0);
        self.registers.set_flag(Flag::Subtract, false);
        self.registers.set_flag(Flag::HalfCarry, true);
        self.registers.set_flag(Flag::Carry, false);
    }

    fn or(&mut self, value: u8) {
        self.registers.a |= value;
        self.registers.set_flag(Flag::Zero, self.registers.a == 0);
        self.registers.set_flag(Flag::Subtract, false);
        self.registers.set_flag(Flag::HalfCarry, false);
        self.registers.set_flag(Flag::Carry, false);
    }

    fn xor(&mut self, value: u8) {
        self.registers.a ^= value;
        self.registers.set_flag(Flag::Zero, self.registers.a == 0);
        self.registers.set_flag(Flag::Subtract, false);
        self.registers.set_flag(Flag::HalfCarry, false);
        self.registers.set_flag(Flag::Carry, false);
    }

    fn cp(&mut self, value: u8) {
        let a = self.registers.a;
        self.registers.set_flag(Flag::Zero, a == value);
        self.registers.set_flag(Flag::Subtract, true);
        self.registers.set_flag(Flag::HalfCarry, (a & 0x0F) < (value & 0x0F));
        self.registers.set_flag(Flag::Carry, a < value);
    }

    fn swap(&mut self, value: u8) -> u8 {
        let result = (value >> 4) | (value << 4);
        self.registers.set_flag(Flag::Zero, result == 0);
        self.registers.set_flag(Flag::Subtract, false);
        self.registers.set_flag(Flag::HalfCarry, false);
        self.registers.set_flag(Flag::Carry, false);
        result
    }

    fn bit(&mut self, bit: u8, value: u8) {
        let result = value & (1 << bit);
        self.registers.set_flag(Flag::Zero, result == 0);
        self.registers.set_flag(Flag::Subtract, false);
        self.registers.set_flag(Flag::HalfCarry, true);
    }

    fn rl(&mut self, is_a: bool, set_zero: bool) {
        let value = if is_a { self.registers.a } else { self.registers.c };
        let carry = if self.registers.get_flag(Flag::Carry) { 1 } else { 0 };
        let new_carry = (value & 0x80) != 0;
        let result = (value << 1) | carry;

        if is_a { self.registers.a = result; } else { self.registers.c = result; }

        self.registers.set_flag(Flag::Zero, set_zero && result == 0);
        self.registers.set_flag(Flag::Subtract, false);
        self.registers.set_flag(Flag::HalfCarry, false);
        self.registers.set_flag(Flag::Carry, new_carry);
    }

    fn rl_reg_full(&mut self, value: u8) -> u8 {
        let carry = if self.registers.get_flag(Flag::Carry) { 1 } else { 0 };
        let new_carry = (value & 0x80) != 0;
        let result = (value << 1) | carry;
        self.registers.set_flag(Flag::Zero, result == 0);
        self.registers.set_flag(Flag::Subtract, false);
        self.registers.set_flag(Flag::HalfCarry, false);
        self.registers.set_flag(Flag::Carry, new_carry);
        result
    }

    fn rr_reg_full(&mut self, value: u8) -> u8 {
        let carry = if self.registers.get_flag(Flag::Carry) { 0x80 } else { 0 };
        let new_carry = (value & 0x01) != 0;
        let result = (value >> 1) | carry;
        self.registers.set_flag(Flag::Zero, result == 0);
        self.registers.set_flag(Flag::Subtract, false);
        self.registers.set_flag(Flag::HalfCarry, false);
        self.registers.set_flag(Flag::Carry, new_carry);
        result
    }

    fn rlc_reg(&mut self, value: u8) -> u8 {
        let carry = (value & 0x80) != 0;
        let result = value.rotate_left(1);
        self.registers.set_flag(Flag::Zero, result == 0);
        self.registers.set_flag(Flag::Subtract, false);
        self.registers.set_flag(Flag::HalfCarry, false);
        self.registers.set_flag(Flag::Carry, carry);
        result
    }

    fn sla(&mut self, value: u8) -> u8 {
        let carry = (value & 0x80) != 0;
        let result = value << 1;
        self.registers.set_flag(Flag::Zero, result == 0);
        self.registers.set_flag(Flag::Subtract, false);
        self.registers.set_flag(Flag::HalfCarry, false);
        self.registers.set_flag(Flag::Carry, carry);
        result
    }

    fn sra(&mut self, value: u8) -> u8 {
        let carry = (value & 0x01) != 0;
        let result = (value >> 1) | (value & 0x80);
        self.registers.set_flag(Flag::Zero, result == 0);
        self.registers.set_flag(Flag::Subtract, false);
        self.registers.set_flag(Flag::HalfCarry, false);
        self.registers.set_flag(Flag::Carry, carry);
        result
    }

    fn srl(&mut self, value: u8) -> u8 {
        let carry = (value & 0x01) != 0;
        let result = value >> 1;
        self.registers.set_flag(Flag::Zero, result == 0);
        self.registers.set_flag(Flag::Subtract, false);
        self.registers.set_flag(Flag::HalfCarry, false);
        self.registers.set_flag(Flag::Carry, carry);
        result
    }

    fn rr(&mut self, is_a: bool, _set_zero: bool) {
        let value = if is_a { self.registers.a } else { 0 };
        let carry = if self.registers.get_flag(Flag::Carry) { 0x80 } else { 0 };
        let new_carry = (value & 0x01) != 0;
        let result = (value >> 1) | carry;

        if is_a { self.registers.a = result; }

        self.registers.set_flag(Flag::Zero, false);
        self.registers.set_flag(Flag::Subtract, false);
        self.registers.set_flag(Flag::HalfCarry, false);
        self.registers.set_flag(Flag::Carry, new_carry);
    }

    fn rlc(&mut self, is_a: bool, _set_zero: bool) {
        let value = if is_a { self.registers.a } else { 0 };
        let carry = (value & 0x80) != 0;
        let result = value.rotate_left(1);

        if is_a { self.registers.a = result; }

        self.registers.set_flag(Flag::Zero, false);
        self.registers.set_flag(Flag::Subtract, false);
        self.registers.set_flag(Flag::HalfCarry, false);
        self.registers.set_flag(Flag::Carry, carry);
    }

    fn rrc(&mut self, is_a: bool, _set_zero: bool) {
        let value = if is_a { self.registers.a } else { 0 };
        let carry = (value & 0x01) != 0;
        let result = value.rotate_right(1);

        if is_a { self.registers.a = result; }

        self.registers.set_flag(Flag::Zero, false);
        self.registers.set_flag(Flag::Subtract, false);
        self.registers.set_flag(Flag::HalfCarry, false);
        self.registers.set_flag(Flag::Carry, carry);
    }

    fn rrc_reg(&mut self, value: u8) -> u8 {
        let carry = (value & 0x01) != 0;
        let result = value.rotate_right(1);
        self.registers.set_flag(Flag::Zero, result == 0);
        self.registers.set_flag(Flag::Subtract, false);
        self.registers.set_flag(Flag::HalfCarry, false);
        self.registers.set_flag(Flag::Carry, carry);
        result
    }

    fn daa(&mut self) {
        let mut a = self.registers.a;
        if !self.registers.get_flag(Flag::Subtract) {
            if self.registers.get_flag(Flag::Carry) || a > 0x99 {
                a = a.wrapping_add(0x60);
                self.registers.set_flag(Flag::Carry, true);
            }
            if self.registers.get_flag(Flag::HalfCarry) || (a & 0x0F) > 0x09 {
                a = a.wrapping_add(0x06);
            }
        } else {
            if self.registers.get_flag(Flag::Carry) {
                a = a.wrapping_sub(0x60);
            }
            if self.registers.get_flag(Flag::HalfCarry) {
                a = a.wrapping_sub(0x06);
            }
        }
        self.registers.a = a;
        self.registers.set_flag(Flag::Zero, a == 0);
        self.registers.set_flag(Flag::HalfCarry, false);
    }

    fn add_hl(&mut self, value: u16) {
        let hl = self.registers.hl();
        let result = hl.wrapping_add(value);
        self.registers.set_flag(Flag::Subtract, false);
        self.registers.set_flag(Flag::HalfCarry, ((hl & 0x0FFF) + (value & 0x0FFF)) > 0x0FFF);
        self.registers.set_flag(Flag::Carry, hl > 0xFFFF - value);
        self.registers.set_hl(result);
    }

    fn add_sp(&mut self, value: i8) {
        let sp = self.registers.sp;
        let result = sp.wrapping_add(value as u16);
        self.registers.set_flag(Flag::Zero, false);
        self.registers.set_flag(Flag::Subtract, false);
        self.registers.set_flag(Flag::HalfCarry, ((sp & 0x0F) + ((value as u16) & 0x0F)) > 0x0F);
        self.registers.set_flag(Flag::Carry, ((sp & 0xFF) + ((value as u16) & 0xFF)) > 0xFF);
        self.registers.sp = result;
    }

    fn sbc(&mut self, value: u8) {
        let a = self.registers.a;
        let carry = if self.registers.get_flag(Flag::Carry) { 1 } else { 0 };
        let result = a.wrapping_sub(value).wrapping_sub(carry);
        self.registers.set_flag(Flag::Zero, result == 0);
        self.registers.set_flag(Flag::Subtract, true);
        self.registers.set_flag(Flag::HalfCarry, (a & 0x0F) < (value & 0x0F) + carry);
        self.registers.set_flag(Flag::Carry, (a as u16) < (value as u16) + (carry as u16));
        self.registers.a = result;
    }
}