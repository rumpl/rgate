#!/usr/bin/env python3
"""Generate original editable RGate learning circuits, not runtime emulators."""
import json
from circuit_builder import Module, ROOT


def save(module, title, filename):
    circuit = {"title": title, "root": "main", "modules": [module.finish()]}
    (ROOT / "examples" / filename).write_text(
        json.dumps({"format": "rgate", "version": 1, "circuit": circuit}, separators=(",", ":")) + "\n"
    )


def controls(module):
    module.gate("CLK", "clock", 1, [("Z", "output", 1, "CLK")], period=100)
    module.gate("RESET_N", "switch", 1, [("Z", "output", 1, "RESET_N")], initial=0)
    module.const("zero", 0, 1)


counter = Module("main")
controls(counter)
counter.gate("ENABLE_N", "switch", 1, [("Z", "output", 1, "ENABLE_N")], initial=0)
counter.gate("STEP", "dip", 8, [("Z", "output", 8, "step")], initial=1)
counter.add("next_count", "count", "step", 8)
counter.register("count", "next_count", "ENABLE_N", 8)
counter.gate("COUNT", "led", 8, [("I", "input", 8, "count")], led_display="hex")
note = counter.gate("instructions", "comment", 1, [])
note["position"] = {"x": 80.0, "y": 520.0}
note["text"] = "8-bit adjustable counter. Start in reset, then RESET_N -> 1.\nENABLE_N=0 counts; 1 holds. STEP selects increment (hex).\nCOUNT wraps modulo 256; probe CLK/count/next_count."
save(counter, "Adjustable 8-bit counter", "adjustable-counter.rgate")

shift = Module("main")
controls(shift)
shift.gate("DATA", "switch", 1, [("Z", "output", 1, "DATA")], initial=0)
shift.tap("low_seven", "history", 8, 0, 7)
shift.concat("next_history", ["DATA", "low_seven"], [1, 7])
shift.register("history", "next_history", "zero", 8)
shift.gate("HISTORY", "led", 8, [("I", "input", 8, "history")], led_display="bar")
shift.gate("HEX", "led", 8, [("I", "input", 8, "history")], led_display="hex")
note = shift.gate("instructions", "comment", 1, [])
note["position"] = {"x": 80.0, "y": 520.0}
note["text"] = "Serial shift register. Start in reset, then RESET_N -> 1.\nDATA is captured into bit 0 on rising CLK. Old bits shift left.\nChange DATA and clock-step; inspect the bar and hex history."
save(shift, "Serial input shift register", "serial-shift-register.rgate")

parity = Module("main")
parity.gate("DATA", "dip", 8, [("Z", "output", 8, "data")], initial=0)
parity.gate("parity", "reduce_xor", 8, [("I", "input", 8, "data"), ("Z", "output", 1, "odd")])
parity.invert("even", "odd")
parity.gate("BITS", "led", 8, [("I", "input", 8, "data")], led_display="bar")
parity.gate("ODD", "led", 1, [("I", "input", 1, "odd")])
parity.gate("EVEN", "led", 1, [("I", "input", 1, "even")])
note = parity.gate("instructions", "comment", 1, [])
note["position"] = {"x": 80.0, "y": 520.0}
note["text"] = "8-bit parity checker. Double-click DATA during simulation to set hex.\nODD=1 when an odd number of data bits are high; EVEN is its inverse.\nTry 00, 01, 03, 7F, FF and probe the reduction XOR."
save(parity, "8-bit parity checker", "parity-checker.rgate")
print("Generated adjustable-counter, serial-shift-register, and parity-checker examples")
