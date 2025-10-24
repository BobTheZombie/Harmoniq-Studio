# Packaging assets

This directory contains distribution tooling for Harmoniq Studio:

- `flatpak/` – Flatpak manifest, cargo vendor data, and notes about PipeWire and
  udev integration. The Flatpak wrapper ensures crash minidumps are collected in
  `$XDG_STATE_HOME/HarmoniqStudio/minidumps`.
- `appimage/` – AppImage launcher, desktop integration files, and build scripts.
  The AppRun wrapper mirrors the Flatpak crash reporting behaviour.

Refer to the README in each subdirectory for build instructions.
