//! Backend-neutral interactive simulation contract. HDL backends can implement this
//! without exposing an event queue or coupling the UI to their internal scheduler.
use crate::{SimError, Simulator, Trace};
use rgate_core::{GateId, NetId, Signal};
use std::collections::BTreeMap;

pub trait SimulationBackend {
    fn time(&self) -> u64;
    fn advance(&mut self, nanoseconds: u64) -> Result<usize, SimError>;
    fn step(&mut self) -> Result<usize, SimError>;
    fn value(&self, net: NetId) -> Option<&Signal>;
    fn set_input(&mut self, gate: GateId, value: Signal) -> Result<(), SimError>;
    fn probe(&mut self, net: NetId) -> Result<(), SimError>;
    fn traces(&self) -> &BTreeMap<NetId, Trace>;
}

impl SimulationBackend for Simulator {
    fn time(&self) -> u64 {
        Simulator::time(self)
    }
    fn advance(&mut self, ns: u64) -> Result<usize, SimError> {
        Simulator::advance(self, ns)
    }
    fn step(&mut self) -> Result<usize, SimError> {
        Simulator::step(self)
    }
    fn value(&self, net: NetId) -> Option<&Signal> {
        Simulator::value(self, net)
    }
    fn set_input(&mut self, gate: GateId, value: Signal) -> Result<(), SimError> {
        Simulator::set_input(self, gate, value)
    }
    fn probe(&mut self, net: NetId) -> Result<(), SimError> {
        Simulator::probe(self, net)
    }
    fn traces(&self) -> &BTreeMap<NetId, Trace> {
        Simulator::traces(self)
    }
}
