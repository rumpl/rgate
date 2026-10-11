#!/usr/bin/env python3
"""Build the schematic surrounding an editable Verilog LED chaser."""
import json
from circuit_builder import Module, ROOT

hdl = Module("led_chaser")
for name in ["clk", "reset", "enable", "reverse"]:
    hdl.port(name, 1, "input")
hdl.port("speed", 4, "input")
hdl.port("leds", 8, "output")
hdl.data["verilog"] = (ROOT / "examples/verilog/led-chaser.sv").read_text()

main = Module("main")
main.gate("CLOCK", "clock", 1, [("Z", "output", 1, "clk")], period=100)
for name, net, initial in [("RESET", "reset", 1), ("ENABLE", "enable", 1), ("REVERSE", "reverse", 0)]:
    main.gate(name, "switch", 1, [("Z", "output", 1, net)], initial=initial)
main.gate("SPEED", "dip", 4, [("Z", "output", 4, "speed")], initial=3)
main.instance("chaser", hdl, {name: name for name in ["clk", "reset", "enable", "reverse", "speed", "leds"]})
main.gate("LED_BAR", "led", 8, [("I", "input", 8, "leds")], led_display="bar")
main.gate("LED_HEX", "led", 8, [("I", "input", 8, "leds")], led_display="hex")
for gate, pos in zip(main.data["gates"], [(100,90), (100,180), (100,270), (100,360), (100,450), (400,270), (680,220), (680,340)]):
    gate["position"] = {"x": float(pos[0]), "y": float(pos[1])}
note = main.gate("instructions", "comment", 1, [])
note["position"] = {"x": 70.0, "y": 550.0}
note["text"] = (
    "Verilog LED chaser\n"
    "Clock-step once with RESET=1, then click RESET to 0.\n"
    "Run or clock-step to move the lit bit. ENABLE=0 pauses it.\n"
    "REVERSE changes direction. SPEED=0 moves every clock; 3 every four clocks.\n"
    "Double-click SPEED during simulation to enter a hex digit (0–F).\n"
    "Stop, select led_chaser in Modules, then Edit to change the source.\n"
    "Browse HDL signals to probe chaser.divider and watch the speed counter."
)
circuit = {"title": "Verilog LED chaser", "root": "main", "modules": [main.finish(), hdl.data]}
(ROOT / "examples/verilog-led-chaser.rgate").write_text(
    json.dumps({"format": "rgate", "version": 1, "circuit": circuit}, separators=(",", ":")) + "\n"
)
print("Generated examples/verilog-led-chaser.rgate")
