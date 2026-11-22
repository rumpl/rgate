# Screenshot provenance

The current captures are in `2026-11-22/`. They were generated from a fresh
RGate web build on 2026-11-22 using headless Chromium, Light appearance, and
Classic canvas theme at 1440×1000 CSS pixels with device scale factor 1.
Dated paths prevent GitHub from reusing cached images after a refresh.

| Image | Captured view |
| --- | --- |
| `editor.png` | Startup full adder, Edit mode, component palette and Messages |
| `led-properties.png` | Unwired LED properties, all five display modes and recommended-width guidance |
| `waveforms.png` | Full-adder live values, three probes, scalar transitions and enlarged waveform panel |
| `lc3.png` | LC-3 root schematic, module hierarchy and memory/CPU connections |
| `terminal.png` | LC-3 after reset release and 450 clock steps, console output `HI`, HALTED=1 |
| `tiny-vga.png` | Two circuit-generated frames, RGB444 color bars and checkerboard, zero sync/range errors |

Browser screenshots share editor controls with desktop but omit the macOS OS
title/menu bar. Rendering and positions can change with theme, viewport, and
persisted panel sizes. The historical before/after captures in
[`../design/ui-refresh/`](../design/ui-refresh/README.md) are intentionally
preserved and are not used as current guide screenshots.

## Component symbols

`2026-11-22/components/` contains 55 cropped screenshots: all 50 built-in
palette components, four additional LED display modes, and one user-defined
module instance with bus ports. Each comes from a validated one-component
`.rgate` fixture rendered by the web app at 200% schematic zoom. Pins are
unwired; LED examples show their editing/off state. Crops retain labels and
pin markers; Frame keeps its complete bounding rectangle.

All images are genuine application captures. Symbols were not redrawn or
composited. Classic vectors were recreated using TkGate artwork as reference
and retain its attribution; see [NOTICE](../../NOTICE). No original TkGate logo
or bitmap files are embedded.

## Reproduce

Install Playwright in `web/node_modules` and its Chromium browser, then run
from the repository root:

```sh
./scripts/build-web.sh
cargo run -p rgate-format --example docs-fixtures -- target/docs-fixtures
python3 -m http.server 8080 --directory target/web
```

In another terminal:

```sh
node scripts/docs-capture.cjs all
```

Captures are written to `target/docs-captures/`; the checked-in set contains
its six full-window screenshots and `components/` directory. Review them before
publishing. Pass a capture name instead of `all` to regenerate only that view;
set `RGATE_URL` to use a different local server. The script exercises real
canvas controls, so UI layout changes may require updating capture coordinates.
