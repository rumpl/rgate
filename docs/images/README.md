# Screenshot provenance

Refreshed on 2026-10-10. These PNGs are genuine captures of RGate’s current web
build in Chrome, at 1440×1000 CSS pixels, using Classic theme. No UI mockups or
overlays were added.

| Image                | Captured view                                                                     |
| -------------------- | --------------------------------------------------------------------------------- |
| `editor.png`         | Startup full adder, Edit mode, component palette and Messages                     |
| `led-properties.png` | LED properties, display choices and recommended-width guidance                    |
| `waveforms.png`      | Full-adder live values, three probes, scalar transitions, enlarged waveform panel |
| `lc3.png`            | LC-3 imported/generated circuit, module hierarchy and memory/CPU connections      |
| `terminal.png`       | LC-3 run after reset release, console output `HI`, HALTED=1                       |

Browser screenshots share editor controls with desktop but omit the macOS OS
title/menu bar. Rendering and positions can change with theme, viewport, and
persisted panel sizes. Links in the guides reference local checked-in files;
viewing docs doesn't require the application server.

Screenshots show RGate’s vector artwork and chip mark. Classic vectors were
recreated using TkGate artwork as reference and retain its attribution; see
[NOTICE](../../NOTICE). No original TkGate logo or bitmap files are embedded.

The `tiny-vga.png` capture shows two circuit-generated frames, RGB444 color
bars, and a checkerboard with zero sync/range errors.

## Component symbols

`components/` contains 51 cropped screenshots: all 46 built-in palette
components, four additional LED display modes, and one user-defined module
instance. They were captured from one-component `.rgate` fixtures in the current
web app, Classic theme. Whitespace was trimmed, but symbols were not redrawn or
composited. Pins are unwired; LED examples show their editing/off state. Frame
keeps its visible bounding rectangle. References embed the local PNG beside the
matching explanation.
