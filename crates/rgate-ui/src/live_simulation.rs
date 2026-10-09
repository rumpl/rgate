//! Retain schematic metadata for live navigation; dispatch runtime operations to one engine.
use rgate_core::{GateId, NetId, Signal};
#[cfg(feature = "icarus")]
use rgate_sim::SimulationBackend;
use rgate_sim::{SimError, Simulator, Trace};
use std::collections::BTreeMap;

pub struct LiveSimulation {
    schematic: Simulator,
    #[cfg(feature = "icarus")]
    hdl: Option<rgate_hdl::IcarusBackend>,
    #[cfg(feature = "icarus")]
    inputs: BTreeMap<GateId, NetId>,
}
impl From<Simulator> for LiveSimulation {
    fn from(schematic: Simulator) -> Self {
        Self {
            schematic,
            #[cfg(feature = "icarus")]
            hdl: None,
            #[cfg(feature = "icarus")]
            inputs: BTreeMap::new(),
        }
    }
}
impl std::ops::Deref for LiveSimulation {
    type Target = Simulator;
    fn deref(&self) -> &Simulator {
        &self.schematic
    }
}
impl std::ops::DerefMut for LiveSimulation {
    fn deref_mut(&mut self) -> &mut Simulator {
        &mut self.schematic
    }
}
impl LiveSimulation {
    pub fn backend_name(&self) -> &str {
        #[cfg(feature = "icarus")]
        if self.hdl.is_some() {
            return "Icarus VPI";
        }
        "Rust schematic"
    }
    pub fn time(&self) -> u64 {
        #[cfg(feature = "icarus")]
        if let Some(hdl) = &self.hdl {
            return hdl.time();
        }
        self.schematic.time()
    }
    pub fn advance(&mut self, ns: u64) -> Result<usize, SimError> {
        #[cfg(feature = "icarus")]
        if let Some(hdl) = &mut self.hdl {
            return hdl.advance(ns);
        }
        self.schematic.advance(ns)
    }
    pub fn step(&mut self) -> Result<usize, SimError> {
        #[cfg(feature = "icarus")]
        if let Some(hdl) = &mut self.hdl {
            return hdl.step();
        }
        self.schematic.step()
    }
    pub fn value(&self, net: NetId) -> Option<&Signal> {
        #[cfg(feature = "icarus")]
        if let Some(hdl) = &self.hdl {
            return hdl.value(net);
        }
        self.schematic.value(net)
    }
    pub fn gate_value(&self, gate: GateId) -> Option<&Signal> {
        #[cfg(feature = "icarus")]
        if let Some(hdl) = &self.hdl {
            return hdl.value(*self.inputs.get(&gate)?);
        }
        self.schematic.gate_value(gate)
    }
    pub fn set_input(&mut self, gate: GateId, value: Signal) -> Result<(), SimError> {
        #[cfg(feature = "icarus")]
        if let Some(hdl) = &mut self.hdl {
            return hdl.set_input(gate, value);
        }
        self.schematic.set_input(gate, value)
    }
    pub fn toggle_input(&mut self, gate: GateId) -> Result<(), SimError> {
        #[cfg(feature = "icarus")]
        if self.hdl.is_some() {
            let value = self.gate_value(gate).ok_or(SimError::NotInput(gate))?;
            let next = if value.to_u64() == Some(0) { 1 } else { 0 };
            return self.set_input(gate, Signal::from_u64(next, value.width()));
        }
        self.schematic.toggle_input(gate)
    }
    pub fn probe(&mut self, net: NetId) -> Result<(), SimError> {
        #[cfg(feature = "icarus")]
        if let Some(hdl) = &mut self.hdl {
            return hdl.probe(net);
        }
        self.schematic.probe(net)
    }
    pub fn unprobe(&mut self, net: NetId) {
        #[cfg(feature = "icarus")]
        if let Some(hdl) = &mut self.hdl {
            hdl.unprobe(net);
            return;
        }
        self.schematic.unprobe(net);
    }
    pub fn label_probe(&mut self, net: NetId, name: String) {
        #[cfg(feature = "icarus")]
        if let Some(hdl) = &mut self.hdl {
            hdl.label_probe(net, name);
            return;
        }
        self.schematic.label_probe(net, name);
    }
    pub fn traces(&self) -> &BTreeMap<NetId, Trace> {
        #[cfg(feature = "icarus")]
        if let Some(hdl) = &self.hdl {
            return hdl.traces();
        }
        self.schematic.traces()
    }
    #[cfg(feature = "icarus")]
    pub fn verilog(circuit: &rgate_core::Circuit, root: &str) -> Result<Self, SimError> {
        let exported = rgate_format::export_interactive(circuit)
            .map_err(|e| SimError::Invalid(e.to_string()))?;
        let mut metadata = circuit.clone();
        for module in &mut metadata.modules {
            module.verilog = None;
        }
        let schematic = Simulator::from_circuit(&metadata, root)?;
        let mut bindings = Vec::new();
        let mut inputs = BTreeMap::new();
        let mut scopes = schematic.scopes().iter().collect::<Vec<_>>();
        scopes.sort_by_key(|(path, _)| path.len());
        for (scope, mapping) in scopes {
            let module = circuit.module(&mapping.module).unwrap();
            let relative = scope
                .strip_prefix(root)
                .unwrap()
                .trim_start_matches('/')
                .replace('/', ".");
            let prefix = if relative.is_empty() {
                String::new()
            } else {
                format!("{relative}.")
            };
            for net in &module.nets {
                let id = mapping.nets[&net.id];
                if bindings.iter().any(|s: &rgate_hdl::HdlSignal| s.net == id) {
                    continue;
                }
                bindings.push(rgate_hdl::HdlSignal {
                    net: id,
                    path: format!("{prefix}{}", net.name),
                    width: net.width,
                    input: None,
                });
            }
            for gate in &module.gates {
                if matches!(
                    gate.kind,
                    rgate_core::GateKind::Switch
                        | rgate_core::GateKind::Dip
                        | rgate_core::GateKind::Peripheral
                ) && let Some(net) = gate.pin("Z").and_then(|p| p.net)
                {
                    let runtime = mapping.gates[&gate.id];
                    inputs.insert(runtime, mapping.nets[&net]);
                    let id = NetId(
                        bindings
                            .iter()
                            .map(|s| s.net.0)
                            .max()
                            .unwrap_or(0)
                            .max(schematic.module().next_net_id().0)
                            + 100_000
                            + runtime.0,
                    );
                    bindings.push(rgate_hdl::HdlSignal {
                        net: id,
                        path: format!("{prefix}__rgate_input_{}", gate.id.0),
                        width: gate.width,
                        input: Some(runtime),
                    });
                }
            }
        }
        let mut sources = vec![exported];
        // A source definition can run independently: expose declared input ports
        // through host-owned registers. Users supply clocks in a testbench source.
        let top = if circuit.module(root).unwrap().verilog.is_some() {
            let module = circuit.module(root).unwrap();
            let mut wrapper = String::from("`timescale 1ns/1ps\nmodule rgate_hdl_host;\n");
            let mut ports = Vec::new();
            for net in module.nets.iter().filter(|n| n.port.is_some()) {
                if net.port == Some(rgate_core::Direction::Input) {
                    wrapper.push_str(&format!("reg [{}:0] {}=0;\n", net.width - 1, net.name));
                    let gate = GateId(1_000_000 + net.id.0);
                    inputs.insert(gate, net.id);
                    bindings.push(rgate_hdl::HdlSignal {
                        net: NetId(2_000_000 + net.id.0),
                        path: net.name.clone(),
                        width: net.width,
                        input: Some(gate),
                    });
                } else {
                    wrapper.push_str(&format!("wire [{}:0] {};\n", net.width - 1, net.name));
                }
                ports.push(format!(".{}({})", net.name, net.name));
            }
            wrapper.push_str(&format!("{root} dut({});\nendmodule\n", ports.join(",")));
            for binding in &mut bindings {
                if binding.input.is_none() {
                    binding.path = format!("dut.{}", binding.path);
                }
            }
            sources.push(wrapper);
            "rgate_hdl_host"
        } else {
            root
        };
        let hdl = rgate_hdl::IcarusBackend::new(sources, top, bindings)?;
        Ok(Self {
            schematic,
            hdl: Some(hdl),
            inputs,
        })
    }
    #[cfg(feature = "icarus")]
    pub fn source_input(&self, net: NetId) -> Option<GateId> {
        self.inputs
            .iter()
            .find_map(|(gate, id)| (*id == net && gate.0 >= 1_000_000).then_some(*gate))
    }
    #[cfg(feature = "icarus")]
    pub fn pwm(circuit: &rgate_core::Circuit) -> Result<Self, SimError> {
        let reference = rgate_format::parse(include_str!("../../../examples/pwm-dimmer.rgate"))
            .map_err(|e| SimError::Invalid(e.to_string()))?
            .circuit;
        // This explicit binding experiment is not HDL synthesis: never run stale
        // bindings after edits to the circuit, even if its title still matches.
        if serde_json::to_value(circuit).unwrap() != serde_json::to_value(&reference).unwrap() {
            return Err(SimError::Invalid("Icarus PWM prototype requires the unmodified bundled PWM circuit. Reopen File → PWM LED dimmer, or use Rust simulation for edits.".into()));
        }
        let schematic = Simulator::from_circuit(circuit, "main")?;
        let mut bindings = Vec::new();
        let mut inputs = BTreeMap::new();
        for (scope, module_name) in [("main", "main"), ("main/dimmer", "PwmDimmer")] {
            let module = circuit.module(module_name).unwrap();
            for net in &module.nets {
                let id = schematic
                    .scoped_net(scope, net.id)
                    .ok_or_else(|| SimError::Invalid(format!("missing scope {scope}")))?;
                if bindings
                    .iter()
                    .any(|binding: &rgate_hdl::HdlSignal| binding.net == id)
                {
                    continue;
                }
                let path = if scope == "main" {
                    net.name.clone()
                } else {
                    format!("dimmer_view.{}", net.name)
                };
                let input = if scope == "main" && matches!(net.name.as_str(), "duty" | "RESET_N") {
                    module
                        .gates
                        .iter()
                        .find(|g| {
                            g.pins.iter().any(|p| {
                                p.net == Some(net.id)
                                    && p.direction == rgate_core::Direction::Output
                            })
                        })
                        .map(|g| g.id)
                } else {
                    None
                };
                for gate in &module.gates {
                    if gate.kind.is_source()
                        && gate.pins.iter().any(|pin| {
                            pin.net == Some(net.id)
                                && pin.direction == rgate_core::Direction::Output
                        })
                        && let Some(runtime) = schematic.scoped_gate(scope, gate.id)
                    {
                        inputs.insert(runtime, id);
                    }
                }
                bindings.push(rgate_hdl::HdlSignal {
                    net: id,
                    path,
                    width: net.width,
                    input,
                });
            }
        }
        let hdl = rgate_hdl::IcarusBackend::new(
            vec![
                include_str!("../../../examples/verilog/projectf/pwm.sv").into(),
                include_str!("../../../examples/verilog/harnesses/pwm-rgate.sv").into(),
            ],
            "pwm_rgate",
            bindings,
        )?;
        Ok(Self {
            schematic,
            hdl: Some(hdl),
            inputs,
        })
    }
}

#[cfg(all(test, feature = "icarus"))]
mod tests {
    use super::*;
    #[test]
    fn pwm_binding_refuses_modified_circuit_instead_of_running_stale_hdl() {
        let mut circuit = rgate_format::parse(include_str!("../../../examples/pwm-dimmer.rgate"))
            .unwrap()
            .circuit;
        circuit.modules[0].gates[0].period = 200;
        assert!(
            LiveSimulation::pwm(&circuit)
                .err()
                .unwrap()
                .to_string()
                .contains("unmodified")
        );
    }
}

#[cfg(all(test, feature = "icarus"))]
mod source_tests {
    use super::*;
    #[test]
    fn source_edits_change_executed_hdl_and_compile_errors_are_reported() {
        let mut circuit = rgate_format::parse(include_str!("../../../examples/pwm-verilog.rgate"))
            .unwrap()
            .circuit;
        let source = circuit.module_mut("pwm").unwrap().verilog.as_mut().unwrap();
        *source = source.replace("cnt < duty", "cnt >= duty");
        let mut sim = LiveSimulation::verilog(&circuit, "main").unwrap();
        sim.advance(100).unwrap();
        let net = circuit
            .module("main")
            .unwrap()
            .nets
            .iter()
            .find(|n| n.name == "pwm_out")
            .unwrap()
            .id;
        assert_eq!(sim.value(net).unwrap().to_u64(), Some(0));
        circuit.module_mut("pwm").unwrap().verilog = Some("module pwm( !!! endmodule".into());
        assert!(
            LiveSimulation::verilog(&circuit, "main")
                .err()
                .unwrap()
                .to_string()
                .contains("iverilog")
        );
    }
}
