use crate::live_simulation::LiveSimulation;
use crate::workspace::SavedProbe;
use rgate_core::{Circuit, GateId, GateKind, NetId, Signal};
use rgate_sim::{SimError, Simulator, Trace};
use std::collections::BTreeSet;

/// Owns live simulation state, running flag, hierarchical debug scope, and probes.
/// Enforces the invariant that `running` and `debug_path` are cleared whenever simulation stops.
#[derive(Default)]
pub struct SimSession {
    pub live: Option<LiveSimulation>,
    pub debug_path: Option<String>,
    pub running: bool,
    pub probes: BTreeSet<NetId>,
    pub saved_probes: BTreeSet<SavedProbe>,
}

impl SimSession {
    pub fn is_active(&self) -> bool {
        self.live.is_some()
    }

    pub fn time(&self) -> u64 {
        self.live.as_ref().map_or(0, |sim| sim.time())
    }

    pub fn traces(&self) -> Vec<Trace> {
        self.live
            .as_ref()
            .map(|sim| sim.traces().values().cloned().collect())
            .unwrap_or_default()
    }

    /// Stop simulation while keeping `probes` intact (used by Stop command).
    pub fn stop(&mut self) {
        self.live = None;
        self.debug_path = None;
        self.running = false;
    }

    /// Stop simulation and clear active probes (used when circuit changes or leaving Simulate).
    pub fn reset(&mut self) {
        self.stop();
        self.probes.clear();
    }

    pub fn clear_all(&mut self) {
        self.reset();
        self.saved_probes.clear();
    }

    pub fn pause(&mut self) {
        self.running = false;
    }

    /// Attach a freshly compiled/created simulator, applying existing and saved probes.
    /// Returns any probe/simulator warning messages.
    pub fn attach(&mut self, mut simulator: LiveSimulation, circuit: &Circuit) -> Vec<String> {
        let mut messages = Vec::new();
        for net in &self.probes {
            if let Err(error) = simulator.probe(*net) {
                messages.push(error.to_string());
            }
        }
        for warning in simulator.warnings() {
            messages.push(warning.clone());
        }
        self.debug_path = Some(simulator.root_path().into());
        self.live = Some(simulator);
        self.restore_saved_probes(circuit);
        messages
    }

    /// Build a simulator from `circuit` at `active_module` if not already active.
    pub fn ensure_started(
        &mut self,
        circuit: &Circuit,
        active_module: &str,
    ) -> Result<Option<Vec<String>>, SimError> {
        if self.live.is_some() {
            return Ok(None);
        }
        let simulator: LiveSimulation = if circuit.modules.iter().any(|m| m.verilog.is_some()) {
            LiveSimulation::verilog(circuit, active_module)?
        } else {
            Simulator::from_circuit(circuit, active_module)?.into()
        };
        Ok(Some(self.attach(simulator, circuit)))
    }

    pub fn advance_running(&mut self) -> Result<(), SimError> {
        if let Some(simulator) = &mut self.live {
            let quantum = simulator.clock_period().clamp(10, 1000);
            if let Err(error) = simulator.advance(quantum) {
                self.running = false;
                return Err(error);
            }
        }
        Ok(())
    }

    pub fn step(&mut self, clock_cycle: bool) -> Result<(), SimError> {
        self.running = false;
        let Some(sim) = &mut self.live else {
            return Ok(());
        };
        let period = sim.clock_period();
        if clock_cycle {
            sim.advance(period)?;
        } else {
            sim.step()?;
        }
        Ok(())
    }

    pub fn advance_vga_frame(&mut self) -> Result<(), SimError> {
        self.running = false;
        let period = self.live.as_ref().map_or(100, |sim| sim.clock_period());
        for _ in 0..1536 {
            if let Some(sim) = &mut self.live {
                sim.advance(period)?;
            }
        }
        Ok(())
    }

    pub fn scoped_net(&self, net: NetId) -> Option<NetId> {
        self.live
            .as_ref()?
            .scoped_net(self.debug_path.as_deref()?, net)
    }

    pub fn scoped_gate(&self, gate: GateId) -> Option<GateId> {
        self.live
            .as_ref()?
            .scoped_gate(self.debug_path.as_deref()?, gate)
    }

    pub fn displayed_value(&self, net: NetId) -> Option<&Signal> {
        self.live.as_ref()?.value(self.scoped_net(net)?)
    }

    pub fn is_probed(&self, net: NetId) -> bool {
        self.probes.contains(&self.scoped_net(net).unwrap_or(net))
    }

    pub fn toggle_probe(&mut self, net: NetId, net_name: &str) -> Result<bool, SimError> {
        let Some(actual) = self.scoped_net(net) else {
            return Ok(false);
        };
        let saved = SavedProbe {
            root: self.live.as_ref().unwrap().root_path().into(),
            path: self.debug_path.clone().unwrap_or_default(),
            net: net_name.to_owned(),
        };
        if self.probes.remove(&actual) {
            self.saved_probes.remove(&saved);
            self.live.as_mut().unwrap().unprobe(actual);
        } else {
            self.probes.insert(actual);
            self.saved_probes.insert(saved);
            let sim = self.live.as_mut().unwrap();
            sim.probe(actual)?;
            let path = self.debug_path.as_deref().unwrap_or("");
            sim.label_probe(actual, format!("{path}/{net_name}"));
        }
        Ok(true)
    }

    pub fn inspect_hdl_signal(&mut self, path: &str, probe: bool) -> Result<(), SimError> {
        let sim = self
            .live
            .as_mut()
            .ok_or_else(|| SimError::Invalid("Simulation inactive".into()))?;
        let net = sim.inspect_hdl(path)?;
        if probe {
            if self.probes.remove(&net) {
                sim.unprobe(net);
            } else {
                sim.probe(net)?;
                sim.label_probe(net, path.replace('.', "/"));
                self.probes.insert(net);
            }
        }
        Ok(())
    }

    pub fn held_reset_input(&self) -> Option<(GateId, NetId)> {
        let sim = self.live.as_ref()?;
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

    pub fn release_terminal_reset(&mut self) -> Result<(), SimError> {
        if let Some((id, _)) = self.held_reset_input()
            && let Some(sim) = &mut self.live
        {
            sim.set_input(id, Signal::from_u64(1, 1))?;
        }
        Ok(())
    }

    pub fn single_scope_for_module(&self, name: &str) -> Option<String> {
        let sim = self.live.as_ref()?;
        let mut matches = sim
            .scopes()
            .iter()
            .filter(|(_, scope)| scope.module == name)
            .map(|(path, _)| path.clone());
        let first = matches.next()?;
        if matches.next().is_none() {
            Some(first)
        } else {
            None
        }
    }

    pub fn scope_matches(&self, path: &str, module_name: &str) -> bool {
        self.live
            .as_ref()
            .is_some_and(|sim| sim.scope_module(path) == Some(module_name))
    }

    pub fn restore_saved_probes(&mut self, circuit: &Circuit) {
        let Some(sim) = &mut self.live else {
            return;
        };
        self.saved_probes.retain(|probe| {
            if probe.root != sim.root_path() {
                return true;
            }
            let Some(name) = sim.scope_module(&probe.path) else {
                return false;
            };
            let Some(module) = circuit.module(name) else {
                return false;
            };
            let Some(net) = module.nets.iter().find(|net| net.name == probe.net) else {
                return false;
            };
            let Some(actual) = sim.scoped_net(&probe.path, net.id) else {
                return false;
            };
            if sim.probe(actual).is_err() {
                return false;
            }
            sim.label_probe(actual, format!("{}/{}", probe.path, probe.net));
            self.probes.insert(actual);
            true
        });
    }
}
