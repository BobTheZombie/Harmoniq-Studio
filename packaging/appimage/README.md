# Harmoniq Studio AppImage assets

The scripts in this directory create a portable AppImage containing the
Harmoniq Studio desktop binary, desktop entry, icon, and minidump crash handler
configuration.

## Build prerequisites

- `cargo`
- `linuxdeploy` and `linuxdeploy-plugin-gtk` (optional but recommended)
- `appimagetool`
- `jq`

## Building

```bash
./packaging/appimage/build-appimage.sh
```

The script produces a versioned AppImage in `dist/` based on the workspace
metadata from `cargo metadata`. Crash minidumps are saved under
`$XDG_STATE_HOME/HarmoniqStudio/minidumps` when the AppImage runs.
