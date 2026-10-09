# Files, workspace, and recovery

[Documentation home](../README.md) ·
[Troubleshooting](../reference/troubleshooting.md)

## Circuit files

| Format/action     | What it does                                                                                                                              |
| ----------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| `.rgate`          | Versioned JSON containing definitions, gates/pins, nets, wires/layout, initial component configuration, comments, and symbols             |
| Open TkGate `.v`  | Imports the supported annotated schematic subset, including coordinates, endpoints, rotations, selected gate options, and supported joins |
| Export Verilog    | Emits executable constructs for supported circuit primitives/modules, plus comments containing native layout metadata                     |
| VCD… in Waveforms | Exports retained runtime signal changes; not a circuit file                                                                               |

All definitions are stored together in one native document. Simulation runtime
state (registers, mutable RAM, TTY queues/output, time/events, trace history) is
not a native circuit save.

Desktop Save uses atomic file replacement through a temporary sibling. Saving an
imported `.v` normally prompts for a new `.rgate` destination, protecting the
imported source. Read Messages for parse/validation/export errors. Unsupported
elements are reported rather than treated as implemented behavior.

Exported Verilog layout comments provide a **lossless RGate round-trip**, not
native TkGate schematic-save compatibility. TTY host I/O refuses executable
Verilog export. A CPU hierarchy without its root host peripheral can be exported
separately. Unknown unsupported components also refuse export. No arbitrary HDL
execution, Verilog compiler, testbench engine, or Tcl interpreter is installed.

## Browser file behavior

Open uploads a local `.rgate`/supported `.v` file. Load hex file uploads memory
text. Save/Save As downloads `.rgate`; Export downloads `.v` if supported. The
browser doesn't silently overwrite local files. Save marks the current app
document saved after a successful download request; retain the downloaded file
yourself.

Uploads stay in your local browser/app; the static server doesn't run
simulation. Clipboard depends on browser permissions and secure context. Quit
asks you to close the tab rather than quitting the OS. Do not rely on native
pathname semantics in the browser.

## Persistent preferences

RGate remembers:

- Theme, left/right sidebar widths, Modules/Nets separator, bottom-panel height.
- Grid, snapping, sidebar tabs, collapsed hierarchy branches.
- Zoom/pan per module or live instance.
- Probes by root/instance path/net name, skipping stale references.
- Waveform range, follow, cursors, name-column width, filter, radix, order,
  grouping.

These settings are separate from your circuit and do not mark it dirty. Up to 32
document workspace keys are retained. A renamed instance/net may invalidate a
saved probe. Merely remembering a live path does not resume CPU execution.

## Autosave/recovery

Unsaved circuit edits receive a **recovery snapshot about every 2 seconds** when
no drag/wire gesture or prompt is in progress. Settings also flush at
saves/document switches/normal close. Recovery doesn't overwrite the original
document.

On next startup choose **Recover circuit** or **Discard recovery**. Recovery is
offered rather than silently replacing an explicitly opened file. A recovered
circuit stays **unsaved** until saved/downloaded. No volatile simulation state
or undo history is restored. Save important work manually: a crash can lose the
latest ~2 seconds, and recovery is not a durable backup system.

| Platform | Storage                                                                                           |
| -------- | ------------------------------------------------------------------------------------------------- |
| macOS    | `~/Library/Application Support/RGate/`                                                            |
| Linux    | `$XDG_STATE_HOME/rgate`, fallback `~/.local/state/rgate`                                          |
| Windows  | `%APPDATA%/RGate`                                                                                 |
| Browser  | `localStorage` for the current origin, keys `rgate.v1.settings.json` and `rgate.v1.recovery.json` |

Desktop `RGATE_STATE_DIR` overrides the location. Corrupt/future-version state
is reported and does not block normal startup. Panel/view values are sanitized.
Storage quota/private mode/read-only directories produce Messages errors while
the in-memory circuit remains open. State over 32 MB is rejected explicitly.

To reset preferences, close the app and remove `settings.json` (or its browser
key). Recovery is separate as `recovery.json`; don't delete it if needed.
Choosing to discard edits on Quit intentionally clears recovery. Concurrent
processes/tabs share storage; last writer wins. Browser site-data clearing
removes local recovery.

## Command line

```sh
cargo run -- --help
cargo run -- examples/lc3.rgate
cargo run -- --simulate examples/parity-checker.rgate 200
cargo run -- --export examples/full-adder.rgate /tmp/full-adder.v
```

`--simulate` starts a fresh runtime, advances the supplied ns (default 200), and
prints root net values. It does not automatically release reset switches or run
interactive input. `--export` converts supported documents; destination `.v`
selects Verilog, otherwise native JSON. CLI commands are desktop-only.

## Distribution and licenses

The web build is a static directory; preserve `pkg/snippets`, fonts/licenses,
LICENSE, and NOTICE. Native artwork/examples and bundled web fonts retain
upstream notices. RGate application code is GPL-3.0-or-later; see
[LICENSE](../../LICENSE) and [NOTICE](../../NOTICE). No warranty is provided.
