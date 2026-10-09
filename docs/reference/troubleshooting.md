# Troubleshooting and limits

[Documentation home](../README.md) · [Simulation](../guides/simulation.md)

## Nothing prints in TTY

- Confirm TX/RD data is a valid eight-bit character and its protocol matches the
  wired pins.
- A byte requires a **rising edge**, not a permanently high strobe.
- For LC-3, release **RESET_N to 1** after initial reset. A running clock with
  reset low doesn't execute code.
- Opening TTY keeps it live; its Run/Clock step controls operate on the same
  simulation.
- Empty output has an explicit explanation. Check PC/IR/HALTED and memory
  program contents instead of assuming the terminal is broken.

## X everywhere

Reset sequential state; inspect active-low enable/clear. Unspecified RAM/ROM
locations start X. Divide-by-zero and unknown selects produce X. Two
incompatible drivers on a net create X. Step and probe controls/data at the same
live instance.

## Z on a signal

The net may float, or its memory/tri-state driver may be disabled. Read
CS/OE/WE/E polarity from the component reference. Z isn't known zero. Ordinary
Buffer/NOT turns a floating input into unknown; digital pass devices can
propagate/release Z.

## Connection rejected or unexpected width

Set widths before wiring. A select pin isn't data width. Concat partitions must
sum to total width and are LSB-first; Tap slices must stay inside the bus. An
8-bit shift selector is normally 3 bits, not 8. Runtime input larger than a
configured bus is rejected. Incompatible connected-property edits roll back.

## Can't place a module

You cannot place a definition inside itself or create indirect recursion. Select
another parent; its other definitions have placement ＋. Define ports in
Interface first. List and Tree aren't interchangeable during live debug: select
the exact instance path when there are repeated copies.

## Shortcuts stop working

Click canvas to restore focus. Search/text/Message selection captures typing
appropriately. Dialogs and menus block accidental canvas edits. Escape ends
repeated placement, unfinished wiring, or ordinary dialogs; recovery needs
Recover/Discard. Pin clicks in Select can start wiring, so press V again if you
meant to select a gate body.

## Wire remains / delete affects too much

Click selects one branch; box selection includes wires intersecting the box.
Nets-row selection represents the net rather than a specific branch. Delete
selected branches or use Cut tool for a wire. A surviving gate keeps a dangling
connection available; deleting both attached gates removes their wire and orphan
nets. Undo restores the operation.

## Tiny or crowded drawing

Fit is an overview for large examples such as LC-3. Zoom into a submodule.
Resize sidebars/bottom panel; long tree labels truncate. Generated LC-3 wiring
avoids component bodies, but manual circuits use your chosen routes. Move
individual segments or reorganize gates; Arrange offers selected-wire routing
and layered layout; they are heuristics, not guaranteed minimum-crossing
textbook layouts.

## Waveform is empty or doesn't show earlier events

Add probes. History begins when probed and retains at most 4096 changes per
signal. Fit/Follow/manual range can show different intervals. A cursor before
retained history shows —. Filtering or collapsed groups may hide rows. A fresh
simulation restores probe settings but not old transition history.

## Lost preferences or recovery warning

Check the state location and permissions in
[files and workspace](../guides/files-and-workspace.md). Browser private
mode/quota/site clearing can prevent retention. Multiple tabs/processes are
last-writer-wins. Recovery snapshots lag roughly two seconds and exclude
simulation runtime. Don't delete recovery data before recovering important
edits; save/download manually.

## Web won't start

Use a recent WebGPU/WebGL2 browser and HTTP(S), not file://. Use localhost/HTTPS
for WebGPU. Keep `pkg/snippets` with the deployed WASM/JS and serve `.wasm` with
appropriate MIME type. Check the on-page error and browser console. The
optimized build is large; use HTTP compression. No backend is required.

If build reports missing bindgen, install the CLI version matching Cargo.lock
and confirm Cargo bin is accessible. The helper searches `~/.cargo/bin`;
compiler must have the wasm32 target. Rust version/build instructions are in
[getting started](../guides/getting-started.md).

## Known limits

- **No behavioral HDL execution**: Verilog imports a schematic subset; no
  `always` testbench engine or xezim/vitamin integration yet.
- **No Tcl plugins** or arbitrary script execution. TTY/GPIO are built-ins.
- Signal/port width cap **4096 bits**; up to 64 gate inputs or partitions.
  Module nesting and expanded-element count also have safety guards.
- Memory addresses up to **32 bits**, sparse initialization capped at one
  million words; no physical access/setup/hold checking.
- Digital transistor behavior has **no analog voltages or drive strengths**.
- No simulation breakpoints, run-until conditions, or general scripted stimulus
  UI yet.
- Waveform history is bounded; no VCD/FST import or FST export.
- Comment markup is a safe subset; image alt-text placeholders only, no
  arbitrary CSS/HTML layout or scripts.
- Symbol editor supports line/rectangle/ellipse/text, not arbitrary SVG/Bézier
  import.
- No general library manager, parameter elaboration, print/PDF output, or full
  native TkGate save compatibility.
- Desktop has been validated on macOS; browser validated in Chrome, including
  WebGPU and WebGL2. Mobile/touch UI and other platform behavior are not claimed
  fully tested.
- LC-3 is an educational circuit, not a full protected/interrupt-capable LC-3 OS
  machine. See [LC-3](../examples/lc3.md).

When reporting a bug, include the `.rgate` document, exact example/module path,
action sequence, platform/browser, screenshot, and copied Messages text. Save a
copy before simplifying the reproducer.
