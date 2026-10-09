# Simulation and live debugging

[Documentation home](../README.md) · [Waveforms](waveforms.md) ·
[TTY](../components/tty.md)

## Start, pause, stop, step

| Action           | Effect                                                                                                         |
| ---------------- | -------------------------------------------------------------------------------------------------------------- |
| Space / Play     | Start or toggle run/pause                                                                                      |
| Simulate tab     | Initialize a simulation for inspection; not automatically continuous Play                                      |
| F6 / Step event  | Pause continuous run and process the next event time                                                           |
| Tab / Clock step | Pause continuous run and advance one fastest clock period in the root hierarchy; fallback 100 ns with no clock |
| Stop button      | Discard runtime state and return to editing                                                                    |
| Edit tab         | Ends live simulation so schematic edits can resume                                                             |

A running GUI advances one root clock period per ~40 ms update, clamped to
10–1000 ns per update. Simulation time is independent of wall time. Different
hardware delays require different clock periods; the app doesn't guarantee
arbitrary fast clocks work.

An edit invalidates the old circuit simulation. Fresh runs reload initial
inputs/memory and start sequential state unknown unless reset. Pause preserves
runtime values; Stop does not.

## Signal states

| State | Meaning        | Common cause                                                         |
| ----- | -------------- | -------------------------------------------------------------------- |
| 0     | Known low      | Reset/constant/logic result                                          |
| 1     | Known high     | Enabled driver/logic result                                          |
| X     | Unknown        | Unreset registers, unknown controls, driver conflict, divide by zero |
| Z     | High impedance | Floating net or disabled tri-state/memory output                     |

Wire/bus colors change with runtime values and theme. Don't use color alone:
Nets, labels, and waveforms show actual values. Before simulation, bus color
identifies width, not a computed value.

Digital drivers resolve bitwise. Z contributes no active drive; equal known
drivers agree; conflicting 0/1 becomes X. X may propagate or be overridden by a
controlling logic input (for example 0 AND X is 0). NMOS/PMOS are digital
models, not voltage/strength solvers.

## Delays and initialization

Most gates use configurable **inertial output delays**: a pending output can be
superseded before it becomes visible. Events at the same timestamp are applied
together before downstream gates evaluate. Registers load on positive CK edges
with active-low EN/CLR as documented; set reset low, let it propagate, then
release it.

The runtime bounds event processing per advance; excessive activity reports a
possible oscillation. This is not timing verification: there are no setup/hold
violation reports, specify/SDF support, or analog effects. RAM writes are
asynchronous and memory images leave unspecified words X.

## Inputs during simulation

- Click Switch to toggle.
- Click DIP to open hex entry, Apply, then resume if paused.
- Click GPIO to increment its output.
- Double-click RAM/ROM for runtime word inspection/edits.
- Double-click TTY for live text output and queued input.

Runtime edits aren't saved initial circuit values. Update Properties in Edit and
Save for persistent initialization.

## Probes

Double-click a wire, or click `○` beside its Nets row. `●` indicates a probe. A
probe starts history **when added**; it cannot reconstruct earlier transitions.
Removing a probe discards its recorded trace. Values and probe marks follow live
instance mappings.

Open Waveforms to inspect history. Up to 4096 changes per signal are retained;
older transitions may be evicted. Probes are saved by name/path in workspace
preferences, but runtime history isn't persisted.

## Live hierarchical debugging

Start the parent/root simulation, then choose a specific instance in the Modules
**Tree**, or double-click a module block. The same root simulation keeps running
or remains paused.

- The child canvas, Nets values, LEDs, and source controls show that copy's
  state.
- Internal signals are independent for repeated instances of the same
  definition.
- Child ports connected to parent nets alias the same physical signal/probe.
- Waveform labels include the full instance path, such as
  `main/cpu/register_file/R0`.
- RAM/TTY operations target the selected instance's runtime devices.
- Run/pause and step controls still operate on the root engine; clock step uses
  root clocks, not a new child simulation.
- **↑ Parent** navigates up without reset.

List definitions with multiple/no live instances need Tree selection. Stop
before editing a shared definition. Recursive modules are rejected before
simulation.

## Diagnose an unexpected result

1. Pause and inspect the net in Nets; determine X vs Z vs a wrong known value.
2. Check reset and enable polarity. `RESET_N=0` holds a circuit in reset; a
   moving clock doesn't mean the CPU executes.
3. Check data/select widths and which bus slices are connected.
4. Probe clock, reset, enable, input, and output; step through their changes.
5. Inspect driver conflicts: disable extra tri-state outputs during memory
   reads.
6. Enter a specific live child instance and inspect its internal signals.
7. Read Messages and copy diagnostic text for reporting.

The LC-3 TTY dialog can identify a recognized low root RESET_N and offer Release
reset. [The LC-3 walkthrough](../examples/lc3.md) explains the full sequence.
