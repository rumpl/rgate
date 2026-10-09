#!/usr/bin/env python3
"""Build an LC-3 schematic and ROM contents. This script does not simulate a CPU."""
import json
from schematic_routing import Router
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def signal(value, width):
    return {"bits": ["High" if value & (1 << bit) else "Low" for bit in range(width)]}


class Module:
    def __init__(self, name):
        self.data = {"name": name, "gates": [], "nets": [], "wires": []}
        self.net_names = {}

    def net(self, name, width, port=None):
        if name in self.net_names:
            net = self.net_names[name]
            assert net["width"] == width, (name, width, net["width"])
            if port:
                net["port"] = port
            return net["id"]
        net = {"id": len(self.data["nets"]) + 1, "name": name, "width": width,
               "show_name": True, "port": port}
        self.data["nets"].append(net)
        self.net_names[name] = net
        return net["id"]

    def gate(self, name, kind, width, pins, **config):
        index = len(self.data["gates"])
        position = {"x": 140.0 + (index % 5) * 220, "y": 100.0 + (index // 5) * 165}
        gate = {"id": index + 1, "name": name, "kind": kind, "position": position,
                "rotation": 0, "width": width, "input_count": config.pop("inputs", 2),
                "delay": 1, "period": config.pop("period", 500),
                "initial": signal(config.pop("initial", 0), width), "pins": [],
                "text": "", "show_name": True, "config": config}
        left = sum(direction == "input" for _, direction, _, _ in pins)
        right = len(pins) - left
        l = r = 0
        for pin, direction, bits, net in pins:
            if direction == "input":
                offset = {"x": -45.0, "y": (l - (left - 1) / 2) * 12}
                l += 1
            else:
                offset = {"x": 45.0, "y": (r - (right - 1) / 2) * 12}
                r += 1
            gate["pins"].append({"name": pin, "direction": direction, "width": bits,
                                 "offset": offset, "net": self.net(net, bits)})
        # Native primitive symbols require their actual electrical pin locations.
        offsets = {
            "not": {"I": (-6, 0), "Z": (10, 0)},
            "buffer": {"I": (-6, 0), "Z": (10, 0)},
            "tri_state": {"I": (-6, 0), "E": (2, -5), "Z": (10, 0)},
            "switch": {"Z": (17, 0)}, "dip": {"Z": (0, 10)},
            "clock": {"Z": (13, 0)}, "ground": {"Z": (0, -6)},
            "led": {"I": (0, 7)},
            "register": {"D": (0, -10), "Q": (0, 11), "CK": (-37, 0), "EN": (39, 5), "CLR": (39, -5)},
            "add": {"A": (-16, -16), "B": (16, -16), "CI": (24, -2), "CO": (-24, -2), "S": (0, 13)},
        }
        if isinstance(kind, str):
            standard = offsets.get(kind, {})
            if kind in ("and", "or", "xor"):
                standard = {f"I{i}": (-11, (i - (gate["input_count"] - 1) / 2) * 5) for i in range(gate["input_count"])}
                standard["Z"] = (10, 0)
            if kind == "mux":
                standard = {f"I{i}": (-29 + 58 * (i + 1) / (gate["input_count"] + 1), -16) for i in range(gate["input_count"])}
                standard.update({"S": (-23, 0), "Z": (0, 13)})
            for pin in gate["pins"]:
                if pin["name"] in standard:
                    x, y = standard[pin["name"]]
                    pin["offset"] = {"x": float(x), "y": float(y)}
        self.data["gates"].append(gate)
        return gate

    def const(self, name, value, width):
        self.gate("constant_" + name, "dip", width, [("Z", "output", width, name)], initial=value)
        return name

    def logic(self, name, kind, *inputs, width=1):
        self.gate(name, kind, width, [(f"I{i}", "input", width, net) for i, net in enumerate(inputs)] +
                  [("Z", "output", width, name)], inputs=len(inputs))
        return name

    def invert(self, name, net, width=1):
        self.gate(name, "not", width, [("I", "input", width, net), ("Z", "output", width, name)])
        return name

    def mux(self, name, inputs, select, width):
        bits = (len(inputs) - 1).bit_length()
        self.gate(name, "mux", width, [(f"I{i}", "input", width, net) for i, net in enumerate(inputs)] +
                  [("S", "input", bits, select), ("Z", "output", width, name)], inputs=len(inputs))
        return name

    def tap(self, name, net, width, offset, bits):
        self.gate(name, "tap", width, [("I", "input", width, net), ("Z", "output", bits, name)],
                  tap_offset=offset, tap_width=bits)
        return name

    def concat(self, name, nets, widths):
        self.gate(name, "concat", sum(widths), [(f"I{i}", "input", bits, net)
                  for i, (net, bits) in enumerate(zip(nets, widths))] +
                  [("Z", "output", sum(widths), name)], partitions=widths)
        return name

    def extend(self, name, net, bits):
        sign = self.tap(name + "_sign", net, bits, bits - 1, 1)
        return self.concat(name, [net] + [sign] * (16 - bits), [bits] + [1] * (16 - bits))

    def add(self, name, a, b, width=16):
        self.gate(name, "add", width, [("A", "input", width, a), ("B", "input", width, b),
                  ("CI", "input", 1, "zero"), ("S", "output", width, name),
                  ("CO", "output", 1, name + "_carry")])
        return name

    def register(self, name, data, enable, width=16):
        self.gate(name, "register", width, [("D", "input", width, data), ("EN", "input", 1, enable),
                  ("CLR", "input", 1, "RESET_N"), ("CK", "input", 1, "CLK"),
                  ("Q", "output", width, name)])
        return name

    def equal(self, name, data, value, width):
        inverted = self.logic(name + "_xor", "xor", data, self.const(name + "_constant", value, width), width=width)
        bits = [self.tap(name + f"_bit{i}", inverted, width, i, 1) for i in range(width)]
        return self.invert(name, self.logic(name + "_any", "or", *bits))

    def instance(self, name, definition, connections):
        pins = [(net["name"], net["port"], net["width"], connections[net["name"]])
                for net in definition.data["nets"] if net["port"]]
        return self.gate(name, {"module": definition.data["name"]}, 1, pins)

    def port(self, name, width, direction):
        self.net(name, width, direction)

    def finish(self):
        # Orthogonal geometry is purely visual; connectivity is carried by the nets.
        router = Router(self.data["gates"])
        for net in self.data["nets"]:
            connected = [(gate, pin) for gate in self.data["gates"] for pin in gate["pins"] if pin["net"] == net["id"]]
            drivers = [(gate, pin) for gate, pin in connected if pin["direction"] != "input"]
            if not drivers:
                continue
            source, source_pin = drivers[0]
            for gate, pin in connected:
                if gate is source and pin is source_pin:
                    continue
                self.data["wires"].append({"id": len(self.data["wires"]) + 1, "net": net["id"],
                    "points": router.route(source, source_pin, gate, pin),
                    "start": {"gate": source["id"], "pin": source_pin["name"]},
                    "end": {"gate": gate["id"], "pin": pin["name"]}})
        return self.data


