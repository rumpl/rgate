use crate::live_simulation::LiveSimulation;
use rgate_core::{Gate, GateId, GateKind, NetId, Point, Signal};

#[test]
fn icarus_drives_standalone_ports_but_not_child_ports() {
    let mut circuit = rgate_core::demo::hierarchical_inverters();
    let mut port = Gate::new(GateId(2), GateKind::InputPort, Point::ZERO);
    port.pin_mut("P").unwrap().net = Some(NetId(1));
    circuit.module_mut("inverter").unwrap().gates.push(port);

    let mut sim = LiveSimulation::verilog(&circuit, "inverter").unwrap();
    assert!(sim.is_input(GateId(2)));
    assert_eq!(sim.source_input(NetId(1)), Some(GateId(2)));
    sim.advance(30).unwrap();
    assert_eq!(sim.value(NetId(2)).unwrap().to_u64(), Some(1));
    sim.toggle_input(GateId(2)).unwrap();
    sim.advance(30).unwrap();
    assert_eq!(sim.value(NetId(2)).unwrap().to_u64(), Some(0));

    let mut sim = LiveSimulation::verilog(&circuit, "main").unwrap();
    let child_port = sim.scopes()["main/u0/inner"].gates[&GateId(2)];
    assert!(!sim.is_input(child_port));
    sim.advance(30).unwrap();
    assert_eq!(sim.value(NetId(2)).unwrap().to_u64(), Some(1));
    sim.set_input(GateId(1), Signal::from_u64(1, 1)).unwrap();
    sim.advance(30).unwrap();
    assert_eq!(sim.value(NetId(2)).unwrap().to_u64(), Some(0));
}
