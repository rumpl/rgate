//! Experimental deterministic replay adapter, NOT a resumable xezim scheduler.
//! Each operation elaborates and runs HDL from time zero with the input journal.
use crate::HdlSignal;
use rgate_core::{GateId, Logic, NetId, Signal};
use rgate_sim::{SimError, SimulationBackend, Trace, Transition};
use std::{collections::BTreeMap, sync::Mutex};

// xezim has process-global configuration and VPI state. Serialize library calls.
static ENGINE: Mutex<()> = Mutex::new(());
const MAX_REPLAY_NS: u64 = 1_000_000;
const MAX_INPUT_CHANGES: usize = 4096;
const MAX_BINDINGS: usize = 256;

pub struct XezimBackend {
    sources: Vec<String>,
    top: String,
    signals: Vec<HdlSignal>,
    inputs: Vec<(u64, GateId, Signal)>,
    time: u64,
    values: BTreeMap<NetId, Signal>,
    traces: BTreeMap<NetId, Trace>,
}

impl XezimBackend {
    pub fn new(sources: Vec<String>, top: &str, signals: Vec<HdlSignal>) -> Result<Self, SimError> {
        if signals.len() > MAX_BINDINGS {
            return Err(SimError::Invalid(
                "HDL prototype supports at most 256 bindings".into(),
            ));
        }
        if !identifier(top)
            || signals
                .iter()
                .any(|s| s.width == 0 || s.width > 4096 || !s.path.split('.').all(identifier))
        {
            return Err(SimError::Invalid(
                "invalid HDL top, signal path, or width".into(),
            ));
        }
        let mut nets = std::collections::BTreeSet::new();
        let mut gates = std::collections::BTreeSet::new();
        let mut paths = std::collections::BTreeSet::new();
        for signal in &signals {
            if !nets.insert(signal.net)
                || !paths.insert(&signal.path)
                || signal.input.is_some_and(|gate| !gates.insert(gate))
            {
                return Err(SimError::Invalid("duplicate HDL binding ID".into()));
            }
        }
        let mut backend = Self {
            sources,
            top: top.into(),
            signals,
            inputs: Vec::new(),
            time: 0,
            values: BTreeMap::new(),
            traces: BTreeMap::new(),
        };
        backend.replay(0)?;
        Ok(backend)
    }

    pub fn signals(&self) -> &[HdlSignal] {
        &self.signals
    }

    fn replay(&mut self, target: u64) -> Result<usize, SimError> {
        if target > MAX_REPLAY_NS {
            return Err(SimError::Invalid(
                "HDL prototype replay limit is 1000000 ns".into(),
            ));
        }
        let _guard = ENGINE
            .lock()
            .map_err(|_| SimError::Invalid("xezim lock poisoned".into()))?;
        let mut harness = format!(
            "`timescale 1ns/1ps\nmodule rgate_xezim_host; {} dut();\n initial begin\n",
            self.top
        );
        let mut previous = 0;
        for (time, gate, value) in &self.inputs {
            let signal = self
                .signals
                .iter()
                .find(|s| s.input == Some(*gate))
                .unwrap();
            let bits = value
                .bits()
                .iter()
                .rev()
                .map(ToString::to_string)
                .collect::<String>();
            harness.push_str(&format!(
                "#{} dut.{} = {}'b{};\n",
                time - previous,
                signal.path,
                signal.width,
                bits
            ));
            previous = *time;
        }
        harness.push_str("end\n");
        // Keep an otherwise idle design alive; the library's public API runs to
        // a limit, rather than offering an interactive pause/resume handle.
        harness.push_str("reg heartbeat=0; always #1 heartbeat=~heartbeat;\n");
        let probes = self
            .signals
            .iter()
            .filter(|s| self.traces.contains_key(&s.net))
            .collect::<Vec<_>>();
        if !probes.is_empty() {
            let print = format!(
                "$strobe(\"RGATE_TRACE{}\"{});",
                "|%b".repeat(probes.len()),
                probes
                    .iter()
                    .map(|s| format!(", dut.{}", s.path))
                    .collect::<String>()
            );
            // $monitor is a singleton shared with user HDL. Use postponed
            // sampling instead, so NBA updates are visible without replacing it.
            harness.push_str(&format!(
                "initial {print}\nalways @({}) {print}\n",
                probes
                    .iter()
                    .map(|s| format!("dut.{}", s.path))
                    .collect::<Vec<_>>()
                    .join(" or ")
            ));
        }
        harness.push_str("endmodule\n");
        let mut sources = self.sources.clone();
        sources.push(harness);
        xezim::compiler::simulator::set_run_length_requested(true);
        let result = xezim::simulate_multi(
            &sources,
            target,
            Some("rgate_xezim_host"),
            &[],
            &[],
            Some(1000),
            false,
            None,
            None,
            &[],
            &[],
            None,
            &[],
            0,
            u64::MAX,
            None,
            &[],
            None,
            None,
            None,
            None,
            false,
        );
        xezim::compiler::simulator::set_run_length_requested(false);
        let sim = result.map_err(SimError::Invalid)?;
        if sim.saw_fatal || sim.error_count > 0 || sim.stuck_clock_aborted {
            return Err(SimError::Invalid(format!(
                "xezim HDL failure: fatal={}, errors={}, stuck={}",
                sim.saw_fatal, sim.error_count, sim.stuck_clock_aborted
            )));
        }
        if sim.finished {
            return Err(SimError::Invalid(
                "HDL called $finish/$stop; interactive replay requires an unfinished harness"
                    .into(),
            ));
        }
        let mut values = BTreeMap::new();
        for signal in &self.signals {
            let path = format!("dut.{}", signal.path);
            let value = sim
                .get_signal(&path)
                .ok_or_else(|| SimError::Invalid(format!("missing HDL signal {path}")))?;
            if value.width != u32::from(signal.width) {
                return Err(SimError::Invalid(format!(
                    "HDL width mismatch for {}: expected {}, got {}",
                    signal.path, signal.width, value.width
                )));
            }
            values.insert(
                signal.net,
                Signal::from_bits(
                    (0..usize::from(signal.width))
                        .map(|bit| match value.get_bit(bit) {
                            xezim::compiler::value::LogicBit::Zero => Logic::Low,
                            xezim::compiler::value::LogicBit::One => Logic::High,
                            xezim::compiler::value::LogicBit::X => Logic::Unknown,
                            xezim::compiler::value::LogicBit::Z => Logic::HighZ,
                        })
                        .collect(),
                ),
            );
        }
        let mut traces = self.traces.clone();
        for trace in traces.values_mut() {
            trace.transitions.clear();
        }
        for output in &sim.output {
            if let Some(message) = output.message.strip_prefix("RGATE_TRACE|") {
                let fields = message.trim().split('|').collect::<Vec<_>>();
                if fields.len() != probes.len() {
                    return Err(SimError::Invalid("malformed xezim trace".into()));
                }
                let time = (output.time as f64 * sim.tick_s * 1e9).round() as u64;
                for (signal, bits) in probes.iter().zip(fields) {
                    let value = parse_bits(bits, signal.width)?;
                    let trace = traces.get_mut(&signal.net).unwrap();
                    if trace
                        .transitions
                        .last()
                        .is_none_or(|last| last.value != value)
                    {
                        trace.transitions.push(Transition { time, value });
                        if trace.transitions.len() > 4096 {
                            trace.transitions.remove(0);
                        }
                    }
                }
            }
        }
        // The trait's usize is backend-specific work, not a shared event count.
        let changes = values
            .iter()
            .filter(|(net, value)| self.values.get(net) != Some(*value))
            .count();
        self.values = values;
        self.traces = traces;
        self.time = target;
        Ok(changes)
    }
}

fn identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}
fn parse_bits(bits: &str, width: u16) -> Result<Signal, SimError> {
    if bits.len() != usize::from(width) {
        return Err(SimError::Invalid(format!(
            "invalid xezim trace width: {bits}"
        )));
    }
    Ok(Signal::from_bits(
        bits.chars()
            .rev()
            .map(|c| match c {
                '0' => Ok(Logic::Low),
                '1' => Ok(Logic::High),
                'x' | 'X' => Ok(Logic::Unknown),
                'z' | 'Z' => Ok(Logic::HighZ),
                _ => Err(SimError::Invalid(format!("invalid xezim bit {c}"))),
            })
            .collect::<Result<_, _>>()?,
    ))
}

impl SimulationBackend for XezimBackend {
    fn time(&self) -> u64 {
        self.time
    }
    fn advance(&mut self, ns: u64) -> Result<usize, SimError> {
        self.replay(self.time.checked_add(ns).ok_or(SimError::TimeOverflow)?)
    }
    /// Prototype quantum step: one nanosecond, NOT the next queued event.
    fn step(&mut self) -> Result<usize, SimError> {
        self.advance(1)
    }
    fn value(&self, net: NetId) -> Option<&Signal> {
        self.values.get(&net)
    }
    fn set_input(&mut self, gate: GateId, value: Signal) -> Result<(), SimError> {
        let signal = self
            .signals
            .iter()
            .find(|s| s.input == Some(gate))
            .ok_or(SimError::NotInput(gate))?;
        if signal.width != value.width() {
            return Err(SimError::Invalid("HDL input width mismatch".into()));
        }
        if self.inputs.len() >= MAX_INPUT_CHANGES {
            return Err(SimError::Invalid(
                "HDL input journal limit is 4096 changes".into(),
            ));
        }
        self.inputs.push((self.time, gate, value));
        if let Err(error) = self.replay(self.time) {
            self.inputs.pop();
            return Err(error);
        }
        Ok(())
    }
    fn probe(&mut self, net: NetId) -> Result<(), SimError> {
        if self.traces.contains_key(&net) {
            return Ok(());
        }
        let signal = self
            .signals
            .iter()
            .find(|s| s.net == net)
            .ok_or(SimError::UnknownNet(net))?;
        self.traces.insert(
            net,
            Trace {
                net,
                name: format!("{}.{}", self.top, signal.path),
                width: signal.width,
                transitions: Vec::new(),
            },
        );
        if let Err(error) = self.replay(self.time) {
            self.traces.remove(&net);
            return Err(error);
        }
        Ok(())
    }
    fn traces(&self) -> &BTreeMap<NetId, Trace> {
        &self.traces
    }
}
