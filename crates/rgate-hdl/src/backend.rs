//! In-process pure-Rust Verilog simulation backend implementing `SimulationBackend`.

use crate::HdlSignal;
use crate::elaborate::{SigId, elaborate};
use crate::lexer::tokenize;
use crate::parser::parse_modules;
use crate::preprocess::Preprocessor;
use crate::sim::VerilogSim;
use crate::value::BitVec;
use rgate_core::{GateId, NetId, Signal};
use rgate_sim::{SimError, SimulationBackend, Trace, Transition};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Debug)]
pub struct DiscoveredSignal {
    pub path: String,
    pub width: u16,
}

pub struct VerilogBackend {
    sim: VerilogSim,
    top: String,
    signals: Vec<HdlSignal>,
    sig_ids: Vec<SigId>,
    sig_to_nets: HashMap<SigId, Vec<NetId>>,
    discovered: Vec<DiscoveredSignal>,
    values: BTreeMap<NetId, Signal>,
    traces: BTreeMap<NetId, Trace>,
}

impl VerilogBackend {
    pub fn new(sources: Vec<String>, top: &str, signals: Vec<HdlSignal>) -> Result<Self, SimError> {
        Self::new_cancellable(sources, top, signals, &Arc::new(AtomicBool::new(false)))
    }

    pub fn new_cancellable(
        sources: Vec<String>,
        top: &str,
        signals: Vec<HdlSignal>,
        cancel: &Arc<AtomicBool>,
    ) -> Result<Self, SimError> {
        if cancel.load(Ordering::Relaxed) {
            return Err(error("Compilation cancelled"));
        }
        if !identifier(top)
            || signals.len() > 256
            || signals
                .iter()
                .any(|s| s.width == 0 || s.width > 4096 || !s.path.split('.').all(identifier))
        {
            return Err(error(
                "invalid HDL top, signal path, width, or binding count",
            ));
        }
        let mut nets = BTreeSet::new();
        let mut gates = BTreeSet::new();
        let mut paths = BTreeSet::new();
        for signal in &signals {
            if !nets.insert(signal.net)
                || !paths.insert(&signal.path)
                || signal.input.is_some_and(|gate| !gates.insert(gate))
            {
                return Err(error("duplicate HDL binding"));
            }
        }

        let mut pp = Preprocessor::new();
        let mut modules = Vec::new();
        for (index, source) in sources.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                return Err(error("Compilation cancelled"));
            }
            let pre = pp
                .process(&format!("source{index}.sv"), source)
                .map_err(error)?;
            let tokens = tokenize(&pre).map_err(error)?;
            let mut parsed = parse_modules(tokens, pre.timescale).map_err(error)?;
            modules.append(&mut parsed);
        }

        if cancel.load(Ordering::Relaxed) {
            return Err(error("Compilation cancelled"));
        }
        let design = elaborate(&modules, top).map_err(error)?;

        if cancel.load(Ordering::Relaxed) {
            return Err(error("Compilation cancelled"));
        }
        let sim = VerilogSim::new(design).map_err(error)?;

        let discovered: Vec<DiscoveredSignal> = sim
            .design
            .signals
            .iter()
            .filter(|s| s.visible && s.width >= 1 && s.width <= 4096)
            .map(|s| DiscoveredSignal {
                path: s.path.clone(),
                width: s.width as u16,
            })
            .collect();

        let mut sig_ids = Vec::with_capacity(signals.len());
        let mut sig_to_nets: HashMap<SigId, Vec<NetId>> = HashMap::new();
        let mut values = BTreeMap::new();

        for signal in &signals {
            let full_path = format!("{top}.{}", signal.path);
            let &sig_id = sim
                .design
                .path_to_sig
                .get(&full_path)
                .ok_or_else(|| error(format!("missing HDL signal {full_path}")))?;
            let actual_width = sim.design.signals[sig_id.0].width;
            if actual_width != usize::from(signal.width) {
                return Err(error(format!(
                    "width mismatch for {full_path}: expected {}, got {actual_width}",
                    signal.width
                )));
            }
            sig_ids.push(sig_id);
            sig_to_nets.entry(sig_id).or_default().push(signal.net);
            values.insert(signal.net, sim.value(sig_id).to_signal());
        }

        Ok(Self {
            sim,
            top: top.to_string(),
            signals,
            sig_ids,
            sig_to_nets,
            discovered,
            values,
            traces: BTreeMap::new(),
        })
    }

    /// Bind a discovered packed signal without recompiling or resetting time.
    pub fn bind_discovered(
        &mut self,
        path: &str,
        input: bool,
    ) -> Result<(NetId, Option<GateId>), SimError> {
        let relative = path
            .strip_prefix(&format!("{}.", self.top))
            .ok_or_else(|| error("signal belongs to a different top"))?;
        if let Some(signal) = self.signals.iter().find(|s| s.path == relative) {
            return Ok((signal.net, signal.input));
        }
        if self.signals.len() >= 256 {
            return Err(error("live signal binding limit is 256"));
        }
        let &sig_id = self
            .sim
            .design
            .path_to_sig
            .get(path)
            .ok_or_else(|| error("unknown HDL signal"))?;
        let width = self.sim.design.signals[sig_id.0].width;
        if width == 0 || width > 4096 || path.chars().any(char::is_whitespace) {
            return Err(error("unsupported signal path or width"));
        }
        let net = NetId(self.signals.iter().map(|s| s.net.0).max().unwrap_or(0) + 1);
        let gate = input.then_some(GateId(3_000_000 + net.0));
        self.signals.push(HdlSignal {
            net,
            path: relative.into(),
            width: width as u16,
            input: gate,
        });
        self.sig_ids.push(sig_id);
        self.sig_to_nets.entry(sig_id).or_default().push(net);
        self.values.insert(net, self.sim.value(sig_id).to_signal());
        Ok((net, gate))
    }

    pub fn unprobe(&mut self, net: NetId) {
        self.traces.remove(&net);
        if let Some(idx) = self.signals.iter().position(|s| s.net == net) {
            let sig_id = self.sig_ids[idx];
            let still_probed = self
                .sig_to_nets
                .get(&sig_id)
                .is_some_and(|nets| nets.iter().any(|n| self.traces.contains_key(n)));
            if !still_probed {
                self.sim.unwatch(sig_id);
            }
        }
    }

    pub fn label_probe(&mut self, net: NetId, name: String) {
        if let Some(trace) = self.traces.get_mut(&net) {
            trace.name = name;
        }
    }

    pub fn signals(&self) -> &[HdlSignal] {
        &self.signals
    }

    pub fn discovered_signals(&self) -> &[DiscoveredSignal] {
        &self.discovered
    }

    /// Exact engine time in ticks; `SimulationBackend::time()` rounds down to ns.
    pub fn time_ticks(&self) -> u64 {
        self.sim.now
    }

    pub fn ticks_per_nanosecond(&self) -> u64 {
        self.sim.design.ticks_per_ns
    }

    fn sync_values_and_traces(&mut self) -> usize {
        let scale = self.sim.design.ticks_per_ns;
        let changes = self.sim.take_changes();
        let mut work = 0usize;
        for (ticks, sig_id, bv) in changes {
            if let Some(nets) = self.sig_to_nets.get(&sig_id) {
                let sig_val = bv.to_signal();
                for &net in nets {
                    if let Some(trace) = self.traces.get_mut(&net)
                        && trace
                            .transitions
                            .last()
                            .is_none_or(|last| last.value != sig_val)
                    {
                        trace.transitions.push(Transition {
                            time: ticks / scale,
                            value: sig_val.clone(),
                        });
                        work += 1;
                        if trace.transitions.len() > 4096 {
                            trace.transitions.remove(0);
                        }
                    }
                }
            }
        }
        // Refresh bound signal snapshots
        for (signal, &sig_id) in self.signals.iter().zip(self.sig_ids.iter()) {
            self.values
                .insert(signal.net, self.sim.value(sig_id).to_signal());
        }
        work
    }
}

impl SimulationBackend for VerilogBackend {
    fn time(&self) -> u64 {
        self.sim.now / self.sim.design.ticks_per_ns
    }

    fn advance(&mut self, ns: u64) -> Result<usize, SimError> {
        let scale = self.sim.design.ticks_per_ns;
        let delay = ns.checked_mul(scale).ok_or(SimError::TimeOverflow)?;
        self.sim
            .now
            .checked_add(delay)
            .ok_or(SimError::TimeOverflow)?;
        self.sim.advance_ticks(delay).map_err(error)?;
        Ok(self.sync_values_and_traces())
    }

    fn step(&mut self) -> Result<usize, SimError> {
        self.sim.step().map_err(error)?;
        Ok(self.sync_values_and_traces())
    }

    fn value(&self, net: NetId) -> Option<&Signal> {
        self.values.get(&net)
    }

    fn set_input(&mut self, gate: GateId, value: Signal) -> Result<(), SimError> {
        let index = self
            .signals
            .iter()
            .position(|s| s.input == Some(gate))
            .ok_or(SimError::NotInput(gate))?;
        if value.width() != self.signals[index].width {
            return Err(error("HDL input width mismatch"));
        }
        let sig_id = self.sig_ids[index];
        let bv = BitVec::from_signal(&value);
        self.sim.set_signal(sig_id, bv).map_err(error)?;
        self.sync_values_and_traces();
        Ok(())
    }

    fn probe(&mut self, net: NetId) -> Result<(), SimError> {
        if self.traces.contains_key(&net) {
            return Ok(());
        }
        let index = self
            .signals
            .iter()
            .position(|s| s.net == net)
            .ok_or(SimError::UnknownNet(net))?;
        let signal = &self.signals[index];
        let sig_id = self.sig_ids[index];
        self.sim.watch(sig_id);
        let initial = self.sim.value(sig_id).to_signal();
        self.traces.insert(
            net,
            Trace {
                net,
                name: signal.path.clone(),
                width: signal.width,
                transitions: vec![Transition {
                    time: self.time(),
                    value: initial,
                }],
            },
        );
        Ok(())
    }

    fn traces(&self) -> &BTreeMap<NetId, Trace> {
        &self.traces
    }
}

/// Syntax-check Verilog/SystemVerilog sources without elaborating or launching simulation.
pub fn check_sources(sources: &[(String, String)]) -> Result<(), SimError> {
    let mut pp = Preprocessor::new();
    for (name, source) in sources {
        let filename = format!("{name}.sv");
        let pre = pp.process(&filename, source).map_err(error)?;
        let tokens = tokenize(&pre).map_err(error)?;
        parse_modules(tokens, pre.timescale).map_err(error)?;
    }
    Ok(())
}

fn error(message: impl Into<String>) -> SimError {
    SimError::Invalid(message.into())
}

fn identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}
