use super::{GateApp, WorkspaceTab};
#[cfg(not(target_family = "wasm"))]
use crate::commands::Command;
use crate::live_simulation::LiveSimulation;
use gpui::{Context, Window};
use rgate_core::{GateId, GateKind, NetId, Point, Signal};
use rgate_editor::Tool;
use rgate_sim::Simulator;

impl GateApp {
    pub fn advance_running(&mut self, cx: &mut Context<Self>) {
        if let Some(simulator) = &mut self.simulation {
            let quantum = simulator.clock_period().clamp(10, 1000);
            if let Err(error) = simulator.advance(quantum) {
                self.running = false;
                self.log(format!("Simulation error: {error}"));
            }
        }
        cx.notify();
    }

    pub fn held_reset_input(&self) -> Option<(GateId, NetId)> {
        let sim = self.simulation.as_ref()?;
        let root = sim.module();
        let net = root.nets.iter().find(|net| net.name == "RESET_N")?;
        if sim.value(net.id)?.to_u64() != Some(0) {
            return None;
        }
        let gate = root.gates.iter().find(|gate| {
            gate.kind == GateKind::Switch
                && gate.pin("Z").is_some_and(|pin| pin.net == Some(net.id))
        })?;
        Some((gate.id, net.id))
    }

    pub fn release_terminal_reset(&mut self, cx: &mut Context<Self>) {
        if let Some((id, _)) = self.held_reset_input()
            && let Some(sim) = &mut self.simulation
            && let Err(error) = sim.set_input(id, Signal::from_u64(1, 1))
        {
            self.error(error);
        }
        cx.notify();
    }

    pub fn ensure_simulator(&mut self) -> bool {
        if self.simulation.is_some() {
            return true;
        }
        if let Err(error) = self.refresh_verilog_ports() {
            self.error(error);
            return false;
        }
        let result: Result<LiveSimulation, rgate_sim::SimError> = if self
            .editor
            .circuit()
            .modules
            .iter()
            .any(|m| m.verilog.is_some())
        {
            LiveSimulation::verilog(self.editor.circuit(), self.editor.active_module())
        } else {
            Simulator::from_circuit(self.editor.circuit(), self.editor.active_module())
                .map(Into::into)
        };
        match result {
            Ok(mut simulator) => {
                self.hdl_diagnostics.clear();
                for net in &self.probes {
                    if let Err(error) = simulator.probe(*net) {
                        self.messages.push(error.to_string());
                    }
                }
                for warning in simulator.warnings() {
                    self.messages.push(warning.clone());
                }
                self.remember_view();
                self.debug_path = Some(simulator.root_path().into());
                self.simulation = Some(simulator);
                self.restore_saved_probes();
                self.remember_view();
                self.editor.set_tool(Tool::Select);
                true
            }
            Err(error) => {
                self.error(error);
                false
            }
        }
    }

    pub fn simulation_net(&self, net: NetId) -> Option<NetId> {
        self.simulation
            .as_ref()?
            .scoped_net(self.debug_path.as_deref()?, net)
    }

    pub fn simulation_gate(&self, gate: GateId) -> Option<GateId> {
        self.simulation
            .as_ref()?
            .scoped_gate(self.debug_path.as_deref()?, gate)
    }

    pub fn displayed_value(&self, net: NetId) -> Option<&Signal> {
        self.simulation.as_ref()?.value(self.simulation_net(net)?)
    }

    pub fn inspect_hdl_signal(&mut self, path: &str, probe: bool, cx: &mut Context<Self>) {
        let result = (|| {
            let sim = self
                .simulation
                .as_mut()
                .ok_or_else(|| rgate_sim::SimError::Invalid("Simulation inactive".into()))?;
            let net = sim.inspect_hdl(path)?;
            if probe {
                if self.probes.remove(&net) {
                    sim.unprobe(net);
                } else {
                    sim.probe(net)?;
                    sim.label_probe(net, path.replace('.', "/"));
                    self.probes.insert(net);
                }
                self.bottom_scope = true;
            }
            Ok::<_, rgate_sim::SimError>(())
        })();
        if let Err(error) = result {
            self.error(error);
        }
        cx.notify();
    }

    pub fn is_probed(&self, net: NetId) -> bool {
        self.probes
            .contains(&self.simulation_net(net).unwrap_or(net))
    }

    pub fn toggle_probe(&mut self, net: NetId, cx: &mut Context<Self>) {
        if !self.ensure_simulator() {
            return;
        }
        let Some(actual) = self.simulation_net(net) else {
            self.log(
                "Choose a live instance in the hierarchy tree to probe this definition.".into(),
            );
            return;
        };
        let saved = crate::workspace::SavedProbe {
            root: self.simulation.as_ref().unwrap().root_path().into(),
            path: self.debug_path.clone().unwrap_or_default(),
            net: self.editor.module().net(net).unwrap().name.clone(),
        };
        if self.probes.remove(&actual) {
            self.saved_probes.remove(&saved);
            self.simulation.as_mut().unwrap().unprobe(actual);
        } else {
            self.probes.insert(actual);
            self.saved_probes.insert(saved);
            let result = self.simulation.as_mut().unwrap().probe(actual);
            if let Err(error) = result {
                self.error(error);
            } else {
                let path = self.debug_path.as_deref().unwrap_or("");
                let name = self
                    .editor
                    .module()
                    .net(net)
                    .map(|net| net.name.as_str())
                    .unwrap_or("");
                self.simulation
                    .as_mut()
                    .unwrap()
                    .label_probe(actual, format!("{path}/{name}"));
            }
        }
        self.bottom_scope = true;
        cx.notify();
    }

    pub fn navigate_scope(&mut self, path: String, name: String, cx: &mut Context<Self>) {
        if self
            .simulation
            .as_ref()
            .is_some_and(|sim| sim.scope_module(&path) == Some(name.as_str()))
        {
            self.remember_view();
            if let Err(error) = self.editor.switch_module(&name) {
                self.error(error);
                return;
            }
            self.debug_path = Some(path.clone());
            self.hover = None;
            self.drag = None;
            self.editor.set_tool(Tool::Select);
            self.restore_view();
            self.tab = WorkspaceTab::Simulate;
            self.status = format!("Live instance: {path}");
            cx.notify();
        } else if self.simulation.is_some() {
            self.log(format!("{path} is not in the running simulation. Stop simulation to edit or run this definition independently."));
            cx.notify();
        } else {
            self.switch_module(name, cx);
        }
    }

    pub fn switch_module(&mut self, name: String, cx: &mut Context<Self>) {
        self.input_swap = None;
        if let Some(sim) = &self.simulation {
            let paths = sim
                .scopes()
                .iter()
                .filter(|(_, scope)| scope.module == name)
                .map(|(path, _)| path.clone())
                .collect::<Vec<_>>();
            if paths.len() == 1 {
                self.navigate_scope(paths[0].clone(), name, cx);
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
            self.probes.clear();
            self.hover = None;
            self.drag = None;
            self.editor.set_tool(Tool::Select);
            self.restore_view();
            self.tab = WorkspaceTab::Edit;
        }
        cx.notify();
    }

    pub fn set_tab(&mut self, tab: WorkspaceTab, window: &mut Window, cx: &mut Context<Self>) {
        match tab {
            WorkspaceTab::Edit => {
                #[cfg(not(target_family = "wasm"))]
                self.cancel_compilation();
                self.remember_view();
                self.running = false;
                self.simulation = None;
                self.debug_path = None;
                self.probes.clear();
                self.tab = tab;
                self.remember_view();
            }
            WorkspaceTab::Interface => {
                self.running = false;
                self.tab = tab;
            }
            WorkspaceTab::Simulate => {
                #[cfg(not(target_family = "wasm"))]
                if self.background_compilation
                    && self.simulation.is_none()
                    && self
                        .editor
                        .circuit()
                        .modules
                        .iter()
                        .any(|m| m.verilog.is_some())
                {
                    self.start_compilation(Command::ClockStep, cx);
                    return;
                }
                if self.ensure_simulator() {
                    self.tab = tab;
                    self.bottom_scope = true;
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
        self.dialog = None;
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
            self.editor.viewport.pan = Point::new(
                f32::from(self.canvas_bounds.size.width) / 2.0,
                f32::from(self.canvas_bounds.size.height) / 2.0,
            ) - center * self.editor.viewport.zoom;
            self.need_fit = false;
        }
        self.focus.focus(window, cx);
        cx.notify();
    }
}
