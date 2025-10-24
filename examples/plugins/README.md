# Harmoniq Test Plugins

This directory contains lightweight stand-ins for CLAP and VST3 plug-ins
that are used for development and CI smoke tests. The plug-ins expose a
minimal entry point that returns a deterministic value so the host can
verify that discovery and loading worked as expected.

## Building

```
cargo build -p clap-testgain
cargo build -p ovst3-testgain
```
