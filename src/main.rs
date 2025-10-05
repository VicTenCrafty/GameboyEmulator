mod cpu;
mod mmu;
mod cartridge;
mod ppu;
mod joypad;
mod timer;
mod apu;

use cpu::Cpu;
use mmu::Mmu;
use cartridge::Cartridge;
use minifb::{Key, Window, WindowOptions};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};

const SCALE: usize = 3;

fn main() {
    let rom_path = "SuperMarioLand.gb";

    println!("Loading ROM: {}", rom_path);
    let cartridge = match Cartridge::load(rom_path) {
        Ok(cart) => cart,
        Err(e) => {
            eprintln!("Failed to load ROM: {}", e);
            return;
        }
    };

    let mut mmu = Mmu::new(cartridge);
    let mut cpu = Cpu::new();

    // Setup audio output
    let audio_buffer = mmu.apu.get_audio_buffer();
    let _stream = setup_audio(Arc::clone(&audio_buffer));

    // Print initial state
    println!("Initial CPU state:");
    println!("  PC: 0x{:04X}", cpu.registers.pc);
    println!("  SP: 0x{:04X}", cpu.registers.sp);
    println!("  AF: 0x{:04X}", cpu.registers.af());
    println!("Initial PPU state:");
    println!("  LCDC: 0x{:02X}", mmu.ppu.lcdc);
    println!("  BGP: 0x{:02X}", mmu.ppu.bgp);
    println!("  OBP0: 0x{:02X}", mmu.ppu.obp0);
    println!("  OBP1: 0x{:02X}", mmu.ppu.obp1);
    println!("");

    let mut window = Window::new(
        "Gameboy Emulator",
        ppu::SCREEN_WIDTH * SCALE,
        ppu::SCREEN_HEIGHT * SCALE,
        WindowOptions::default(),
    )
    .unwrap_or_else(|e| {
        panic!("Failed to create window: {}", e);
    });

    window.set_target_fps(60);

    // Performance tracking
    let mut frame_count = 0;
    let start_time = std::time::Instant::now();

    println!("\nControls:");
    println!("  Arrow Keys - D-Pad");
    println!("  Z - A Button");
    println!("  X - B Button");
    println!("  Enter - Start");
    println!("  Shift - Select");
    println!("  ESC - Exit");
    println!("\nSave files (.sav) are stored in the same directory as your ROM");
    println!("Auto-saves every 5 seconds");
    println!("\nStarting emulation...\n");

    let mut last_save_frame = 0;

    while window.is_open() && !window.is_key_down(Key::Escape) {
        // Handle input
        mmu.joypad.set_up(window.is_key_down(Key::Up));
        mmu.joypad.set_down(window.is_key_down(Key::Down));
        mmu.joypad.set_left(window.is_key_down(Key::Left));
        mmu.joypad.set_right(window.is_key_down(Key::Right));
        mmu.joypad.set_a(window.is_key_down(Key::Z));
        mmu.joypad.set_b(window.is_key_down(Key::X));
        mmu.joypad.set_start(window.is_key_down(Key::Enter));
        mmu.joypad.set_select(window.is_key_down(Key::LeftShift) || window.is_key_down(Key::RightShift));

        // Run until frame is complete
        mmu.ppu.frame_ready = false;
        let mut cycles_this_frame = 0;

        while !mmu.ppu.frame_ready && cycles_this_frame < 80000 {
            let cycles = cpu.step(&mut mmu);
            mmu.step(cycles); // Step timer and DMA
            mmu.ppu.step(cycles);

            // Check for STAT interrupt
            if mmu.ppu.stat_interrupt {
                mmu.if_reg |= 0x02; // STAT interrupt
            }

            // Check for joypad interrupt
            if mmu.joypad.interrupt_requested {
                mmu.if_reg |= 0x10; // Joypad interrupt
                mmu.joypad.interrupt_requested = false;
            }

            cycles_this_frame += cycles;
        }

        // VBlank interrupt
        if mmu.ppu.frame_ready {
            mmu.if_reg |= 0x01;
        }

        // Update screen
        window
            .update_with_buffer(&mmu.ppu.framebuffer, ppu::SCREEN_WIDTH, ppu::SCREEN_HEIGHT)
            .unwrap();

        frame_count += 1;
        if frame_count % 60 == 0 {
            let elapsed = start_time.elapsed().as_secs_f64();
            let fps = frame_count as f64 / elapsed;
            println!("FPS: {:.2} | Frames: {} | Cycles/Frame: {}", fps, frame_count, cycles_this_frame);
        }

        // Auto-save every 5 seconds (300 frames at 60fps)
        if frame_count - last_save_frame >= 300 {
            mmu.cartridge.save();
            last_save_frame = frame_count;
        }
    }

    // Final save on exit
    mmu.cartridge.save();

    println!("\nEmulator closed.");
    println!("Total frames rendered: {}", frame_count);
}

fn setup_audio(audio_buffer: Arc<Mutex<Vec<f32>>>) -> cpal::Stream {
    let host = cpal::default_host();
    let device = host.default_output_device().expect("No audio output device");
    let config = device.default_output_config().expect("No default audio config");

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => build_stream::<f32>(&device, &config.into(), audio_buffer),
        cpal::SampleFormat::I16 => build_stream::<i16>(&device, &config.into(), audio_buffer),
        cpal::SampleFormat::U16 => build_stream::<u16>(&device, &config.into(), audio_buffer),
        _ => panic!("Unsupported sample format"),
    };

    stream.play().expect("Failed to play audio stream");
    println!("Audio output initialized");
    stream
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    audio_buffer: Arc<Mutex<Vec<f32>>>,
) -> cpal::Stream
where
    T: cpal::Sample + cpal::SizedSample + cpal::FromSample<f32>,
{
    let channels = config.channels as usize;

    device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            let mut buffer = audio_buffer.lock().unwrap();
            for frame in data.chunks_mut(channels) {
                let sample = if !buffer.is_empty() {
                    buffer.remove(0)
                } else {
                    0.0
                };

                for channel in frame.iter_mut() {
                    *channel = T::from_sample(sample);
                }
            }
        },
        |err| eprintln!("Audio stream error: {}", err),
        None,
    )
    .expect("Failed to build audio stream")
}
