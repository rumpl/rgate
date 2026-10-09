use crate::Simulator;
use rgate_core::{Direction, Gate, GateId, GateKind, Logic, Module, Net, NetId, Point, Signal};

fn circuit(kind: GateKind, inputs: &[(&str, u64)]) -> (Module, GateId) {
    let mut module = Module::new("main");
    let mut gate = Gate::new(GateId(100), kind, Point::ZERO);
    for pin in &mut gate.pins {
        let net = NetId(module.nets.len() as u64 + 1);
        let width = pin.width.unwrap_or(gate.width);
        pin.net = Some(net);
        module.nets.push(Net::new(net, &pin.name, width));
        if let Some((_, value)) = inputs.iter().find(|(name, _)| *name == pin.name) {
            let mut source = Gate::new(
                GateId(module.gates.len() as u64 + 1),
                GateKind::Dip,
                Point::ZERO,
            );
            source.width = width;
            source.initial = Signal::from_u64(*value, width);
            source.pin_mut("Z").unwrap().net = Some(net);
            module.gates.push(source);
        }
    }
    module.gates.push(gate);
    (module, GateId(100))
}
fn net(module: &Module, name: &str) -> NetId {
    module.nets.iter().find(|net| net.name == name).unwrap().id
}
fn output(kind: GateKind, inputs: &[(&str, u64)], pin: &str) -> Signal {
    let (module, _) = circuit(kind, inputs);
    let mut sim = Simulator::new(&module).unwrap();
    sim.advance(20).unwrap();
    sim.value(net(&module, pin)).unwrap().clone()
}
fn source(module: &Module, pin: &str) -> GateId {
    let net = net(module, pin);
    module
        .gates
        .iter()
        .find(|gate| gate.kind == GateKind::Dip && gate.pin("Z").unwrap().net == Some(net))
        .unwrap()
        .id
}

#[test]
fn arithmetic_division_and_shifts_are_bounded() {
    assert_eq!(
        output(GateKind::Multiply, &[("A", 20), ("B", 20)], "P").to_u64(),
        Some(144)
    );
    assert_eq!(
        output(GateKind::Divide, &[("A", 23), ("B", 5)], "Q").to_u64(),
        Some(4)
    );
    assert_eq!(
        output(GateKind::Divide, &[("A", 23), ("B", 5)], "R").to_u64(),
        Some(3)
    );
    assert_eq!(
        output(GateKind::Divide, &[("A", 23), ("B", 0)], "Q").bit(0),
        Logic::Unknown
    );
    assert_eq!(
        output(GateKind::ShiftLeft, &[("I", 0x81), ("S", 1)], "Z").to_u64(),
        Some(2)
    );
    assert_eq!(
        output(GateKind::ShiftRight, &[("I", 0x81), ("S", 1)], "Z").to_u64(),
        Some(0x40)
    );
    assert_eq!(
        output(
            GateKind::ArithmeticShiftRight,
            &[("I", 0x81), ("S", 1)],
            "Z"
        )
        .to_u64(),
        Some(0xc0)
    );
    assert_eq!(
        output(GateKind::RotateLeft, &[("I", 0x81), ("S", 9)], "Z").to_u64(),
        Some(3)
    );
    assert_eq!(
        output(GateKind::RotateRight, &[("I", 0x81), ("S", 1)], "Z").to_u64(),
        Some(0xc0)
    );
    assert_eq!(
        output(GateKind::ShiftLeft, &[("I", 1), ("S", 255)], "Z").to_u64(),
        Some(128) // 8-bit bus selects use 3 bits, so 255 becomes shift-by-7.
    );
}

#[test]
fn bus_partition_order_and_four_state_values_are_preserved() {
    assert_eq!(
        output(GateKind::Concat, &[("I0", 0xb), ("I1", 0xa)], "Z").to_u64(),
        Some(0xab)
    );
    assert_eq!(
        output(GateKind::Splitter, &[("I", 0xab)], "Z0").to_u64(),
        Some(0xb)
    );
    assert_eq!(
        output(GateKind::Splitter, &[("I", 0xab)], "Z1").to_u64(),
        Some(0xa)
    );
    let (mut module, id) = circuit(GateKind::Tap, &[("I", 0xab)]);
    let tap = module.gate_mut(id).unwrap();
    tap.config.tap_offset = 4;
    tap.config.tap_width = 4;
    tap.pin_mut("Z").unwrap().width = Some(4);
    module
        .nets
        .iter_mut()
        .find(|net| net.name == "Z")
        .unwrap()
        .width = 4;
    let mut sim = Simulator::new(&module).unwrap();
    sim.advance(20).unwrap();
    assert_eq!(sim.value(net(&module, "Z")).unwrap().to_u64(), Some(0xa));
    sim.set_input(
        source(&module, "I"),
        Signal::from_bits(vec![
            Logic::Low,
            Logic::Low,
            Logic::Low,
            Logic::Low,
            Logic::HighZ,
            Logic::Unknown,
            Logic::High,
            Logic::Low,
        ]),
    )
    .unwrap();
    sim.advance(20).unwrap();
    assert_eq!(
        sim.value(net(&module, "Z")).unwrap().bits(),
        &[Logic::HighZ, Logic::Unknown, Logic::High, Logic::Low]
    );
}

#[test]
fn routing_and_mos_controls_work() {
    assert_eq!(
        output(GateKind::Decoder, &[("I", 1), ("E", 1)], "Z1").to_u64(),
        Some(1)
    );
    assert_eq!(
        output(GateKind::Decoder, &[("I", 1), ("E", 0)], "Z1").to_u64(),
        Some(0)
    );
    assert_eq!(
        output(GateKind::Demux, &[("F", 1), ("S", 1), ("E", 1)], "Z1").to_u64(),
        Some(1)
    );
    assert_eq!(
        output(GateKind::Demux, &[("F", 1), ("S", 1), ("E", 1)], "Z0").to_u64(),
        Some(0)
    );
    assert_eq!(
        output(GateKind::Nmos, &[("S", 1), ("G", 1)], "Z").bit(0),
        Logic::High
    );
    assert_eq!(
        output(GateKind::Nmos, &[("S", 1), ("G", 0)], "Z").bit(0),
        Logic::HighZ
    );
    assert_eq!(
        output(GateKind::Pmos, &[("S", 1), ("G", 0)], "Z").bit(0),
        Logic::High
    );
}

#[test]
fn jk_holds_sets_resets_and_toggles_on_edges() {
    let (module, id) = circuit(
        GateKind::Jkff,
        &[("J", 1), ("K", 1), ("CK", 0), ("PRE", 1), ("CLR", 0)],
    );
    let mut sim = Simulator::new(&module).unwrap();
    sim.advance(20).unwrap();
    assert_eq!(sim.value(net(&module, "Q")).unwrap().to_u64(), Some(0));
    sim.set_input(source(&module, "CLR"), Signal::from_u64(1, 1))
        .unwrap();
    sim.set_input(source(&module, "CK"), Signal::from_u64(1, 1))
        .unwrap();
    sim.advance(20).unwrap();
    assert_eq!(sim.value(net(&module, "Q")).unwrap().to_u64(), Some(1));
    sim.set_input(source(&module, "CK"), Signal::from_u64(0, 1))
        .unwrap();
    sim.advance(20).unwrap();
    assert_eq!(sim.value(net(&module, "Q")).unwrap().to_u64(), Some(1));
    sim.set_input(source(&module, "CK"), Signal::from_u64(1, 1))
        .unwrap();
    sim.advance(20).unwrap();
    assert_eq!(sim.value(net(&module, "Q")).unwrap().to_u64(), Some(0));
    assert_eq!(sim.value(net(&module, "_Q")).unwrap().to_u64(), Some(1));
    assert!(sim.gate_value(id).is_some());
}

#[test]
fn rom_reads_image_and_disabled_output_floats() {
    let (mut module, id) = circuit(GateKind::Rom, &[("A", 1), ("OE", 0)]);
    module.gate_mut(id).unwrap().config.memory =
        vec![Signal::from_u64(0x12, 8), Signal::from_u64(0xab, 8)];
    let mut sim = Simulator::new(&module).unwrap();
    sim.advance(20).unwrap();
    assert_eq!(sim.value(net(&module, "D")).unwrap().to_u64(), Some(0xab));
    sim.set_input(source(&module, "OE"), Signal::from_u64(1, 1))
        .unwrap();
    sim.advance(20).unwrap();
    assert_eq!(sim.value(net(&module, "D")).unwrap().bit(0), Logic::HighZ);
    sim.set_memory_word(id, 1, Signal::from_u64(0xcd, 8))
        .unwrap();
    assert_eq!(sim.memory_words(id).unwrap()[&1].to_u64(), Some(0xcd));
}

#[test]
fn ram_writes_external_bus_then_reads_after_driver_release() {
    let (module, id) = circuit(
        GateKind::Ram,
        &[("A", 2), ("D", 0x55), ("CS", 0), ("WE", 0), ("OE", 1)],
    );
    let mut sim = Simulator::new(&module).unwrap();
    sim.advance(20).unwrap();
    assert_eq!(sim.memory_words(id).unwrap()[&2].to_u64(), Some(0x55));
    sim.set_input(source(&module, "WE"), Signal::from_u64(1, 1))
        .unwrap();
    sim.set_input(source(&module, "D"), Signal::filled(8, Logic::HighZ))
        .unwrap();
    sim.set_input(source(&module, "OE"), Signal::from_u64(0, 1))
        .unwrap();
    sim.advance(20).unwrap();
    assert_eq!(sim.value(net(&module, "D")).unwrap().to_u64(), Some(0x55));
    sim.set_input(source(&module, "D"), Signal::from_u64(0xaa, 8))
        .unwrap();
    sim.advance(20).unwrap();
    assert_eq!(sim.value(net(&module, "D")).unwrap().bit(0), Logic::Unknown);
}

#[test]
fn terminal_strobes_and_receive_queue_are_independent() {
    let (module, id) = circuit(GateKind::Tty, &[("TX", 65), ("WR", 0), ("RD", 0)]);
    let mut sim = Simulator::new(&module).unwrap();
    sim.advance(20).unwrap();
    sim.set_input(source(&module, "WR"), Signal::from_u64(1, 1))
        .unwrap();
    sim.advance(20).unwrap();
    assert_eq!(sim.terminal_output(id).unwrap(), "A");
    sim.send_terminal(id, "bc").unwrap();
    sim.advance(20).unwrap();
    assert_eq!(sim.value(net(&module, "RX")).unwrap().to_u64(), Some(98));
    assert_eq!(sim.value(net(&module, "READY")).unwrap().to_u64(), Some(1));
    sim.set_input(source(&module, "RD"), Signal::from_u64(1, 1))
        .unwrap();
    sim.advance(20).unwrap();
    assert_eq!(sim.value(net(&module, "RX")).unwrap().to_u64(), Some(99));
    assert_eq!(sim.terminal_output(id).unwrap(), "A");
}

#[test]
fn every_extended_component_has_pins_and_valid_defaults() {
    let kinds = [
        GateKind::Jkff,
        GateKind::Decoder,
        GateKind::Demux,
        GateKind::Multiply,
        GateKind::Divide,
        GateKind::ShiftLeft,
        GateKind::ShiftRight,
        GateKind::ArithmeticShiftRight,
        GateKind::RotateLeft,
        GateKind::RotateRight,
        GateKind::Concat,
        GateKind::Splitter,
        GateKind::Tap,
        GateKind::Ram,
        GateKind::Rom,
        GateKind::Nmos,
        GateKind::Pmos,
        GateKind::Tty,
        GateKind::Peripheral,
    ];
    for kind in kinds {
        let gate = Gate::new(GateId(1), kind, Point::ZERO);
        assert!(!gate.pins.is_empty());
        gate.validate_component().unwrap();
        assert!(
            gate.pins
                .iter()
                .any(|pin| pin.direction != Direction::Input)
        );
    }
}

#[test]
fn bus_rom_example_simulates_through_partition_and_join() {
    let circuit = rgate_core::demo::bus_memory();
    let mut sim = Simulator::from_circuit(&circuit, "main").unwrap();
    sim.advance(40).unwrap();
    assert_eq!(sim.value(NetId(5)).unwrap().to_u64(), Some(0x42));
    assert_eq!(sim.value(NetId(4)).unwrap().to_u64(), Some(0xab));
    sim.toggle_input(GateId(1)).unwrap();
    sim.advance(40).unwrap();
    assert_eq!(sim.value(NetId(5)).unwrap().bit(0), Logic::Unknown);
}

#[test]
fn lc3_schematic_runs_sum_program_and_console_trap() {
    let circuit: rgate_core::Circuit = {
        let document: serde_json::Value =
            serde_json::from_str(include_str!("../../../examples/lc3.rgate")).unwrap();
        serde_json::from_value(document["circuit"].clone()).unwrap()
    };
    let root = circuit.module("main").unwrap();
    let reset = root
        .gates
        .iter()
        .find(|gate| gate.name == "RESET_N")
        .unwrap()
        .id;
    let memory = root
        .gates
        .iter()
        .find(|gate| gate.name == "main_memory")
        .unwrap()
        .id;
    let console = root
        .gates
        .iter()
        .find(|gate| gate.name == "console")
        .unwrap()
        .id;
    let halted = net(root, "HALTED");
    let mut sim = Simulator::from_circuit(&circuit, "main").unwrap();
    sim.advance(100).unwrap();
    sim.set_input(reset, Signal::from_u64(1, 1)).unwrap();
    for _ in 0..2000 {
        sim.advance(500).unwrap();
        if sim.value(halted).unwrap().to_u64() == Some(1) {
            break;
        }
    }
    assert_eq!(sim.value(halted).unwrap().to_u64(), Some(1));
    assert_eq!(
        sim.memory_words(memory).unwrap()[&0x3020].to_u64(),
        Some(15)
    );
    assert_eq!(sim.terminal_output(console).unwrap(), "HI\n");
    assert!(sim.warnings().is_empty());
}

#[test]
fn lc3_circuit_executes_indirect_memory_and_subroutine_paths() {
    let document: serde_json::Value =
        serde_json::from_str(include_str!("../../../examples/lc3.rgate")).unwrap();
    let mut circuit: rgate_core::Circuit =
        serde_json::from_value(document["circuit"].clone()).unwrap();
    let root = circuit.module_mut("main").unwrap();
    let memory = root
        .gates
        .iter_mut()
        .find(|gate| gate.name == "main_memory")
        .unwrap();
    memory.config.memory = vec![Signal::from_u64(0, 16); 0x4100];
    // LEA/LDR/STR/LDI/STI, immediate/register AND, NOT, JSR, JSRR, RET.
    for (address, word) in [
        (0x3000, 0xe21f),
        (0x3001, 0x6040),
        (0x3002, 0x7041),
        (0x3003, 0xa41f),
        (0x3004, 0xb41f),
        (0x3005, 0x5002),
        (0x3006, 0x903f),
        (0x3007, 0x4805),
        (0x3008, 0x281c),
        (0x3009, 0x4100),
        (0x300a, 0xf025),
        (0x300d, 0x16e1),
        (0x300e, 0xc1c0),
        (0x300f, 0x16e1),
        (0x3010, 0xc1c0),
        (0x3020, 0x00f0),
        (0x3021, 0),
        (0x3023, 0x4000),
        (0x3024, 0x4001),
        (0x3025, 0x300f),
        (0x4000, 0x1234),
        (0x4001, 0),
    ] {
        memory.config.memory[address] = Signal::from_u64(word, 16);
    }
    let memory = memory.id;
    let root = circuit.module("main").unwrap();
    let reset = root
        .gates
        .iter()
        .find(|gate| gate.name == "RESET_N")
        .unwrap()
        .id;
    let mut sim = Simulator::from_circuit(&circuit, "main").unwrap();
    sim.advance(100).unwrap();
    sim.set_input(reset, Signal::from_u64(1, 1)).unwrap();
    for _ in 0..1000 {
        sim.advance(500).unwrap();
        if sim.value(net(root, "HALTED")).unwrap().to_u64() == Some(1) {
            break;
        }
    }
    assert_eq!(sim.value(net(root, "HALTED")).unwrap().to_u64(), Some(1));
    assert_eq!(sim.value(net(root, "R0")).unwrap().to_u64(), Some(0xffcf));
    assert_eq!(sim.value(net(root, "R2")).unwrap().to_u64(), Some(0x1234));
    assert_eq!(sim.value(net(root, "R3")).unwrap().to_u64(), Some(2));
    let words = sim.memory_words(memory).unwrap();
    assert_eq!(words[&0x3021].to_u64(), Some(0xf0));
    assert_eq!(words[&0x4001].to_u64(), Some(0x1234));
}

#[test]
fn generated_lc3_wires_avoid_other_components_and_match_native_pins() {
    let document: serde_json::Value =
        serde_json::from_str(include_str!("../../../examples/lc3.rgate")).unwrap();
    let circuit: rgate_core::Circuit = serde_json::from_value(document["circuit"].clone()).unwrap();
    circuit.validate().unwrap();
    for module in &circuit.modules {
        for gate in &module.gates {
            if matches!(
                gate.kind,
                GateKind::And
                    | GateKind::Or
                    | GateKind::Xor
                    | GateKind::Buffer
                    | GateKind::Not
                    | GateKind::TriState
                    | GateKind::Register
                    | GateKind::Add
                    | GateKind::Mux
                    | GateKind::Clock
                    | GateKind::Ground
                    | GateKind::Dip
                    | GateKind::Switch
                    | GateKind::Led
            ) {
                let mut native = gate.clone();
                native.reset_pins();
                for pin in &gate.pins {
                    assert!(
                        pin.offset.distance(native.pin(&pin.name).unwrap().offset) < 0.001,
                        "{}.{}.{}",
                        module.name,
                        gate.name,
                        pin.name
                    );
                }
            }
        }
        for wire in &module.wires {
            for segment in wire.points.windows(2) {
                let [a, b] = [segment[0], segment[1]];
                assert!(a.x == b.x || a.y == b.y, "nonorthogonal route");
                for gate in &module.gates {
                    if gate.kind == GateKind::Comment
                        || wire.start.as_ref().is_some_and(|pin| pin.gate == gate.id)
                        || wire.end.as_ref().is_some_and(|pin| pin.gate == gate.id)
                    {
                        continue;
                    }
                    let rect = gate.bounds();
                    let crosses = if a.x == b.x {
                        rect.min.x < a.x
                            && a.x < rect.max.x
                            && a.y.min(b.y).max(rect.min.y) < a.y.max(b.y).min(rect.max.y)
                    } else {
                        rect.min.y < a.y
                            && a.y < rect.max.y
                            && a.x.min(b.x).max(rect.min.x) < a.x.max(b.x).min(rect.max.x)
                    };
                    assert!(
                        !crosses,
                        "{} wire {} crosses {}",
                        module.name, wire.id.0, gate.name
                    );
                }
            }
        }
    }
}

#[test]
fn reduction_broadcast_buffer_and_tristate_variants_match_verilog() {
    for (kind, expected) in [
        (GateKind::ReduceAnd, 0),
        (GateKind::ReduceNand, 1),
        (GateKind::ReduceOr, 1),
        (GateKind::ReduceNor, 0),
        (GateKind::ReduceXor, 0),
        (GateKind::ReduceXnor, 1),
    ] {
        let value = output(kind, &[("I0", 3)], "Z");
        assert_eq!(value.width(), 1);
        assert_eq!(value.to_u64(), Some(expected));
    }
    let mut module = Module::new("main");
    module.nets = vec![
        Net::new(NetId(1), "scalar", 1),
        Net::new(NetId(2), "out", 8),
    ];
    let mut input_gate = Gate::new(GateId(1), GateKind::Switch, Point::ZERO);
    input_gate.initial = Signal::from_u64(1, 1);
    input_gate.pin_mut("Z").unwrap().net = Some(NetId(1));
    let mut gate = Gate::new(GateId(2), GateKind::And, Point::ZERO);
    gate.width = 8;
    gate.input_count = 1;
    gate.reset_pins();
    gate.pin_mut("I0").unwrap().net = Some(NetId(1));
    gate.pin_mut("Z").unwrap().net = Some(NetId(2));
    module.gates = vec![input_gate, gate];
    let mut sim = Simulator::new(&module).unwrap();
    sim.advance(20).unwrap();
    assert_eq!(sim.value(NetId(2)).unwrap().to_u64(), Some(255));
    let (module, _) = circuit(GateKind::Buffer, &[]);
    let mut sim = Simulator::new(&module).unwrap();
    sim.advance(20).unwrap();
    assert_eq!(sim.value(net(&module, "Z")).unwrap().bit(0), Logic::Unknown);
    for low in [false, true] {
        for inverted in [false, true] {
            let (mut module, id) = circuit(GateKind::TriState, &[("I", 1), ("E", u64::from(!low))]);
            let gate = module.gate_mut(id).unwrap();
            gate.config.enable_low = low;
            gate.config.invert_output = inverted;
            let mut sim = Simulator::new(&module).unwrap();
            sim.advance(20).unwrap();
            assert_eq!(
                sim.value(net(&module, "Z")).unwrap().to_u64(),
                Some(u64::from(!inverted))
            );
            sim.toggle_input(source(&module, "E")).unwrap();
            sim.advance(20).unwrap();
            assert_eq!(sim.value(net(&module, "Z")).unwrap().bit(0), Logic::HighZ);
        }
    }
}

#[test]
fn phased_asymmetric_clock_and_sparse_32bit_memory() {
    let mut module = Module::new("main");
    module.nets.push(Net::new(NetId(1), "clock", 1));
    let mut clock = Gate::new(GateId(1), GateKind::Clock, Point::ZERO);
    clock.period = 100;
    clock.config.clock_phase = 10;
    clock.config.clock_duty = 25;
    clock.pin_mut("Z").unwrap().net = Some(NetId(1));
    module.gates.push(clock);
    let mut sim = Simulator::new(&module).unwrap();
    sim.probe(NetId(1)).unwrap();
    sim.advance(210).unwrap();
    let edges = sim.traces()[&NetId(1)]
        .transitions
        .iter()
        .map(|change| change.time)
        .collect::<Vec<_>>();
    assert_eq!(edges, vec![0, 85, 110, 185, 210]);
    let (mut module, id) = circuit(GateKind::Rom, &[("A", 0xffff_ffff), ("OE", 0)]);
    let gate = module.gate_mut(id).unwrap();
    gate.config.address_bits = 32;
    gate.pin_mut("A").unwrap().width = Some(32);
    gate.config
        .sparse_memory
        .insert(u32::MAX, Signal::from_u64(0xab, 8));
    module
        .nets
        .iter_mut()
        .find(|net| net.name == "A")
        .unwrap()
        .width = 32;
    let address = source(&module, "A");
    let gate = module.gate_mut(address).unwrap();
    gate.width = 32;
    gate.initial = Signal::from_u64(u32::MAX as u64, 32);
    let mut sim = Simulator::new(&module).unwrap();
    sim.advance(20).unwrap();
    assert_eq!(sim.value(net(&module, "D")).unwrap().to_u64(), Some(0xab));
    assert!(
        rgate_core::parse_sparse_memory("@FFFFFFFF AB", 8, 32)
            .unwrap()
            .contains_key(&u32::MAX)
    );
}

#[test]
fn tkgate_terminal_handshake_receives_and_transmits_bytes() {
    let mut module = Module::new("main");
    let mut terminal = Gate::new(GateId(100), GateKind::Tty, Point::ZERO);
    terminal.config.tty_tkgate = true;
    terminal.reset_pins();
    for pin in &mut terminal.pins {
        let id = NetId(module.nets.len() as u64 + 1);
        pin.net = Some(id);
        module
            .nets
            .push(Net::new(id, &pin.name, pin.width.unwrap()));
        if pin.direction == Direction::Input {
            let mut driver = Gate::new(
                GateId(module.gates.len() as u64 + 1),
                GateKind::Dip,
                Point::ZERO,
            );
            driver.width = pin.width.unwrap();
            driver.initial = Signal::from_u64(
                if pin.name == "RD" {
                    65
                } else if pin.name == "CTS" {
                    1
                } else {
                    0
                },
                driver.width,
            );
            driver.pin_mut("Z").unwrap().net = Some(id);
            module.gates.push(driver);
        }
    }
    module.gates.push(terminal);
    let mut sim = Simulator::new(&module).unwrap();
    sim.advance(20).unwrap();
    sim.toggle_input(source(&module, "DSR")).unwrap();
    sim.advance(20).unwrap();
    assert_eq!(sim.terminal_output(GateId(100)).unwrap(), "A");
    sim.send_terminal(GateId(100), "b").unwrap();
    sim.toggle_input(source(&module, "CTS")).unwrap();
    sim.advance(20).unwrap();
    assert_eq!(sim.value(net(&module, "TD")).unwrap().to_u64(), Some(98));
    assert_eq!(sim.value(net(&module, "RTS")).unwrap().to_u64(), Some(1));
    sim.toggle_input(source(&module, "CTS")).unwrap();
    sim.advance(20).unwrap();
    assert_eq!(sim.value(net(&module, "RTS")).unwrap().to_u64(), Some(0));
}

#[test]
fn wide_memory_and_arithmetic_work_through_event_queue() {
    let (mut module, id) = circuit(GateKind::Multiply, &[]);
    for gate in &mut module.gates {
        gate.width = 128;
        gate.initial = Signal::from_u64(0, 128);
        gate.reset_pins();
    }
    module.nets.clear();
    let mut product = module.gate(id).unwrap().clone();
    for pin in &mut product.pins {
        let net = NetId(module.nets.len() as u64 + 1);
        pin.net = Some(net);
        module.nets.push(Net::new(net, &pin.name, 128));
    }
    module.gates.clear();
    for (index, pin) in ["A", "B"].iter().enumerate() {
        let mut source = Gate::new(GateId(index as u64 + 1), GateKind::Dip, Point::ZERO);
        source.width = 128;
        source.initial =
            Signal::parse(if index == 0 { "FFFFFFFFFFFFFFFF" } else { "2" }, 128, 16).unwrap();
        source.pin_mut("Z").unwrap().net = product.pin(pin).unwrap().net;
        module.gates.push(source);
    }
    module.gates.push(product);
    let mut sim = Simulator::new(&module).unwrap();
    sim.advance(20).unwrap();
    assert_eq!(
        sim.value(net(&module, "P")).unwrap().display_value(),
        "0x0000000000000001FFFFFFFFFFFFFFFE"
    );
}

#[test]
fn hierarchical_eight_bit_adder_propagates_sum_and_carry_through_all_levels() {
    let circuit = rgate_core::demo::hierarchical_adder();
    let mut sim = crate::Simulator::from_circuit(&circuit, "main").unwrap();
    assert!(sim.warnings().is_empty());
    sim.advance(500).unwrap();
    assert_eq!(
        sim.value(rgate_core::NetId(4)).unwrap().to_u64(),
        Some(0x5c)
    );
    assert_eq!(sim.value(rgate_core::NetId(5)).unwrap().to_u64(), Some(0));
    for a in 0..=255_u64 {
        for b in [0, 1, 0x55, 0xaa, 255 - a, 255] {
            for cin in 0..=1 {
                sim.set_input(rgate_core::GateId(1), rgate_core::Signal::from_u64(a, 8))
                    .unwrap();
                sim.set_input(rgate_core::GateId(2), rgate_core::Signal::from_u64(b, 8))
                    .unwrap();
                sim.set_input(rgate_core::GateId(3), rgate_core::Signal::from_u64(cin, 1))
                    .unwrap();
                sim.advance(500).unwrap();
                let expected = a + b + cin;
                assert_eq!(
                    sim.value(rgate_core::NetId(4)).unwrap().to_u64(),
                    Some(expected & 255),
                    "A={a} B={b} Cin={cin}"
                );
                assert_eq!(
                    sim.value(rgate_core::NetId(5)).unwrap().to_u64(),
                    Some(expected >> 8),
                    "A={a} B={b} Cin={cin}"
                );
            }
        }
    }
}

#[test]
fn tiny_vga_circuit_has_exact_timing_and_rom_generated_frame() {
    let document: serde_json::Value =
        serde_json::from_str(include_str!("../../../examples/tiny-vga.rgate")).unwrap();
    let circuit: rgate_core::Circuit = serde_json::from_value(document["circuit"].clone()).unwrap();
    let root = circuit.module("main").unwrap();
    let reset = root
        .gates
        .iter()
        .find(|gate| gate.name == "RESET_N")
        .unwrap()
        .id;
    let screen = root
        .gates
        .iter()
        .find(|gate| gate.name == "screen")
        .unwrap()
        .id;
    let mut sim = Simulator::from_circuit(&circuit, "main").unwrap();
    sim.advance(20).unwrap();
    sim.set_input(reset, Signal::from_u64(1, 1)).unwrap();
    let hsync = net(root, "HSYNC");
    let vsync = net(root, "VSYNC");
    let de = net(root, "DE");
    sim.probe(hsync).unwrap();
    sim.probe(vsync).unwrap();
    sim.probe(de).unwrap();
    for _ in 0..48 * 32 * 3 {
        sim.advance(100).unwrap();
    }
    let frame = sim.vga_frame(screen).unwrap();
    assert!(frame.frames >= 2, "{} frames", frame.frames);
    assert_eq!(frame.sync_errors, 0);
    assert!(frame.sampled_pixels >= 32 * 24 * 2);
    for (x, color) in [
        (0, 0xff0000),
        (4, 0xff8800),
        (8, 0xffff00),
        (12, 0x00ff00),
        (16, 0x00ffff),
        (20, 0x0000ff),
        (24, 0x8800ff),
        (28, 0xffffff),
    ] {
        assert_eq!(frame.pixels[x], color, "color bar {x}");
    }
    assert_eq!(frame.pixels[16 * 32], 0);
    assert_eq!(frame.pixels[16 * 32 + 4], 0xffffff);
    let transitions = &sim.traces()[&hsync].transitions;
    let lows = transitions
        .iter()
        .filter(|change| change.value.to_u64() == Some(0))
        .map(|change| change.time)
        .collect::<Vec<_>>();
    assert_eq!(lows[1] - lows[0], 4800);
    let first = transitions
        .iter()
        .position(|change| change.value.to_u64() == Some(0))
        .unwrap();
    assert_eq!(transitions[first + 1].time - transitions[first].time, 400);
    let transitions = &sim.traces()[&vsync].transitions;
    let first = transitions
        .iter()
        .position(|change| change.value.to_u64() == Some(0))
        .unwrap();
    assert_eq!(transitions[first + 1].time - transitions[first].time, 9600);
    assert!(sim.warnings().is_empty());
}

#[test]
fn pwm_schematic_has_correct_duty_fraction_and_counter_wrap() {
    let document: serde_json::Value =
        serde_json::from_str(include_str!("../../../examples/pwm-dimmer.rgate")).unwrap();
    let circuit: rgate_core::Circuit = serde_json::from_value(document["circuit"].clone()).unwrap();
    let root = circuit.module("main").unwrap();
    let reset = root.gates.iter().find(|g| g.name == "RESET_N").unwrap().id;
    let duty = root.gates.iter().find(|g| g.name == "DUTY").unwrap().id;
    for value in [0, 1, 64, 128, 192, 255] {
        let mut sim = Simulator::from_circuit(&circuit, "main").unwrap();
        sim.advance(30).unwrap();
        sim.set_input(reset, Signal::from_u64(1, 1)).unwrap();
        sim.set_input(duty, Signal::from_u64(value, 8)).unwrap();
        let mut high = 0;
        for _ in 0..256 {
            sim.advance(100).unwrap();
            high += usize::from(sim.value(net(root, "led")).unwrap().to_u64() == Some(1));
        }
        assert_eq!(high, value as usize);
        assert_eq!(sim.value(net(root, "count")).unwrap().to_u64(), Some(0));
        assert!(sim.warnings().is_empty());
    }
}

#[test]
fn original_learning_examples_execute_counter_shift_and_parity() {
    fn load(name: &str) -> rgate_core::Circuit {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("../../examples/{name}.rgate")),
        )
        .unwrap();
        let document: serde_json::Value = serde_json::from_str(&source).unwrap();
        serde_json::from_value(document["circuit"].clone()).unwrap()
    }
    let circuit = load("adjustable-counter");
    let root = circuit.module("main").unwrap();
    let id = |name: &str| root.gates.iter().find(|g| g.name == name).unwrap().id;
    let mut sim = Simulator::from_circuit(&circuit, "main").unwrap();
    sim.advance(30).unwrap();
    sim.set_input(id("RESET_N"), Signal::from_u64(1, 1))
        .unwrap();
    sim.advance(500).unwrap();
    assert_eq!(sim.value(net(root, "count")).unwrap().to_u64(), Some(5));
    sim.set_input(id("ENABLE_N"), Signal::from_u64(1, 1))
        .unwrap();
    sim.advance(300).unwrap();
    assert_eq!(sim.value(net(root, "count")).unwrap().to_u64(), Some(5));
    sim.set_input(id("STEP"), Signal::from_u64(3, 8)).unwrap();
    sim.set_input(id("ENABLE_N"), Signal::from_u64(0, 1))
        .unwrap();
    sim.advance(100).unwrap();
    assert_eq!(sim.value(net(root, "count")).unwrap().to_u64(), Some(8));
    let circuit = load("serial-shift-register");
    let root = circuit.module("main").unwrap();
    let id = |name: &str| root.gates.iter().find(|g| g.name == name).unwrap().id;
    let mut sim = Simulator::from_circuit(&circuit, "main").unwrap();
    sim.advance(30).unwrap();
    sim.set_input(id("RESET_N"), Signal::from_u64(1, 1))
        .unwrap();
    for (bit, expected) in [(1, 1), (0, 2), (1, 5), (1, 11)] {
        sim.set_input(id("DATA"), Signal::from_u64(bit, 1)).unwrap();
        sim.advance(100).unwrap();
        assert_eq!(
            sim.value(net(root, "history")).unwrap().to_u64(),
            Some(expected)
        );
    }
    let circuit = load("parity-checker");
    let root = circuit.module("main").unwrap();
    let input = root.gates.iter().find(|g| g.name == "DATA").unwrap().id;
    let mut sim = Simulator::from_circuit(&circuit, "main").unwrap();
    for value in 0u64..256 {
        sim.set_input(input, Signal::from_u64(value, 8)).unwrap();
        sim.advance(10).unwrap();
        let odd = u64::from(value.count_ones() % 2 == 1);
        assert_eq!(sim.value(net(root, "odd")).unwrap().to_u64(), Some(odd));
        assert_eq!(
            sim.value(net(root, "even")).unwrap().to_u64(),
            Some(1 - odd)
        );
    }
}
