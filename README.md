# Game Boy Emulator

A high-accuracy Game Boy (DMG) emulator written in Rust.

## Features

### CPU
- ✅ Full Sharp LR35902 CPU instruction set (all 256 opcodes + CB prefix)
- ✅ Accurate cycle timing
- ✅ Complete interrupt handling (VBlank, STAT, Timer, Serial, Joypad)
- ✅ Proper IME (Interrupt Master Enable) scheduling

### PPU (Graphics)
- ✅ Background rendering with scrolling
- ✅ Window layer support with proper positioning
- ✅ Sprite rendering (OBJ) with priority system
- ✅ 10 sprites per scanline limit
- ✅ 8x8 and 8x16 sprite modes
- ✅ Sprite flipping (horizontal/vertical)
- ✅ Sprite-to-background priority
- ✅ Dot-based timing (456 dots per scanline)
- ✅ Accurate LCD mode transitions
- ✅ STAT interrupts (Mode 0/1/2, LYC=LY)
- ✅ LCD on/off handling
- ✅ Gameboy Color support semi-implemented (not all games work. when you encounter a game that doesnt work, please submit an issue request!)

### Memory
- ✅ Full memory map emulation
- ✅ DMA (Direct Memory Access) transfer
- ✅ MBC1 cartridge support (ROM/RAM banking)
- ✅ MBC2 cartridge support (built-in RAM)
- ✅ MBC3 cartridge support (RTC registers stubbed)
- ✅ ROM-only cartridge support

### Input
- ✅ Full joypad emulation
- ✅ Joypad interrupts on button press

### Timer
- ✅ DIV register (16384 Hz)
- ✅ TIMA/TMA/TAC registers
- ✅ Timer interrupts
- ✅ Configurable timer frequencies

### APU (Audio)
- ✅ Register emulation for all 4 channels
- ✅ Channel 1: Square wave with sweep
- ✅ Channel 2: Square wave
- ✅ Channel 3: Programmable wave
- ✅ Channel 4: Noise
- ✅ Master volume and panning
- ✅ Audio output implemented (but may sound a little bit weird)

## Controls

- **Arrow Keys** - D-Pad
- **Z** - A Button
- **X** - B Button
- **Enter** - Start
- **Shift** - Select
- **ESC** - Exit

## Building

```bash
cargo build --release
```

## Running

```bash
cargo run --release
```

## Tested Games

- ✅ **Super Mario Land** - Fully playable
- ✅ **Tetris** - Fully playable
- ✅ **Dr. Mario** - Fully playable
- ✅ **Pokemon Red/Blue** - Playable (MBC3 support)

## Accuracy

This emulator aims for high accuracy:

- Cycle-accurate CPU timing
- Dot-based PPU timing
- Proper interrupt handling
- Accurate sprite priority
- Window rendering
- Timer precision

## Completion Status

**~95% Complete**

### What's Working
- All CPU instructions
- Graphics (background, window, sprites)
- Input
- Timers
- Interrupts
- Cartridge types (MBC1/2/3)
- Save RAM

## Architecture

```
src/
├── main.rs       - Entry point, main loop
├── cpu.rs        - CPU emulation (LR35902)
├── ppu.rs        - Graphics (PPU)
├── mmu.rs        - Memory management
├── cartridge.rs  - ROM/RAM handling, MBC
├── timer.rs      - Timer subsystem
├── joypad.rs     - Input handling
└── apu.rs        - Audio (registers only)
```

## Performance

Runs at full speed (60 FPS) on modern hardware with optimized release builds.

## License

MIT

