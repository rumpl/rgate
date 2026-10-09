# LC-3 CPU circuit walkthrough

[Examples](README.md) · [Simulation](../guides/simulation.md) ·
[TTY](../components/tty.md)

Open **File → LC-3 CPU circuit**, or:

```sh
cargo run -- examples/lc3.rgate
```

![LC-3 root schematic and reusable modules](../images/lc3.png)

## What is implemented

This is a **microcoded schematic**, not a Rust CPU emulator. Ordinary registers,
muxes, gates, taps/concatenation, RAM, and ROM implement execution.

| Definition   | Contents                                                                                                                       |
| ------------ | ------------------------------------------------------------------------------------------------------------------------------ |
| main         | 500 ns clock, RESET_N switch, 64K×16 logical RAM, TTY, PC/IR/register/status LEDs                                              |
| LC3          | PC/IR/MAR/MDR/CC registers, ADD/AND/NOT datapath, operand muxes, sign extension, memory bus driver, TTY display-address decode |
| RegisterFile | Eight 16-bit registers, one write decoder, two eight-way read muxes                                                            |
| Control      | Microstate register, opcode dispatch logic, editable 256×32 control ROM                                                        |

Generated wires attach to native pin coordinates and avoid unrelated component
bodies. Crossings without junction connection are not electrical merges. The
complete circuit is still dense; Fit is an overview, not a textbook datapath
layout. Zoom into modules for inspection.

## Run the included program

1. Start simulation with **RESET_N=0**. Let reset propagate; this clears the
   registers.
2. Click RESET_N to **1**. It is active-low: leaving it at 0 holds the CPU in
   reset indefinitely, even though the clock/time advance.
3. Run with Play/Space, or Tab through 500 ns clock periods.
4. Double-click `console` while simulating. The terminal stays live. If the
   reset warning appears, click **Release reset (RESET_N → 1)**, then Run.
5. The output becomes `HI` followed by a newline.
6. `HALTED=1` means the program reached HALT. The simulator clock may keep
   running; the CPU's microstate stays halted.
7. Double-click `main_memory` and inspect **decimal address 12320**, which is
   hex **x3020**. Its value is **0x000F (15)**.

![Terminal showing HI after the program executes](../images/terminal.png)

## Program

The included program clears R0/R1, loads count 5, sums down to 1, stores the
result at x3020, loads H/I/newline characters, calls TRAP x21, then TRAP x25.

- `examples/lc3-program.asm` documents source/instructions. No assembler is
  bundled.
- `examples/lc3-program.hex` contains initial RAM words and addresses.
- TRAP x21 reads a vector at x0021 and runs ordinary LC-3 code at x3100.
- The minimal OUT handler writes to **xFE06**; the circuit decodes that store to
  strobe the TTY. It deliberately clobbers R2.
- TRAP x25 halts through the control-ROM path.

## Inspect a running CPU

Probe PC, IR, STATE, CC, and R0/R1; then enter `main/cpu/controller` or
`main/cpu/register_file` in Tree. Those are **live instances of the root run**.
Run/pause/step preserve time and state. Use ↑ Parent to return. Child-port
probes can alias root signals; internal probes carry full hierarchical labels.

STATE is a microstate, not an instruction counter. Fetch/execute commonly takes
several cycles. Don't shorten the clock without checking combinational settling.
VCD can export retained traces for external viewing.

## Load or modify a program

Stop simulation, open RAM Properties, and load/edit a hex image. Save an edited
copy of the document. Fresh simulation reloads initial RAM; runtime inspector
modifications are volatile. RESET_N must be exercised again. Microcode lives in
the Control module's ROM and is also ordinary editable configuration.

## Instruction scope and limits

Implemented paths: **BR, ADD, AND, NOT, LD/ST, LDR/STR, LDI/STI, LEA, JSR/JSRR,
JMP/RET, TRAP vector indirection**. End-to-end tests cover sum/output/halt and
indirect-memory/subroutine paths.

Not a complete LC-3 machine/OS: no RTI, interrupts, privilege/PSR, protected
memory, keyboard MMIO, packaged GETC/PUTS service image, or assembler.
RTI/reserved opcodes lead to halt. The memory/device model is digital and
timing-simplified.

Executable Verilog export of the full root fails intentionally because TTY uses
host I/O. Component-only CPU/RegisterFile/Control modules are exportable when
separated from the host terminal. There is no behavioral Verilog simulator
hidden inside the CPU example.
