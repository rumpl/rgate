# Examples

[Documentation home](../README.md) · [LC-3 walkthrough](lc3.md)

Open a `.rgate` file from the repository's `examples/` directory using **File →
Open…**, or pass its path to `cargo run --`. These circuits are editable; stop
simulation before changing definitions.

| File                          | What to try                                                                                       |
| ----------------------------- | ------------------------------------------------------------------------------------------------- |
| `full-adder.rgate`            | Toggle A/B/Cin and inspect Sum/Cout.                                                              |
| `hierarchical-adder8.rgate`   | Inspect repeated half/full/nibble adders in live hierarchy.                                       |
| `adjustable-counter.rgate`    | Release RESET_N, count with ENABLE_N=0, change STEP, and observe modulo-256 wrap.                 |
| `serial-shift-register.rgate` | Toggle DATA and clock-step: capture bit 0 and shift previous bits left through a bar/hex display. |
| `parity-checker.rgate`        | Set an 8-bit DATA word; ODD/EVEN indicate the parity of its high bits.                            |
| `pwm-dimmer.rgate`            | Component-based register/adder PWM; set DUTY and inspect pulse width.                             |
| `pwm-verilog.rgate`           | Edit the Project F PWM source and run its schematic parent through Icarus.                        |
| `tiny-vga.rgate`              | Circuit-generated raster timing, pattern ROM, and visual display. [Walkthrough](tiny-vga.md).     |
| `lc3.rgate`                   | Release reset, sum 1–5, print HI, and halt. [Walkthrough](lc3.md).                                |

The counter, shift register, and parity circuits are original RGate examples,
replacing the previously retained TkGate circuits. Regenerate them with
`scripts/generate-learning-examples.py`. Their logic consists of editable gates,
registers, taps, and buses—not host-side controller emulators. Importer tests
use independently authored minimal annotated-Verilog fixtures instead of
upstream examples/tutorials.

## Read examples rather than just running them

- Identify control polarity before toggling reset/enable.
- Use Interface to understand a child's input/output ports.
- Enter specific Tree instances during simulation; List definitions can be
  ambiguous.
- Open Properties in Edit to inspect widths, delay, clock, partitions, and
  memory initialization.
- Add probes before the transitions you want to record.
- Use Messages for unsupported-import warnings.

The generated LC-3 is intentionally component-based. `scripts/generate-lc3.py`
and `scripts/schematic_routing.py` construct its document/geometry/control-ROM
contents; they don't execute CPU instructions. Regeneration replaces the example
file, so save edited copies elsewhere.
