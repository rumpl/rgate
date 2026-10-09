mod adder;
pub use adder::hierarchical_adder;

use crate::{Circuit, Gate, GateId, GateKind, Module, Net, NetId, PinRef, Point, Signal, Wire};

pub fn full_adder() -> Circuit {
    let mut module = Module::new("main");
    let mut gates = [
        (GateKind::Switch, "A", Point::new(70.0, 100.0)),
        (GateKind::Switch, "B", Point::new(70.0, 160.0)),
        (GateKind::Switch, "Cin", Point::new(70.0, 260.0)),
        (GateKind::Xor, "xor_ab", Point::new(220.0, 130.0)),
        (GateKind::Xor, "xor_sum", Point::new(400.0, 110.0)),
        (GateKind::And, "and_ab", Point::new(220.0, 210.0)),
        (GateKind::And, "and_carry", Point::new(400.0, 230.0)),
        (GateKind::Or, "or_cout", Point::new(520.0, 270.0)),
        (GateKind::Led, "Sum", Point::new(590.0, 110.0)),
        (GateKind::Led, "Cout", Point::new(590.0, 270.0)),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (kind, name, position))| {
        let mut gate = Gate::new(GateId(index as u64 + 1), kind, position);
        gate.name = name.into();
        if gate.kind == GateKind::Led {
            gate.rotation = 3;
        }
        gate
    })
    .collect::<Vec<_>>();
    gates[0].initial = Signal::from_u64(1, 1);
    module.gates = gates;
    let connections: &[(&str, &[(usize, &str)])] = &[
        ("A", &[(0, "Z"), (3, "I0"), (5, "I0")]),
        ("B", &[(1, "Z"), (3, "I1"), (5, "I1")]),
        ("Cin", &[(2, "Z"), (4, "I0"), (6, "I0")]),
        ("AxorB", &[(3, "Z"), (4, "I1"), (6, "I1")]),
        ("AB", &[(5, "Z"), (7, "I0")]),
        ("carry", &[(6, "Z"), (7, "I1")]),
        ("Sum", &[(4, "Z"), (8, "I")]),
        ("Cout", &[(7, "Z"), (9, "I")]),
    ];
    for (index, (name, pins)) in connections.iter().enumerate() {
        let net = NetId(index as u64 + 1);
        module.nets.push(Net::new(net, *name, 1));
        for &(gate, pin) in *pins {
            module.gates[gate].pin_mut(pin).unwrap().net = Some(net);
        }
        let (source_index, source_pin) = pins[0];
        let source = PinRef::new(module.gates[source_index].id, source_pin);
        let start = module.pin_position(&source).unwrap();
        for (branch, &(target_index, target_pin)) in pins[1..].iter().enumerate() {
            let target = PinRef::new(module.gates[target_index].id, target_pin);
            let end = module.pin_position(&target).unwrap();
            let elbow_x = start.x + 28.0 + branch as f32 * 18.0;
            module.wires.push(Wire {
                id: module.next_wire_id(),
                net,
                points: vec![
                    start,
                    Point::new(elbow_x, start.y),
                    Point::new(elbow_x, end.y),
                    end,
                ],
                start: Some(source.clone()),
                end: Some(target),
            });
        }
    }
    let mut title = Gate::new(
        module.next_gate_id(),
        GateKind::Comment,
        Point::new(45.0, 35.0),
    );
    title.name = "title".into();
    title.text = "1-bit full adder".into();
    module.gates.push(title);
    let mut note = Gate::new(
        module.next_gate_id(),
        GateKind::Comment,
        Point::new(45.0, 330.0),
    );
    note.name = "instructions".into();
    note.text = "Sum = A ⊕ B ⊕ Cin\nCout = A·B + Cin·(A ⊕ B)\n\nPress Play, then click the switches.\nDouble-click a wire to add a waveform probe.".into();
    module.gates.push(note);
    Circuit {
        title: "Full adder".into(),
        root: "main".into(),
        modules: vec![module],
    }
}

pub fn clocked_flip_flop() -> Circuit {
    let mut module = Module::new("main");
    let mut clock = Gate::new(GateId(1), GateKind::Clock, Point::new(100.0, 160.0));
    clock.name = "clock".into();
    let mut data = Gate::new(GateId(2), GateKind::Switch, Point::new(100.0, 90.0));
    data.name = "D".into();
    data.initial = Signal::from_u64(1, 1);
    let mut ff = Gate::new(GateId(3), GateKind::Dff, Point::new(250.0, 90.0));
    ff.name = "dff".into();
    let mut led = Gate::new(GateId(4), GateKind::Led, Point::new(380.0, 85.0));
    led.name = "Q".into();
    led.rotation = 3;
    module.gates = vec![clock, data, ff, led];
    for (index, (name, source, target)) in [
        (
            "CK",
            PinRef::new(GateId(1), "Z"),
            PinRef::new(GateId(3), "CK"),
        ),
        (
            "D",
            PinRef::new(GateId(2), "Z"),
            PinRef::new(GateId(3), "D"),
        ),
        (
            "Q",
            PinRef::new(GateId(3), "Q"),
            PinRef::new(GateId(4), "I"),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let net = NetId(index as u64 + 1);
        module.nets.push(Net::new(net, name, 1));
        for reference in [&source, &target] {
            module
                .gate_mut(reference.gate)
                .unwrap()
                .pin_mut(&reference.pin)
                .unwrap()
                .net = Some(net);
        }
        let a = module.pin_position(&source).unwrap();
        let b = module.pin_position(&target).unwrap();
        module.wires.push(Wire {
            id: module.next_wire_id(),
            net,
            points: vec![a, Point::new(b.x, a.y), b],
            start: Some(source),
            end: Some(target),
        });
    }
    Circuit {
        title: "Clocked D flip-flop".into(),
        root: "main".into(),
        modules: vec![module],
    }
}

/// Two independent instances of a reusable inverter, wrapped in a second module.
pub fn hierarchical_inverters() -> Circuit {
    use crate::{Direction, Module, Net};
    let mut child = Module::new("inverter");
    let mut input = Net::new(NetId(1), "A", 1);
    input.port = Some(Direction::Input);
    let mut output = Net::new(NetId(2), "Y", 1);
    output.port = Some(Direction::Output);
    child.nets = vec![input, output];
    let mut inverter = Gate::new(GateId(1), GateKind::Not, Point::new(100.0, 100.0));
    inverter.name = "inv".into();
    inverter.pin_mut("I").unwrap().net = Some(NetId(1));
    inverter.pin_mut("Z").unwrap().net = Some(NetId(2));
    child.gates.push(inverter);
    let mut circuit = Circuit {
        title: "Hierarchical inverters".into(),
        ..Default::default()
    };
    circuit.modules.push(child.clone());
    let mut wrapper = Module::new("wrapped_inverter");
    wrapper.nets = child.nets.clone();
    let mut nested = circuit
        .module_instance("inverter", GateId(1), Point::new(180.0, 100.0))
        .unwrap();
    nested.name = "inner".into();
    nested.pin_mut("A").unwrap().net = Some(NetId(1));
    nested.pin_mut("Y").unwrap().net = Some(NetId(2));
    wrapper.gates.push(nested);
    circuit.modules.push(wrapper);
    let mut main = Module::new("main");
    for (index, value) in [(0u64, 0), (1, 1)] {
        let input_net = NetId(index * 2 + 1);
        let output_net = NetId(index * 2 + 2);
        main.nets
            .push(Net::new(input_net, format!("input{index}"), 1));
        main.nets
            .push(Net::new(output_net, format!("output{index}"), 1));
        let mut switch = Gate::new(
            GateId(index * 3 + 1),
            GateKind::Switch,
            Point::new(80.0, 100.0 + index as f32 * 120.0),
        );
        switch.name = format!("switch{index}");
        switch.initial = Signal::from_u64(value, 1);
        switch.pin_mut("Z").unwrap().net = Some(input_net);
        let mut instance = circuit
            .module_instance(
                "wrapped_inverter",
                GateId(index * 3 + 2),
                Point::new(250.0, switch.position.y),
            )
            .unwrap();
        instance.name = format!("u{index}");
        instance.pin_mut("A").unwrap().net = Some(input_net);
        instance.pin_mut("Y").unwrap().net = Some(output_net);
        let mut led = Gate::new(
            GateId(index * 3 + 3),
            GateKind::Led,
            Point::new(420.0, switch.position.y),
        );
        led.name = format!("LED{index}");
        led.rotation = 3;
        led.pin_mut("I").unwrap().net = Some(output_net);
        let source = PinRef::new(switch.id, "Z");
        let target = PinRef::new(instance.id, "A");
        main.gates.extend([switch, instance, led]);
        for (net, a, b) in [
            (input_net, source, target),
            (
                output_net,
                PinRef::new(GateId(index * 3 + 2), "Y"),
                PinRef::new(GateId(index * 3 + 3), "I"),
            ),
        ] {
            main.wires.push(Wire {
                id: main.next_wire_id(),
                net,
                points: vec![
                    main.pin_position(&a).unwrap(),
                    main.pin_position(&b).unwrap(),
                ],
                start: Some(a),
                end: Some(b),
            });
        }
    }
    circuit.modules[0] = main;
    circuit
}

/// Editable bus partitioning and asynchronous ROM example.
pub fn bus_memory() -> Circuit {
    let mut module = Module::new("main");
    for (index, kind, name, position) in [
        (1, GateKind::Dip, "address", Point::new(70.0, 100.0)),
        (2, GateKind::Splitter, "split", Point::new(220.0, 100.0)),
        (3, GateKind::Concat, "join", Point::new(390.0, 100.0)),
        (4, GateKind::Rom, "lookup", Point::new(560.0, 100.0)),
        (5, GateKind::Led, "data", Point::new(710.0, 100.0)),
        (6, GateKind::Ground, "enable", Point::new(440.0, 210.0)),
    ] {
        let mut gate = Gate::new(GateId(index), kind, position);
        gate.name = name.into();
        if gate.kind == GateKind::Dip {
            gate.initial = Signal::from_u64(0xab, 8);
        }
        if gate.kind == GateKind::Rom {
            gate.config.memory = vec![Signal::filled(8, crate::Logic::Unknown); 172];
            gate.config.memory[0xab] = Signal::from_u64(0x42, 8);
            gate.config.memory[0xaa] = Signal::from_u64(0x41, 8);
        }
        if gate.kind == GateKind::Led {
            gate.width = 8;
            gate.initial = Signal::from_u64(0, 8);
            gate.rotation = 3;
        }
        module.gates.push(gate);
    }
    for (name, width, a, b) in [
        (
            "address",
            8,
            PinRef::new(GateId(1), "Z"),
            PinRef::new(GateId(2), "I"),
        ),
        (
            "low",
            4,
            PinRef::new(GateId(2), "Z0"),
            PinRef::new(GateId(3), "I0"),
        ),
        (
            "high",
            4,
            PinRef::new(GateId(2), "Z1"),
            PinRef::new(GateId(3), "I1"),
        ),
        (
            "rejoined",
            8,
            PinRef::new(GateId(3), "Z"),
            PinRef::new(GateId(4), "A"),
        ),
        (
            "data",
            8,
            PinRef::new(GateId(4), "D"),
            PinRef::new(GateId(5), "I"),
        ),
        (
            "OE",
            1,
            PinRef::new(GateId(6), "Z"),
            PinRef::new(GateId(4), "OE"),
        ),
    ] {
        let net = module.next_net_id();
        module.nets.push(Net::new(net, name, width));
        for reference in [&a, &b] {
            module
                .gate_mut(reference.gate)
                .unwrap()
                .pin_mut(&reference.pin)
                .unwrap()
                .net = Some(net);
        }
        let start = module.pin_position(&a).unwrap();
        let end = module.pin_position(&b).unwrap();
        let middle = (start.x + end.x) / 2.0;
        module.wires.push(Wire {
            id: module.next_wire_id(),
            net,
            points: vec![
                start,
                Point::new(middle, start.y),
                Point::new(middle, end.y),
                end,
            ],
            start: Some(a),
            end: Some(b),
        });
    }
    let mut note = Gate::new(GateId(7), GateKind::Comment, Point::new(40.0, 270.0));
    note.name = "instructions".into();
    note.text="Bus splitter → concatenation → ROM\nI0/Z0 is the low nibble. Initial address AB reads 42.\nRun, then click the DIP to increment the address.\nDouble-click the ROM during simulation to inspect memory.".into();
    module.gates.push(note);
    Circuit {
        title: "Buses and ROM".into(),
        root: "main".into(),
        modules: vec![module],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn examples_are_valid() {
        full_adder().validate().unwrap();
        clocked_flip_flop().validate().unwrap();
        bus_memory().validate().unwrap();
    }
}
