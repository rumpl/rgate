use crate::{
    Circuit, Direction, Gate, GateId, GateKind, LedDisplay, Module, Net, NetId, PinRef, Point,
    Signal, Wire,
};

/// An eight-bit ripple-carry adder with four reusable levels beneath the test bench.
pub fn hierarchical_adder() -> Circuit {
    let mut circuit = Circuit {
        title: "Hierarchical 8-bit ripple-carry adder".into(),
        ..Default::default()
    };
    circuit.modules.push(half_adder());
    circuit.modules.push(full_adder(&circuit));
    circuit
        .modules
        .push(ripple_adder(&circuit, 4, "full_adder", 1));
    circuit.modules.push(ripple_adder(&circuit, 8, "adder4", 4));
    circuit.modules[0] = test_bench(&circuit);
    circuit
}

struct Builder {
    module: Module,
}

impl Builder {
    fn new(name: &str) -> Self {
        Self {
            module: Module::new(name),
        }
    }

    fn net(&mut self, name: &str, width: u16, port: Option<Direction>) -> NetId {
        let id = self.module.next_net_id();
        let mut net = Net::new(id, name, width);
        net.port = port;
        self.module.nets.push(net);
        id
    }

    fn gate(&mut self, kind: GateKind, name: &str, x: f32, y: f32, width: u16) -> GateId {
        let id = self.module.next_gate_id();
        let mut gate = Gate::new(id, kind, Point::new(x, y));
        gate.name = name.into();
        gate.width = width;
        gate.initial = Signal::from_u64(0, width);
        gate.reset_pins();
        self.module.gates.push(gate);
        id
    }

    fn instance(
        &mut self,
        circuit: &Circuit,
        definition: &str,
        name: &str,
        x: f32,
        y: f32,
    ) -> GateId {
        let id = self.module.next_gate_id();
        let mut gate = circuit
            .module_instance(definition, id, Point::new(x, y))
            .expect("child definition exists");
        gate.name = name.into();
        self.module.gates.push(gate);
        id
    }

    fn bind(&mut self, id: GateId, pin: &str, net: NetId) {
        self.module.gate_mut(id).unwrap().pin_mut(pin).unwrap().net = Some(net);
    }

    fn partition(
        &mut self,
        kind: GateKind,
        name: &str,
        x: f32,
        y: f32,
        width: u16,
        chunk: u16,
    ) -> GateId {
        let id = self.gate(kind, name, x, y, width);
        let gate = self.module.gate_mut(id).unwrap();
        gate.config.partitions = vec![chunk; usize::from(width / chunk)];
        gate.reset_pins();
        id
    }

    fn note(&mut self, text: &str, x: f32, y: f32) {
        let id = self.gate(GateKind::Comment, "guide", x, y, 1);
        let gate = self.module.gate_mut(id).unwrap();
        gate.text = text.into();
        gate.show_name = false;
    }

    fn finish(mut self) -> Module {
        for net in self.module.nets.clone() {
            let mut pins = self
                .module
                .gates
                .iter()
                .flat_map(|gate| {
                    gate.pins
                        .iter()
                        .filter(|pin| pin.net == Some(net.id))
                        .map(move |pin| (PinRef::new(gate.id, &pin.name), pin.direction))
                })
                .collect::<Vec<_>>();
            if pins.is_empty() {
                continue;
            }
            let source_index = pins
                .iter()
                .position(|(_, direction)| *direction == Direction::Output);
            let (source, start) = if let Some(index) = source_index {
                let (pin, _) = pins.remove(index);
                let position = self.module.pin_position(&pin).unwrap();
                (Some(pin), position)
            } else {
                let first = self.module.pin_position(&pins[0].0).unwrap();
                (None, Point::new(first.x - 65., first.y))
            };
            for (branch, (target, _)) in pins.into_iter().enumerate() {
                let end = self.module.pin_position(&target).unwrap();
                let track = if start.x < end.x {
                    start.x + (end.x - start.x) * 0.5
                } else {
                    start.x + 25. + branch as f32 * 10.
                };
                self.module.wires.push(Wire {
                    id: self.module.next_wire_id(),
                    net: net.id,
                    points: vec![
                        start,
                        Point::new(track, start.y),
                        Point::new(track, end.y),
                        end,
                    ],
                    start: source.clone(),
                    end: Some(target),
                });
            }
            if net.port == Some(Direction::Output) {
                self.module.wires.push(Wire {
                    id: self.module.next_wire_id(),
                    net: net.id,
                    points: vec![start, start + Point::new(65., 0.)],
                    start: source,
                    end: None,
                });
            }
        }
        self.module
    }
}

fn ports(builder: &mut Builder, width: u16) -> [NetId; 5] {
    [
        builder.net("A", width, Some(Direction::Input)),
        builder.net("B", width, Some(Direction::Input)),
        builder.net("Cin", 1, Some(Direction::Input)),
        builder.net("Sum", width, Some(Direction::Output)),
        builder.net("Cout", 1, Some(Direction::Output)),
    ]
}

fn half_adder() -> Module {
    let mut b = Builder::new("half_adder");
    let a = b.net("A", 1, Some(Direction::Input));
    let input_b = b.net("B", 1, Some(Direction::Input));
    let sum = b.net("Sum", 1, Some(Direction::Output));
    let carry = b.net("Carry", 1, Some(Direction::Output));
    let xor = b.gate(GateKind::Xor, "sum_xor", 220., 100., 1);
    let and = b.gate(GateKind::And, "carry_and", 220., 190., 1);
    for gate in [xor, and] {
        b.bind(gate, "I0", a);
        b.bind(gate, "I1", input_b);
    }
    b.bind(xor, "Z", sum);
    b.bind(and, "Z", carry);
    b.note(
        "HALF ADDER — basic gates only\nSum = A XOR B; Carry = A AND B",
        60.,
        270.,
    );
    b.finish()
}

fn full_adder(circuit: &Circuit) -> Module {
    let mut b = Builder::new("full_adder");
    let [a, input_b, cin, sum, cout] = ports(&mut b, 1);
    let partial = b.net("A_xor_B", 1, None);
    let carry_ab = b.net("carry_AB", 1, None);
    let carry_cin = b.net("carry_Cin", 1, None);
    let first = b.instance(circuit, "half_adder", "half_AB", 220., 110.);
    let second = b.instance(circuit, "half_adder", "half_Cin", 440., 110.);
    let or = b.gate(GateKind::Or, "carry_or", 600., 230., 1);
    for (gate, pin, net) in [
        (first, "A", a),
        (first, "B", input_b),
        (first, "Sum", partial),
        (first, "Carry", carry_ab),
        (second, "A", partial),
        (second, "B", cin),
        (second, "Sum", sum),
        (second, "Carry", carry_cin),
        (or, "I0", carry_ab),
        (or, "I1", carry_cin),
        (or, "Z", cout),
    ] {
        b.bind(gate, pin, net);
    }
    b.note("FULL ADDER — two half adders + OR\nDouble-click either half-adder to see XOR and AND gates.", 70., 320.);
    b.finish()
}

fn ripple_adder(circuit: &Circuit, width: u16, child: &str, chunk: u16) -> Module {
    let mut b = Builder::new(&format!("adder{width}"));
    let [a, input_b, cin, sum, cout] = ports(&mut b, width);
    let count = width / chunk;
    let middle_y = 90. + f32::from(count - 1) * 70.;
    let split_a = b.partition(GateKind::Splitter, "split_A", 160., middle_y, width, chunk);
    let split_b = b.partition(GateKind::Splitter, "split_B", 320., middle_y, width, chunk);
    let join = b.partition(GateKind::Concat, "join_Sum", 690., middle_y, width, chunk);
    b.bind(split_a, "I", a);
    b.bind(split_b, "I", input_b);
    b.bind(join, "Z", sum);
    let mut carry = cin;
    for index in 0..count {
        let low = b.net(&format!("A_{index}"), chunk, None);
        let high = b.net(&format!("B_{index}"), chunk, None);
        let result = b.net(&format!("Sum_{index}"), chunk, None);
        let next_carry = if index + 1 == count {
            cout
        } else {
            b.net(&format!("carry_{}", (index + 1) * chunk), 1, None)
        };
        let instance = b.instance(
            circuit,
            child,
            &format!("stage{index}"),
            510.,
            90. + f32::from(index) * 140.,
        );
        for (gate, pin, net) in [
            (split_a, format!("Z{index}"), low),
            (split_b, format!("Z{index}"), high),
            (join, format!("I{index}"), result),
            (instance, "A".into(), low),
            (instance, "B".into(), high),
            (instance, "Cin".into(), carry),
            (instance, "Sum".into(), result),
            (instance, "Cout".into(), next_carry),
        ] {
            b.bind(gate, &pin, net);
        }
        carry = next_carry;
    }
    b.note(&format!("{width}-BIT RIPPLE ADDER — {count} x {child}\nStage 0 is least significant. Carry flows from top to bottom.\nSplitters/concatenation only route bits; all arithmetic is in the child modules."), 80., 100. + f32::from(count) * 140.);
    b.finish()
}

fn test_bench(circuit: &Circuit) -> Module {
    let mut b = Builder::new("main");
    let a = b.net("A", 8, None);
    let input_b = b.net("B", 8, None);
    let cin = b.net("Cin", 1, None);
    let sum = b.net("Sum", 8, None);
    let cout = b.net("Cout", 1, None);
    let input_a = b.gate(GateKind::Dip, "A", 100., 90., 8);
    let input_b_gate = b.gate(GateKind::Dip, "B", 100., 190., 8);
    let input_cin = b.gate(GateKind::Switch, "Cin", 100., 290., 1);
    for (gate, value) in [(input_a, 0x35), (input_b_gate, 0x27)] {
        b.module.gate_mut(gate).unwrap().initial = Signal::from_u64(value, 8);
    }
    let adder = b.instance(circuit, "adder8", "adder", 370., 190.);
    let result = b.gate(GateKind::Led, "Sum_hex", 620., 160., 8);
    b.module.gate_mut(result).unwrap().config.led_display = LedDisplay::Hex;
    b.module.gate_mut(result).unwrap().rotation = 3;
    let overflow = b.gate(GateKind::Led, "Cout", 620., 230., 1);
    b.module.gate_mut(overflow).unwrap().rotation = 3;
    for (gate, pin, net) in [
        (input_a, "Z", a),
        (input_b_gate, "Z", input_b),
        (input_cin, "Z", cin),
        (adder, "A", a),
        (adder, "B", input_b),
        (adder, "Cin", cin),
        (adder, "Sum", sum),
        (adder, "Cout", cout),
        (result, "I", sum),
        (overflow, "I", cout),
    ] {
        b.bind(gate, pin, net);
    }
    b.note("8-BIT ADDER — modules inside modules\nmain -> adder8 -> adder4 -> full_adder -> half_adder -> XOR / AND\n\nInitial: 35 + 27 + 0 = 5C (hex). Cout lights on overflow.\nRun, click A/B DIP inputs to increment; click Cin to toggle carry-in.\nEdit input values via Properties. Double-click module blocks to explore.", 50., 370.);
    b.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_adder_is_valid_and_arithmetic_uses_only_basic_gates() {
        let circuit = hierarchical_adder();
        circuit.validate().unwrap();
        assert_eq!(circuit.modules.len(), 5);
        for name in ["half_adder", "full_adder", "adder4", "adder8"] {
            assert!(
                circuit
                    .module(name)
                    .unwrap()
                    .gates
                    .iter()
                    .all(|gate| matches!(
                        gate.kind,
                        GateKind::Xor
                            | GateKind::And
                            | GateKind::Or
                            | GateKind::Module(_)
                            | GateKind::Splitter
                            | GateKind::Concat
                            | GateKind::Comment
                    ))
            );
        }
        let rows = circuit.hierarchy_rows(&Default::default());
        assert!(
            rows.iter()
                .any(|row| row.path == "main/adder/stage1/stage3/half_Cin" && row.depth == 4)
        );
    }
}
