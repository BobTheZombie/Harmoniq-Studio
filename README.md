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
- `crates/clap-sys`, `crates/clap-host`, `crates/clap-plugin-authoring`: CLAP SDK
  bindings, safe host wrappers, and helper utilities for authoring new CLAP plug-ins.
- `crates/clap-scanner`, `crates/clap-validate`: command-line tooling for
  discovering, indexing, and validating CLAP plug-ins.
- `examples/clap-testgain`, `examples/clap-testsynth`: reference CLAP plug-ins
  built with the authoring helpers.

## Getting started

### Linux prerequisites

The native UI build links against X11 libraries. Install the development
packages before running `cargo` so `pkg-config` can discover `x11-xcb` and
`GL`:

```bash
sudo apt update
sudo apt install pkg-config libx11-xcb-dev libgl1-mesa-dev
```

Set `PKG_CONFIG_PATH` manually only if the packages are installed in a
non-standard location.

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

### Plugin discovery and library

Harmoniq Studio now keeps a persistent plugin database under
`~/.config/HarmoniqStudio/plugins.json`. When the application starts it scans the
following locations:

| Format | System | User |
| --- | --- | --- |
| CLAP | `/usr/share/harmoniq-studio/plugins/clap` | `~/.clap` |
| VST3 | `/usr/share/harmoniq-studio/plugins/vst3` | `~/.vst3` |
| OpenVST3 shim | `/usr/share/harmoniq-studio/plugins/ovst3` | `~/.vst3` |
| Harmoniq native | `/usr/share/harmoniq-studio/plugins/harmoniq` | `~/.harmoniq/plugins` |

Each candidate bundle is probed for metadata such as vendor, category, channel
layout, editor support, and whether it exposes instrument voices. Results are
deduplicated and stored so the plugin browser can open instantly on subsequent
runs.

To force a rescan open **Plugins → Add Plugins…** or run the CLI helper:

```bash
cargo run -p harmoniq-app -- --open-plugin-scanner
```

This re-indexes the configured locations, merges the results into the JSON
database, and refreshes the in-app browser.

### Using the Plugin Library

Open **Plugins → Plugin Library…** to browse all installed instruments and
effects. The dialog includes search, format toggles (CLAP, VST3, OpenVST3, and
Harmoniq), and quick category chips (Instrument, Effect, Dynamics, EQ, Reverb,
Delay, Mod, Distortion, Utility). Selecting an entry reveals its metadata and
offers three actions:

- **Add to Channel as Instrument** – attaches the instrument to a new Channel
  Rack lane.
- **Add to Channel as Effect** – inserts the processor on the selected channel
  effect chain.
- **Add to Mixer Insert** – makes the processor available on the mixer.

When an instrument is attached to a channel its piano roll opens automatically,
allowing you to sketch MIDI clips and audition notes immediately. Use the Q/W
shortcuts to adjust quantize strength, Delete to remove notes, and the standard
copy/paste shortcuts for arranging phrases.

### Hosting modes

For third-party plug-ins you can choose between in-process hosting (lowest
latency, but a misbehaving plug-in can crash the DAW) and the sandboxed bridge
process (slightly higher latency, isolates crashes). Set your preferred default
under **Options → Preferences → Plugins**; per-plug-in overrides are also saved
in the project file so experimental instruments can run safely in the bridge
without affecting the rest of the session.

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

### Mixer (Reaper-style)

The mixer now embraces a dense, Reaper-inspired presentation. Strips default to
76 px “narrow” lanes and expand to 120 px “wide” lanes, with zoomable geometry
(80 %–150 %) so you can dial in the perfect density on high-DPI displays. The
master bus remains pinned on the right at 1.8× width and always renders its
dual true-peak meters, latency, and CPU/PDC readouts.

A capture gallery for the mixer will be published under `docs/mixer/` as the UI
stabilizes.

Quick interactions:

- `N` / `W` toggle density. Hold `⌘`/`Ctrl` and tap `+` or `−` (or use View →
  Zoom) to scale strips between 80 % and 150 %.
- Mouse wheel scrolls the bank; `Ctrl`/`⌘` + `←`/`→` jumps eight tracks; `G`
  toggles per-track tinting. Shift-click extends selection.
- Pan and width knobs support fine adjust with `Ctrl`/`⌘`, `Alt` to reset, and
  double-click for numeric entry. Faders expose the same gesture set plus a
  context menu for reset and trim actions.
- Inserts surface eight (narrow) or twelve (wide) visible slots with bypass
  dots, drag reordering, and overflow popovers. Sends list four or six targets
  with compact level sliders and pre/post toggles.
- GPU meters approximate true-peak with peak-hold and clip latching—click the
  clip LED to clear. Virtualization ensures only visible strips render so
  sessions with hundreds of tracks stay responsive.

Toggle the panel with View → Mixer or the `M` shortcut.

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
