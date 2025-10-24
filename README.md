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

### Menu bar and shortcuts

The desktop build now ships with a full menu bar that mirrors familiar DAW
workflows: File, Edit, View, Insert, Track, MIDI, Transport, Options, and Help.
Every menu action posts a `Command` onto a lock-free UI → app command bus so the
non-realtime thread can safely talk to the audio engine, update UI state, or
open dialogs. The File menu also maintains an MRU list in
`~/.config/HarmoniqStudio/recent.json`, while keyboard accelerators are loaded
from `~/.config/HarmoniqStudio/shortcuts.json` and can be customized per user.

Key default shortcuts:

| Action | macOS | Windows/Linux |
| --- | --- | --- |
| New Project | ⌘N | Ctrl+N |
| Open Project | ⌘O | Ctrl+O |
| Save Project | ⌘S | Ctrl+S |
| Save As | ⌘⇧S | Ctrl+Shift+S |
| Export/Render | ⌘E | Ctrl+E |
| Undo / Redo | ⌘Z / ⌘⇧Z | Ctrl+Z / Ctrl+Shift+Z |
| Cut / Copy / Paste | ⌘X / ⌘C / ⌘V | Ctrl+X / Ctrl+C / Ctrl+V |
| Delete | Del | Del |
| Select All | ⌘A | Ctrl+A |
| Toggle Mixer / Piano Roll / Browser | M / P / B | M / P / B |
| Zoom In / Out | ⌘+ / ⌘− | Ctrl+Plus / Ctrl+Minus |
| Toggle Fullscreen | F11 | F11 |
| Arm Track / Solo / Mute | R / S / M | R / S / M |
| Quantize | Q | Q |
| Play / Stop / Record | Space / 0 / ⌘R | Space / 0 / Ctrl+R |
| Loop / Go to Start / Tap Tempo | L / Home / T | L / Home / T |

Options → Audio Device… opens the realtime device dialog, and Transport menu
items mirror the transport buttons along the top bar. Shortcuts are consumed so
text inputs remain unaffected when invoking global commands.

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

### QWERTY keyboard input

When no external MIDI devices are detected Harmoniq Studio automatically
enables a built-in QWERTY keyboard controller. The computer keyboard is mapped
to a piano layout so you can sketch ideas without hardware attached:

| Control | Action |
| --- | --- |
| `Q W E R T Y U` | White keys for the active octave |
| `2 3 5 6 7` | Black keys (C♯, D♯, F♯, G♯, A♯) |
| `Space` | Sustain pedal (hold / release) |
| `Z` / `X` | Octave down / up |
| `[` / `/` | Decrement / increment MIDI channel |
| `C` / `V` + modifier | Cycle through velocity presets |
| `Shift` | Accent notes (+20 velocity) |
| `Esc` | MIDI panic (All Notes Off) |

Toggle the device from the CLI using `--qwerty` or disable it with
`--no-qwerty`. You can also tailor the response with `--qwerty-octave`,
`--qwerty-velocity`, `--qwerty-curve`, and `--qwerty-channel`. Configuration is
saved to `~/.config/HarmoniqStudio/qwerty.json` so your layout, velocity curve,
and channel persist between sessions.

### OpenASIO backend

For ultra low latency setups on Linux, Harmoniq Studio can host OpenASIO
drivers via an optional feature flag. Build the workspace with OpenASIO support
enabled:

```bash
cargo build --workspace --features openasio
```

To run against the bundled CPAL reference driver:

```bash
cargo run -p harmoniq-app --features openasio -- \
  --audio-backend openasio \
  --openasio-driver target/debug/libopenasio_driver_cpal.so \
  --openasio-sr 48000 \
  --openasio-buffer 128
```

To use the AMD Family 17h ALSA OpenASIO driver, point the CLI at the shared
library and device name:

```bash
cargo run -p harmoniq-app --features openasio -- \
  --audio-backend openasio \
  --openasio-driver /path/to/libopenasio_driver_alsa17h.so \
  --openasio-device "hw:0,0" \
  --openasio-buffer 128
```

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
