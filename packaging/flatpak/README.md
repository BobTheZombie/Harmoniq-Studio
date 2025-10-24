# Harmoniq Studio Flatpak manifest

This directory contains the Flatpak manifest and generated cargo sources for
building Harmoniq Studio from source. To refresh the vendor list after updating
`Cargo.lock`, run:

```bash
python3 -m pip install --user flatpak-cargo-generator  # once
~/.local/bin/flatpak-cargo-generator Cargo.lock -o packaging/flatpak/cargo-sources.json
```

Build the Flatpak locally:

```bash
flatpak-builder --user --install --force-clean build-dir packaging/flatpak/com.harmoniq.Studio.yml
```

The wrapper script sets `HARMONIQ_MINIDUMP_DIR` so crash reports are collected
under `$XDG_STATE_HOME/HarmoniqStudio/minidumps`.
