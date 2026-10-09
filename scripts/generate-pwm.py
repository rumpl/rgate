#!/usr/bin/env python3
"""Generate an editable PWM circuit counterpart to Project F's PWM HDL."""
import json
from circuit_builder import Module, ROOT

pwm = Module("PwmDimmer")
for name, width in [("CLK", 1), ("RESET_N", 1), ("duty", 8)]:
    pwm.port(name, width, "input")
pwm.port("led", 1, "output")
pwm.port("count", 8, "output")
pwm.const("zero", 0, 1)
pwm.const("one", 1, 8)
pwm.const("carry_one", 1, 1)
pwm.register("count", "next_count", "zero", 8)
pwm.add("next_count", "count", "one", 8)
pwm.invert("not_duty", "duty", 8)
# Unsigned subtraction: carry from count + ~duty + 1 means count >= duty.
pwm.gate("unsigned_compare", "add", 8, [
    ("A", "input", 8, "count"), ("B", "input", 8, "not_duty"),
    ("CI", "input", 1, "carry_one"), ("S", "output", 8, "difference"),
    ("CO", "output", 1, "greater_or_equal")])
pwm.invert("led", "greater_or_equal")

main = Module("main")
main.gate("pixel_clock", "clock", 1, [("Z", "output", 1, "CLK")], period=100)
main.gate("RESET_N", "switch", 1, [("Z", "output", 1, "RESET_N")], initial=0)
main.gate("DUTY", "dip", 8, [("Z", "output", 8, "duty")], initial=128)
main.instance("dimmer", pwm, {name: name for name in ["CLK", "RESET_N", "duty", "led", "count"]})
main.gate("PWM_LED", "led", 1, [("I", "input", 1, "led")])
main.gate("COUNTER", "led", 8, [("I", "input", 8, "count")], led_display="hex")
main.gate("DUTY_VALUE", "led", 8, [("I", "input", 8, "duty")], led_display="decimal")
for gate, pos in zip(main.data["gates"], [(100,100),(100,230),(100,360),(360,230),(620,160),(620,300),(320,440)]):
    gate["position"] = {"x": float(pos[0]), "y": float(pos[1])}
note = main.gate("instructions", "comment", 1, [])
note["position"] = {"x": 80.0, "y": 540.0}
note["text"] = ("PWM LED dimmer — ordinary register, adders and NOT gates.\n"
    "Start simulation, let reset settle, then set RESET_N to 1.\n"
    "Change DUTY (0–255); probe led and count in Waveforms.\n"
    "Duty fraction = DUTY/256: 0 off, 128 half, 255 almost always on.\n"
    "100 ns clock; PWM period = 25600 ns. LED shows digital state, not averaged brightness.\n"
    "Counterpart of examples/verilog/projectf/pwm.sv; not automatic HDL synthesis.")
circuit = {"title": "PWM LED dimmer", "root": "main", "modules": [main.finish(), pwm.finish()]}
(ROOT / "examples/pwm-dimmer.rgate").write_text(json.dumps({"format": "rgate", "version": 1, "circuit": circuit}, separators=(",", ":")) + "\n")
print("Generated examples/pwm-dimmer.rgate")

# Explicit source-module version: the RTL itself is editable in the UI.
hdl_main = Module("main")
hdl_main.gate("clock", "clock", 1, [("Z", "output", 1, "clk")], period=100)
hdl_main.gate("DUTY", "dip", 8, [("Z", "output", 8, "duty")], initial=128)
hdl = Module("pwm")
hdl.port("clk", 1, "input")
hdl.port("duty", 8, "input")
hdl.port("pwm_out", 1, "output")
hdl.net("cnt", 8)
hdl.data["verilog"] = (ROOT / "examples/verilog/projectf/pwm.sv").read_text()
hdl_main.instance("dimmer", hdl, {name:name for name in ["clk", "duty", "pwm_out"]})
hdl_main.gate("PWM_LED", "led", 1, [("I", "input", 1, "pwm_out")])
for gate,pos in zip(hdl_main.data["gates"],[(100,100),(100,250),(350,180),(580,180)]):
    gate["position"]={"x":float(pos[0]),"y":float(pos[1])}
note=hdl_main.gate("source_instructions", "comment", 1, [])
note["position"]={"x":80.0,"y":360.0}
note["text"]="Editable Verilog PWM. Select pwm in Modules, then Module → Edit Verilog source.\nRun from main using Icarus. Double-click DUTY for live input; probe pwm_out.\nDouble-click dimmer during simulation to inspect cnt without resetting."
(ROOT / "examples/pwm-verilog.rgate").write_text(json.dumps({"format":"rgate","version":1,"circuit":{"title":"Editable Verilog PWM","root":"main","modules":[hdl_main.finish(),hdl.data]}},separators=(",",":"))+"\n")
print("Generated examples/pwm-verilog.rgate")
