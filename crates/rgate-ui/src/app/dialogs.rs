use super::{GateApp, PropertyDialog, WorkspaceTab};
use anyhow::{Context as _, Result};
use gpui::{Context, Entity, Window};
use gpui_component::input::{InputState, TextareaState};
use rgate_core::{Circuit, GateId, GateKind, Module, NetId, Signal};
use rgate_editor::Tool;
use std::path::PathBuf;

pub struct NetDialog {
    pub net: NetId,
    pub name: Entity<InputState>,
    pub width: Entity<InputState>,
    pub show_name: bool,
    pub error: Option<String>,
}

pub enum Dialog {
    #[cfg_attr(target_family = "wasm", allow(dead_code))]
    VerilogPorts {
        module: String,
        error: Option<String>,
    },
    HdlSignals {
        query: Entity<InputState>,
    },
    HdlTop {
        circuit: Circuit,
        path: Option<PathBuf>,
        selected: String,
    },
    Verilog {
        creating: bool,
        name: Entity<InputState>,
        source: Entity<TextareaState>,
        interface: Entity<TextareaState>,
        error: Option<String>,
    },
    HdlInput {
        net: NetId,
        value: Entity<InputState>,
        error: Option<String>,
    },
    Vga {
        gate: GateId,
    },
    Find {
        query: Entity<InputState>,
    },
    Recovery,
    Symbol(crate::symbol_editor::SymbolDialog),
    Input {
        gate: GateId,
        value: Entity<InputState>,
        error: Option<String>,
    },
    Memory {
        gate: GateId,
        address: Entity<InputState>,
        value: Entity<InputState>,
        error: Option<String>,
    },
    Net(Box<NetDialog>),
    Terminal {
        gate: GateId,
        input: Entity<InputState>,
        error: Option<String>,
    },
    Properties(Box<PropertyDialog>),
    NewModule {
        name: Entity<InputState>,
        error: Option<String>,
    },
    Help,
    About,
}

impl GateApp {
    pub fn dismiss_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.dialog, Some(Dialog::Recovery)) {
            return;
        }
        self.dialog = None;
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub fn apply_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.dialog, Some(Dialog::VerilogPorts { .. })) {
            match self.refresh_verilog_ports() {
                Ok(()) => self.log("Module ports refreshed from source.".into()),
                Err(error) => {
                    if let Some(Dialog::VerilogPorts { error: slot, .. }) = &mut self.dialog {
                        *slot = Some(error.to_string());
                    }
                }
            }
            cx.notify();
            return;
        }
        if let Some(Dialog::HdlTop {
            circuit,
            path,
            selected,
        }) = self.dialog.as_ref()
        {
            let mut circuit = circuit.clone();
            circuit.root = selected.clone();
            let path = path.clone();
            self.dialog = None;
            self.load_document(circuit, path);
            cx.notify();
            return;
        }
        if let Some(Dialog::Find { query }) = &self.dialog {
            if let Some(result) =
                rgate_editor::search(self.editor.circuit(), &query.read(cx).value())
                    .into_iter()
                    .next()
            {
                self.select_search_result(result, window, cx);
            }
            return;
        }
        if matches!(self.dialog, Some(Dialog::Recovery)) {
            self.restore_recovery(window, cx);
            return;
        }
        if matches!(
            self.dialog,
            Some(Dialog::Help | Dialog::About | Dialog::Vga { .. } | Dialog::HdlSignals { .. })
        ) {
            self.dialog = None;
            self.focus.focus(window, cx);
            cx.notify();
            return;
        }

        let terminal_dialog = matches!(
            self.dialog,
            Some(
                Dialog::Terminal { .. }
                    | Dialog::Memory { .. }
                    | Dialog::Input { .. }
                    | Dialog::HdlInput { .. }
            )
        );
        let creating_module = matches!(self.dialog, Some(Dialog::NewModule { .. }));
        let runtime_dialog_gate = match self.dialog.as_ref() {
            Some(
                Dialog::Memory { gate, .. }
                | Dialog::Terminal { gate, .. }
                | Dialog::Input { gate, .. },
            ) => self.simulation_gate(*gate),
            _ => None,
        };

        let result = self.commit_dialog(runtime_dialog_gate, cx);
        match result {
            Ok(()) => {
                self.dialog = None;
                if !terminal_dialog {
                    self.changed();
                }
                if creating_module {
                    self.probes.clear();
                    self.hover = None;
                    self.drag = None;
                    self.editor.set_tool(Tool::Select);
                    self.tab = WorkspaceTab::Edit;
                    self.need_fit = true;
                    self.log(format!(
                        "Created module {}. Place gates, then connect pins to create nets.",
                        self.editor.active_module()
                    ));
                }
                self.focus.focus(window, cx);
            }
            Err(error) => self.set_dialog_error(error),
        }
        cx.notify();
    }

    fn commit_dialog(
        &mut self,
        runtime_dialog_gate: Option<GateId>,
        cx: &Context<Self>,
    ) -> Result<()> {
        match self.dialog.as_ref() {
            Some(Dialog::Verilog {
                creating,
                name,
                source,
                interface,
                ..
            }) => {
                let creating = *creating;
                let name = name.read(cx).value().trim().to_owned();
                let source = source.read(cx).value().to_string();
                let interface = interface.read(cx).value().to_string();
                self.apply_verilog_dialog(creating, name, source, interface)
            }
            Some(Dialog::HdlInput { net, value, .. }) => {
                let net = *net;
                let raw = value.read(cx).value().to_string();
                self.apply_hdl_input_dialog(net, &raw)
            }
            Some(Dialog::Symbol(dialog)) => self
                .editor
                .set_module_symbol(&dialog.module, dialog.shapes.clone())
                .map_err(Into::into),
            Some(Dialog::Input { gate, value, .. }) => {
                let gate = *gate;
                let raw = value.read(cx).value().to_string();
                self.apply_input_dialog(gate, runtime_dialog_gate, &raw)
            }
            Some(Dialog::Memory {
                gate,
                address,
                value,
                ..
            }) => {
                let gate = *gate;
                let address = address.read(cx).value().to_string();
                let raw = value.read(cx).value().to_string();
                self.apply_memory_dialog(gate, runtime_dialog_gate, &address, &raw)
            }
            Some(Dialog::Terminal { input, .. }) => {
                let text = input.read(cx).value().to_string();
                self.simulation
                    .as_mut()
                    .context("Start simulation before sending terminal input.")
                    .and_then(|sim| {
                        sim.send_terminal(
                            runtime_dialog_gate
                                .context("Terminal is not in this live instance.")?,
                            &text,
                        )
                        .map_err(Into::into)
                    })
            }
            Some(Dialog::Net(dialog)) => {
                let id = dialog.net;
                let name = dialog.name.read(cx).value().trim().to_owned();
                let width = dialog.width.read(cx).value().to_string();
                let show = dialog.show_name;
                self.apply_net_dialog(id, name, &width, show)
            }
            Some(Dialog::Properties(dialog))
                if self.editor.module().gate(dialog.gate).is_some_and(|gate| {
                    matches!(gate.kind, GateKind::Frame | GateKind::Comment)
                }) =>
            {
                let dialog = dialog.as_ref();
                let id = dialog.gate;
                let name = dialog.name.read(cx).value().trim().to_owned();
                let frame_width = dialog.frame_width.read(cx).value().to_string();
                let frame_height = dialog.frame_height.read(cx).value().to_string();
                let title = dialog.comment.read(cx).value().replace("\\n", "\n");
                let show_name = dialog.show_name;
                self.apply_annotation_properties(
                    id,
                    name,
                    &frame_width,
                    &frame_height,
                    title,
                    show_name,
                )
            }
            Some(Dialog::Properties(_)) => self.apply_gate_properties(cx),
            Some(Dialog::NewModule { name, .. }) => self
                .editor
                .create_module(name.read(cx).value().trim().to_owned())
                .map_err(Into::into),
            _ => Ok(()),
        }
    }

    fn apply_verilog_dialog(
        &mut self,
        creating: bool,
        name: String,
        source: String,
        interface: String,
    ) -> Result<()> {
        if !crate::verilog_editor::identifier(&name) {
            anyhow::bail!("Use a simple Verilog module name");
        }
        let observations =
            crate::verilog_editor::interface(&interface).map_err(anyhow::Error::msg)?;
        let mut existing = if creating {
            Vec::new()
        } else {
            self.editor.module().nets.clone()
        };
        for net in observations.into_iter().filter(|n| n.port.is_none()) {
            if !existing.iter().any(|n| n.name == net.name) {
                let mut net = net;
                net.id = NetId(existing.iter().map(|n| n.id.0).max().unwrap_or(0) + 1);
                existing.push(net);
            }
        }
        let mut nets = rgate_format::hdl::inferred_nets(&source, &name, &existing)?;
        if source.trim().is_empty() || source.len() > 1_000_000 {
            anyhow::bail!("Source must contain 1–1000000 bytes");
        }
        if !creating
            && self
                .editor
                .module()
                .nets
                .iter()
                .filter(|n| n.port.is_some())
                .map(|n| (&n.name, n.width, n.port))
                .collect::<Vec<_>>()
                != nets
                    .iter()
                    .filter(|n| n.port.is_some())
                    .map(|n| (&n.name, n.width, n.port))
                    .collect::<Vec<_>>()
            && self.editor.circuit().modules.iter().any(|m| {
                m.gates.iter().any(|g| {
                    g.kind == GateKind::Module(name.clone())
                        && g.pins.iter().any(|p| p.net.is_some())
                })
            })
        {
            anyhow::bail!("Disconnect module instances before changing HDL interface ports");
        }
        if !creating && name != self.editor.active_module() {
            anyhow::bail!("Renaming source modules is not supported yet");
        }
        if creating {
            let mut definition = Module::new(name);
            definition.verilog = Some(source);
            definition.nets = nets;
            self.editor.create_definition(definition)?;
            return Ok(());
        }
        let mut next = self.editor.module().next_net_id().0;
        for net in &mut nets {
            net.id = if let Some(old) = self
                .editor
                .module()
                .nets
                .iter()
                .find(|n| n.name == net.name)
            {
                old.id
            } else {
                let id = NetId(next);
                next += 1;
                id
            };
        }
        self.editor.edit_module(|module| {
            module.verilog = Some(source);
            module.nets = nets;
        })?;
        Ok(())
    }

    fn apply_hdl_input_dialog(&mut self, net: NetId, raw: &str) -> Result<()> {
        let sim = self.simulation.as_mut().context("Simulation inactive")?;
        let gate = sim.source_input(net).context("Not an interactive input")?;
        let width = sim.value(net).context("Missing input")?.width();
        let value = Signal::parse(raw.trim_start_matches("0x"), width, 16)
            .map_err(anyhow::Error::msg)?;
        sim.set_input(gate, value)?;
        Ok(())
    }

    fn apply_input_dialog(
        &mut self,
        gate: GateId,
        runtime_gate: Option<GateId>,
        raw: &str,
    ) -> Result<()> {
        let width = self
            .editor
            .module()
            .gate(gate)
            .context("Input no longer exists")?
            .width;
        let word = Signal::parse(raw.trim_start_matches("0x"), width, 16)
            .map_err(anyhow::Error::msg)?;
        self.simulation
            .as_mut()
            .context("Simulation inactive")?
            .set_input(runtime_gate.context("No runtime input")?, word)?;
        self.simulation.as_mut().unwrap().advance(30)?;
        Ok(())
    }

    fn apply_memory_dialog(
        &mut self,
        gate: GateId,
        runtime_gate: Option<GateId>,
        address_raw: &str,
        value_raw: &str,
    ) -> Result<()> {
        let address: usize = address_raw
            .parse()
            .context("Address must be a decimal number.")?;
        let width = self
            .editor
            .module()
            .gate(gate)
            .context("Memory no longer exists.")?
            .width;
        let word = Signal::parse(value_raw.trim_start_matches("0x"), width, 16)
            .map_err(anyhow::Error::msg)?;
        let runtime_id = runtime_gate.context("Memory is not in this live instance.")?;
        self.simulation
            .as_mut()
            .context("Simulation is not active.")?
            .set_memory_word(runtime_id, address, word)?;
        Ok(())
    }

    fn apply_net_dialog(
        &mut self,
        id: NetId,
        name: String,
        width_raw: &str,
        show: bool,
    ) -> Result<()> {
        if name.is_empty() {
            anyhow::bail!("Net name must not be empty.");
        }
        let width: u16 = width_raw.parse().context("Width must be 1–4096.")?;
        if !(1..=4096).contains(&width) {
            anyhow::bail!("Width must be 1–4096.");
        }
        if self.editor.module().gates.iter().any(|gate| {
            gate.pins
                .iter()
                .any(|pin| pin.net == Some(id) && gate.pin_width(&pin.name) != width)
        }) {
            anyhow::bail!(
                "Width conflicts with connected pins. Disconnect or resize the components first."
            );
        }
        self.editor.edit_module(|module| {
            if let Some(net) = module.nets.iter_mut().find(|net| net.id == id) {
                net.name = name;
                net.width = width;
                net.show_name = show;
            }
        })?;
        Ok(())
    }

    fn set_dialog_error(&mut self, error: anyhow::Error) {
        match &mut self.dialog {
            Some(Dialog::Verilog { error: slot, .. } | Dialog::HdlInput { error: slot, .. }) => {
                *slot = Some(format!("{error:#}"))
            }
            Some(Dialog::Input { error: slot, .. }) => *slot = Some(format!("{error:#}")),
            Some(Dialog::Net(dialog)) => dialog.error = Some(format!("{error:#}")),
            Some(Dialog::Memory { error: slot, .. }) => *slot = Some(format!("{error:#}")),
            Some(Dialog::Terminal { error: slot, .. }) => *slot = Some(format!("{error:#}")),
            Some(Dialog::Properties(dialog)) => dialog.error = Some(format!("{error:#}")),
            Some(Dialog::NewModule { error: slot, .. }) => *slot = Some(format!("{error:#}")),
            _ => self.error(error),
        }
    }
}
