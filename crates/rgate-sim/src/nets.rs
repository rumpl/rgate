use crate::{MAX_TRACE_CHANGES, SimError, Trace, Transition};
use rgate_core::{Direction, Logic, Module, NetId, Signal};
use std::collections::{BTreeMap, HashMap};

/// Owns live net values, multi-driver states, fanout index, and probed waveform traces.
#[derive(Clone, Debug)]
pub(crate) struct NetStore {
    values: HashMap<NetId, Signal>,
    drivers: HashMap<(usize, String), Signal>,
    fanout: HashMap<NetId, Vec<usize>>,
    traces: BTreeMap<NetId, Trace>,
}

impl NetStore {
    pub(crate) fn from_module(module: &Module) -> Self {
        let values = module
            .nets
            .iter()
            .map(|net| (net.id, Signal::filled(net.width, Logic::HighZ)))
            .collect();
        let mut drivers = HashMap::new();
        let mut fanout: HashMap<NetId, Vec<usize>> = HashMap::new();
        for (index, gate) in module.gates.iter().enumerate() {
            for pin in &gate.pins {
                if let Some(net) = pin.net {
                    if pin.direction != Direction::Input {
                        let width = module.net(net).unwrap().width;
                        drivers.insert(
                            (index, pin.name.clone()),
                            Signal::filled(width, Logic::HighZ),
                        );
                    }
                    if pin.direction != Direction::Output {
                        fanout.entry(net).or_default().push(index);
                    }
                }
            }
        }
        Self {
            values,
            drivers,
            fanout,
            traces: BTreeMap::new(),
        }
    }

    pub(crate) fn value(&self, net: NetId) -> Option<&Signal> {
        self.values.get(&net)
    }

    pub(crate) fn traces(&self) -> &BTreeMap<NetId, Trace> {
        &self.traces
    }

    pub(crate) fn set_driver(&mut self, key: (usize, String), value: Signal) {
        self.drivers.insert(key, value);
    }

    pub(crate) fn resolve_external(&self, module: &Module, exclude_gate: usize, net: NetId, width: u16) -> Signal {
        self.drivers
            .iter()
            .filter(|((driver, pin), _)| {
                *driver != exclude_gate
                    && module.gates[*driver].pin(pin).and_then(|pin| pin.net) == Some(net)
            })
            .fold(Signal::filled(width, Logic::HighZ), |value, (_, drive)| {
                value.zip(drive, Logic::resolve)
            })
    }

    /// Resolves all drivers on `net`, updates its value & waveform trace if changed,
    /// and returns its fanout gate indices when the net changed.
    pub(crate) fn resolve_net(&mut self, module: &Module, net: NetId, time: u64) -> Option<&[usize]> {
        let width = module.net(net).unwrap().width;
        let resolved = self
            .drivers
            .iter()
            .filter(|((index, pin), _)| {
                module.gates[*index].pin(pin).and_then(|pin| pin.net) == Some(net)
            })
            .fold(Signal::filled(width, Logic::HighZ), |value, (_, drive)| {
                value.zip(drive, Logic::resolve)
            });
        if self.values[&net] == resolved {
            return None;
        }
        self.values.insert(net, resolved.clone());
        if let Some(trace) = self.traces.get_mut(&net) {
            if trace
                .transitions
                .last()
                .is_some_and(|change| change.time == time)
            {
                trace.transitions.last_mut().unwrap().value = resolved;
            } else {
                trace.transitions.push(Transition {
                    time,
                    value: resolved,
                });
                if trace.transitions.len() > MAX_TRACE_CHANGES {
                    trace.transitions.remove(0);
                }
            }
        }
        self.fanout.get(&net).map(Vec::as_slice)
    }

    pub(crate) fn probe(&mut self, module: &Module, net: NetId, time: u64) -> Result<(), SimError> {
        if self.traces.contains_key(&net) {
            return Ok(());
        }
        let definition = module.net(net).ok_or(SimError::UnknownNet(net))?;
        self.traces.insert(
            net,
            Trace {
                net,
                name: definition.name.clone(),
                width: definition.width,
                transitions: vec![Transition {
                    time,
                    value: self.values[&net].clone(),
                }],
            },
        );
        Ok(())
    }

    pub(crate) fn label_probe(&mut self, net: NetId, name: String) {
        if let Some(trace) = self.traces.get_mut(&net) {
            trace.name = name;
        }
    }

    pub(crate) fn unprobe(&mut self, net: NetId) {
        self.traces.remove(&net);
    }
}
