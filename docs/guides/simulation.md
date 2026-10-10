# Simulation and live debugging

[Documentation home](../README.md) · [Waveforms](waveforms.md) ·
[Verilog modules](verilog-modules.md)

Run a circuit, change its inputs, and follow the resulting signals through its
modules. The Simulate workspace shows live wire values, Nets, displays, and
recorded waveforms.

## Start, pause, stop, step

| Action           | Effect                                                                          |
| ---------------- | ------------------------------------------------------------------------------- |
| Space / Play     | Start simulation or toggle run/pause                                            |
| Simulate tab     | Open the live circuit for inspection                                            |
| F6 / Step        | Advance to the next event time or Verilog timestep                              |
| Tab / Clock step | Advance one root clock period; a circuit without a clock uses a 100 ns interval |
| Stop             | End the run and return to editing                                               |
| Edit tab         | Return to editing and prepare a fresh run                                       |

Pause preserves the current values. A fresh run reloads initial inputs and
memory contents. Simulation time is shown in nanoseconds; use step controls to
study changes at your own pace.

## Signal states

| State | Meaning        | What to inspect                                         |
| ----- | -------------- | ------------------------------------------------------- |
| 0     | Known low      | Reset, constants, and logic outputs                     |
| 1     | Known high     | Enabled drivers and logic outputs                       |
| X     | Unknown        | Reset, control values, addresses, and competing drivers |
| Z     | High impedance | Floating connections and disabled drivers               |

Wire colors follow the current theme. Nets, labels, and waveforms show the
values explicitly, including each bit of a bus. Conflicting low/high drivers
produce X; a disabled tri-state output contributes Z.

## Delays and initialization

Gate Properties includes an output delay. Allow signals to settle before a clock
edge samples them. Registers use the control polarities in the
[component reference](../components/sequential-and-memory.md): assert CLR low,
let reset propagate, then release it.

Use a clock period long enough for your circuit’s data path. In Verilog modules,
clocked assignments and delays follow the source you write. Initialize storage
through reset logic or the source’s initialization statements.

## Inputs during simulation

- Click a Switch to toggle its value.
- Double-click a DIP to enter a hexadecimal bus value.
- Click GPIO to increment its output.
- Double-click RAM/ROM to inspect or change a runtime word.
- Double-click TTY for live text output and queued input.
- For a standalone Verilog module, select an input net and choose **Module → Set
  selected HDL input…**.

These controls change the current run. To choose values for future runs, Stop,
update initial values or memory images in Properties, and Save.

## Probes

Double-click a wire or click `○` beside a Nets row. The filled marker indicates
recording. Open Waveforms to inspect transitions from the moment the probe was
added.

Add probes before the events you want to study. RGate retains recent history for
each signal and remembers probe names and paths for your next session.

## Live hierarchical debugging

Run the parent circuit, then open a specific instance in Modules Tree or
double-click its block. Displays and internal signals show that instance’s
state. Repeated instances can hold different values even when they share a
definition.

- Probe ports to follow the connection to the parent.
- Probe internal nets to follow the selected child.
- Waveform labels include paths such as `main/cpu/register_file/R0`.
- RAM and TTY dialogs operate on the selected live device.
- Run, pause, and step continue the same root simulation.
- **↑ Parent** takes you back through the hierarchy.

Stop before editing definitions. See
[modules](modules.md#simulation-inside-hierarchy).

## Diagnose an unexpected result

1. Pause and read the affected value in Nets.
2. Check reset and enable polarity.
3. Check data widths, select widths, and bus slices.
4. Probe clock, reset, data, and output; step through their changes.
5. Inspect shared-bus drivers and their enable controls.
6. Open the relevant live instance and follow its internal signals.
7. Read Messages for diagnostics.

The [LC-3 walkthrough](../examples/lc3.md) shows reset, live CPU inspection, and
terminal output together.
