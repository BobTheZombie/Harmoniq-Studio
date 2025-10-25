# WestCoast Whine Synth

WestCoast Whine Synth is a CLAP and VST3 polyphonic synthesizer built with [`nih-plug`](https://github.com/robbert-vdh/nih-plug). It recreates the early 1990s West Coast "whine" lead sound with sine-dominant oscillators, expressive legato glide, and lightweight chorus and plate reverb processing.

## Building

```bash
cargo build -p westcoast-whine-synth
```

> **Linux build note:** the `nih_plug_egui` editor depends on system OpenGL headers.
> Install your distribution's OpenGL development package (e.g. `libgl1-mesa-dev`
> on Debian/Ubuntu) before running `cargo check` or `cargo build`.

## Bundling

```bash
cargo xtask bundle westcoast_whine --format clap,vst3
```

## Demo Renderer

Enable the optional renderer to export a short demo phrase to `/tmp/westcoast_whine_demo.wav`:

```bash
cargo run -p westcoast-whine-synth --bin render_demo --features demo-render
```
