# Plugin Scanning and Library

Harmoniq Studio now ships with a lightweight plugin discovery pipeline.
The `harmoniq-plugin-scanner` crate scans common CLAP, VST3 and Harmoniq
plugin folders, records the results in a JSON database and exposes the
metadata through the application UI.

Run the scanner directly via:

```
cargo run -p harmoniq-plugin-scanner
```

Inside the app you can access the new **Plugins** menu, which contains
entries for **Add Plugins…** (launches the scanner UI) and
**Plugin Library…** (opens the installed plugin list). The dialogs use
the shared JSON database so CLI and GUI workflows stay in sync.
