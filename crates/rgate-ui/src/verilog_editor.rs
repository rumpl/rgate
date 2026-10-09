use crate::{
    app::{Dialog, GateApp},
    input::TextField,
};
use gpui::{AppContext, Context, Focusable, Window};
use rgate_core::{Direction, Net, NetId};

pub const TEMPLATE: &str = "`timescale 1ns/1ps\nmodule MODULE(input wire clk, input wire reset, input wire enable, output reg [7:0] count);\n    always @(posedge clk)\n        if (reset) count <= 0;\n        else if (enable) count <= count + 1;\nendmodule\n";
impl GateApp {
    pub fn edit_verilog(&mut self, creating: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.simulation.is_some() {
            self.log("Stop simulation before editing Verilog.".into());
            return;
        }
        let mut name = self.editor.active_module().to_owned();
        if creating {
            let mut index = 1;
            while self
                .editor
                .circuit()
                .module(&format!("hdl{index}"))
                .is_some()
            {
                index += 1;
            }
            name = format!("hdl{index}");
        }
        if !creating && self.editor.module().verilog.is_none() {
            self.log("This is a schematic module. Choose New Verilog module instead.".into());
            return;
        }
        let source = if creating {
            TEMPLATE.replace("MODULE", &name)
        } else {
            self.editor.module().verilog.clone().unwrap()
        };
        let interface = if creating {
            "input clk 1\ninput reset 1\ninput enable 1\noutput count 8".into()
        } else {
            self.editor
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
        let source = cx.new(|cx| TextField::new(source, cx).multiline());
        source.read(cx).focus_handle(cx).focus(window, cx);
        self.dialog = Some(Dialog::Verilog {
            creating,
            name: cx.new(|cx| TextField::new(name, cx)),
            source,
            interface: cx.new(|cx| TextField::new(interface, cx).multiline()),
            error: None,
        });
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
