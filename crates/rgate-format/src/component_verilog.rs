use crate::verilog::{identifier, output, signal};
use anyhow::{Result, bail};
use rgate_core::{Gate, GateKind, Module};
use std::fmt::Write;

pub fn export(text: &mut String, module: &Module, gate: &Gate) -> Result<()> {
    let s = |pin: &str| signal(module, gate, pin);
    let assign = |text: &mut String, pin: &str, expression: String| -> Result<()> {
        if let Some(net) = output(module, gate, pin) {
            writeln!(text, "  assign #{} {net} = {expression};", gate.delay)?;
        }
        Ok(())
    };
    match gate.kind {
        GateKind::Multiply => assign(text, "P", format!("{} * {}", s("A"), s("B")))?,
        GateKind::Divide => {
            assign(text, "Q", format!("{} / {}", s("A"), s("B")))?;
            assign(text, "R", format!("{} % {}", s("A"), s("B")))?;
        }
        GateKind::ShiftLeft | GateKind::ShiftRight | GateKind::ArithmeticShiftRight => {
            let op = match gate.kind {
                GateKind::ShiftLeft => "<<",
                GateKind::ShiftRight => ">>",
                _ => ">>>",
            };
            let data = if gate.kind == GateKind::ArithmeticShiftRight {
                format!("$signed({})", s("I"))
            } else {
                s("I")
            };
            assign(text, "Z", format!("{data} {op} {}", s("S")))?;
        }
        GateKind::RotateLeft | GateKind::RotateRight => {
            let (a, b) = if gate.kind == GateKind::RotateLeft {
                ("<<", ">>")
            } else {
                (">>", "<<")
            };
            assign(
                text,
                "Z",
                format!(
                    "({} {a} ({} % {})) | ({} {b} (({} - ({} % {})) % {}))",
                    s("I"),
                    s("S"),
                    gate.width,
                    s("I"),
                    gate.width,
                    s("S"),
                    gate.width,
                    gate.width
                ),
            )?;
        }
        GateKind::Concat => assign(
            text,
            "Z",
            format!(
                "{{{}}}",
                (0..gate.config.partitions.len())
                    .rev()
                    .map(|index| s(&format!("I{index}")))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )?,
        GateKind::Splitter | GateKind::Tap => {
            let base = identifier(&format!("__rgate_{}_bus", gate.id.0));
            writeln!(
                text,
                "  wire [{}:0] {base};\n  assign {base} = {};",
                gate.width - 1,
                s("I")
            )?;
            if gate.kind == GateKind::Tap {
                assign(
                    text,
                    "Z",
                    format!(
                        "{base}[{}:{}]",
                        gate.config.tap_offset + gate.config.tap_width - 1,
                        gate.config.tap_offset
                    ),
                )?;
            } else {
                let mut offset = 0;
                for (index, width) in gate.config.partitions.iter().enumerate() {
                    assign(
                        text,
                        &format!("Z{index}"),
                        format!("{base}[{}:{offset}]", offset + width - 1),
                    )?;
                    offset += width;
                }
            }
        }
        GateKind::Decoder | GateKind::Demux => {
            let selection = s(if gate.kind == GateKind::Decoder {
                "I"
            } else {
                "S"
            });
            let enable = if gate.pin("E").is_some_and(|pin| pin.net.is_some()) {
                s("E")
            } else {
                "1'b1".into()
            };
            for index in 0..gate.input_count {
                let data = if gate.kind == GateKind::Decoder {
                    "1'b1".into()
                } else {
                    s("F")
                };
                assign(
                    text,
                    &format!("Z{index}"),
                    format!(
                        "({enable} && {selection} == {index}) ? {data} : {}'b0",
                        gate.pin_width(&format!("Z{index}"))
                    ),
                )?;
            }
        }
        GateKind::Nmos | GateKind::Pmos => {
            if let Some(net) = output(module, gate, "Z") {
                let primitive = if gate.kind == GateKind::Nmos {
                    "nmos"
                } else {
                    "pmos"
                };
                writeln!(
                    text,
                    "  {primitive} #{} {} [{}:0] ({net}, {}, {});",
                    gate.delay,
                    identifier(&gate.name),
                    gate.width - 1,
                    s("S"),
                    s("G")
                )?;
            }
        }
        GateKind::Jkff => {
            let state = identifier(&format!("__rgate_{}_q", gate.id.0));
            let control = |name: &str| {
                if gate.pin(name).is_some_and(|pin| pin.net.is_some()) {
                    s(name)
                } else {
                    "1'b1".into()
                }
            };
            let clear = control("CLR");
            let preset = control("PRE");
            writeln!(
                text,
                "  reg [{}:0] {state};\n  always @(posedge {} or negedge {clear} or negedge {preset})\n    if (!{clear} && !{preset}) {state} <= {}'bx;\n    else if (!{clear}) {state} <= 0;\n    else if (!{preset}) {state} <= ~{}'b0;\n    else {state} <= ({} & ~{state}) | (~{} & {state});",
                gate.width - 1,
                s("CK"),
                gate.width,
                gate.width,
                s("J"),
                s("K")
            )?;
            assign(text, "Q", state.clone())?;
            assign(text, "_Q", format!("~{state}"))?;
        }
        GateKind::Ram | GateKind::Rom => {
            let memory = identifier(&format!("__rgate_{}_memory", gate.id.0));
            let depth = 1u64 << gate.config.address_bits;
            writeln!(
                text,
                "  reg [{}:0] {memory} [0:{}];",
                gate.width - 1,
                depth - 1
            )?;
            if !gate.config.memory.is_empty() || !gate.config.sparse_memory.is_empty() {
                writeln!(text, "  initial begin")?;
                for (address, word) in gate
                    .config
                    .memory
                    .iter()
                    .enumerate()
                    .map(|(address, word)| (address as u64, word))
                    .chain(
                        gate.config
                            .sparse_memory
                            .iter()
                            .map(|(address, word)| (u64::from(*address), word)),
                    )
                {
                    let bits = word
                        .bits()
                        .iter()
                        .rev()
                        .map(|bit| bit.to_string().to_lowercase())
                        .collect::<String>();
                    writeln!(text, "    {memory}[{address}] = {}'b{bits};", gate.width)?;
                }
                writeln!(text, "  end")?;
            }
            let selected = if gate.kind == GateKind::Ram {
                format!("!{}", s("CS"))
            } else {
                "1'b1".into()
            };
            assign(
                text,
                "D",
                format!(
                    "({selected} && !{}) ? {memory}[{}] : {}'bz",
                    s("OE"),
                    s("A"),
                    gate.width
                ),
            )?;
            if gate.kind == GateKind::Ram {
                writeln!(
                    text,
                    "  always @({}, {}, {}, {})\n    if ({selected} && !{}) {memory}[{}] <= {};",
                    s("A"),
                    s("D"),
                    s("CS"),
                    s("WE"),
                    s("WE"),
                    s("A"),
                    s("D")
                )?;
            }
        }
        GateKind::Peripheral => assign(
            text,
            "Z",
            format!(
                "{}'b{}",
                gate.width,
                gate.initial
                    .resized(gate.width)
                    .bits()
                    .iter()
                    .rev()
                    .map(|bit| bit.to_string().to_lowercase())
                    .collect::<String>()
            ),
        )?,
        GateKind::Vga => bail!(
            "VGA display {} uses host graphics; export the controller module separately",
            gate.name
        ),
        GateKind::Tty => bail!(
            "TTY {} uses host I/O; executable Verilog export requires a peripheral backend",
            gate.name
        ),
        _ => bail!("unsupported component {}", gate.name),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rgate_core::{Circuit, GateId, Net, NetId, Point};
    #[test]
    fn extended_exports_roundtrip_and_tty_is_explicitly_not_executable() {
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
            GateKind::Peripheral,
        ];
        for kind in kinds {
            let mut circuit = Circuit::default();
            let mut gate = Gate::new(GateId(1), kind, Point::ZERO);
            for pin in &mut gate.pins {
                let id = NetId(circuit.modules[0].nets.len() as u64 + 1);
                circuit.modules[0].nets.push(Net::new(
                    id,
                    &pin.name,
                    pin.width.unwrap_or(gate.width),
                ));
                pin.net = Some(id);
            }
            circuit.modules[0].gates.push(gate);
            let source = crate::verilog::export(&circuit).unwrap();
            assert_eq!(
                crate::verilog::embedded_document(&source).unwrap().unwrap(),
                circuit
            );
        }
        let mut circuit = Circuit::default();
        circuit.modules[0]
            .gates
            .push(Gate::new(GateId(1), GateKind::Tty, Point::ZERO));
        assert!(
            crate::verilog::export(&circuit)
                .unwrap_err()
                .to_string()
                .contains("host I/O")
        );
        assert_eq!(
            crate::native::decode(&crate::native::encode(&circuit).unwrap()).unwrap(),
            circuit
        );
    }
}
