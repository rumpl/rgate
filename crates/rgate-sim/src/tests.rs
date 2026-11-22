use crate::*;
use rgate_core::{Gate, Net, PinRef, Point, demo};

#[test]
fn full_adder_truth_table() {
    let circuit = demo::full_adder();
    let module = circuit.module("main").unwrap();
    for a in 0..2 {
        for b in 0..2 {
            for ci in 0..2 {
                let mut sim = Simulator::new(module).unwrap();
                sim.set_input(GateId(1), Signal::from_u64(a, 1)).unwrap();
                sim.set_input(GateId(2), Signal::from_u64(b, 1)).unwrap();
                sim.set_input(GateId(3), Signal::from_u64(ci, 1)).unwrap();
                sim.advance(100).unwrap();
                assert_eq!(
                    sim.value(NetId(7)).unwrap().to_u64(),
                    Some((a + b + ci) & 1)
                );
                assert_eq!(
                    sim.value(NetId(8)).unwrap().to_u64(),
                    Some((a + b + ci) >> 1)
                );
            }
        }
    }
}

#[test]
fn flip_flop_samples_only_on_rising_edges() {
    let circuit = demo::clocked_flip_flop();
    let mut sim = Simulator::new(circuit.module("main").unwrap()).unwrap();
    sim.probe(NetId(3)).unwrap();
    sim.advance_to(60).unwrap();
    assert_eq!(sim.value(NetId(3)).unwrap().to_u64(), Some(1));
    sim.set_input(GateId(2), Signal::from_u64(0, 1)).unwrap();
    sim.advance_to(120).unwrap();
    assert_eq!(sim.value(NetId(3)).unwrap().to_u64(), Some(1));
    sim.advance_to(160).unwrap();
    assert_eq!(sim.value(NetId(3)).unwrap().to_u64(), Some(0));
    assert!(sim.traces()[&NetId(3)].transitions.len() >= 3);
}

#[test]
fn conflicting_drivers_resolve_to_x() {
    let mut module = Module::new("main");
    module.nets.push(Net::new(NetId(1), "conflict", 1));
    for index in 0..2 {
        let mut gate = Gate::new(GateId(index + 1), GateKind::Switch, Point::ZERO);
        gate.initial = Signal::from_u64(index, 1);
        gate.pin_mut("Z").unwrap().net = Some(NetId(1));
        module.gates.push(gate);
    }
    let sim = Simulator::new(&module).unwrap();
    assert_eq!(sim.value(NetId(1)).unwrap().bit(0), Logic::Unknown);
}

#[test]
fn short_pulses_are_filtered_by_inertial_delay() {
    let mut module = Module::new("main");
    let mut input = Gate::new(GateId(1), GateKind::Switch, Point::ZERO);
    input.pin_mut("Z").unwrap().net = Some(NetId(1));
    let mut buffer = Gate::new(GateId(2), GateKind::Buffer, Point::new(40.0, 0.0));
    buffer.delay = 10;
    buffer.pin_mut("I").unwrap().net = Some(NetId(1));
    buffer.pin_mut("Z").unwrap().net = Some(NetId(2));
    module.gates = vec![input, buffer];
    module.nets = vec![Net::new(NetId(1), "in", 1), Net::new(NetId(2), "out", 1)];
    let mut sim = Simulator::new(&module).unwrap();
    sim.advance(20).unwrap();
    sim.probe(NetId(2)).unwrap();
    sim.toggle_input(GateId(1)).unwrap();
    sim.advance(3).unwrap();
    sim.toggle_input(GateId(1)).unwrap();
    sim.advance(20).unwrap();
    assert_eq!(sim.value(NetId(2)).unwrap().to_u64(), Some(0));
    assert_eq!(sim.traces()[&NetId(2)].transitions.len(), 1);
}

#[test]
fn malformed_clock_is_rejected() {
    let mut module = Module::new("main");
    let mut gate = Gate::new(GateId(1), GateKind::Clock, Point::ZERO);
    gate.period = 0;
    module.gates.push(gate);
    assert!(Simulator::new(&module).is_err());
}

#[test]
fn zero_delay_oscillation_is_bounded() {
    let mut module = Module::new("main");
    module.nets.push(Net::new(NetId(1), "loop", 1));
    let mut gate = Gate::new(GateId(1), GateKind::Not, Point::ZERO);
    gate.delay = 0;
    for pin in ["I", "Z"] {
        gate.pin_mut(pin).unwrap().net = Some(NetId(1));
    }
    module.gates.push(gate);
    // An uninitialized loop settles to X instead of spinning indefinitely.
    let sim = Simulator::new(&module).unwrap();
    assert_eq!(sim.value(NetId(1)).unwrap().bit(0), Logic::Unknown);
    assert!(sim.module().pin(&PinRef::new(GateId(1), "Z")).is_some());
}

#[test]
fn bus_adder_preserves_sum_and_carry() {
    use rgate_core::{Gate, Net, Point};
    let mut module = Module::new("main");
    for (id, name, width) in [
        (1, "A", 8),
        (2, "B", 8),
        (3, "CI", 1),
        (4, "S", 8),
        (5, "CO", 1),
    ] {
        module.nets.push(Net::new(NetId(id), name, width));
    }
    for (id, value, width) in [(1, 255, 8), (2, 2, 8), (3, 1, 1)] {
        let mut gate = Gate::new(GateId(id), GateKind::Dip, Point::ZERO);
        gate.width = width;
        gate.initial = Signal::from_u64(value, width);
        gate.pin_mut("Z").unwrap().net = Some(NetId(id));
        module.gates.push(gate);
    }
    let mut adder = Gate::new(GateId(4), GateKind::Add, Point::ZERO);
    for (pin, net) in [("A", 1), ("B", 2), ("CI", 3), ("S", 4), ("CO", 5)] {
        adder.pin_mut(pin).unwrap().net = Some(NetId(net));
    }
    module.gates.push(adder);
    let mut simulator = Simulator::new(&module).unwrap();
    simulator.advance(20).unwrap();
    assert_eq!(simulator.value(NetId(4)).unwrap().to_u64(), Some(2));
    assert_eq!(simulator.value(NetId(5)).unwrap().to_u64(), Some(1));
}

#[test]
fn tri_state_disabled_output_floats() {
    use rgate_core::{Gate, Net, Point};
    let mut module = Module::new("main");
    for (id, name) in [(1, "data"), (2, "enable"), (3, "out")] {
        module.nets.push(Net::new(NetId(id), name, 1));
    }
    for id in 1..=2 {
        let mut source = Gate::new(GateId(id), GateKind::Switch, Point::ZERO);
        source.initial = Signal::from_u64(1, 1);
        source.pin_mut("Z").unwrap().net = Some(NetId(id));
        module.gates.push(source);
    }
    let mut buffer = Gate::new(GateId(3), GateKind::TriState, Point::ZERO);
    for (pin, net) in [("I", 1), ("E", 2), ("Z", 3)] {
        buffer.pin_mut(pin).unwrap().net = Some(NetId(net));
    }
    module.gates.push(buffer);
    let mut simulator = Simulator::new(&module).unwrap();
    simulator.advance(20).unwrap();
    assert_eq!(simulator.value(NetId(3)).unwrap().bit(0), Logic::High);
    simulator.toggle_input(GateId(2)).unwrap();
    simulator.advance(20).unwrap();
    assert_eq!(simulator.value(NetId(3)).unwrap().bit(0), Logic::HighZ);
}

#[test]
fn standalone_input_ports_drive_without_changing_the_document() {
    let mut circuit = rgate_core::demo::hierarchical_inverters();
    let module = circuit.module_mut("inverter").unwrap();
    let mut port = Gate::new(GateId(2), GateKind::InputPort, rgate_core::Point::ZERO);
    port.pin_mut("P").unwrap().net = Some(NetId(1));
    module.gates.push(port);
    let original = circuit.clone();
    let mut sim = Simulator::from_circuit(&circuit, "inverter").unwrap();
    assert!(sim.is_input(GateId(2)));
    sim.set_input(GateId(2), Signal::from_u64(0, 1)).unwrap();
    sim.advance(30).unwrap();
    assert_eq!(sim.value(NetId(2)).unwrap().to_u64(), Some(1));
    sim.toggle_input(GateId(2)).unwrap();
    sim.advance(30).unwrap();
    assert_eq!(sim.value(NetId(2)).unwrap().to_u64(), Some(0));
    assert_eq!(sim.module(), original.module("inverter").unwrap());
    assert_eq!(circuit, original);

    let mut parent = Simulator::from_circuit(&circuit, "main").unwrap();
    let child_port = parent.scopes()["main/u0/inner"].gates[&GateId(2)];
    assert!(!parent.is_input(child_port));
    assert!(
        parent
            .set_input(child_port, Signal::from_u64(1, 1))
            .is_err()
    );
    parent.advance(30).unwrap();
    assert_eq!(parent.value(NetId(2)).unwrap().to_u64(), Some(1));
    parent.toggle_input(GateId(1)).unwrap();
    parent.advance(30).unwrap();
    assert_eq!(parent.value(NetId(2)).unwrap().to_u64(), Some(0));
}

#[test]
fn bus_input_ports_accept_full_width_values() {
    let mut module = Module::new("bus");
    module.nets.push(rgate_core::Net::new(NetId(1), "data", 16));
    module.nets[0].port = Some(Direction::Input);
    let mut port = Gate::new(GateId(1), GateKind::InputPort, rgate_core::Point::ZERO);
    port.width = 16;
    port.pin_mut("P").unwrap().net = Some(NetId(1));
    module.gates.push(port);
    let mut sim = Simulator::new(&module).unwrap();
    sim.set_input(GateId(1), Signal::from_u64(0xabcd, 16))
        .unwrap();
    sim.advance(30).unwrap();
    assert_eq!(sim.value(NetId(1)).unwrap().to_u64(), Some(0xabcd));
    assert_eq!(sim.module(), &module);
}

#[test]
fn nested_instances_simulate_independently_and_follow_parent_switches() {
    let circuit = rgate_core::demo::hierarchical_inverters();
    let mut simulator = Simulator::from_circuit(&circuit, "main").unwrap();
    assert!(simulator.warnings().is_empty());
    simulator.probe(NetId(2)).unwrap();
    simulator.advance(20).unwrap();
    assert_eq!(simulator.value(NetId(2)).unwrap().to_u64(), Some(1));
    assert_eq!(simulator.value(NetId(4)).unwrap().to_u64(), Some(0));
    simulator.toggle_input(GateId(1)).unwrap();
    simulator.advance(20).unwrap();
    assert_eq!(simulator.value(NetId(2)).unwrap().to_u64(), Some(0));
    assert_eq!(simulator.value(NetId(4)).unwrap().to_u64(), Some(0));
    assert!(simulator.traces()[&NetId(2)].transitions.len() >= 3);
    assert_eq!(simulator.module(), circuit.module("main").unwrap());
}
