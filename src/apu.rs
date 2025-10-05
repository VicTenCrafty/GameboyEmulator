// Basic APU (Audio Processing Unit) implementation with audio output

use std::sync::{Arc, Mutex};

const SAMPLE_RATE: u32 = 48000;
const BUFFER_SIZE: usize = 2048;

pub struct Apu {
    // Audio buffer shared with output thread
    pub audio_buffer: Arc<Mutex<Vec<f32>>>,
    sample_counter: f32,

    // Channel state
    ch1_freq_timer: i32,
    ch1_duty_pos: u8,
    ch1_volume: u8,
    ch1_volume_initial: u8,
    ch1_envelope_timer: u8,
    ch1_enabled: bool,
    ch1_length_counter: u16,

    ch2_freq_timer: i32,
    ch2_duty_pos: u8,
    ch2_volume: u8,
    ch2_volume_initial: u8,
    ch2_envelope_timer: u8,
    ch2_enabled: bool,
    ch2_length_counter: u16,

    ch3_freq_timer: i32,
    ch3_wave_pos: u8,
    ch3_enabled: bool,
    ch3_length_counter: u16,

    ch4_lfsr: u16,
    ch4_freq_timer: i32,
    ch4_volume: u8,
    ch4_volume_initial: u8,
    ch4_envelope_timer: u8,
    ch4_enabled: bool,
    ch4_length_counter: u16,

    // High-pass filter state
    capacitor: f32,
    // Low-pass filter state (for smoothing)
    last_output: f32,
    // Channel control
    pub nr50: u8, // Master volume & VIN panning
    pub nr51: u8, // Sound panning
    pub nr52: u8, // Sound on/off

    // Channel 1 - Square wave with sweep
    pub nr10: u8, // Sweep
    pub nr11: u8, // Length timer & duty cycle
    pub nr12: u8, // Volume & envelope
    pub nr13: u8, // Period low
    pub nr14: u8, // Period high & control

    // Channel 2 - Square wave
    pub nr21: u8, // Length timer & duty cycle
    pub nr22: u8, // Volume & envelope
    pub nr23: u8, // Period low
    pub nr24: u8, // Period high & control

    // Channel 3 - Wave output
    pub nr30: u8, // DAC enable
    pub nr31: u8, // Length timer
    pub nr32: u8, // Output level
    pub nr33: u8, // Period low
    pub nr34: u8, // Period high & control
    pub wave_ram: [u8; 16], // Wave pattern RAM

    // Channel 4 - Noise
    pub nr41: u8, // Length timer
    pub nr42: u8, // Volume & envelope
    pub nr43: u8, // Frequency & randomness
    pub nr44: u8, // Control

    // Internal state
    frame_sequencer: u8,
    cycles: u32,
}

impl Apu {
    pub fn new() -> Self {
        Apu {
            audio_buffer: Arc::new(Mutex::new(Vec::new())),
            sample_counter: 0.0,

            ch1_freq_timer: 0,
            ch1_duty_pos: 0,
            ch1_volume: 0,
            ch1_volume_initial: 0,
            ch1_envelope_timer: 0,
            ch1_enabled: false,
            ch1_length_counter: 0,

            ch2_freq_timer: 0,
            ch2_duty_pos: 0,
            ch2_volume: 0,
            ch2_volume_initial: 0,
            ch2_envelope_timer: 0,
            ch2_enabled: false,
            ch2_length_counter: 0,

            ch3_freq_timer: 0,
            ch3_wave_pos: 0,
            ch3_enabled: false,
            ch3_length_counter: 0,

            ch4_lfsr: 0x7FFF,
            ch4_freq_timer: 0,
            ch4_volume: 0,
            ch4_volume_initial: 0,
            ch4_envelope_timer: 0,
            ch4_enabled: false,
            ch4_length_counter: 0,

            capacitor: 0.0,
            last_output: 0.0,

            nr50: 0,
            nr51: 0,
            nr52: 0xF1, // All channels enabled by default

            nr10: 0,
            nr11: 0,
            nr12: 0,
            nr13: 0,
            nr14: 0,

            nr21: 0,
            nr22: 0,
            nr23: 0,
            nr24: 0,

            nr30: 0,
            nr31: 0,
            nr32: 0,
            nr33: 0,
            nr34: 0,
            wave_ram: [0; 16],

            nr41: 0,
            nr42: 0,
            nr43: 0,
            nr44: 0,

            frame_sequencer: 0,
            cycles: 0,
        }
    }

    pub fn get_audio_buffer(&self) -> Arc<Mutex<Vec<f32>>> {
        Arc::clone(&self.audio_buffer)
    }

    pub fn step(&mut self, cycles: u32) {
        if (self.nr52 & 0x80) == 0 {
            return; // APU is off
        }

        // Update channel timers first
        self.update_channels(cycles);

        self.cycles += cycles;

        // Generate audio samples - GB CPU is ~4.19MHz, we need 48kHz samples
        self.sample_counter += cycles as f32;
        let cycles_per_sample = 4194304.0 / SAMPLE_RATE as f32; // ~87 cycles per sample

        while self.sample_counter >= cycles_per_sample {
            self.sample_counter -= cycles_per_sample;
            self.generate_sample();
        }

        // Frame sequencer runs at 512 Hz (every 8192 cycles)
        while self.cycles >= 8192 {
            self.cycles -= 8192;
            self.tick_frame_sequencer();
        }
    }

    fn generate_sample(&mut self) {
        let mut sample_left = 0.0;
        let mut sample_right = 0.0;

        // Channel 1 - Square with sweep
        if self.ch1_enabled && (self.nr52 & 0x01) != 0 && self.ch1_volume > 0 {
            let duty = (self.nr11 >> 6) & 0x03;
            let duty_pattern = match duty {
                0 => [0, 0, 0, 0, 0, 0, 0, 1], // 12.5%
                1 => [1, 0, 0, 0, 0, 0, 0, 1], // 25%
                2 => [1, 0, 0, 0, 0, 1, 1, 1], // 50%
                3 => [0, 1, 1, 1, 1, 1, 1, 0], // 75%
                _ => [0; 8],
            };
            // Convert to -1.0 to 1.0 range to remove DC offset
            let output = if duty_pattern[self.ch1_duty_pos as usize] == 1 {
                self.ch1_volume as f32 / 15.0
            } else {
                -(self.ch1_volume as f32 / 15.0)
            };

            if (self.nr51 & 0x01) != 0 { sample_right += output; }
            if (self.nr51 & 0x10) != 0 { sample_left += output; }
        }

        // Channel 2 - Square
        if self.ch2_enabled && (self.nr52 & 0x02) != 0 && self.ch2_volume > 0 {
            let duty = (self.nr21 >> 6) & 0x03;
            let duty_pattern = match duty {
                0 => [0, 0, 0, 0, 0, 0, 0, 1],
                1 => [1, 0, 0, 0, 0, 0, 0, 1],
                2 => [1, 0, 0, 0, 0, 1, 1, 1],
                3 => [0, 1, 1, 1, 1, 1, 1, 0],
                _ => [0; 8],
            };
            let output = if duty_pattern[self.ch2_duty_pos as usize] == 1 {
                self.ch2_volume as f32 / 15.0
            } else {
                -(self.ch2_volume as f32 / 15.0)
            };

            if (self.nr51 & 0x02) != 0 { sample_right += output; }
            if (self.nr51 & 0x20) != 0 { sample_left += output; }
        }

        // Channel 3 - Wave
        if self.ch3_enabled && (self.nr52 & 0x04) != 0 && (self.nr30 & 0x80) != 0 {
            let sample_byte = self.wave_ram[(self.ch3_wave_pos / 2) as usize];
            let nibble = if (self.ch3_wave_pos & 1) == 0 {
                (sample_byte >> 4) & 0x0F
            } else {
                sample_byte & 0x0F
            };

            let volume_shift = (self.nr32 >> 5) & 0x03;
            let output = if volume_shift > 0 {
                ((nibble >> (volume_shift - 1)) as f32 / 7.5) - 1.0
            } else {
                0.0
            };

            if (self.nr51 & 0x04) != 0 { sample_right += output; }
            if (self.nr51 & 0x40) != 0 { sample_left += output; }
        }

        // Channel 4 - Noise
        if self.ch4_enabled && (self.nr52 & 0x08) != 0 && self.ch4_volume > 0 {
            let output = if (self.ch4_lfsr & 1) == 0 {
                self.ch4_volume as f32 / 15.0
            } else {
                -(self.ch4_volume as f32 / 15.0)
            };

            if (self.nr51 & 0x08) != 0 { sample_right += output; }
            if (self.nr51 & 0x80) != 0 { sample_left += output; }
        }

        // Apply master volume
        let left_vol = ((self.nr50 >> 4) & 0x07) as f32 / 7.0;
        let right_vol = (self.nr50 & 0x07) as f32 / 7.0;

        sample_left *= left_vol * 0.15;
        sample_right *= right_vol * 0.15;

        // Mix to mono
        let mut sample = (sample_left + sample_right) * 0.5;

        // High-pass filter to remove DC offset (capacitor charge/discharge)
        let filtered = sample - self.capacitor;
        self.capacitor = sample - filtered * 0.996;
        sample = filtered;

        // Low-pass filter for smoothing (reduces aliasing and harshness)
        // Simple one-pole filter
        let alpha = 0.85; // Higher = more smoothing
        sample = self.last_output * alpha + sample * (1.0 - alpha);
        self.last_output = sample;

        if let Ok(mut buffer) = self.audio_buffer.lock() {
            if buffer.len() < BUFFER_SIZE * 2 {
                buffer.push(sample);
            }
        }
    }

    fn update_channels(&mut self, cycles: u32) {
        // Channel 1 frequency
        if self.ch1_enabled {
            self.ch1_freq_timer -= cycles as i32;
            while self.ch1_freq_timer <= 0 {
                let freq = ((self.nr14 as u16 & 0x07) << 8) | self.nr13 as u16;
                let period = ((2048 - freq) * 4) as i32;
                self.ch1_freq_timer += period;
                self.ch1_duty_pos = (self.ch1_duty_pos + 1) & 7;
            }
        }

        // Channel 2 frequency
        if self.ch2_enabled {
            self.ch2_freq_timer -= cycles as i32;
            while self.ch2_freq_timer <= 0 {
                let freq = ((self.nr24 as u16 & 0x07) << 8) | self.nr23 as u16;
                let period = ((2048 - freq) * 4) as i32;
                self.ch2_freq_timer += period;
                self.ch2_duty_pos = (self.ch2_duty_pos + 1) & 7;
            }
        }

        // Channel 3 frequency
        if self.ch3_enabled {
            self.ch3_freq_timer -= cycles as i32;
            while self.ch3_freq_timer <= 0 {
                let freq = ((self.nr34 as u16 & 0x07) << 8) | self.nr33 as u16;
                let period = ((2048 - freq) * 2) as i32;
                self.ch3_freq_timer += period;
                self.ch3_wave_pos = (self.ch3_wave_pos + 1) & 31;
            }
        }

        // Channel 4 - Noise
        if self.ch4_enabled {
            self.ch4_freq_timer -= cycles as i32;
            while self.ch4_freq_timer <= 0 {
                let divisor = match self.nr43 & 0x07 {
                    0 => 8,
                    n => (n as i32) * 16,
                };
                let shift = (self.nr43 >> 4) & 0x0F;
                let period = if shift < 14 {
                    divisor << shift
                } else {
                    8192
                };

                self.ch4_freq_timer += period;

                let bit = (self.ch4_lfsr ^ (self.ch4_lfsr >> 1)) & 1;
                self.ch4_lfsr >>= 1;
                self.ch4_lfsr |= bit << 14;

                if (self.nr43 & 0x08) != 0 {
                    self.ch4_lfsr &= !(1 << 6);
                    self.ch4_lfsr |= bit << 6;
                }
            }
        }
    }

    fn tick_frame_sequencer(&mut self) {
        self.frame_sequencer = (self.frame_sequencer + 1) % 8;

        match self.frame_sequencer {
            0 | 4 => {
                // Length counter tick
                if self.ch1_length_counter > 0 && (self.nr14 & 0x40) != 0 {
                    self.ch1_length_counter -= 1;
                    if self.ch1_length_counter == 0 {
                        self.ch1_enabled = false;
                    }
                }
                if self.ch2_length_counter > 0 && (self.nr24 & 0x40) != 0 {
                    self.ch2_length_counter -= 1;
                    if self.ch2_length_counter == 0 {
                        self.ch2_enabled = false;
                    }
                }
                if self.ch3_length_counter > 0 && (self.nr34 & 0x40) != 0 {
                    self.ch3_length_counter -= 1;
                    if self.ch3_length_counter == 0 {
                        self.ch3_enabled = false;
                    }
                }
                if self.ch4_length_counter > 0 && (self.nr44 & 0x40) != 0 {
                    self.ch4_length_counter -= 1;
                    if self.ch4_length_counter == 0 {
                        self.ch4_enabled = false;
                    }
                }
            }
            2 | 6 => {
                // Sweep tick (channel 1 only)
                // Simplified - full sweep would require more state
            }
            7 => {
                // Envelope tick
                self.tick_envelope_ch1();
                self.tick_envelope_ch2();
                self.tick_envelope_ch4();
            }
            _ => {}
        }
    }

    fn tick_envelope_ch1(&mut self) {
        let period = self.nr12 & 0x07;
        if period == 0 {
            return;
        }

        if self.ch1_envelope_timer > 0 {
            self.ch1_envelope_timer -= 1;
        }

        if self.ch1_envelope_timer == 0 {
            self.ch1_envelope_timer = period;
            let add_mode = (self.nr12 & 0x08) != 0;

            if add_mode && self.ch1_volume < 15 {
                self.ch1_volume += 1;
            } else if !add_mode && self.ch1_volume > 0 {
                self.ch1_volume -= 1;
            }
        }
    }

    fn tick_envelope_ch2(&mut self) {
        let period = self.nr22 & 0x07;
        if period == 0 {
            return;
        }

        if self.ch2_envelope_timer > 0 {
            self.ch2_envelope_timer -= 1;
        }

        if self.ch2_envelope_timer == 0 {
            self.ch2_envelope_timer = period;
            let add_mode = (self.nr22 & 0x08) != 0;

            if add_mode && self.ch2_volume < 15 {
                self.ch2_volume += 1;
            } else if !add_mode && self.ch2_volume > 0 {
                self.ch2_volume -= 1;
            }
        }
    }

    fn tick_envelope_ch4(&mut self) {
        let period = self.nr42 & 0x07;
        if period == 0 {
            return;
        }

        if self.ch4_envelope_timer > 0 {
            self.ch4_envelope_timer -= 1;
        }

        if self.ch4_envelope_timer == 0 {
            self.ch4_envelope_timer = period;
            let add_mode = (self.nr42 & 0x08) != 0;

            if add_mode && self.ch4_volume < 15 {
                self.ch4_volume += 1;
            } else if !add_mode && self.ch4_volume > 0 {
                self.ch4_volume -= 1;
            }
        }
    }

    pub fn read_register(&self, address: u16) -> u8 {
        match address {
            0xFF10 => self.nr10 | 0x80,
            0xFF11 => self.nr11 | 0x3F,
            0xFF12 => self.nr12,
            0xFF13 => 0xFF, // Write-only
            0xFF14 => self.nr14 | 0xBF,

            0xFF16 => self.nr21 | 0x3F,
            0xFF17 => self.nr22,
            0xFF18 => 0xFF, // Write-only
            0xFF19 => self.nr24 | 0xBF,

            0xFF1A => self.nr30 | 0x7F,
            0xFF1B => 0xFF, // Write-only
            0xFF1C => self.nr32 | 0x9F,
            0xFF1D => 0xFF, // Write-only
            0xFF1E => self.nr34 | 0xBF,

            0xFF20 => 0xFF, // Write-only
            0xFF21 => self.nr42,
            0xFF22 => self.nr43,
            0xFF23 => self.nr44 | 0xBF,

            0xFF24 => self.nr50,
            0xFF25 => self.nr51,
            0xFF26 => self.nr52,

            0xFF30..=0xFF3F => self.wave_ram[(address - 0xFF30) as usize],

            _ => 0xFF,
        }
    }

    pub fn write_register(&mut self, address: u16, value: u8) {
        // If APU is off, ignore writes (except to NR52)
        if address != 0xFF26 && (self.nr52 & 0x80) == 0 {
            return;
        }

        match address {
            0xFF10 => self.nr10 = value,
            0xFF11 => {
                self.nr11 = value;
                self.ch1_length_counter = 64 - (value & 0x3F) as u16;
            }
            0xFF12 => self.nr12 = value,
            0xFF13 => self.nr13 = value,
            0xFF14 => {
                self.nr14 = value;
                if (value & 0x80) != 0 {
                    // Trigger channel 1
                    self.ch1_enabled = true;
                    self.ch1_volume = (self.nr12 >> 4) & 0x0F;
                    self.ch1_volume_initial = self.ch1_volume;
                    self.ch1_envelope_timer = self.nr12 & 0x07;
                    let freq = ((self.nr14 as u16 & 0x07) << 8) | self.nr13 as u16;
                    self.ch1_freq_timer = ((2048 - freq) * 4) as i32;
                    self.ch1_duty_pos = 0;

                    // Length counter
                    if self.ch1_length_counter == 0 {
                        self.ch1_length_counter = 64;
                    }
                }
            }

            0xFF16 => {
                self.nr21 = value;
                self.ch2_length_counter = 64 - (value & 0x3F) as u16;
            }
            0xFF17 => self.nr22 = value,
            0xFF18 => self.nr23 = value,
            0xFF19 => {
                self.nr24 = value;
                if (value & 0x80) != 0 {
                    // Trigger channel 2
                    self.ch2_enabled = true;
                    self.ch2_volume = (self.nr22 >> 4) & 0x0F;
                    self.ch2_volume_initial = self.ch2_volume;
                    self.ch2_envelope_timer = self.nr22 & 0x07;
                    let freq = ((self.nr24 as u16 & 0x07) << 8) | self.nr23 as u16;
                    self.ch2_freq_timer = ((2048 - freq) * 4) as i32;
                    self.ch2_duty_pos = 0;

                    // Length counter
                    if self.ch2_length_counter == 0 {
                        self.ch2_length_counter = 64;
                    }
                }
            }

            0xFF1A => self.nr30 = value,
            0xFF1B => {
                self.nr31 = value;
                self.ch3_length_counter = 256 - value as u16;
            }
            0xFF1C => self.nr32 = value,
            0xFF1D => self.nr33 = value,
            0xFF1E => {
                self.nr34 = value;
                if (value & 0x80) != 0 {
                    // Trigger channel 3
                    self.ch3_enabled = true;
                    let freq = ((self.nr34 as u16 & 0x07) << 8) | self.nr33 as u16;
                    self.ch3_freq_timer = ((2048 - freq) * 2) as i32;
                    self.ch3_wave_pos = 0;

                    // Length counter
                    if self.ch3_length_counter == 0 {
                        self.ch3_length_counter = 256;
                    }
                }
            }

            0xFF20 => {
                self.nr41 = value;
                self.ch4_length_counter = 64 - (value & 0x3F) as u16;
            }
            0xFF21 => self.nr42 = value,
            0xFF22 => self.nr43 = value,
            0xFF23 => {
                self.nr44 = value;
                if (value & 0x80) != 0 {
                    // Trigger channel 4
                    self.ch4_enabled = true;
                    self.ch4_volume = (self.nr42 >> 4) & 0x0F;
                    self.ch4_volume_initial = self.ch4_volume;
                    self.ch4_envelope_timer = self.nr42 & 0x07;
                    self.ch4_lfsr = 0x7FFF;

                    // Length counter
                    if self.ch4_length_counter == 0 {
                        self.ch4_length_counter = 64;
                    }
                }
            }

            0xFF24 => self.nr50 = value,
            0xFF25 => self.nr51 = value,
            0xFF26 => {
                let old_power = (self.nr52 & 0x80) != 0;
                let new_power = (value & 0x80) != 0;

                if old_power && !new_power {
                    // Power off - reset all registers
                    self.nr10 = 0;
                    self.nr11 = 0;
                    self.nr12 = 0;
                    self.nr13 = 0;
                    self.nr14 = 0;
                    self.nr21 = 0;
                    self.nr22 = 0;
                    self.nr23 = 0;
                    self.nr24 = 0;
                    self.nr30 = 0;
                    self.nr31 = 0;
                    self.nr32 = 0;
                    self.nr33 = 0;
                    self.nr34 = 0;
                    self.nr41 = 0;
                    self.nr42 = 0;
                    self.nr43 = 0;
                    self.nr44 = 0;
                    self.nr50 = 0;
                    self.nr51 = 0;
                }

                self.nr52 = (value & 0x80) | (self.nr52 & 0x0F);
            }

            0xFF30..=0xFF3F => {
                self.wave_ram[(address - 0xFF30) as usize] = value;
            }

            _ => {}
        }
    }
}
