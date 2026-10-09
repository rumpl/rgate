# RGate user documentation

RGate is a Rust/GPUI digital circuit editor and event-driven simulator inspired
by TkGate. Build schematics, reuse modules, run circuits, inspect live
instances, and measure signals in the waveform viewer. The desktop and web
applications share the same circuit model and simulator.

![RGate editor showing the full-adder example](images/editor.png)

## Guide

1. [Getting started](guides/getting-started.md) — installation, first circuit,
   native and web launch.
2. [Using the interface](guides/interface.md) — panels, menus, search, editing,
   wires, properties, and themes.
3. [Component reference](components/README.md) — every built-in component, pin
   names, widths, behavior, and configuration.
4. [Modules and symbols](guides/modules.md) — interfaces, instances, hierarchy,
   and custom drawings.
5. [Simulation and live debugging](guides/simulation.md) — logic states,
   stepping, probes, memory, terminal I/O, and diagnosis.
6. [Waveform viewer](guides/waveforms.md) — timeline navigation, cursors, radix,
   ordering, grouping, and VCD.
7. [Files, workspace, and recovery](guides/files-and-workspace.md) —
   import/export, autosave, persistence, CLI, and browser differences.
8. [Examples](examples/README.md) — bundled circuits and the LC-3 walkthrough.
9. [Keyboard and mouse reference](reference/shortcuts.md).
10. [Troubleshooting and limits](reference/troubleshooting.md).

[Experimental HDL backend evaluation](guides/hdl-backend.md) — opt-in native
Icarus VPI and xezim runners and downloaded Verilog examples.

[Verilog source modules](guides/verilog-modules.md) — create/edit stored HDL,
place instances, and run with Icarus.

[Xezim upstream contribution proposal](development/xezim-interactive-api.md).

## Conventions

- **Cmd/Ctrl** means Command on macOS and Control on other platforms. A focused
  text field uses normal text editing instead of circuit shortcuts.
- Pin directions in the component reference are **relative to the component**:
  input goes into it, output leaves it, and inout can both receive and drive.
- Bit 0 is the least-significant bit. Signal widths are **1–4096 bits**, with a
  deliberate resource cap. Memory addresses are word indexes, not byte offsets.
- All simulation timestamps and gate delays are in **nanoseconds**. Wall-clock
  time is not simulated time.
- **Active-low** means a control is asserted at `0`; rising edge means `0 → 1`.
- Screenshots are genuine captures of the current web build in Chrome at
  1440×1000, using Classic theme. The desktop uses the same editor; the OS
  title/menu bar and file dialogs differ. Some examples are large: zoom in
  rather than relying on the fit-to-window overview.

## Scope

RGate executes its circuit graph using the Rust simulator. In the editor,
Verilog remains an import/export format, **not the active simulation backend**.
The separate native `rgate-hdl` command prototypes HDL execution using Icarus
VPI or xezim. Native builds with the Icarus feature also support
[editable Verilog modules](guides/verilog-modules.md) in the editor. It does not
run arbitrary Verilog testbenches or Tcl plugins. See
[limits](reference/troubleshooting.md#known-limits) before relying on timing
fidelity or TkGate compatibility.

For source/build details and licensing, see the [project README](../README.md),
[LICENSE](../LICENSE), and [NOTICE](../NOTICE).
