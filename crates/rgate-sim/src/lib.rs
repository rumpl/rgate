mod backend;
mod vcd;
mod vga;
pub use backend::SimulationBackend;
pub use vcd::export_vcd;
pub use vga::VgaFrame;
mod components;

use rgate_core::{Circuit, Direction, Gate, GateId, GateKind, Logic, Module, NetId, Signal};
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
                "Verilog source requires the native Icarus backend".into(),
            ));
        }
        let flattened = circuit
            .flatten_with_scopes(name)
            .map_err(SimError::Invalid)?;
        let mut simulator = Self::new(&flattened.module)?;
        simulator.scopes = flattened.scopes;
        simulator.root_path = name.into();
        simulator.source = circuit
            .module(name)
            .ok_or_else(|| SimError::Invalid(format!("missing module {name}")))?
            .clone();
        Ok(simulator)
    }

    pub fn new(module: &Module) -> Result<Self, SimError> {
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
            source: module.clone(),
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

    fn input(&self, gate: &Gate, name: &str) -> Signal {
        gate.pin(name)
            .and_then(|pin| pin.net)
            .and_then(|net| self.values.get(&net))
            .cloned()
            .unwrap_or_else(|| Signal::filled(gate.pin_width(name), Logic::HighZ))
    }

    fn scalar_input(&self, gate: &Gate, name: &str, default: Logic) -> Logic {
        if gate.pin(name).is_none_or(|pin| pin.net.is_none()) {
            default
        } else {
            self.input(gate, name).bit(0)
        }
    }

    fn evaluate(&mut self, index: usize) -> Result<(), SimError> {
        let gate = &self.flattened.gates[index];
        let mut outputs: Vec<(String, Signal)> = Vec::new();
        let output = match &gate.kind {
            kind if kind.is_logic() => {
                let operation = match kind {
                    GateKind::And | GateKind::Nand | GateKind::ReduceAnd | GateKind::ReduceNand => {
                        Logic::and
                    }
                    GateKind::Or | GateKind::Nor | GateKind::ReduceOr | GateKind::ReduceNor => {
                        Logic::or
                    }
                    _ => Logic::xor,
                };
                let inputs = gate
                    .pins
                    .iter()
                    .filter(|pin| pin.direction == Direction::Input)
                    .map(|pin| self.input(gate, &pin.name).broadcast_or_resize(gate.width));
                let mut value = inputs
                    .reduce(|a, b| a.zip(&b, operation))
                    .unwrap_or_else(|| Signal::filled(gate.width, Logic::Unknown));
                if gate.config.reduction || kind.is_reduction() {
                    value = Signal::scalar(
                        value
                            .bits()
                            .iter()
                            .copied()
                            .reduce(operation)
                            .unwrap_or(Logic::Unknown),
                    );
                }
                Some(
                    if matches!(
                        kind,
                        GateKind::Nand
                            | GateKind::Nor
                            | GateKind::Xnor
                            | GateKind::ReduceNand
                            | GateKind::ReduceNor
                            | GateKind::ReduceXnor
                    ) {
                        value.map(|bit| !bit)
                    } else {
                        value
                    },
                )
            }
            GateKind::Buffer => Some(self.input(gate, "I").map(Logic::buffered)),
            GateKind::Not => Some(self.input(gate, "I").map(|bit| !bit)),
            GateKind::TriState => {
                let enable = self.input(gate, "E").bit(0);
                let active = if gate.config.enable_low {
                    Logic::Low
                } else {
                    Logic::High
                };
                let data = self.input(gate, "I").map(|bit| {
                    if gate.config.invert_output {
                        !bit
                    } else {
                        bit.buffered()
                    }
                });
                Some(if enable == active {
                    data
                } else if matches!(enable, Logic::Low | Logic::High) {
                    Signal::filled(gate.width, Logic::HighZ)
                } else {
                    data.map(|bit| {
                        if bit == Logic::HighZ {
                            Logic::HighZ
                        } else {
                            Logic::Unknown
                        }
                    })
                })
            }
            GateKind::Switch | GateKind::Dip | GateKind::Clock | GateKind::Peripheral => {
                Some(self.states[index].source.clone())
            }
            GateKind::Ground => Some(Signal::from_u64(0, gate.width)),
            GateKind::Vdd => Some(Signal::filled(gate.width, Logic::High)),
            GateKind::Mux => Some(match self.input(gate, "S").to_u64() {
                Some(selection) if selection < u64::from(gate.input_count) => {
                    self.input(gate, &format!("I{selection}"))
                }
                _ => Signal::filled(gate.width, Logic::Unknown),
            }),
            GateKind::Add => {
                let (sum, carry) = self.input(gate, "A").add_with_carry(
                    &self.input(gate, "B"),
                    self.input(gate, "CI").bit(0),
                    gate.width,
                );
                let carry = Signal::scalar(carry);
                outputs.push(("S".into(), sum));
                outputs.push(("CO".into(), carry));
                None
            }
            GateKind::Dff | GateKind::Register => {
                let clock = self.scalar_input(gate, "CK", Logic::Low);
                let clear = self.scalar_input(gate, "CLR", Logic::High);
                let enable = self.scalar_input(gate, "EN", Logic::Low);
                if clear == Logic::Low {
                    self.states[index].stored = Signal::from_u64(0, gate.width);
                } else if clock == Logic::High && self.states[index].previous_clock == Logic::Low {
                    self.states[index].stored = match (clear, enable) {
                        (Logic::High, Logic::Low) => self.input(gate, "D").resized(gate.width),
                        (Logic::High, Logic::High) => self.states[index].stored.clone(),
                        _ => Signal::filled(gate.width, Logic::Unknown),
                    };
                }
                self.states[index].previous_clock = clock;
                outputs.push(("Q".into(), self.states[index].stored.clone()));
                if gate.kind == GateKind::Dff {
                    outputs.push(("_Q".into(), self.states[index].stored.map(|bit| !bit)));
                }
                None
            }
            GateKind::Module(_) | GateKind::Unsupported(_) => {
                outputs.extend(
                    gate.pins
                        .iter()
                        .filter(|pin| pin.direction == Direction::Output)
                        .map(|pin| {
                            (
                                pin.name.clone(),
                                Signal::filled(gate.pin_width(&pin.name), Logic::Unknown),
                            )
                        }),
                );
                None
            }
            kind if kind.is_extended() => None,
            _ => None,
        };
        if let Some(value) = output {
            outputs.push(("Z".into(), value.resized(gate.pin_width("Z"))));
        }
        let delay = if gate.kind.is_source() {
            0
        } else if gate.kind == GateKind::Tty && gate.config.tty_tkgate {
            10
        } else {
            gate.delay
        };
        if gate.kind.is_extended() && gate.kind != GateKind::Peripheral {
            outputs = self.evaluate_component(index);
        }
        for (pin, value) in outputs {
            let key = (index, pin.clone());
            if self.intended.get(&key) == Some(&value) {
                continue;
            }
            self.intended.insert(key.clone(), value.clone());
            let generation = self.generations.entry(key).or_default();
            *generation += 1;
            let generation = *generation;
            let time = self.time.checked_add(delay).ok_or(SimError::TimeOverflow)?;
            self.push(
                time,
                EventKind::Drive {
                    gate: index,
                    pin,
                    value,
                    generation,
                },
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rgate_core::{Net, PinRef, Point, demo};

    #[test]
    fn full_adder_truth_table() {
        let circuit = demo::full_adder();
        let module = circuit.module("main").unwrap();
        for a in 0..2 {
            for b in 0..2 {
                for ci in 0..2 {
                    let mut sim = Simulator::new(module).unwrap();
                    sim.set_input(GateId(1), Signal::from_u64(a, 1)).unwrap();
                    sim.set_input(GateId(2), Signal::from_u64(b, 1)).unwrap();
                    sim.set_input(GateId(3), Signal::from_u64(ci, 1)).unwrap();
                    sim.advance(100).unwrap();
                    assert_eq!(
                        sim.value(NetId(7)).unwrap().to_u64(),
                        Some((a + b + ci) & 1)
                    );
                    assert_eq!(
                        sim.value(NetId(8)).unwrap().to_u64(),
                        Some((a + b + ci) >> 1)
                    );
                }
            }
        }
    }

    #[test]
    fn flip_flop_samples_only_on_rising_edges() {
        let circuit = demo::clocked_flip_flop();
        let mut sim = Simulator::new(circuit.module("main").unwrap()).unwrap();
        sim.probe(NetId(3)).unwrap();
        sim.advance_to(60).unwrap();
        assert_eq!(sim.value(NetId(3)).unwrap().to_u64(), Some(1));
        sim.set_input(GateId(2), Signal::from_u64(0, 1)).unwrap();
        sim.advance_to(120).unwrap();
        assert_eq!(sim.value(NetId(3)).unwrap().to_u64(), Some(1));
        sim.advance_to(160).unwrap();
        assert_eq!(sim.value(NetId(3)).unwrap().to_u64(), Some(0));
        assert!(sim.traces()[&NetId(3)].transitions.len() >= 3);
    }

    #[test]
    fn conflicting_drivers_resolve_to_x() {
        let mut module = Module::new("main");
        module.nets.push(Net::new(NetId(1), "conflict", 1));
        for index in 0..2 {
            let mut gate = Gate::new(GateId(index + 1), GateKind::Switch, Point::ZERO);
            gate.initial = Signal::from_u64(index, 1);
            gate.pin_mut("Z").unwrap().net = Some(NetId(1));
            module.gates.push(gate);
        }
        let sim = Simulator::new(&module).unwrap();
        assert_eq!(sim.value(NetId(1)).unwrap().bit(0), Logic::Unknown);
    }

    #[test]
    fn short_pulses_are_filtered_by_inertial_delay() {
        let mut module = Module::new("main");
        let mut input = Gate::new(GateId(1), GateKind::Switch, Point::ZERO);
        input.pin_mut("Z").unwrap().net = Some(NetId(1));
        let mut buffer = Gate::new(GateId(2), GateKind::Buffer, Point::new(40.0, 0.0));
        buffer.delay = 10;
        buffer.pin_mut("I").unwrap().net = Some(NetId(1));
        buffer.pin_mut("Z").unwrap().net = Some(NetId(2));
        module.gates = vec![input, buffer];
        module.nets = vec![Net::new(NetId(1), "in", 1), Net::new(NetId(2), "out", 1)];
        let mut sim = Simulator::new(&module).unwrap();
        sim.advance(20).unwrap();
        sim.probe(NetId(2)).unwrap();
        sim.toggle_input(GateId(1)).unwrap();
        sim.advance(3).unwrap();
        sim.toggle_input(GateId(1)).unwrap();
        sim.advance(20).unwrap();
        assert_eq!(sim.value(NetId(2)).unwrap().to_u64(), Some(0));
        assert_eq!(sim.traces()[&NetId(2)].transitions.len(), 1);
    }

    #[test]
    fn malformed_clock_is_rejected() {
        let mut module = Module::new("main");
        let mut gate = Gate::new(GateId(1), GateKind::Clock, Point::ZERO);
        gate.period = 0;
        module.gates.push(gate);
        assert!(Simulator::new(&module).is_err());
    }

    #[test]
    fn zero_delay_oscillation_is_bounded() {
        let mut module = Module::new("main");
        module.nets.push(Net::new(NetId(1), "loop", 1));
        let mut gate = Gate::new(GateId(1), GateKind::Not, Point::ZERO);
        gate.delay = 0;
        for pin in ["I", "Z"] {
            gate.pin_mut(pin).unwrap().net = Some(NetId(1));
        }
        module.gates.push(gate);
        // An uninitialized loop settles to X instead of spinning indefinitely.
        let sim = Simulator::new(&module).unwrap();
        assert_eq!(sim.value(NetId(1)).unwrap().bit(0), Logic::Unknown);
        assert!(sim.module().pin(&PinRef::new(GateId(1), "Z")).is_some());
    }

    #[test]
    fn bus_adder_preserves_sum_and_carry() {
        use rgate_core::{Gate, Net, Point};
        let mut module = Module::new("main");
        for (id, name, width) in [
            (1, "A", 8),
            (2, "B", 8),
            (3, "CI", 1),
            (4, "S", 8),
            (5, "CO", 1),
        ] {
            module.nets.push(Net::new(NetId(id), name, width));
        }
        for (id, value, width) in [(1, 255, 8), (2, 2, 8), (3, 1, 1)] {
            let mut gate = Gate::new(GateId(id), GateKind::Dip, Point::ZERO);
            gate.width = width;
            gate.initial = Signal::from_u64(value, width);
            gate.pin_mut("Z").unwrap().net = Some(NetId(id));
            module.gates.push(gate);
        }
        let mut adder = Gate::new(GateId(4), GateKind::Add, Point::ZERO);
        for (pin, net) in [("A", 1), ("B", 2), ("CI", 3), ("S", 4), ("CO", 5)] {
            adder.pin_mut(pin).unwrap().net = Some(NetId(net));
        }
        module.gates.push(adder);
        let mut simulator = Simulator::new(&module).unwrap();
        simulator.advance(20).unwrap();
        assert_eq!(simulator.value(NetId(4)).unwrap().to_u64(), Some(2));
        assert_eq!(simulator.value(NetId(5)).unwrap().to_u64(), Some(1));
    }

    #[test]
    fn tri_state_disabled_output_floats() {
        use rgate_core::{Gate, Net, Point};
        let mut module = Module::new("main");
        for (id, name) in [(1, "data"), (2, "enable"), (3, "out")] {
            module.nets.push(Net::new(NetId(id), name, 1));
        }
        for id in 1..=2 {
            let mut source = Gate::new(GateId(id), GateKind::Switch, Point::ZERO);
            source.initial = Signal::from_u64(1, 1);
            source.pin_mut("Z").unwrap().net = Some(NetId(id));
            module.gates.push(source);
        }
        let mut buffer = Gate::new(GateId(3), GateKind::TriState, Point::ZERO);
        for (pin, net) in [("I", 1), ("E", 2), ("Z", 3)] {
            buffer.pin_mut(pin).unwrap().net = Some(NetId(net));
        }
        module.gates.push(buffer);
        let mut simulator = Simulator::new(&module).unwrap();
        simulator.advance(20).unwrap();
        assert_eq!(simulator.value(NetId(3)).unwrap().bit(0), Logic::High);
        simulator.toggle_input(GateId(2)).unwrap();
        simulator.advance(20).unwrap();
        assert_eq!(simulator.value(NetId(3)).unwrap().bit(0), Logic::HighZ);
    }
    #[test]
    fn nested_instances_simulate_independently_and_follow_parent_switches() {
        let circuit = rgate_core::demo::hierarchical_inverters();
        let mut simulator = Simulator::from_circuit(&circuit, "main").unwrap();
        assert!(simulator.warnings().is_empty());
        simulator.probe(NetId(2)).unwrap();
        simulator.advance(20).unwrap();
        assert_eq!(simulator.value(NetId(2)).unwrap().to_u64(), Some(1));
        assert_eq!(simulator.value(NetId(4)).unwrap().to_u64(), Some(0));
        simulator.toggle_input(GateId(1)).unwrap();
        simulator.advance(20).unwrap();
        assert_eq!(simulator.value(NetId(2)).unwrap().to_u64(), Some(0));
        assert_eq!(simulator.value(NetId(4)).unwrap().to_u64(), Some(0));
        assert!(simulator.traces()[&NetId(2)].transitions.len() >= 3);
        assert_eq!(simulator.module(), circuit.module("main").unwrap());
    }
}

#[cfg(test)]
mod component_tests;
