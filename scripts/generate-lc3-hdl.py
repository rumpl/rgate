#!/usr/bin/env python3
"""Build a schematic LC-3 wrapper around editable HDL base modules."""
import json
from circuit_builder import Module, ROOT


def source_module(name, filename, inputs, outputs):
    module = Module(name)
    for direction, ports in [("input", inputs), ("output", outputs)]:
        for port, width in ports:
            module.port(port, width, direction)
    module.data["verilog"] = (ROOT / "examples/verilog/lc3" / filename).read_text()
    return module


registers = [(f"R{i}", 16) for i in range(8)]
rf = source_module("LC3RegisterFileHDL", "register-file.sv",
    [("CLK", 1), ("RESET_N", 1), ("WRITE", 1), ("DR", 3), ("SR1", 3), ("SR2", 3), ("DATA", 16)],
    [("A", 16), ("B", 16)] + registers)
control = source_module("LC3ControlHDL", "control.sv",
    [("CLK", 1), ("RESET_N", 1), ("OPCODE", 4), ("HALT_TRAP", 1)],
    [("STATE", 8), ("CONTROL", 32)])
datapath = source_module("LC3DatapathHDL", "datapath.sv",
    [("CLK", 1), ("RESET_N", 1), ("CONTROL", 32), ("MEM_READ", 16), ("A", 16), ("B", 16)],
    [("DR", 3), ("SR1", 3), ("SR2", 3), ("WRITE_REG", 1), ("BUS", 16), ("PC", 16),
     ("IR", 16), ("MEM_ADDR", 16), ("MEM_WRITE", 16), ("WRITE_MEM", 1), ("CC", 3),
     ("OPCODE", 4), ("HALT_TRAP", 1), ("HALTED", 1)])
memory = source_module("LC3MemoryHDL", "memory.sv",
    [("CLK", 1), ("RESET_N", 1), ("ADDRESS", 16), ("WRITE_DATA", 16), ("WRITE", 1)],
    [("READ_DATA", 16), ("RESULT", 16), ("CONSOLE", 24), ("TX", 8), ("TX_COUNT", 16)])

cpu = Module("LC3HDL")
for name, width in [("CLK", 1), ("RESET_N", 1), ("MEM_READ", 16)]:
    cpu.port(name, width, "input")
for name, width in [("MEM_ADDR", 16), ("MEM_WRITE", 16), ("WRITE_MEM", 1), ("PC", 16),
                    ("IR", 16), ("STATE", 8), ("CC", 3), ("HALTED", 1)] + registers:
    cpu.port(name, width, "output")
for name, definition, overrides in [
    ("controller", control, {}),
    ("datapath", datapath, {}),
    ("register_file", rf, {"WRITE": "WRITE_REG", "DATA": "BUS"}),
]:
    connections = {net["name"]: overrides.get(net["name"], net["name"])
                   for net in definition.data["nets"] if net["port"]}
    gate = cpu.instance(name, definition, connections)
    gate["position"] = {"x": {"controller": 220., "datapath": 560., "register_file": 900.}[name], "y": 280.}
note = cpu.gate("architecture", "comment", 1, [])
note["position"] = {"x": 100., "y": 500.}
note["text"] = ("LC-3 HDL base modules connected by schematic nets.\n"
    "CONTROL drives the datapath; BUS writes the register file.\n"
    "A/B are asynchronous register reads. Memory is outside the CPU.\n"
    "Double-click a block in Edit to change its SystemVerilog source.\n"
    "During simulation use Browse HDL signals for internal observations.")

main = Module("main")
main.gate("clock", "clock", 1, [("Z", "output", 1, "CLK")], period=500)
main.gate("RESET_N", "switch", 1, [("Z", "output", 1, "RESET_N")], initial=0)
main.instance("cpu", cpu, {net["name"]: net["name"] for net in cpu.data["nets"] if net["port"]})
main.instance("main_memory", memory, {
    "CLK": "CLK", "RESET_N": "RESET_N", "ADDRESS": "MEM_ADDR",
    "WRITE_DATA": "MEM_WRITE", "WRITE": "WRITE_MEM", "READ_DATA": "MEM_READ",
    "RESULT": "RESULT", "CONSOLE": "CONSOLE", "TX": "TX", "TX_COUNT": "TX_COUNT",
})
for gate, position in zip(main.data["gates"], [(80, 100), (80, 200), (360, 280), (740, 220)]):
    gate["position"] = {"x": float(position[0]), "y": float(position[1])}
for index, (name, width, mode) in enumerate([
    ("PC", 16, "hex"), ("IR", 16, "hex"), ("R0", 16, "hex"), ("R1", 16, "hex"),
    ("STATE", 8, "hex"), ("CC", 3, "bar"), ("HALTED", 1, "bit"), ("RESULT", 16, "hex"),
    ("CONSOLE", 24, "hex"), ("TX", 8, "hex"), ("TX_COUNT", 16, "decimal"),
]):
    led = main.gate("watch_" + name, "led", width, [("I", "input", width, name)], led_display=mode)
    led["position"] = {"x": 120. + (index % 4) * 240, "y": 650. + (index // 4) * 110}
    led["rotation"] = 3
note = main.gate("instructions", "comment", 1, [])
note["position"] = {"x": 40., "y": 1030.}
note["text"] = ("LC-3 with HDL base modules (desktop / Icarus).\n"
    "Clock-step once with RESET_N=0, then click RESET_N to 1.\n"
    "Run or Tab through 500 ns cycles; HALTED=1 finishes the program.\n"
    "Expected: RESULT=000F, CONSOLE=48490A (HI + newline), TX_COUNT=3.\n"
    "Open cpu to see control/datapath/register-file HDL blocks.\n"
    "Memory contains the same lc3-program.hex image as the schematic CPU.\n"
    "Stop and edit LC3MemoryHDL source to change the embedded program.\n"
    "Console is an HDL byte recorder, not the schematic TTY peripheral.\n"
    "RTI/reserved opcodes halt; no interrupts, PSR, protection or OS.")

circuit = {"title": "LC-3 with HDL base modules", "root": "main", "modules": [
    main.finish(), cpu.finish(), rf.data, control.data, datapath.data, memory.data,
]}
(ROOT / "examples/lc3-hdl.rgate").write_text(
    json.dumps({"format": "rgate", "version": 1, "circuit": circuit}, separators=(",", ":")) + "\n")
print("Generated examples/lc3-hdl.rgate")
