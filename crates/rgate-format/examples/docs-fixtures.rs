//! Generate validated, unwired component fixtures for documentation captures.
use anyhow::Result;
use rgate_core::{
    Circuit, Direction, Gate, GateId, GateKind, LedDisplay, Module, Net, NetId, Point,
};
use std::{fs, path::PathBuf};

fn main() -> Result<()> {
    let output = PathBuf::from(std::env::args().nth(1).expect("output directory"));
    fs::create_dir_all(&output)?;
    let kinds = [
        GateKind::And,
        GateKind::Nand,
        GateKind::Or,
        GateKind::Nor,
        GateKind::Xor,
        GateKind::Xnor,
        GateKind::Buffer,
        GateKind::Not,
        GateKind::TriState,
        GateKind::ReduceAnd,
        GateKind::ReduceNand,
        GateKind::ReduceOr,
        GateKind::ReduceNor,
        GateKind::ReduceXor,
        GateKind::ReduceXnor,
        GateKind::Frame,
        GateKind::Switch,
        GateKind::Dip,
        GateKind::Led,
        GateKind::Clock,
        GateKind::Ground,
        GateKind::Vdd,
        GateKind::Dff,
        GateKind::Register,
        GateKind::Mux,
        GateKind::Add,
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
        GateKind::InputPort,
        GateKind::OutputPort,
        GateKind::InoutPort,
        GateKind::Vga,
        GateKind::Comment,
        GateKind::Module("Example".into()),
    ];
    let mut fixtures = Vec::new();
    for kind in kinds {
        let name = if matches!(kind, GateKind::Module(_)) {
            "module".into()
        } else {
            serde_json::to_value(&kind)?.as_str().unwrap().to_owned()
        };
        fixtures.push((name, kind, LedDisplay::Bit));
    }
    for (name, mode) in [
        ("led_bar", LedDisplay::Bar),
        ("led_hex", LedDisplay::Hex),
        ("led_decimal", LedDisplay::Decimal),
        ("led_seven_segment", LedDisplay::SevenSegment),
    ] {
        fixtures.push((name.into(), GateKind::Led, mode));
    }
    let mut manifest = Vec::new();
    for (name, kind, mode) in fixtures {
        let mut gate = Gate::new(GateId(1), kind.clone(), Point::new(0.0, 0.0));
        gate.name = kind.name().into();
        gate.show_name = false;
        if kind == GateKind::Led {
            gate.config.led_display = mode;
            gate.width = mode.recommended_width();
            gate.reset_pins();
        }
        if kind == GateKind::Comment {
            gate.text = "Comment".into();
        }
        if kind == GateKind::Frame {
            gate.text = "Frame".into();
        }
        let mut module = Module::new("main");
        module.gates.push(gate);
        let mut circuit = Circuit {
            title: format!("Component: {name}"),
            root: "main".into(),
            modules: vec![module],
        };
        if matches!(kind, GateKind::Module(_)) {
            let mut definition = Module::new("Example");
            for (index, (name, direction)) in [
                ("A", Direction::Input),
                ("B", Direction::Input),
                ("Y", Direction::Output),
            ]
            .into_iter()
            .enumerate()
            {
                let mut net = Net::new(NetId(index as u64 + 1), name, 8);
                net.port = Some(direction);
                definition.nets.push(net);
            }
            circuit.modules.push(definition);
            let mut instance = circuit
                .module_instance("Example", GateId(1), Point::ZERO)
                .map_err(anyhow::Error::msg)?;
            instance.show_name = false;
            circuit.modules[0].gates[0] = instance;
        }
        circuit.validate()?;
        circuit.validate_hierarchy().map_err(anyhow::Error::msg)?;
        let bounds = circuit.modules[0].gates[0].bounds();
        rgate_format::save(&output.join(format!("{name}.rgate")), &circuit)?;
        manifest.push(serde_json::json!({"name": name, "min": bounds.min, "max": bounds.max}));
    }
    fs::write(
        output.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;
    Ok(())
}
