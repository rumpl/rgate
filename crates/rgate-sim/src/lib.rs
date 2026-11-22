mod backend;
mod components;
mod eval;
mod vcd;
mod vga;
pub use backend::SimulationBackend;
pub use vcd::export_vcd;
pub use vga::VgaFrame;

use rgate_core::{Circuit, Direction, GateId, GateKind, Logic, Module, NetId, Signal};
use std::{
    cmp::{Ordering, Reverse},
    collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap},
};
use thiserror::Error;

const MAX_EVENTS_PER_ADVANCE: usize = 100_000;
const MAX_TRACE_CHANGES: usize = 4096;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Transition {
    pub time: u64,
    pub value: Signal,
}

#[derive(Clone, Debug)]
pub struct Trace {
    pub net: NetId,
    pub name: String,
    pub width: u16,
    pub transitions: Vec<Transition>,
}

#[derive(Debug, Error)]
pub enum SimError {
    #[error("invalid circuit: {0}")]
    Invalid(String),
    #[error("simulation exceeded {MAX_EVENTS_PER_ADVANCE} events; possible oscillation")]
    Oscillation,
    #[error("gate {} is not an interactive input", .0.0)]
    NotInput(GateId),
    #[error("simulation time cannot move backwards")]
    Backwards,
    #[error("simulation time overflow")]
    TimeOverflow,
    #[error("unknown net {}", .0.0)]
    UnknownNet(NetId),
}

#[derive(Clone, Debug)]
enum EventKind {
    Drive {
        gate: usize,
        pin: String,
        value: Signal,
        generation: u64,
    },
    Clock(usize),
    TerminalCapture {
        gate: usize,
        byte: u8,
    },
    TerminalConsume {
        gate: usize,
    },
}

#[derive(Clone, Debug)]
struct Event {
    time: u64,
    sequence: u64,
    kind: EventKind,
}

impl PartialEq for Event {
    fn eq(&self, other: &Self) -> bool {
        (self.time, self.sequence) == (other.time, other.sequence)
    }
}
impl Eq for Event {}
impl PartialOrd for Event {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Event {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.time, self.sequence).cmp(&(other.time, other.sequence))
    }
}

#[derive(Clone, Debug)]
struct GateState {
    source: Signal,
    stored: Signal,
    previous_clock: Logic,
    memory: HashMap<usize, Signal>,
    terminal: Vec<u8>,
    received: std::collections::VecDeque<u8>,
    previous_write: Logic,
    previous_read: Logic,
    vga: Option<vga::VgaState>,
}

/// Deterministic event-driven, four-state simulator with per-output inertial delays.
/// Events at the same timestamp are resolved together before downstream gates evaluate.
#[derive(Clone, Debug)]
pub struct Simulator {
    flattened: Module,
    source: Module,
    scopes: HashMap<String, rgate_core::InstanceScope>,
    root_path: String,
    time: u64,
    sequence: u64,
    queue: BinaryHeap<Reverse<Event>>,
    states: Vec<GateState>,
    values: HashMap<NetId, Signal>,
    drivers: HashMap<(usize, String), Signal>,
    intended: HashMap<(usize, String), Signal>,
    generations: HashMap<(usize, String), u64>,
    fanout: HashMap<NetId, Vec<usize>>,
    traces: BTreeMap<NetId, Trace>,
    warnings: Vec<String>,
}

impl Simulator {
    pub fn from_circuit(circuit: &Circuit, name: &str) -> Result<Self, SimError> {
        if circuit
            .modules
            .iter()
            .any(|module| module.verilog.is_some())
        {
            return Err(SimError::Invalid(
                "Verilog source requires the built-in Verilog backend".into(),
            ));
        }
        let flattened = circuit
            .flatten_with_scopes(name)
            .map_err(SimError::Invalid)?;
        let input_ports = circuit
            .module(name)
            .unwrap()
            .gates
            .iter()
            .filter(|gate| gate.kind == GateKind::InputPort)
            .map(|gate| gate.id)
            .collect::<Vec<_>>();
        let mut simulator = Self::with_input_ports(&flattened.module, &input_ports)?;
        simulator.scopes = flattened.scopes;
        simulator.root_path = name.into();
        simulator.source = circuit
            .module(name)
            .ok_or_else(|| SimError::Invalid(format!("missing module {name}")))?
            .clone();
        Ok(simulator)
    }

    pub fn new(module: &Module) -> Result<Self, SimError> {
        let input_ports = module
            .gates
            .iter()
            .filter(|gate| gate.kind == GateKind::InputPort)
            .map(|gate| gate.id)
            .collect::<Vec<_>>();
        Self::with_input_ports(module, &input_ports)
    }

    fn with_input_ports(module: &Module, input_ports: &[GateId]) -> Result<Self, SimError> {
        let source = module.clone();
        let mut runtime = module.clone();
        // Only the simulation root owns its inputs; nested ports remain passive.
        for gate in &mut runtime.gates {
            if input_ports.contains(&gate.id) {
                gate.kind = GateKind::Dip;
                if let Some(pin) = gate.pin_mut("P") {
                    pin.name = "Z".into();
                    pin.direction = Direction::Output;
                }
            }
        }
        let module = &runtime;
        module
            .validate()
            .map_err(|error| SimError::Invalid(error.to_string()))?;
        let states = module.gates.iter().map(GateState::extended).collect();
        let values = module
            .nets
            .iter()
            .map(|net| (net.id, Signal::filled(net.width, Logic::HighZ)))
            .collect();
        let mut simulator = Self {
            flattened: module.clone(),
            source,
            scopes: HashMap::from([(
                module.name.clone(),
                rgate_core::InstanceScope {
                    module: module.name.clone(),
                    nets: module.nets.iter().map(|net| (net.id, net.id)).collect(),
                    gates: module.gates.iter().map(|gate| (gate.id, gate.id)).collect(),
                },
            )]),
            root_path: module.name.clone(),
            time: 0,
            sequence: 0,
            queue: BinaryHeap::new(),
            states,
            values,
            drivers: HashMap::new(),
            intended: HashMap::new(),
            generations: HashMap::new(),
            fanout: HashMap::new(),
            traces: BTreeMap::new(),
            warnings: Vec::new(),
        };
        for (index, gate) in module.gates.iter().enumerate() {
            for pin in &gate.pins {
                if let Some(net) = pin.net {
                    if pin.direction != Direction::Input {
                        let width = module.net(net).unwrap().width;
                        simulator.drivers.insert(
                            (index, pin.name.clone()),
                            Signal::filled(width, Logic::HighZ),
                        );
                    }
                    if pin.direction != Direction::Output {
                        simulator.fanout.entry(net).or_default().push(index);
                    }
                }
            }
            if matches!(gate.kind, GateKind::Unsupported(_) | GateKind::Module(_)) {
                simulator.warnings.push(format!(
                    "{} ({}) is not simulated; outputs remain X",
                    gate.name,
                    gate.kind.name()
                ));
            }
            if gate.kind == GateKind::Clock {
                simulator.states[index].source = Signal::from_u64(0, 1);
                simulator.push(
                    gate.config
                        .clock_phase
                        .saturating_add(gate.clock_low_time()),
                    EventKind::Clock(index),
                );
            }
        }
        for index in 0..module.gates.len() {
            simulator.evaluate(index)?;
        }
        simulator.advance_to(0)?;
        Ok(simulator)
    }

    pub fn root_path(&self) -> &str {
        &self.root_path
    }
    pub fn scopes(&self) -> &HashMap<String, rgate_core::InstanceScope> {
        &self.scopes
    }
    pub fn scoped_net(&self, path: &str, net: NetId) -> Option<NetId> {
        self.scopes.get(path)?.nets.get(&net).copied()
    }
    pub fn scoped_gate(&self, path: &str, gate: GateId) -> Option<GateId> {
        self.scopes.get(path)?.gates.get(&gate).copied()
    }
    pub fn scope_module(&self, path: &str) -> Option<&str> {
        Some(&self.scopes.get(path)?.module)
    }
    pub fn clock_period(&self) -> u64 {
        self.flattened
            .gates
            .iter()
            .filter(|gate| gate.kind == GateKind::Clock)
            .map(|gate| gate.period)
            .min()
            .unwrap_or(100)
    }

    pub fn time(&self) -> u64 {
        self.time
    }
    pub fn module(&self) -> &Module {
        &self.source
    }
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }
    pub fn traces(&self) -> &BTreeMap<NetId, Trace> {
        &self.traces
    }
    pub fn value(&self, net: NetId) -> Option<&Signal> {
        self.values.get(&net)
    }

    pub fn gate_value(&self, id: GateId) -> Option<&Signal> {
        let index = self.flattened.gates.iter().position(|gate| gate.id == id)?;
        let gate = &self.flattened.gates[index];
        if gate.kind.is_source() {
            Some(&self.states[index].source)
        } else {
            gate.pins
                .iter()
                .find_map(|pin| pin.net.and_then(|net| self.value(net)))
        }
    }

    pub fn probe(&mut self, net: NetId) -> Result<(), SimError> {
        if self.traces.contains_key(&net) {
            return Ok(());
        }
        let definition = self.flattened.net(net).ok_or(SimError::UnknownNet(net))?;
        self.traces.insert(
            net,
            Trace {
                net,
                name: definition.name.clone(),
                width: definition.width,
                transitions: vec![Transition {
                    time: self.time,
                    value: self.values[&net].clone(),
                }],
            },
        );
        Ok(())
    }

    pub fn label_probe(&mut self, net: NetId, name: String) {
        if let Some(trace) = self.traces.get_mut(&net) {
            trace.name = name;
        }
    }

    pub fn unprobe(&mut self, net: NetId) {
        self.traces.remove(&net);
    }

    pub fn is_input(&self, id: GateId) -> bool {
        self.flattened.gates.iter().any(|gate| {
            gate.id == id
                && matches!(
                    gate.kind,
                    GateKind::Switch | GateKind::Dip | GateKind::Peripheral
                )
        })
    }

    pub fn set_input(&mut self, id: GateId, value: Signal) -> Result<(), SimError> {
        let index = self
            .flattened
            .gates
            .iter()
            .position(|gate| gate.id == id)
            .ok_or(SimError::NotInput(id))?;
        let gate = &self.flattened.gates[index];
        if !matches!(
            gate.kind,
            GateKind::Switch | GateKind::Dip | GateKind::Peripheral
        ) {
            return Err(SimError::NotInput(id));
        }
        self.states[index].source = value.resized(gate.width);
        self.evaluate(index)?;
        self.advance_to(self.time).map(|_| ())
    }

    pub fn toggle_input(&mut self, id: GateId) -> Result<(), SimError> {
        let current = self.gate_value(id).ok_or(SimError::NotInput(id))?;
        let value = if current.width() == 1 {
            Signal::scalar(if current.bit(0) == Logic::High {
                Logic::Low
            } else {
                Logic::High
            })
        } else {
            current.incremented()
        };
        self.set_input(id, value)
    }

    pub fn advance(&mut self, nanoseconds: u64) -> Result<usize, SimError> {
        let target = self
            .time
            .checked_add(nanoseconds)
            .ok_or(SimError::TimeOverflow)?;
        self.advance_to(target)
    }

    pub fn step(&mut self) -> Result<usize, SimError> {
        let target = self
            .queue
            .peek()
            .map(|event| event.0.time)
            .unwrap_or(self.time.checked_add(1).ok_or(SimError::TimeOverflow)?);
        self.advance_to(target)
    }

    pub fn advance_to(&mut self, target: u64) -> Result<usize, SimError> {
        if target < self.time {
            return Err(SimError::Backwards);
        }
        let mut processed = 0;
        while self
            .queue
            .peek()
            .is_some_and(|event| event.0.time <= target)
        {
            self.time = self.queue.peek().unwrap().0.time;
            let mut affected = BTreeSet::new();
            while self
                .queue
                .peek()
                .is_some_and(|event| event.0.time == self.time)
            {
                if processed >= MAX_EVENTS_PER_ADVANCE {
                    return Err(SimError::Oscillation);
                }
                processed += 1;
                let event = self.queue.pop().unwrap().0;
                match event.kind {
                    EventKind::Drive {
                        gate,
                        pin,
                        value,
                        generation,
                    } => {
                        let key = (gate, pin.clone());
                        if self.generations.get(&key).copied().unwrap_or(0) != generation {
                            continue;
                        }
                        if let Some(net) =
                            self.flattened.gates[gate].pin(&pin).and_then(|pin| pin.net)
                        {
                            self.drivers
                                .insert(key, value.resized(self.flattened.net(net).unwrap().width));
                            affected.insert(net);
                        }
                    }
                    EventKind::TerminalCapture { gate, byte } => {
                        if self.states[gate].terminal.len() >= 65536 {
                            self.states[gate].terminal.drain(..32768);
                        }
                        self.states[gate].terminal.push(byte);
                    }
                    EventKind::TerminalConsume { gate } => {
                        self.states[gate].received.pop_front();
                        self.evaluate(gate)?;
                    }
                    EventKind::Clock(index) => {
                        self.states[index].source = self.states[index].source.map(|bit| !bit);
                        self.evaluate(index)?;
                        let next = self
                            .time
                            .checked_add(if self.states[index].source.bit(0) == Logic::High {
                                self.flattened.gates[index].clock_high_time()
                            } else {
                                self.flattened.gates[index].clock_low_time()
                            })
                            .ok_or(SimError::TimeOverflow)?;
                        self.push(next, EventKind::Clock(index));
                    }
                }
            }
            let mut gates = BTreeSet::new();
            for net in affected {
                let width = self.flattened.net(net).unwrap().width;
                let resolved = self
                    .drivers
                    .iter()
                    .filter(|((index, pin), _)| {
                        self.flattened.gates[*index]
                            .pin(pin)
                            .and_then(|pin| pin.net)
                            == Some(net)
                    })
                    .fold(Signal::filled(width, Logic::HighZ), |value, (_, drive)| {
                        value.zip(drive, Logic::resolve)
                    });
                if self.values[&net] != resolved {
                    self.values.insert(net, resolved.clone());
                    if let Some(trace) = self.traces.get_mut(&net) {
                        if trace
                            .transitions
                            .last()
                            .is_some_and(|change| change.time == self.time)
                        {
                            trace.transitions.last_mut().unwrap().value = resolved;
                        } else {
                            trace.transitions.push(Transition {
                                time: self.time,
                                value: resolved,
                            });
                            if trace.transitions.len() > MAX_TRACE_CHANGES {
                                trace.transitions.remove(0);
                            }
                        }
                    }
                    if let Some(listeners) = self.fanout.get(&net) {
                        gates.extend(listeners);
                    }
                }
            }
            for index in gates {
                self.evaluate(index)?;
            }
        }
        self.time = target;
        Ok(processed)
    }

    pub fn terminal_output(&self, id: GateId) -> Result<String, SimError> {
        let index = self
            .flattened
            .gates
            .iter()
            .position(|gate| gate.id == id && gate.kind == GateKind::Tty)
            .ok_or(SimError::NotInput(id))?;
        Ok(String::from_utf8_lossy(&self.states[index].terminal).into_owned())
    }

    pub fn send_terminal(&mut self, id: GateId, text: &str) -> Result<(), SimError> {
        let index = self
            .flattened
            .gates
            .iter()
            .position(|gate| gate.id == id && gate.kind == GateKind::Tty)
            .ok_or(SimError::NotInput(id))?;
        if self.states[index].received.len() + text.len() > 65536 {
            return Err(SimError::Invalid("terminal input queue is full".into()));
        }
        self.states[index].received.extend(text.as_bytes());
        self.evaluate(index)?;
        self.advance_to(self.time).map(|_| ())
    }

    pub fn memory_words(&self, id: GateId) -> Result<BTreeMap<usize, Signal>, SimError> {
        let index = self
            .flattened
            .gates
            .iter()
            .position(|gate| gate.id == id && matches!(gate.kind, GateKind::Ram | GateKind::Rom))
            .ok_or(SimError::NotInput(id))?;
        Ok(self.states[index]
            .memory
            .iter()
            .map(|(address, value)| (*address, value.clone()))
            .collect())
    }

    pub fn set_memory_word(
        &mut self,
        id: GateId,
        address: usize,
        value: Signal,
    ) -> Result<(), SimError> {
        let index = self
            .flattened
            .gates
            .iter()
            .position(|gate| gate.id == id && matches!(gate.kind, GateKind::Ram | GateKind::Rom))
            .ok_or(SimError::NotInput(id))?;
        let gate = &self.flattened.gates[index];
        if address as u64 >= (1u64 << gate.config.address_bits) {
            return Err(SimError::Invalid("memory address out of range".into()));
        }
        self.states[index]
            .memory
            .insert(address, value.resized(gate.width));
        self.evaluate(index)?;
        self.advance_to(self.time).map(|_| ())
    }

    fn push(&mut self, time: u64, kind: EventKind) {
        self.sequence += 1;
        self.queue.push(Reverse(Event {
            time,
            sequence: self.sequence,
            kind,
        }));
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod component_tests;
