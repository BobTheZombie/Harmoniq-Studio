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
- `crates/harmoniq-plugin-host`: Host layers for loading third-party binaries
  such as LinuxVST, VST2/3, AudioUnit, and RTAS modules.

## Getting started

```bash
cargo run -p harmoniq-app -- --sample-rate 48000 --block-size 512
```

The CLI renders a few blocks of audio using the built-in sine synthesizer, noise
source, and gain stage, demonstrating the graph scheduler and mixdown pipeline.

### Native UI mode

Launch the egui-powered desktop prototype with the default settings:

```bash
cargo run -p harmoniq-app
```

Add CLI flags (such as `--sample-rate` or `--midi-input`) to adjust the realtime
engine configuration. To temporarily disable the windowed interface, append
`--headless` to switch back to the minimal CLI renderer.

### Realtime audio and MIDI

Harmoniq Studio now streams audio using native backends such as ALSA, JACK, and
PulseAudio on Linux, WASAPI/ASIO on Windows, and CoreAudio on macOS. Select the
backend and MIDI controller directly from the CLI:

```bash
cargo run -p harmoniq-app -- --audio-backend jack --midi-input "Launchpad"
```

Available hosts and MIDI devices can be enumerated using `--list-audio-backends`
and `--list-midi-inputs`. On Linux the `pulseaudio` selector reuses the ALSA
compatibility layer so it also works with PipeWire setups. Headless mode
defaults to realtime streaming but can be disabled with `--disable-audio` for
offline graph validation.

### Desktop integration (Linux)

Harmoniq Studio ships with a freedesktop-compatible launcher entry and icon in
`resources/desktop` and `resources/icons`. The launcher starts the application
in the native UI mode.

1. Build the release binary:

   ```bash
   cargo build --release -p harmoniq-app
   ```

2. Install the binary somewhere in your `PATH` (for example `/usr/local/bin`):

   ```bash
   sudo install -Dm755 target/release/harmoniq-app /usr/local/bin/harmoniq-studio
   ```

3. Copy the desktop entry and icon to your local data directory:

   ```bash
   install -Dm644 resources/desktop/harmoniq-studio.desktop \
     ~/.local/share/applications/harmoniq-studio.desktop
   install -Dm644 resources/icons/harmoniq-studio.svg \
     ~/.local/share/icons/hicolor/scalable/apps/harmoniq-studio.svg
   ```

4. Update the desktop database (optional but recommended):

   ```bash
   update-desktop-database ~/.local/share/applications
   ```

You can verify the launcher with `desktop-file-validate` or by searching for
"Harmoniq Studio" in your desktop environment's application overview.

## Roadmap highlights

- [ ] Real-time safe command queues for UI ↔ engine communication
- [ ] Cross-platform native UI (egui/iced) with playlist, mixer, and piano roll
- [ ] Plugin SDK for Harmoniq-native instruments and effects
- [x] Host layers for LinuxVST, VST2/3, AU, and RTAS binaries
- [ ] Offline bouncing, automation lanes, and clip launching

Contributions and ideas are welcome as we grow Harmoniq Studio into a
production-ready DAW.
