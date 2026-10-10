# Files, workspace, and recovery

[Documentation home](../README.md) ·
[Troubleshooting](../reference/troubleshooting.md)

## Open and save circuits

Choose **File → Open…** to open a `.rgate` document or import a TkGate
schematic. Use **Save** to update your document and **Save as…** to create
another copy.

A `.rgate` file keeps the complete design together: schematic modules, Verilog
source, ports, wires, component properties, memory images, comments, and custom
symbols. Set initial input values and memory contents in Properties before
saving. A fresh simulation starts from these saved settings.

When importing a TkGate file, save the result as a `.rgate` document. Messages
shows the import details so you can review the converted circuit.

## Export

**File → Export Verilog…** writes your source modules and exportable schematic
logic as a Verilog file. RGate also retains the design layout in its export so
you can reopen it in RGate.

Use **VCD…** in Waveforms to save recorded signal changes for an external
viewer. See [waveform export](waveforms.md#vcd-export).

## Browser files

In the browser, Open selects a local file and Save downloads your design. Keep
the downloaded `.rgate` file somewhere you can find again. Save As creates
another download; loading a memory image also uses a local file picker.

Circuit editing and schematic simulation run on your device. Clipboard access
follows your browser’s permissions. Close the tab when you finish.

## Your workspace

RGate remembers your theme, panel sizes, grid/snapping choices, module views,
and waveform preferences. Each document can retain its own zoom, pan, named
probes, and signal-list arrangement.

When you start a new simulation, saved probes attach to their matching signals
and begin recording. Give nets and instances stable names to keep them easy to
find across sessions.

## Recover unsaved edits

RGate periodically saves a recovery snapshot of your circuit edits. If recovery
is offered at startup, choose **Recover circuit** to reopen the design, then
Save it. Choose **Discard recovery** when you want to continue with your saved
files instead.

Use Save regularly and keep copies of important designs. Save a copy before
reorganizing a large hierarchy or trying a different program image.

## Reset preferences

Close RGate before clearing its saved settings. Desktop settings live in:

| Platform | Location                                          |
| -------- | ------------------------------------------------- |
| macOS    | `~/Library/Application Support/RGate/`            |
| Linux    | `$XDG_STATE_HOME/rgate` or `~/.local/state/rgate` |
| Windows  | `%APPDATA%/RGate`                                 |

Removing `settings.json` resets preferences. Keep `recovery.json` until you have
recovered any work you need. In the browser, clearing RGate’s site data resets
preferences and recovery; download your circuit first.
