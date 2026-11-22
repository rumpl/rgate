use super::{Dialog, GateApp, NetDialog, PropertyDialog, WorkspaceTab};
use anyhow::{Context as _, Result};
use gpui::{Context, Window};
use rgate_core::{GateId, GateKind, Module, NetId, Signal};
use rgate_editor::Tool;

impl GateApp {
    pub fn select_led_display(
        &mut self,
        mode: rgate_core::LedDisplay,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(Dialog::Properties(dialog)) = &mut self.dialogs.active {
            let connected = self
                .editor
                .module()
                .gate(dialog.gate)
                .is_some_and(|gate| gate.pins.iter().any(|pin| pin.net.is_some()));
            dialog.select_led_display(mode, connected, window, cx);
        }
        cx.notify();
    }

    pub fn show_properties(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(net) = self.editor.selection.net {
            if let Some(definition) = self.editor.module().net(net) {
                self.dialogs
                    .show(Dialog::Net(Box::new(NetDialog::from_net(definition, window, cx))));
            }
            return;
        }
        let Some(id) = self.editor.selection.gates.iter().next().copied() else {
            self.log("Select a gate first; module ports are edited in the Interface tab.".into());
            return;
        };
        let Some(gate) = self.editor.module().gate(id) else {
            return;
        };
        let dialog = PropertyDialog::from_gate(gate, window, cx);
        self.dialogs.show(Dialog::Properties(Box::new(dialog)));
        self.sim.pause();
    }

    pub fn dismiss_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.dialogs.dismiss() {
            return;
        }
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub fn apply_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.dialogs.active, Some(Dialog::VerilogPorts { .. })) {
            match crate::verilog_editor::refresh_ports(&mut self.editor) {
                Ok(()) => self.log("Module ports refreshed from source.".into()),
                Err(error) => {
                    let _ = self.dialogs.set_error(error);
                }
            }
            cx.notify();
            return;
        }
        if let Some(Dialog::HdlTop {
            circuit,
            path,
            selected,
        }) = self.dialogs.active.as_ref()
        {
            let mut circuit = circuit.clone();
            circuit.root = selected.clone();
            let path = path.clone();
            self.dialogs.close();
            self.load_document(circuit, path);
            cx.notify();
            return;
        }
        if let Some(Dialog::Find { query }) = &self.dialogs.active {
            if let Some(result) =
                rgate_editor::search(self.editor.circuit(), &query.read(cx).value())
                    .into_iter()
                    .next()
            {
                self.select_search_result(result, window, cx);
            }
            return;
        }
        if self.dialogs.is_recovery() {
            self.restore_recovery(window, cx);
            return;
        }
        if matches!(
            self.dialogs.active,
            Some(Dialog::Help | Dialog::About | Dialog::Vga { .. } | Dialog::HdlSignals { .. })
        ) {
            self.dialogs.close();
            self.focus.focus(window, cx);
            cx.notify();
            return;
        }

        let terminal_dialog = matches!(
            self.dialogs.active,
            Some(
                Dialog::Terminal { .. }
                    | Dialog::Memory { .. }
                    | Dialog::Input { .. }
                    | Dialog::HdlInput { .. }
            )
        );
        let creating_module = matches!(self.dialogs.active, Some(Dialog::NewModule { .. }));
        let runtime_dialog_gate = match self.dialogs.active.as_ref() {
            Some(
                Dialog::Memory { gate, .. }
                | Dialog::Terminal { gate, .. }
                | Dialog::Input { gate, .. },
            ) => self.sim.scoped_gate(*gate),
            _ => None,
        };

        let result = self.commit_dialog(runtime_dialog_gate, cx);
        match result {
            Ok(()) => {
                self.dialogs.close();
                if !terminal_dialog {
                    self.changed();
                }
                if creating_module {
                    self.sim.probes.clear();
                    self.canvas.clear_interaction();
                    self.editor.set_tool(Tool::Select);
                    self.chrome.tab = WorkspaceTab::Edit;
                    self.canvas.need_fit = true;
                    self.log(format!(
                        "Created module {}. Place gates, then connect pins to create nets.",
                        self.editor.active_module()
                    ));
                }
                self.focus.focus(window, cx);
            }
            Err(error) => {
                if let Some(unhandled) = self.dialogs.set_error(error) {
                    self.error(unhandled);
                }
            }
        }
        cx.notify();
    }

    fn commit_dialog(
        &mut self,
        runtime_dialog_gate: Option<GateId>,
        cx: &Context<Self>,
    ) -> Result<()> {
        match self.dialogs.active.as_ref() {
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
                let sim = self.sim.live.as_mut().context("Simulation inactive")?;
                let gate = sim.source_input(net).context("Not an interactive input")?;
                let width = sim.value(net).context("Missing input")?.width();
                let value = Signal::parse(raw.trim_start_matches("0x"), width, 16)
                    .map_err(anyhow::Error::msg)?;
                sim.set_input(gate, value)?;
                Ok(())
            }
            Some(Dialog::Symbol(dialog)) => self
                .editor
                .set_module_symbol(&dialog.module, dialog.shapes.clone())
                .map_err(Into::into),
            Some(Dialog::Input { gate, value, .. }) => {
                let width = self
                    .editor
                    .module()
                    .gate(*gate)
                    .context("Input no longer exists")?
                    .width;
                let word = Signal::parse(
                    value.read(cx).value().trim_start_matches("0x"),
                    width,
                    16,
                )
                .map_err(anyhow::Error::msg)?;
                let sim = self.sim.live.as_mut().context("Simulation inactive")?;
                sim.set_input(runtime_dialog_gate.context("No runtime input")?, word)?;
                sim.advance(30)?;
                Ok(())
            }
            Some(Dialog::Memory {
                gate,
                address,
                value,
                ..
            }) => {
                let address: usize = address
                    .read(cx)
                    .value()
                    .parse()
                    .context("Address must be a decimal number.")?;
                let width = self
                    .editor
                    .module()
                    .gate(*gate)
                    .context("Memory no longer exists.")?
                    .width;
                let word = Signal::parse(
                    value.read(cx).value().trim_start_matches("0x"),
                    width,
                    16,
                )
                .map_err(anyhow::Error::msg)?;
                let runtime_id =
                    runtime_dialog_gate.context("Memory is not in this live instance.")?;
                self.sim
                    .live
                    .as_mut()
                    .context("Simulation is not active.")?
                    .set_memory_word(runtime_id, address, word)?;
                Ok(())
            }
            Some(Dialog::Terminal { input, .. }) => {
                let text = input.read(cx).value().to_string();
                self.sim
                    .live
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
            Some(Dialog::Net(dialog)) => dialog.apply_to_editor(&mut self.editor, cx),
            Some(Dialog::Properties(dialog)) => dialog.apply_to_editor(&mut self.editor, cx),
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
}
