# RGate user documentation

Build digital circuits, write Verilog modules, and explore how signals move
through your design. RGate brings schematic editing, live simulation, module
hierarchies, displays, and waveforms into one workspace.

**[Try it out in your browser](https://rumpl.github.io/rgate/app/)** to draw and
simulate schematics. Use the desktop app to edit and run Verilog modules.

![RGate editor showing the full-adder example](images/editor.png)

## Start here

- [Getting started](guides/getting-started.md): open a circuit and build your
  first design.
- [Using the interface](guides/interface.md): tools, panels, wiring, and
  arrangement.
- [Verilog modules](guides/verilog-modules.md): write source, connect ports, and
  run it.
- [Modules and symbols](guides/modules.md): reuse circuits and inspect their
  instances.
- [Simulation](guides/simulation.md): run, pause, step, and change inputs.
- [Waveforms](guides/waveforms.md): record signals and measure timing.
- [Files and recovery](guides/files-and-workspace.md): save designs and restore
  your workspace.
- [Component reference](components/README.md): pins, controls, and worked
  examples.
- [Example circuits](examples/README.md): counters, an LC-3 computer, and a VGA
  display.
- [Shortcuts](reference/shortcuts.md) and
  [troubleshooting](reference/troubleshooting.md).

## Reading the reference

Pin directions are relative to the component: an input receives a signal, an
output drives one, and an inout does both. Bit 0 is the least-significant bit.
Bus widths range from 1 to 4096 bits.

Simulation time is measured in nanoseconds. **Active-low** controls are asserted
at `0`; a rising edge is a transition from `0` to `1`. Signals can also be
unknown (`X`) or floating (`Z`). Each component page explains its controls and
shows how to use them.
