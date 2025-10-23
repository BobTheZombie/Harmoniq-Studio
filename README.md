# Harmoniq Studio

Harmoniq Studio is a Linux-first, cross-platform digital audio workstation
prototype written in Rust. The workspace is organised as a collection of crates
that model the audio engine, routing graph, plugin hosting layer, project
format, MIDI subsystem, and egui-powered desktop experience.

## Workspace layout

- `crates/harmoniq-utils` – shared utilities (dB conversions, lock-free queues,
  profiling helpers).
- `crates/harmoniq-graph` – routing graph primitives with plugin delay
  compensation (PDC) support.
- `crates/harmoniq-audioio` – cross-platform audio I/O abstraction backed by
  CPAL with placeholders for JACK/PipeWire.
- `crates/harmoniq-midi` – MIDI input management with Linux/ALSA focus.
- `crates/harmoniq-project` – serde-powered project model and persistence.
- `crates/harmoniq-plugin-sdk` – Harmoniq plugin ABI definition and helpers.
- `crates/harmoniq-host` – plugin hosting façade with feature-gated backends.
- `crates/harmoniq-engine` – real-time engine, transport, automation and mixer
  state handling.
- `crates/harmoniq-ui` – egui widgets for transport, mixer, browser, and
  arrangement stubs.
- `crates/harmoniq-app` – desktop launcher bundling the engine and UI.
- `crates/harmoniq-tests` – integration test harness for graph and engine
  validation.
- `examples/hello-harmoniq-plugin` – minimal Harmoniq plugin exposing a gain
  descriptor.

## Building

The workspace targets Rust 1.75 or newer. Start by fetching dependencies and
running the full workspace check:

```bash
cargo check --all-targets
```

### Run the desktop prototype

The egui front-end talks to the audio engine through a lock-free command queue
and displays transport plus mixer state at 60 Hz. Launch it with:

```bash
cargo run -p harmoniq-app
```

Command-line features provide alternate audio backends:

- JACK: `cargo run -p harmoniq-app --features jack`
- PipeWire (stub): `cargo run -p harmoniq-app --features pipewire`

### Run the integration tests

Graph topology and engine alignment tests live inside the dedicated
`harmoniq-tests` crate:

```bash
cargo test -p harmoniq-tests
```

### Build the example Harmoniq plugin

The minimal gain plugin builds as a `cdylib` for experimentation with the
Harmoniq plugin SDK:

```bash
cargo build -p hello-harmoniq-plugin
```

The resulting library exposes the `harmoniq_plugin_entry` symbol returning a
descriptor compatible with the host facade.

## Realtime contract

- The audio callback avoids blocking locks, allocations, logging, or I/O.
- Engine commands flow through single-producer/single-consumer ring buffers.
- Snapshots for UI consumption are produced on a non-real-time thread.

These guarantees are stubbed out in the prototype and provide the scaffolding
for future DSP implementations.
