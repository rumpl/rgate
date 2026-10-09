# Create and edit Verilog modules

[Modules](modules.md) · [Icarus backend](icarus-backend.md) ·
[xezim upstream proposal](../development/xezim-interactive-api.md)

## Desktop setup

```sh
brew install icarus-verilog
./scripts/bundle-macos.sh dev
# or: cargo run -p rgate
```

Verilog source and interfaces are saved in native `.rgate` documents, including
autosave/recovery. The source editor is also available in default/browser
builds, but **execution requires native Icarus tools; the desktop launcher
enables its Icarus feature by default**. Source is trusted code, not sandboxed.

## First example

Open **File → Editable Verilog PWM**. Select `pwm` in Modules, then select the
**Edit** tab (or **Module → Edit Verilog source…**). The native Edit tab embeds
**GPUI Kit’s published `gpui-component` editor (0.7.1)**, with Verilog
Tree-sitter syntax colors, line numbers, scrolling, selection, indentation,
search, and text undo/redo. It uses a monospace font and RGate’s editor
background/colors. GPUI is aligned to the component’s published 0.3.8 snapshot.
No vendored source, compatibility patches, or Zed internal editor dependencies
are used. The source is the actual Project F PWM module, not a hidden Rust
controller or a separately fixed HDL binding.

Typing in the native code editor updates the document immediately; Save persists
it. The **Interface…** button edits observed signals/ports in a separate
Apply/Cancel dialog. Return to `main`, then start simulation. The clock, DUTY,
and LED schematic surround an instance of that source module. Double-click DUTY
to change the runtime byte. Probe `pwm_out`; open the live `dimmer` instance to
probe `cnt`. Stop before editing source; the next run recompiles it.

## Create a source module

Choose **Module → New Verilog module…**. A counter template is provided. Enter:

- Module name, matching the declaration in source.
- Multiline Verilog/SystemVerilog source. Paste keeps newlines; Enter inserts a
  newline, Tab inserts spaces. New-module setup accepts initial source; after
  creation use the native syntax-highlighted Edit tab. The browser still uses
  the basic multiline source dialog.
- Explicit interface/observed signals, one per line:

```text
input clk 1
input reset 1
input enable 1
output count 8
signal internal_state 8
```

Directions are `input`, `output`, `inout`, or `signal` (internal observation).
Names must be unique simple identifiers and widths 1–4096. The declarations must
match the HDL; Icarus diagnostics and binding errors appear in Messages when you
run. Source changes can remain unfinished without requiring compilation; Run
reports compiler errors. Port inference is not implemented.

Source modules cannot contain schematic gates/wires. Use them as module
instances in a **schematic parent**: switch to the parent and place the module
using Components or Modules placement controls. Declared ports provide instance
pins. Connect clocks, switches/DIPs, and LEDs as usual. When the document
contains source modules, starting simulation uses Icarus's interactive export of
the schematic plus the stored source. All definitions are compiled, so even
unused broken source can fail compilation.

The first version supports components already covered by executable Verilog
export. Host TTY/VGA sinks and other unsupported exports fail loudly; they are
not silently simulated by a second engine. Includes, external memory images,
compiler defines, and arbitrary top parameter overrides are not configured yet.
Rename is not supported from the source editor. Disconnect connected instances
before changing interface ports.

## Run a source module independently

Select the source module and run. RGate creates an HDL wrapper with
zero-initialized registers for declared input ports. Select an input in Nets,
then choose **Module → Set selected HDL input…** to deposit a hex value. Set
clock low/high manually for a simple counter, or build a schematic parent with a
Clock for automatic cycles. Inout ports aren't interactive inputs in this
wrapper.

Probe declared outputs/internal signals through Nets and Waveforms. Internal HDL
instance hierarchy is not automatically turned into schematic definitions; add
explicitly observed local signals to the interface field. Schematic instance
paths retain their existing live-debug mapping. Step is the next HDL timestep;
on a design with no future events it can time out. Clock step uses the schematic
clock period or the default 100 ns quantum.

## Persistence and limits

- `.rgate` stores source and the explicit signal interface, not runtime state.
- Verilog export emits stored source directly alongside schematic modules and
  layout metadata. Reopening RGate's metadata-bearing export is lossless. Plain
  arbitrary `.v` import still follows the existing TkGate importer; paste source
  into the source editor rather than assuming it imports as a source module.
- Maximum source size is 1 MB per module; the VPI adapter supports at most 256
  bindings and simple dot-separated paths. Memories, arrays, escaped names, and
  generated-scope indexing aren't exposed generally.
- Compilation/runtime requests are synchronous. Runtime timeouts kill the child;
  compiler cancellation and asynchronous startup remain future work.
- Live writes take effect one engine precision tick later. Waveform timestamps
  remain integer ns.
- The earlier **PWM LED dimmer — Icarus HDL (experimental)** fixed-binding
  example remains available; use **Editable Verilog PWM** for the new source
  workflow.
