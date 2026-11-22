use super::{GateApp, WorkspaceTab};
#[cfg(not(target_family = "wasm"))]
use crate::commands::Command;
use gpui::{Context, Window};
use rgate_core::NetId;
use rgate_editor::Tool;

impl GateApp {
    #[cfg(not(target_family = "wasm"))]
    pub(super) fn poll_hdl_tasks(&mut self, cx: &mut Context<Self>) {
        if let Some(completed) = self.hdl.poll_compile(self.editor.circuit()) {
            match completed.result {
                Ok(sim) => {
                    let warnings = self.sim.attach(sim, self.editor.circuit());
                    for warning in warnings {
                        self.console.push_raw(warning);
                    }
                    self.hdl.clear_diagnostics();
                    self.chrome.tab = WorkspaceTab::Simulate;
                    self.chrome.bottom_scope = true;
                    self.sim.running = matches!(completed.command, Command::PlayPause);
                    if matches!(completed.command, Command::Step | Command::ClockStep)
                        && let Err(error) =
                            self.sim.step(matches!(completed.command, Command::ClockStep))
                    {
                        self.error(error);
                    }
                    self.log("Verilog ready.".into());
                }
                Err(error) => self.error(error),
            }
            cx.notify();
        }
        if self
            .hdl
            .poll_lint(self.editor.circuit(), self.sim.is_active())
        {
            cx.notify();
        }
    }

    #[cfg(not(target_family = "wasm"))]
    pub fn start_compilation(&mut self, command: Command, cx: &mut Context<Self>) {
        if self.hdl.is_compiling() {
            return;
        }
        if let Err(error) = crate::verilog_editor::refresh_ports(&mut self.editor) {
            self.error(error);
            return;
        }
        if self.hdl.start(
            self.editor.circuit().clone(),
            self.editor.active_module().to_owned(),
            command,
        ) {
            self.sim.pause();
            self.log("Compiling Verilog… Stop cancels.".into());
            cx.notify();
        }
    }

    pub fn advance_running(&mut self, cx: &mut Context<Self>) {
        if let Err(error) = self.sim.advance_running() {
            self.log(format!("Simulation error: {error}"));
        }
        cx.notify();
    }

    pub fn release_terminal_reset(&mut self, cx: &mut Context<Self>) {
        if let Err(error) = self.sim.release_terminal_reset() {
            self.error(error);
        }
        cx.notify();
    }

    pub fn ensure_simulator(&mut self) -> bool {
        if self.sim.is_active() {
            return true;
        }
        if let Err(error) = crate::verilog_editor::refresh_ports(&mut self.editor) {
            self.error(error);
            return false;
        }
        self.remember_view();
        match self
            .sim
            .ensure_started(self.editor.circuit(), self.editor.active_module())
        {
            Ok(Some(messages)) => {
                self.hdl.clear_diagnostics();
                for message in messages {
                    self.console.push_raw(message);
                }
                self.remember_view();
                self.editor.set_tool(Tool::Select);
                true
            }
            Ok(None) => true,
            Err(error) => {
                self.error(error);
                false
            }
        }
    }

    pub fn inspect_hdl_signal(&mut self, path: &str, probe: bool, cx: &mut Context<Self>) {
        match self.sim.inspect_hdl_signal(path, probe) {
            Ok(()) => {
                if probe {
                    self.chrome.bottom_scope = true;
                }
            }
            Err(error) => self.error(error),
        }
        cx.notify();
    }

    pub fn toggle_probe(&mut self, net: NetId, cx: &mut Context<Self>) {
        if !self.ensure_simulator() {
            return;
        }
        let name = self
            .editor
            .module()
            .net(net)
            .map(|net| net.name.clone())
            .unwrap_or_default();
        match self.sim.toggle_probe(net, &name) {
            Ok(true) => self.chrome.bottom_scope = true,
            Ok(false) => {
                self.log(
                    "Choose a live instance in the hierarchy tree to probe this definition.".into(),
                );
                return;
            }
            Err(error) => {
                self.error(error);
                self.chrome.bottom_scope = true;
            }
        }
        cx.notify();
    }

    pub fn navigate_scope(&mut self, path: String, name: String, cx: &mut Context<Self>) {
        if self.sim.scope_matches(&path, &name) {
            self.remember_view();
            if let Err(error) = self.editor.switch_module(&name) {
                self.error(error);
                return;
            }
            self.sim.debug_path = Some(path.clone());
            self.canvas.clear_interaction();
            self.editor.set_tool(Tool::Select);
            self.restore_view();
            self.chrome.tab = WorkspaceTab::Simulate;
            self.console.set_status(format!("Live instance: {path}"));
            cx.notify();
        } else if self.sim.is_active() {
            self.log(format!("{path} is not in the running simulation. Stop simulation to edit or run this definition independently."));
            cx.notify();
        } else {
            self.switch_module(name, cx);
        }
    }

    pub fn switch_module(&mut self, name: String, cx: &mut Context<Self>) {
        self.canvas.input_swap = None;
        if self.sim.is_active() {
            if let Some(path) = self.sim.single_scope_for_module(&name) {
                self.navigate_scope(path, name, cx);
            } else {
                self.log("Choose the specific instance in Tree view; this definition has multiple or no running instances.".into());
                cx.notify();
            }
            return;
        }
        self.remember_view();
        if let Err(error) = self.editor.switch_module(&name) {
            self.error(error);
        } else {
            self.changed();
            self.canvas.clear_interaction();
            self.editor.set_tool(Tool::Select);
            self.restore_view();
            self.chrome.tab = WorkspaceTab::Edit;
        }
        cx.notify();
    }

    pub fn set_tab(&mut self, tab: WorkspaceTab, window: &mut Window, cx: &mut Context<Self>) {
        match tab {
            WorkspaceTab::Edit => {
                if self.hdl.cancel() {
                    self.log("Compilation cancelled.".into());
                }
                self.remember_view();
                self.sim.reset();
                self.chrome.tab = tab;
                self.remember_view();
            }
            WorkspaceTab::Interface => {
                self.sim.pause();
                self.chrome.tab = tab;
            }
            WorkspaceTab::Simulate => {
                #[cfg(not(target_family = "wasm"))]
                if self.hdl.should_compile_in_background(
                    self.sim.is_active(),
                    self.editor.circuit(),
                    &Command::ClockStep,
                ) {
                    self.start_compilation(Command::ClockStep, cx);
                    return;
                }
                if self.ensure_simulator() {
                    self.chrome.tab = tab;
                    self.chrome.bottom_scope = true;
                }
            }
        }
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub fn tool_hint(&self) -> &'static str {
        match self.editor.tool {
            Tool::Select => {
                "Select / move: drag gates or wire segments; Shift-click adds gates; double-click opens properties."
            }
            Tool::Wire => {
                "Wire: click a pin, click to add corners, then click a target pin or wire. Esc cancels."
            }
            Tool::Pan => {
                "Scroll: drag to pan the circuit. Scroll wheel also pans; ⌘/Ctrl-scroll zooms."
            }
            Tool::Delete => "Cut: click a wire to disconnect it, or a gate to delete it.",
            Tool::Place(_) => {
                "Place: click the canvas to add a component. Esc returns to selection."
            }
        }
    }

    pub fn select_search_result(
        &mut self,
        result: rgate_editor::SearchResult,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dialogs.close();
        self.switch_module(result.module.clone(), cx);
        if self.editor.active_module() != result.module.as_str() {
            self.log(
                "Select the exact live instance in Tree view before locating this result.".into(),
            );
            cx.notify();
            return;
        }
        let center = match result.target {
            rgate_editor::SearchTarget::Gate(id) => {
                self.editor.select(id, false);
                self.editor.module().gate(id).map(|gate| gate.position)
            }
            rgate_editor::SearchTarget::Net(id) => {
                self.editor.selection.clear();
                self.editor.selected_wires.clear();
                self.editor.selected_net = Some(id);
                self.editor
                    .module()
                    .wires
                    .iter()
                    .find(|wire| wire.net == id)
                    .and_then(|wire| wire.points.first().copied())
            }
        };
        if let Some(center) = center {
            self.canvas.center_on(&mut self.editor, center);
        }
        self.focus.focus(window, cx);
        cx.notify();
    }
}
