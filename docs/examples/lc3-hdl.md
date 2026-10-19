# LC-3 with HDL base modules

[Examples](README.md) · [Schematic LC-3](lc3.md) ·
[Verilog modules](../guides/verilog-modules.md)

Download [lc3-hdl.rgate](../../examples/lc3-hdl.rgate) and open it with
**File → Open…** on desktop. This is a second LC-3 design: the CPU is a schematic
of reusable SystemVerilog blocks rather than a collection of primitive gates.
The original [lc3.rgate](../../examples/lc3.rgate) is unchanged.

## Architecture

| Definition | Contents |
| ---------- | -------- |
| main | 500 ns clock, active-low RESET_N switch, CPU/memory instances, debug displays |
| LC3HDL | Schematic wiring between the controller, datapath, and register file |
| LC3ControlHDL | Microstate register, opcode dispatch, combinational microcode table |
| LC3DatapathHDL | PC/IR/MAR/MDR/CC registers, ALU, sign extension, address generation, bus mux |
| LC3RegisterFileHDL | Eight clocked 16-bit registers and two asynchronous read ports |
| LC3MemoryHDL | 64K×16 memory with embedded program, result observation, console-byte recorder |

The control words and instruction microstates match the original schematic
CPU. Memory uses separate read/write data signals and a synchronous write
interface instead of the original bidirectional RAM bus. The console records
writes to xFE06 in HDL; it does **not** use the schematic TTY peripheral, which
is not currently executable by the HDL backend.

## Run the program

1. Start with **RESET_N=0** and clock-step once (Tab). This initializes the CPU
   registers and console recorder while preserving the embedded program image.
2. Click RESET_N to **1**.
3. Run with Play/Space, or clock-step through instruction execution.
4. Pause when **HALTED=1**. The simulated clock can continue, but the CPU stays
   in state xFF.

The included program is the same [LC-3 program](../../examples/lc3-program.asm)
as the schematic example: sum 1–5, store 15, print H/I/newline through the OUT
trap handler, and halt.

| Display | Expected final value |
| ------- | -------------------- |
| RESULT | 000F: memory word at x3020, decimal 15 |
| CONSOLE | 48490A: ASCII H, I, newline |
| TX | 0A: last console byte |
| TX_COUNT | 3: number of console writes |
| R1 | 0000: loop counter completed |
| STATE | FF: halted microstate |
| HALTED | 1 |

PC, IR, R0/R1, and CC show the evolving machine state. R0 ends with the newline
character, not the sum; RESULT preserves the sum stored in memory.

## Inspect and edit

Double-click `cpu` to inspect its schematic wiring. In Edit, double-click the
controller, datapath, or register-file block to open its source definition.
During simulation use **Simulate → Browse HDL signals…** to probe internal
signals such as `cpu.datapath.alu_result`, `cpu.datapath.offset`, or the
controller's control word. Add probes before the events you want to capture.

The `.rgate` document embeds all source code and the memory image, so no external
HDL files or `$readmemh` paths are needed at runtime. Source copies are in
[examples/verilog/lc3](../../examples/verilog/lc3/). To change the program, stop
simulation and edit the initial memory assignments in **LC3MemoryHDL**, then
start a fresh run and exercise reset. The primitive RAM inspector does not edit
this HDL memory.

The CPU executes BR, ADD, AND, NOT, LD/ST, LDR/STR, LDI/STI, LEA, JSR/JSRR,
JMP/RET, and TRAP vector indirection. As in the original example, RTI and
reserved opcodes halt; interrupts, PSR, protection, and a complete OS are not
implemented. TRAP x25 is intercepted to halt, and the minimal OUT handler
clobbers R2.

## Regenerate

After editing the checked-in HDL source copies, regenerate the embedded example:

```sh
python3 scripts/generate-lc3-hdl.py
```
