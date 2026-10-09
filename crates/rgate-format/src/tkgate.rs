use anyhow::{Context, Result, bail};
use regex::Regex;
use rgate_core::{
    Circuit, Direction, Gate, GateKind, Module, Net, NetId, Pin, PinRef, Point, Signal, Wire,
};
use std::{collections::HashMap, sync::LazyLock};

static MODULE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?ms)^module\s+(\w+)[^\n]*\n(.*?)^endmodule").unwrap());
static DECLARATION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(wire|reg|supply0|supply1|input|output|inout)\s+(?:\[(\d+):(\d+)\]\s+)?([A-Za-z_]\w*)\s*;").unwrap()
});
static SEGMENT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{(\d+)\}((?:\([^)]*\))+)\{(\d+)\}").unwrap());
static COORDINATE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\(([^,()]+),\s*(-?\d+(?:\.\d+)?)\)").unwrap());
static POSITION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"@\(\s*(-?\d+(?:\.\d+)?)\s*,\s*(-?\d+(?:\.\d+)?)\s*\)").unwrap());
static ROTATION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"/R:(\d+)").unwrap());
static STATE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"/st:(\d+)").unwrap());
static PERIOD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"/omega:(\d+)").unwrap());
static ENDPOINTS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"/w:\[([^]]*)\]").unwrap());
static INSTANCE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*([A-Za-z_]\w*)\s+(?:#\(([^)]*)\)\s+)?([A-Za-z_]\w*)\s*\((.*?)\);\s*//:(.*)")
        .unwrap()
});
static CONCAT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*assign\s+(\w+)\s*=\s*\{\s*([\w\s,]+)\s*\}\s*;\s*//:\s*CONCAT\s+(\w+)\s+(.*)$")
        .unwrap()
});
static CONNECTION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\.([A-Za-z_]\w*)\(\s*([A-Za-z_]\w*)\s*\)").unwrap());
static ANNOTATION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"//:\s*(SWITCH|DIP|LED|GROUND|VDD|comment|frame)\s+(\w+)\s*(?:\((\w+)\))?\s*@")
        .unwrap()
});
static PRIMITIVE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^_(?:GG|GGN)([A-Z]+)(\d+)?(?:x(\d+))?").unwrap());
static TITLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"//: property title = "([^"]*)""#).unwrap());

#[derive(Clone, Debug)]
pub struct ImportResult {
    pub circuit: Circuit,
    pub warnings: Vec<String>,
}

pub fn import(source: &str) -> Result<ImportResult> {
    // Builtin Verilog models are not user-editable graphical modules.
    let source = source.split("//: /builtinBegin").next().unwrap_or(source);
    let mut modules = Vec::new();
    let mut warnings = Vec::new();
    let mut root = None;
    for captures in MODULE.captures_iter(source) {
        let name = captures[1].to_owned();
        let body = &captures[2];
        let header = captures
            .get(0)
            .unwrap()
            .as_str()
            .lines()
            .next()
            .unwrap_or("");
        if header.contains("root_module") {
            root = Some(name.clone());
        }
        let module = import_module(&name, body, &mut warnings)?;
        modules.push(module);
    }
    if modules.is_empty() {
        bail!("no TkGate/Verilog modules found");
    }
    let mut circuit = Circuit {
        title: TITLE
            .captures(source)
            .map(|title| title[1].to_owned())
            .unwrap_or_else(|| "Imported TkGate circuit".into()),
        root: root.unwrap_or_else(|| modules[0].name.clone()),
        modules,
    };
    let interfaces = circuit
        .modules
        .iter()
        .map(|module| {
            (
                module.name.clone(),
                module
                    .nets
                    .iter()
                    .filter_map(|net| {
                        net.port
                            .map(|direction| (net.name.clone(), (direction, net.width)))
                    })
                    .collect::<HashMap<_, _>>(),
            )
        })
        .collect::<HashMap<_, _>>();
    for module in &mut circuit.modules {
        for gate in &mut module.gates {
            if let GateKind::Module(name) = &gate.kind {
                if let Some(ports) = interfaces.get(name) {
                    for pin in &mut gate.pins {
                        if let Some(&(direction, width)) = ports.get(&pin.name) {
                            pin.direction = direction;
                            pin.width = Some(width);
                        }
                    }
                } else {
                    gate.kind = GateKind::Unsupported(name.clone());
                }
            }
        }
    }
    circuit
        .validate()
        .context("invalid geometry or connectivity in TkGate file")?;
    Ok(ImportResult { circuit, warnings })
}

fn import_module(name: &str, body: &str, warnings: &mut Vec<String>) -> Result<Module> {
    let mut module = Module::new(name);
    let mut names: HashMap<String, NetId> = HashMap::new();
    let mut endpoint_map: HashMap<(NetId, i32), (usize, bool)> = HashMap::new();
    let mut current_net = None;
    for line in body.lines() {
        if let Some(capture) = DECLARATION.captures(line) {
            let width = if let (Some(high), Some(low)) = (capture.get(2), capture.get(3)) {
                let high: u32 = high.as_str().parse()?;
                let low: u32 = low.as_str().parse()?;
                let width = high.abs_diff(low) + 1;
                if width > 4096 {
                    bail!(
                        "net {} is {width} bits; supported widths are 1–4096",
                        &capture[4]
                    );
                }
                width as u16
            } else {
                1
            };
            let id = module.next_net_id();
            let mut net = Net::new(id, &capture[4], width);
            net.show_name = !line.contains("/sn:0");
            net.port = match &capture[1] {
                "input" => Some(Direction::Input),
                "output" => Some(Direction::Output),
                "inout" => Some(Direction::InOut),
                _ => None,
            };
            names.insert(net.name.clone(), id);
            module.nets.push(net);
            current_net = Some(id);
        } else if !line.trim_start().starts_with("//: {") {
            current_net = None;
        }
        if let Some(net) = current_net {
            for segment in SEGMENT.captures_iter(line) {
                let points = COORDINATE
                    .captures_iter(&segment[2])
                    .map(|coordinate| -> Result<Point> {
                        let x = coordinate[1]
                            .rsplit(':')
                            .next()
                            .unwrap()
                            .trim()
                            .parse::<f32>()?;
                        let y = coordinate[2].parse::<f32>()?;
                        Ok(Point::new(x, y))
                    })
                    .collect::<Result<Vec<_>>>()?;
                if points.len() < 2 {
                    continue;
                }
                let index = module.wires.len();
                endpoint_map.insert((net, segment[1].parse()?), (index, true));
                endpoint_map.insert((net, segment[3].parse()?), (index, false));
                module.wires.push(Wire {
                    id: module.next_wire_id(),
                    net,
                    points,
                    start: None,
                    end: None,
                });
            }
        }
    }
    let mut comment = None;
    for line in body.lines() {
        if let Some(index) = comment {
            if let Some((_, text)) = line.split_once("//: /line:") {
                let text = decode_comment(text.trim())?;
                let gate: &mut Gate = &mut module.gates[index];
                if !gate.text.is_empty() {
                    gate.text.push('\n');
                }
                gate.text.push_str(&text);
            } else if line.contains("//: /end") {
                comment = None;
            }
            continue;
        }
        if let Some(capture) = ANNOTATION.captures(line) {
            let kind = match &capture[1] {
                "SWITCH" => GateKind::Switch,
                "DIP" => GateKind::Dip,
                "LED" => GateKind::Led,
                "GROUND" => GateKind::Ground,
                "VDD" => GateKind::Vdd,
                "frame" => GateKind::Frame,
                _ => GateKind::Comment,
            };
            let position = position(line).context("missing gate position")?;
            let mut gate = Gate::new(module.next_gate_id(), kind, position);
            gate.name = capture[2].to_owned();
            metadata(&mut gate, line)?;
            if gate.kind == GateKind::Comment || gate.kind == GateKind::Frame {
                comment = Some(module.gates.len());
            } else if let Some(net_name) = capture.get(3) {
                let net = *names
                    .get(net_name.as_str())
                    .with_context(|| format!("unknown net {}", net_name.as_str()))?;
                gate.width = module.net(net).unwrap().width;
                gate.initial = gate.initial.resized(gate.width);
                gate.pins[0].net = Some(net);
                attach(
                    &mut gate,
                    0,
                    endpoint_numbers(line).first().copied(),
                    &mut module,
                    &endpoint_map,
                );
            }
            module.gates.push(gate);
        } else if let Some(capture) = CONCAT.captures(line) {
            let output = *names
                .get(&capture[1])
                .context("unknown concatenation output net")?;
            let inputs = capture[2]
                .split(',')
                .map(|name| {
                    names
                        .get(name.trim())
                        .copied()
                        .context("unknown concatenation input net")
                })
                .collect::<Result<Vec<_>>>()?;
            let mut gate = Gate::new(
                module.next_gate_id(),
                GateKind::Concat,
                position(&capture[4]).context("missing concatenation position")?,
            );
            gate.name = capture[3].to_owned();
            gate.width = module.net(output).unwrap().width;
            gate.initial = gate.initial.resized(gate.width);
            gate.config.partitions = inputs
                .iter()
                .rev()
                .map(|net| module.net(*net).unwrap().width)
                .collect();
            gate.reset_pins();
            metadata(&mut gate, &capture[4])?;
            let endpoints = endpoint_numbers(&capture[4]);
            let output_index = gate.pins.iter().position(|pin| pin.name == "Z").unwrap();
            gate.pins[output_index].net = Some(output);
            gate.pins[output_index].offset = Point::new(1.0, 0.0);
            attach(
                &mut gate,
                output_index,
                endpoints.first().copied(),
                &mut module,
                &endpoint_map,
            );
            for (index, net) in inputs.iter().rev().enumerate() {
                let pin_index = gate
                    .pins
                    .iter()
                    .position(|pin| pin.name == format!("I{index}"))
                    .unwrap();
                gate.pins[pin_index].net = Some(*net);
                gate.pins[pin_index].offset = Point::new(
                    -5.0,
                    (index as f32 - (inputs.len() as f32 - 1.0) / 2.0) * 10.0,
                );
                attach(
                    &mut gate,
                    pin_index,
                    endpoints.get(inputs.len() - index).copied(),
                    &mut module,
                    &endpoint_map,
                );
            }
            module.gates.push(gate);
        } else if let Some(capture) = INSTANCE.captures(line) {
            let primitive = &capture[1];
            let (mut kind, inputs, width) = primitive_kind(primitive);
            if capture[4].contains('~') {
                kind = GateKind::Unsupported(primitive.into());
            }
            if matches!(kind, GateKind::Unsupported(_)) {
                warnings.push(format!(
                    "{name}.{}: {primitive} is displayed but not simulated",
                    &capture[3]
                ));
            }
            let Some(position) = position(&capture[5]) else {
                warnings.push(format!(
                    "{name}.{} has no TkGate placement annotation",
                    &capture[3]
                ));
                continue;
            };
            let mut gate = Gate::new(module.next_gate_id(), kind, position);
            gate.name = capture[3].to_owned();
            gate.width = width;
            gate.input_count = inputs;
            if gate.kind.is_logic() && inputs == 1 && width > 1 {
                gate.config.reduction = true;
            }
            if gate.kind == GateKind::TriState {
                gate.config.invert_output =
                    primitive.contains("NBUFIF") || primitive.contains("NOTIF");
                gate.config.enable_low =
                    primitive.contains("BUFIF0") || primitive.contains("NOTIF0");
            }
            gate.initial = gate.initial.resized(width);
            if matches!(gate.kind, GateKind::Ram | GateKind::Rom) {
                let digits = primitive
                    .trim_start_matches("_GG")
                    .trim_start_matches("RAM")
                    .trim_start_matches("ROM");
                gate.config.address_bits = digits
                    .split('x')
                    .next()
                    .unwrap_or("8")
                    .parse()
                    .context("invalid memory address width")?;
            }
            gate.reset_pins();
            metadata(&mut gate, &capture[5])?;
            if let Some(delays) = capture.get(2) {
                let values = delays
                    .as_str()
                    .split(',')
                    .filter_map(|value| value.trim().parse::<u64>().ok())
                    .collect::<Vec<_>>();
                gate.delay = if matches!(gate.kind, GateKind::Dff | GateKind::Register) {
                    values.last().copied().unwrap_or(4)
                } else {
                    values.first().copied().unwrap_or(4)
                };
            }
            let endpoints = endpoint_numbers(&capture[5]);
            for (connection_index, connection) in CONNECTION.captures_iter(&capture[4]).enumerate()
            {
                let net = *names
                    .get(&connection[2])
                    .with_context(|| format!("unknown net {}", &connection[2]))?;
                let pin_name = &connection[1];
                if gate.pin(pin_name).is_none() {
                    let direction = if matches!(pin_name, "Z" | "Q" | "_Q" | "Y" | "CO") {
                        Direction::Output
                    } else {
                        Direction::Input
                    };
                    let offset = Point::new(
                        if direction == Direction::Output {
                            35.0
                        } else {
                            -35.0
                        },
                        connection_index as f32 * 8.0 - 12.0,
                    );
                    gate.pins.push(Pin::new(pin_name, direction, offset));
                }
                let pin_index = gate
                    .pins
                    .iter()
                    .position(|pin| pin.name == pin_name)
                    .unwrap();
                gate.pins[pin_index].net = Some(net);
                attach(
                    &mut gate,
                    pin_index,
                    endpoints.get(connection_index).copied(),
                    &mut module,
                    &endpoint_map,
                );
            }
            module.gates.push(gate);
        } else if line.contains("@(") && !line.contains("//: joint") && !line.contains("//: /line")
        {
            warnings.push(format!(
                "{name}: unsupported annotated statement: {}",
                line.trim()
            ));
        } else if line.trim_start().starts_with("assign ")
            || line.trim_start().starts_with("always ")
            || line.trim_start().starts_with("initial ")
        {
            warnings.push(format!("{name}: HDL is not executed: {}", line.trim()));
        }
    }
    Ok(module)
}

fn attach(
    gate: &mut Gate,
    pin_index: usize,
    endpoint: Option<i32>,
    module: &mut Module,
    map: &HashMap<(NetId, i32), (usize, bool)>,
) {
    let net = gate.pins[pin_index].net.unwrap();
    if let Some((index, start)) = endpoint
        .and_then(|endpoint| map.get(&(net, endpoint)))
        .copied()
    {
        let wire = &mut module.wires[index];
        let reference = PinRef::new(gate.id, &gate.pins[pin_index].name);
        let point = if start {
            wire.points[0]
        } else {
            *wire.points.last().unwrap()
        };
        gate.pins[pin_index].offset = (point - gate.position).rotated((4 - gate.rotation % 4) % 4);
        if start {
            wire.start = Some(reference);
        } else {
            wire.end = Some(reference);
        }
    }
}

fn metadata(gate: &mut Gate, source: &str) -> Result<()> {
    gate.rotation = ROTATION
        .captures(source)
        .map(|capture| capture[1].parse::<u8>())
        .transpose()?
        .unwrap_or(0)
        % 4;
    gate.show_name = !source.contains("/sn:0");
    if gate.kind == GateKind::Led {
        let display = source
            .split("/type:")
            .nth(1)
            .and_then(|value| value.split_whitespace().next());
        gate.config.led_display = match display {
            Some("1") => rgate_core::LedDisplay::Bar,
            Some("2") => rgate_core::LedDisplay::Hex,
            Some("3") => rgate_core::LedDisplay::Decimal,
            Some("4") => rgate_core::LedDisplay::SevenSegment,
            _ => rgate_core::LedDisplay::Bit,
        };
    }
    if let Some(state) = STATE.captures(source) {
        gate.initial = Signal::from_u64(state[1].parse()?, gate.width);
    }
    if let Some(period) = PERIOD.captures(source) {
        gate.period = period[1].parse()?;
    }
    if gate.kind == GateKind::Clock {
        if let Some(value) = source
            .split("/phi:")
            .nth(1)
            .and_then(|value| value.split_whitespace().next())
        {
            gate.config.clock_phase = value.parse()?;
        }
        if let Some(value) = source
            .split("/duty:")
            .nth(1)
            .and_then(|value| value.split_whitespace().next())
        {
            gate.config.clock_duty = value.parse()?;
        }
    }
    if gate.kind == GateKind::Frame {
        if let Some(value) = source
            .split("/wi:")
            .nth(1)
            .and_then(|value| value.split_whitespace().next())
        {
            gate.config.frame_width = value.parse()?;
        }
        if let Some(value) = source
            .split("/ht:")
            .nth(1)
            .and_then(|value| value.split_whitespace().next())
        {
            gate.config.frame_height = value.parse()?;
        }
    }
    Ok(())
}

fn position(source: &str) -> Option<Point> {
    let capture = POSITION.captures(source)?;
    Some(Point::new(
        capture[1].parse().ok()?,
        capture[2].parse().ok()?,
    ))
}

fn endpoint_numbers(source: &str) -> Vec<i32> {
    ENDPOINTS
        .captures(source)
        .map(|capture| {
            capture[1]
                .split_whitespace()
                .filter_map(|number| number.parse().ok())
                .collect()
        })
        .unwrap_or_default()
}

fn decode_comment(source: &str) -> Result<String> {
    let text: String = if source.starts_with('"') {
        serde_json::from_str(source).or_else(|_| -> Result<String, serde_json::Error> {
            Ok(source
                .trim_matches('"')
                .replace("\\\"", "\"")
                .replace("\\n", "\n"))
        })?
    } else {
        source.to_owned()
    };
    Ok(text)
}

fn primitive_kind(name: &str) -> (GateKind, u8, u16) {
    if name.starts_with("_GGCLOCK") {
        return (GateKind::Clock, 2, 1);
    }
    let Some(capture) = PRIMITIVE.captures(name) else {
        return (GateKind::Module(name.into()), 2, 1);
    };
    let kind = match &capture[1] {
        "AND" => GateKind::And,
        "NAND" => GateKind::Nand,
        "OR" => GateKind::Or,
        "NOR" => GateKind::Nor,
        "XOR" => GateKind::Xor,
        "NXOR" | "XNOR" => GateKind::Xnor,
        "BUF" => GateKind::Buffer,
        "NBUF" => GateKind::Not,
        "BUFIF" | "NBUFIF" | "NOTIF" => GateKind::TriState,
        "FF" => GateKind::Dff,
        "REG" => GateKind::Register,
        "MUX" => GateKind::Mux,
        "ADD" => GateKind::Add,
        "JKFF" => GateKind::Jkff,
        "DECODER" => GateKind::Decoder,
        "DEMUX" => GateKind::Demux,
        "MUL" => GateKind::Multiply,
        "DIV" => GateKind::Divide,
        "LSHIFT" => GateKind::ShiftLeft,
        "RSHIFT" => GateKind::ShiftRight,
        "ARSHIFT" => GateKind::ArithmeticShiftRight,
        "ROLL" => GateKind::RotateLeft,
        "ROR" => GateKind::RotateRight,
        "RAM" => GateKind::Ram,
        "ROM" => GateKind::Rom,
        "NMOS" => GateKind::Nmos,
        "PMOS" => GateKind::Pmos,
        _ => GateKind::Unsupported(name.into()),
    };
    let number = capture
        .get(2)
        .and_then(|number| number.as_str().parse::<u16>().ok())
        .unwrap_or(1);
    let (inputs, width) = if matches!(kind, GateKind::Ram | GateKind::Rom) {
        (
            2,
            capture
                .get(3)
                .and_then(|width| width.as_str().parse::<u16>().ok())
                .unwrap_or(8),
        )
    } else if kind.is_logic() || matches!(kind, GateKind::Mux | GateKind::Decoder | GateKind::Demux)
    {
        (
            number.clamp(1, 64) as u8,
            capture
                .get(3)
                .and_then(|width| width.as_str().parse::<u16>().ok())
                .unwrap_or(1)
                .clamp(1, 4096),
        )
    } else {
        (2, number.clamp(1, 4096))
    };
    (kind, inputs, width)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Independently authored minimal importer fixtures, not upstream circuits.
    const ADDER: &str = r#"module main; //: root_module
reg left; //: {0}(30,40)(90,40){1}
reg right;
wire sum;
//: SWITCH left_control (left) @(13,40) /st:0 /w:[0]
//: SWITCH right_control (right) @(20,90) /st:1
_GGXOR2 parity (.I0(left),.I1(right),.Z(sum)); //: @(110,40) /w:[1 -1 -1]
//: LED result (sum) @(180,40)
endmodule
"#;
    const COUNTER: &str = r#"module main; //: root_module
reg [7:0] data;
wire [7:0] state;
wire clock;
//: DIP input_word (data) @(80,60) /st:5
_GGCLOCK_P100_0_50 oscillator (.Z(clock)); //: @(80,150) /omega:100
_GGREG8 #(20) storage (.D(data),.Q(state),.CK(clock)); //: @(180,100)
endmodule
"#;

    #[test]
    fn imports_authored_logic_fixture_and_exact_endpoints() {
        let result = import(ADDER).unwrap();
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        let module = result.circuit.module("main").unwrap();
        assert_eq!(module.nets.len(), 3);
        assert_eq!(
            module
                .gates
                .iter()
                .filter(|gate| gate.kind == GateKind::Switch)
                .count(),
            2
        );
        assert_eq!(
            module
                .gates
                .iter()
                .filter(|gate| gate.kind == GateKind::Xor)
                .count(),
            1
        );
        for wire in &module.wires {
            if let Some(reference) = &wire.start {
                assert_eq!(module.pin_position(reference).unwrap(), wire.points[0]);
            }
            if let Some(reference) = &wire.end {
                assert_eq!(
                    module.pin_position(reference).unwrap(),
                    *wire.points.last().unwrap()
                );
            }
        }
    }

    #[test]
    fn imports_bus_counter_without_builtin_modules() {
        let result = import(COUNTER).unwrap();
        assert_eq!(result.circuit.modules.len(), 1);
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        let module = &result.circuit.modules[0];
        let register = module
            .gates
            .iter()
            .find(|gate| gate.kind == GateKind::Register)
            .unwrap();
        assert_eq!(register.width, 8);
        assert_eq!(register.delay, 20);
        let clock = module
            .gates
            .iter()
            .find(|gate| gate.kind == GateKind::Clock)
            .unwrap();
        assert_eq!(clock.period, 100);
    }

    #[test]
    fn imports_hierarchical_module_list() {
        let source = "module root; //: root_module\nendmodule\nmodule helper;\nendmodule\n";
        let result = import(source).unwrap();
        assert_eq!(result.circuit.modules.len(), 2);
        assert_eq!(result.circuit.root, "root");
    }

    #[test]
    fn invalid_and_oversized_files_fail() {
        assert!(import("not a circuit").is_err());
        assert!(import("module main;\nwire [4096:0] huge;\nendmodule\n").is_err());
    }
    #[test]
    fn new_primitive_names_map_to_simulated_components() {
        for (name, kind, width) in [
            ("_GGMUL16", GateKind::Multiply, 16),
            ("_GGDIV8", GateKind::Divide, 8),
            ("_GGLSHIFT16", GateKind::ShiftLeft, 16),
            ("_GGARSHIFT8", GateKind::ArithmeticShiftRight, 8),
            ("_GGROM8x32", GateKind::Rom, 32),
            ("_GGRAM16x8", GateKind::Ram, 8),
            ("_GGJKFF", GateKind::Jkff, 1),
            ("_GGDECODER4", GateKind::Decoder, 1),
            ("_GGNMOS", GateKind::Nmos, 1),
        ] {
            let (actual, _, actual_width) = primitive_kind(name);
            assert_eq!(actual, kind);
            assert_eq!(actual_width, width);
        }
    }
    #[test]
    fn imports_all_original_led_display_types() {
        for (number, mode) in [
            (0, rgate_core::LedDisplay::Bit),
            (1, rgate_core::LedDisplay::Bar),
            (2, rgate_core::LedDisplay::Hex),
            (3, rgate_core::LedDisplay::Decimal),
            (4, rgate_core::LedDisplay::SevenSegment),
        ] {
            let source = format!(
                "module main; //: root_module\nwire [7:0] data;\n//: LED g0 (data) @(100,100) /type:{number}\nendmodule\n"
            );
            let result = import(&source).unwrap();
            assert_eq!(result.circuit.modules[0].gates[0].config.led_display, mode);
        }
    }
    #[test]
    fn authored_concat_preserves_order_and_all_wire_endpoints() {
        let result=import("module main; //: root_module\nwire [2:0] combined;\nwire a;\nwire b;\nwire c;\nassign combined={c,b,a}; //: CONCAT join @(160,100) /w:[-1 -1 -1 -1]\nendmodule\n").unwrap();
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        let module = &result.circuit.modules[0];
        let join = module
            .gates
            .iter()
            .find(|gate| gate.kind == GateKind::Concat)
            .unwrap();
        assert!(join.is_compact_bus_join());
        assert_eq!(join.config.partitions, vec![1, 1, 1]);
        for (pin, name) in [("Z", "combined"), ("I0", "a"), ("I1", "b"), ("I2", "c")] {
            assert_eq!(
                module
                    .net(join.pin(pin).unwrap().net.unwrap())
                    .unwrap()
                    .name,
                name
            );
        }
        for wire in &module.wires {
            if let Some(pin) = &wire.start {
                assert_eq!(module.pin_position(pin).unwrap(), wire.points[0]);
            }
            if let Some(pin) = &wire.end {
                assert_eq!(
                    module.pin_position(pin).unwrap(),
                    *wire.points.last().unwrap()
                );
            }
        }
    }
}
