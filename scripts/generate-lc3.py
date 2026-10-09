#!/usr/bin/env python3
"""Generate the component-based LC-3 circuit, not a CPU emulator."""
import json
from circuit_builder import Module, signal, ROOT

# Register file: eight ordinary registers, one write decoder, two read muxes.
rf = Module("RegisterFile")
for name, width in [("CLK", 1), ("RESET_N", 1), ("WRITE", 1), ("DR", 3), ("SR1", 3), ("SR2", 3), ("DATA", 16)]:
    rf.port(name, width, "input")
for name in ["A", "B"] + [f"R{i}" for i in range(8)]:
    rf.port(name, 16, "output")
rf.gate("write_decoder", "decoder", 1, [("I", "input", 3, "DR"), ("E", "input", 1, "WRITE")] +
        [(f"Z{i}", "output", 1, f"write{i}") for i in range(8)], inputs=8)
for i in range(8):
    rf.register(f"R{i}", "DATA", rf.invert(f"enable_n{i}", f"write{i}"))
rf.mux("A", [f"R{i}" for i in range(8)], "SR1", 16)
rf.mux("B", [f"R{i}" for i in range(8)], "SR2", 16)

# ROM microsequencer: state register and opcode-based dispatch constructed from gates.
control = Module("Control")
for name, width in [("CLK", 1), ("RESET_N", 1), ("OPCODE", 4), ("HALT_TRAP", 1)]:
    control.port(name, width, "input")
control.port("CONTROL", 32, "output")
control.port("STATE", 8, "output")
control.const("zero", 0, 1)
control.const("zero3", 0, 3)
control.const("zero1", 0, 1)
control.const("sixteen", 16, 8)
control.const("halt_state", 255, 8)
control.const("always_enabled", 0, 1)
control.register("STATE", "state_next", "always_enabled", 8)
control.concat("dispatch_base", ["zero3", "OPCODE", "zero1"], [3, 4, 1])
control.add("dispatch", "dispatch_base", "sixteen", 8)
control.tap("rom_next", "CONTROL", 32, 0, 8)
control.tap("dispatch_enable", "CONTROL", 32, 28, 1)
control.tap("halt_check", "CONTROL", 32, 29, 1)
control.logic("halt_now", "and", "HALT_TRAP", "halt_check")
control.mux("normal_next", ["rom_next", "dispatch"], "dispatch_enable", 8)
control.mux("state_next", ["normal_next", "halt_state"], "halt_now", 8)

# Control word: next[7:0], bus[10:8], PC/IR/MAR/MDR/RF/CC loads[16:11],
# write[17], ALU[19:18], EA offset[21:20], base[22], forceR7[23],
# branch[24], halt[25], unused[26], JSR[27], dispatch[28], checkHALT[29], storeSR[30].
def micro(next_state=1, bus=0, pc=0, ir=0, mar=0, mdr=0, reg=0, cc=0, write=0,
          alu=0, ea=0, base=0, r7=0, branch=0, halt=0, jsr=0, dispatch=0, halt_check=0, store_sr=0):
    return (next_state | (bus << 8) | (pc << 11) | (ir << 12) | (mar << 13) | (mdr << 14) |
            (reg << 15) | (cc << 16) | (write << 17) | (alu << 18) | (ea << 20) |
            (base << 22) | (r7 << 23) | (branch << 24) | (halt << 25) |
            (jsr << 27) | (dispatch << 28) | (halt_check << 29) | (store_sr << 30))

rom = [micro(255, halt=1)] * 256
rom[0] = micro(1, bus=6, pc=1)                 # boot PC=x3000
rom[1] = micro(2, bus=1, mar=1)                # MAR=PC
rom[2] = micro(3, bus=3, ir=1)                 # IR=memory
rom[3] = micro(4, bus=2, pc=1)                 # PC++
rom[4] = micro(dispatch=1)                    # decode opcode into first microstate
rom[16] = micro(bus=4, pc=1, branch=1)        # BR
rom[24] = micro(bus=0, reg=1, cc=1)           # ADD
rom[32] = micro(33, bus=4, mar=1)             # LD
rom[33] = micro(bus=3, reg=1, cc=1)
rom[40] = micro(41, bus=4, mar=1)             # ST
rom[41] = micro(42, bus=5, mdr=1, store_sr=1)
rom[42] = micro(write=1)
rom[48] = micro(49, bus=1, reg=1, r7=1)       # JSR/JSRR: R7=PC then PC=EA/BaseR
rom[49] = micro(bus=4, pc=1, ea=2, jsr=1)
rom[56] = micro(bus=0, reg=1, cc=1, alu=1)    # AND
rom[64] = micro(65, bus=4, mar=1, ea=1, base=1)  # LDR
rom[65] = micro(bus=3, reg=1, cc=1)
rom[72] = micro(73, bus=4, mar=1, ea=1, base=1)  # STR
rom[73] = micro(74, bus=5, mdr=1, store_sr=1)
rom[74] = micro(write=1)
rom[88] = micro(bus=0, reg=1, cc=1, alu=2)    # NOT
rom[96] = micro(97, bus=4, mar=1)             # LDI
rom[97] = micro(98, bus=3, mar=1)
rom[98] = micro(bus=3, reg=1, cc=1)
rom[104] = micro(105, bus=4, mar=1)           # STI
rom[105] = micro(106, bus=3, mar=1)
rom[106] = micro(107, bus=5, mdr=1, store_sr=1)
rom[107] = micro(write=1)
rom[112] = micro(bus=5, pc=1)                 # JMP/RET
rom[128] = micro(bus=4, reg=1, cc=1)          # LEA
rom[136] = micro(137, bus=1, reg=1, r7=1)     # TRAP
rom[137] = micro(138, bus=7, mar=1, halt_check=1)
rom[138] = micro(bus=3, pc=1)                 # trap-vector indirection
rom[255] = micro(255, halt=1)
control.gate("microcode_ROM", "rom", 32, [("A", "input", 8, "STATE"),
             ("D", "output", 32, "CONTROL"), ("OE", "input", 1, "zero")],
             address_bits=8, memory=[signal(word, 32) for word in rom])

cpu = Module("LC3")
for name, width in [("CLK", 1), ("RESET_N", 1), ("MEM_DATA", 16)]:
    cpu.port(name, width, "in_out" if name == "MEM_DATA" else "input")
for name, width in [("MEM_ADDR", 16), ("WE", 1), ("OE", 1), ("CS", 1), ("PC", 16), ("IR", 16),
                    ("STATE", 8), ("CC", 3), ("HALTED", 1), ("TTY_TX", 8), ("TTY_WR", 1)]:
    cpu.port(name, width, "output")
for i in range(8):
    cpu.port(f"R{i}", 16, "output")
for name, value, width in [("zero", 0, 1), ("one", 1, 16), ("boot", 0x3000, 16),
                           ("seven", 7, 3), ("zero8", 0, 8)]:
    cpu.const(name, value, width)
cpu.tap("opcode", "IR", 16, 12, 4)
cpu.tap("dr", "IR", 16, 9, 3)
cpu.tap("sr1", "IR", 16, 6, 3)
cpu.tap("sr2", "IR", 16, 0, 3)
cpu.tap("immediate", "IR", 16, 5, 1)
cpu.tap("jsr_long", "IR", 16, 11, 1)
cpu.tap("trap_byte", "IR", 16, 0, 8)
cpu.equal("halt_trap", "trap_byte", 0x25, 8)
cpu.instance("controller", control, {"CLK": "CLK", "RESET_N": "RESET_N", "OPCODE": "opcode",
             "HALT_TRAP": "halt_trap", "CONTROL": "control_word", "STATE": "STATE"})
fields = [("bus_select", 8, 3), ("ld_pc", 11, 1), ("ld_ir", 12, 1), ("ld_mar", 13, 1),
          ("ld_mdr", 14, 1), ("ld_reg", 15, 1), ("ld_cc", 16, 1), ("write_mem", 17, 1),
          ("alu_select", 18, 2), ("ea_select", 20, 2), ("base_select", 22, 1),
          ("force_r7", 23, 1), ("branch", 24, 1), ("HALTED", 25, 1), ("jsr", 27, 1),
          ("store_source", 30, 1)]
for name, offset, width in fields:
    cpu.tap(name, "control_word", 32, offset, width)
cpu.mux("write_index", ["dr", "seven"], "force_r7", 3)
cpu.mux("read_index", ["sr1", "dr"], "store_source", 3)
connections = {"CLK": "CLK", "RESET_N": "RESET_N", "WRITE": "ld_reg", "DR": "write_index",
               "SR1": "read_index", "SR2": "sr2", "DATA": "BUS", "A": "source_a", "B": "source_b"}
connections.update({f"R{i}": f"R{i}" for i in range(8)})
cpu.instance("register_file", rf, connections)
for bits in [5, 6, 9, 11]:
    cpu.tap(f"offset{bits}", "IR", 16, 0, bits)
    cpu.extend(f"signed{bits}", f"offset{bits}", bits)
cpu.mux("operand_b", ["source_b", "signed5"], "immediate", 16)
cpu.add("alu_add", "source_a", "operand_b")
cpu.logic("alu_and", "and", "source_a", "operand_b", width=16)
cpu.invert("alu_not", "source_a", 16)
cpu.mux("alu_result", ["alu_add", "alu_and", "alu_not", "source_a"], "alu_select", 16)
cpu.mux("offset", ["signed9", "signed6", "signed11", "source_b"], "ea_select", 16)
cpu.mux("ea_base", ["PC", "source_a"], "base_select", 16)
cpu.add("effective_address", "ea_base", "offset")
cpu.invert("jsrr", "jsr_long")
cpu.logic("jsrr_selected", "and", "jsrr", "jsr")
cpu.mux("address_or_jump", ["effective_address", "source_a"], "jsrr_selected", 16)
cpu.add("pc_plus_one", "PC", "one")
cpu.concat("trap_address", ["trap_byte", "zero8"], [8, 8])
cpu.mux("BUS", ["alu_result", "PC", "pc_plus_one", "MEM_DATA", "address_or_jump", "source_a", "boot", "trap_address"], "bus_select", 16)
cpu.logic("cc_match", "and", "CC", "dr", width=3)
cc_bits = [cpu.tap(f"cc_match{i}", "cc_match", 3, i, 1) for i in range(3)]
cpu.logic("branch_taken", "or", *cc_bits)
cpu.invert("not_branch", "branch")
cpu.logic("allow_pc", "or", "not_branch", "branch_taken")
cpu.logic("load_pc", "and", "ld_pc", "allow_pc")
for name, control_name in [("PC", "load_pc"), ("IR", "ld_ir"), ("MEM_ADDR", "ld_mar"), ("MDR", "ld_mdr")]:
    cpu.register(name, "BUS", cpu.invert(name + "_enable_n", control_name))
cpu.equal("is_zero", "BUS", 0, 16)
cpu.tap("negative", "BUS", 16, 15, 1)
cpu.invert("not_zero", "is_zero")
cpu.invert("not_negative", "negative")
cpu.logic("positive", "and", "not_zero", "not_negative")
cpu.concat("new_cc", ["positive", "is_zero", "negative"], [1, 1, 1])
cpu.register("CC", "new_cc", cpu.invert("cc_enable_n", "ld_cc"), 3)
cpu.invert("WE", "write_mem")
cpu.gate("output_enable", "buffer", 1, [("I", "input", 1, "write_mem"), ("Z", "output", 1, "OE")])
cpu.gate("chip_select", "ground", 1, [("Z", "output", 1, "CS")])
cpu.gate("memory_bus_driver", "tri_state", 16, [("I", "input", 16, "MDR"), ("E", "input", 1, "write_mem"), ("Z", "output", 16, "MEM_DATA")])
cpu.equal("display_address", "MEM_ADDR", 0xfe06, 16)
cpu.logic("TTY_WR", "and", "write_mem", "display_address")
cpu.tap("TTY_TX", "MDR", 16, 0, 8)

# Assemble a tiny directed program, plus a normal LC-3 OUT trap handler.
program = {
    0x3000: 0x5020,  # AND R0,R0,#0
    0x3001: 0x5260,  # AND R1,R1,#0
    0x3002: 0x1265,  # ADD R1,R1,#5
    0x3003: 0x1001,  # ADD R0,R0,R1
    0x3004: 0x127f,  # ADD R1,R1,#-1
    0x3005: 0x03fd,  # BRp x3003
    0x3006: 0x3019,  # ST R0,x3020 (result=15)
    0x3007: 0x2019,  # LD R0,x3021 ('H')
    0x3008: 0xf021,  # OUT via trap vector
    0x3009: 0x2018,  # LD R0,x3022 ('I')
    0x300a: 0xf021,
    0x300b: 0x2017,  # LD R0,x3023 (newline)
    0x300c: 0xf021,
    0x300d: 0xf025,  # HALT
    0x3020: 0, 0x3021: 0x48, 0x3022: 0x49, 0x3023: 0x0a,
    0x0021: 0x3100,
    0x3100: 0x2402,  # LD R2,x3103 (display address)
    0x3101: 0x7080,  # STR R0,R2,#0
    0x3102: 0xc1c0,  # RET
    0x3103: 0xfe06,
}
memory = [signal(0, 16) for _ in range(max(program) + 1)]
for address, word in program.items():
    memory[address] = signal(word, 16)

main = Module("main")
main.gate("clock", "clock", 1, [("Z", "output", 1, "CLK")], period=500)
main.gate("RESET_N", "switch", 1, [("Z", "output", 1, "RESET_N")], initial=0)
connections = {net["name"]: net["name"] for net in cpu.data["nets"] if net["port"]}
main.instance("cpu", cpu, connections)
main.gate("main_memory", "ram", 16, [("A", "input", 16, "MEM_ADDR"), ("D", "in_out", 16, "MEM_DATA"),
          ("WE", "input", 1, "WE"), ("OE", "input", 1, "OE"), ("CS", "input", 1, "CS")], address_bits=16, memory=memory)
main.const("zero", 0, 1)
main.gate("console", "tty", 8, [("TX", "input", 8, "TTY_TX"), ("WR", "input", 1, "TTY_WR"),
          ("RD", "input", 1, "zero"), ("RX", "output", 8, "console_rx"), ("READY", "output", 1, "console_ready")])
for index, (name, width, mode) in enumerate([("PC", 16, "hex"), ("IR", 16, "hex"), ("R0", 16, "hex"),
                    ("R1", 16, "hex"), ("STATE", 8, "hex"), ("CC", 3, "bar"), ("HALTED", 1, "bit")]):
    led = main.gate("watch_" + name, "led", width, [("I", "input", width, name)], led_display=mode)
    led["position"] = {"x": 120.0 + (index % 4) * 260, "y": 760.0 + (index // 4) * 120}
    led["rotation"] = 3
# Root placement leaves internal logic to its own editable modules.
for gate, position in zip(main.data["gates"][:6], [(80,120),(80,240),(370,260),(720,230),(80,390),(950,400)]):
    gate["position"] = {"x": float(position[0]), "y": float(position[1])}
comment = main.gate("instructions", "comment", 1, [])
comment["position"] = {"x": 40.0, "y": 1050.0}
comment["text"] = ("LC-3 circuit: registers, muxes, gates, RAM and microcode ROM — no CPU emulator.\n"
    "1. Run with RESET_N=0 to initialize registers. 2. Click RESET_N to 1.\n"
    "3. Run or Tab through clock cycles; probe PC, IR, R0/R1, STATE and CC.\n"
    "The program sums 1..5 (RAM[x3020]=15), prints HI and halts.\n"
    "Double-click cpu/controller/register_file to inspect the circuit modules.\n"
    "Double-click console during simulation to read output. HALTED=1 ends the program.\n"
    "RTI/reserved opcodes halt; no interrupts, protection, PSR or OS. Clock=500 ns.")

circuit = {"title": "LC-3 microcoded circuit", "root": "main",
           "modules": [main.finish(), cpu.finish(), rf.finish(), control.finish()]}
(ROOT / "examples/lc3.rgate").write_text(json.dumps({"format": "rgate", "version": 1, "circuit": circuit}, separators=(",", ":")) + "\n")
(ROOT / "examples/lc3-program.hex").write_text("\n".join(f"@{address:04X} {word:04X}" for address, word in sorted(program.items())) + "\n")
print(f"Wrote examples/lc3.rgate: {sum(len(module['gates']) for module in circuit['modules'])} primitive gates/instances")
