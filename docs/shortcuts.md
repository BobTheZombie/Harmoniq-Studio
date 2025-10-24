# QWERTY Keyboard Controls

The typing keyboard can operate as a lightweight MIDI controller when no
hardware devices are connected (or when enabled explicitly with `--qwerty`).
This table lists the default mappings.

| Key | Action |
| --- | --- |
| `Q W E R T Y U` | White keys for the current octave |
| `2 3 5 6 7` | Sharps/black keys above the white-key row |
| `Z` / `X` | Shift the base octave down or up |
| `[` / `/` | Decrease or increase the MIDI channel (1-16) |
| `Space` | Sustain pedal (CC64) |
| `Shift` (while playing) | Accent notes (+20 velocity) |
| `C` / `V` + modifier | Cycle velocity presets backward / forward |
| `1` … `0` + modifier | Select an absolute velocity preset |
| `Esc` | Panic (All Notes Off + CC64 reset) |

Velocity presets are accessed by holding Control/Alt/Super while tapping the
number row or `C`/`V`. The configuration file at
`~/.config/HarmoniqStudio/qwerty.json` stores the active octave, MIDI channel,
velocity curve, layout, and sustain key.
