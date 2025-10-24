# PipeWire and JACK interoperability

The Flatpak build exposes PipeWire and PulseAudio sockets to ensure realtime
capture and playback. When the host runs PipeWire in JACK compatibility mode
Harmoniq Studio automatically detects the bridge through CPAL and the
`PIPEWIRE_LATENCY` environment variable exported by the wrapper script. To fine-
tune latency from the host, set:

```bash
flatpak override --user --env=PIPEWIRE_LATENCY=256/48000 com.harmoniq.Studio
```

If you rely on the native JACK daemon rather than PipeWire, enable the JACK
runtime extension and allow the session bus:

```bash
flatpak install org.freedesktop.Platform.GL.default//23.08
flatpak run --device=all --socket=session-bus com.harmoniq.Studio --audio-backend jack
```

The application stores crash minidumps under
`$XDG_STATE_HOME/HarmoniqStudio/minidumps`. Share the directory with your
support staff when reporting driver issues.
