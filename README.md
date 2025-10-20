# Harmoniq Studio

Harmoniq Studio is an early-stage, native, multi-platform digital audio workstation
written in Rust. The focus of this initial prototype is the **Harmoniq audio engine**—
a modular, high-definition processing core designed for low-latency recording,
professional mixing, and live performance.

## Project layout

- `crates/harmoniq-engine`: Core DSP abstractions, plugin graph execution, buffer
  management, and transport state.
- `crates/harmoniq-plugins`: Reference collection of built-in native processors
  (synthesizers and utilities) built on top of the engine.
- `crates/harmoniq-app`: Command-line harness that wires the engine and built-in
  plugins together. This is a staging ground for future GUI, mixer, sequencer,
  and piano roll components.

## Getting started

```bash
cargo run -p harmoniq-app -- --sample-rate 48000 --block-size 512
```

The CLI renders a few blocks of audio using the built-in sine synthesizer, noise
source, and gain stage, demonstrating the graph scheduler and mixdown pipeline.

## Roadmap highlights

- [ ] Real-time safe command queues for UI ↔ engine communication
- [ ] Cross-platform native UI (egui/iced) with playlist, mixer, and piano roll
- [ ] Plugin SDK for Harmoniq-native instruments and effects
- [ ] Host layers for LinuxVST, VST2/3, AU, and RTAS binaries
- [ ] Offline bouncing, automation lanes, and clip launching

Contributions and ideas are welcome as we grow Harmoniq Studio into a
production-ready DAW.
