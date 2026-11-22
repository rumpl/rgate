use crate::app::{Dialog, GateApp};
use gpui::{Context, Window};
use rgate_core::{Direction, Net, NetId};
use rgate_editor::Editor;

pub const TEMPLATE: &str = "`timescale 1ns/1ps\nmodule MODULE(input wire clk, input wire reset, input wire enable, output reg [7:0] count);\n    always @(posedge clk)\n        if (reset) count <= 0;\n        else if (enable) count <= count + 1;\nendmodule\n";

/// Re-infers net/port interfaces for every Verilog source module in `editor`.
pub fn refresh_ports(editor: &mut Editor) -> anyhow::Result<()> {
    let active = editor.active_module().to_owned();
    let sources = editor
        .circuit()
        .modules
        .iter()
        .filter_map(|m| {
            m.verilog
                .as_ref()
                .map(|s| (m.name.clone(), s.clone(), m.nets.clone()))
        })
        .collect::<Vec<_>>();
    let updates = sources
        .into_iter()
        .map(|(name, source, nets)| {
            Ok((
                name.clone(),
                rgate_format::hdl::inferred_nets(&source, &name, &nets)
                    .map_err(|error| anyhow::anyhow!("{name}.sv:1: {error:#}"))?,
            ))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let before = editor.clone();
    for (name, nets) in updates {
        if let Err(error) = (|| {
            editor.switch_module(&name)?;
            editor.edit_module(|m| m.nets = nets)?;
            editor.switch_module(&active)
        })() {
            *editor = before;
            return Err(error.into());
        }
    }
    Ok(())
}

/// Builds the Verilog module editor dialog (or port-refresh dialog on native).
pub fn build_dialog<T: 'static>(
    editor: &Editor,
    creating: bool,
    window: &mut Window,
    cx: &mut Context<T>,
) -> Result<Dialog, &'static str> {
    let mut name = editor.active_module().to_owned();
    if creating {
        let mut index = 1;
        while editor.circuit().module(&format!("hdl{index}")).is_some() {
            index += 1;
        }
        name = format!("hdl{index}");
    }
    if !creating && editor.module().verilog.is_none() {
        return Err("This is a schematic module. Choose New Verilog module instead.");
    }
    #[cfg(not(target_family = "wasm"))]
    if !creating {
        return Ok(Dialog::VerilogPorts {
            module: name,
            error: None,
        });
    }
    let source = if creating {
        TEMPLATE.replace("MODULE", &name)
    } else {
        editor.module().verilog.clone().unwrap()
    };
    let interface = if creating {
        "input clk 1\ninput reset 1\ninput enable 1\noutput count 8".into()
    } else {
        editor
            .module()
            .nets
            .iter()
            .map(|n| {
                format!(
                    "{} {} {}",
                    match n.port {
                        Some(Direction::Input) => "input",
                        Some(Direction::Output) => "output",
                        Some(Direction::InOut) => "inout",
                        None => "signal",
                    },
                    n.name,
                    n.width
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let source = crate::input::text_area(source, window, cx);
    Ok(Dialog::Verilog {
        creating,
        name: crate::input::text_field(name, window, cx),
        source,
        interface: crate::input::text_area(interface, window, cx),
        error: None,
    })
}

impl GateApp {
    pub fn edit_verilog(&mut self, creating: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.sim.is_active() {
            self.log("Stop simulation before editing Verilog.".into());
            return;
        }
        match build_dialog(&self.editor, creating, window, cx) {
            Ok(dialog) => self.dialogs.show(dialog),
            Err(message) => self.log(message.into()),
        }
        cx.notify();
    }
}

pub fn interface(text: &str) -> Result<Vec<Net>, String> {
    let mut result = Vec::new();
    let mut names = std::collections::BTreeSet::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() != 3 {
            return Err("Use one line per signal: input|output|inout|signal name width".into());
        }
        let direction = match fields[0] {
            "input" => Some(Direction::Input),
            "output" => Some(Direction::Output),
            "inout" => Some(Direction::InOut),
            "signal" => None,
            _ => return Err("Unknown interface direction".into()),
        };
        if !identifier(fields[1]) || !names.insert(fields[1]) {
            return Err("Signal names must be unique simple Verilog identifiers".into());
        }
        let width: u16 = fields[2].parse().map_err(|_| "Invalid signal width")?;
        if !(1..=4096).contains(&width) {
            return Err("Widths must be 1–4096".into());
        }
        let mut net = Net::new(NetId(result.len() as u64 + 1), fields[1], width);
        net.port = direction;
        result.push(net);
    }
    Ok(result)
}

pub fn identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}
