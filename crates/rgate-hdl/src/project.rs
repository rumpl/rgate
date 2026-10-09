//! Explicit source sets and bindings: no schematic import or automatic HDL synthesis.
use crate::HdlSignal;
#[cfg(feature = "xezim")]
use crate::XezimBackend;
use rgate_core::{GateId, Logic, NetId, Signal};
use rgate_sim::{SimError, SimulationBackend};
use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HdlProject {
    pub top: String,
    pub sources: Vec<String>,
    pub signals: Vec<ProjectSignal>,
    pub actions: Vec<Action>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectSignal {
    pub path: String,
    pub width: u16,
    #[serde(default)]
    pub input: bool,
}
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Advance { ns: u64 },
    Step,
    Set { signal: String, bits: String },
    Probe { signal: String },
    Check { signal: String, bits: String },
}
impl HdlProject {
    pub fn load(path: &Path) -> Result<Self, SimError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| SimError::Invalid(format!("{}: {e}", path.display())))?;
        serde_json::from_str(&content)
            .map_err(|e| SimError::Invalid(format!("{}: {e}", path.display())))
    }
    #[cfg(feature = "xezim")]
    pub fn instantiate(&self, directory: &Path) -> Result<XezimBackend, SimError> {
        let mut paths = std::collections::BTreeSet::new();
        for signal in &self.signals {
            if !paths.insert(&signal.path) {
                return Err(SimError::Invalid(format!(
                    "duplicate signal path {}",
                    signal.path
                )));
            }
        }
        let sources = self
            .sources
            .iter()
            .map(|path| {
                let path = directory.join(path);
                std::fs::read_to_string(&path)
                    .map_err(|e| SimError::Invalid(format!("{}: {e}", path.display())))
            })
            .collect::<Result<Vec<_>, _>>()?;
        XezimBackend::new(
            sources,
            &self.top,
            self.signals
                .iter()
                .enumerate()
                .map(|(index, signal)| HdlSignal {
                    net: NetId(index as u64 + 1),
                    path: signal.path.clone(),
                    width: signal.width,
                    input: signal.input.then_some(GateId(index as u64 + 1)),
                })
                .collect(),
        )
    }
    #[cfg(feature = "icarus")]
    pub fn instantiate_icarus(&self, directory: &Path) -> Result<crate::IcarusBackend, SimError> {
        let sources = self
            .sources
            .iter()
            .map(|path| {
                std::fs::read_to_string(directory.join(path))
                    .map_err(|e| SimError::Invalid(format!("{path}: {e}")))
            })
            .collect::<Result<Vec<_>, _>>()?;
        crate::IcarusBackend::new(
            sources,
            &self.top,
            self.signals
                .iter()
                .enumerate()
                .map(|(index, s)| HdlSignal {
                    net: NetId(index as u64 + 1),
                    path: s.path.clone(),
                    width: s.width,
                    input: s.input.then_some(GateId(index as u64 + 1)),
                })
                .collect(),
        )
    }
    pub fn execute(&self, backend: &mut (impl SimulationBackend + ?Sized)) -> Result<(), SimError> {
        for action in &self.actions {
            match action {
                Action::Advance { ns } => {
                    backend.advance(*ns)?;
                }
                Action::Step => {
                    backend.step()?;
                }
                Action::Set { signal, bits } => {
                    let binding = self.find(signal)?;
                    let gate = binding
                        .input
                        .ok_or(SimError::NotInput(GateId(binding.net.0)))?;
                    backend.set_input(gate, bit_string(bits, binding.width)?)?;
                }
                Action::Probe { signal } => {
                    let net = self.find(signal)?.net;
                    backend.probe(net)?;
                }
                Action::Check { signal, bits } => {
                    let binding = self.find(signal)?;
                    let expected = bit_string(bits, binding.width)?;
                    let actual = backend.value(binding.net).unwrap();
                    if actual != &expected {
                        return Err(SimError::Invalid(format!(
                            "at {} ns {signal}: expected {expected}, got {actual}",
                            backend.time()
                        )));
                    }
                }
            }
        }
        Ok(())
    }
    fn find(&self, path: &str) -> Result<HdlSignal, SimError> {
        self.signals
            .iter()
            .enumerate()
            .find(|(_, s)| s.path == path)
            .map(|(index, s)| HdlSignal {
                net: NetId(index as u64 + 1),
                path: s.path.clone(),
                width: s.width,
                input: s.input.then_some(GateId(index as u64 + 1)),
            })
            .ok_or_else(|| SimError::Invalid(format!("unbound HDL signal {path}")))
    }
}
fn bit_string(bits: &str, width: u16) -> Result<Signal, SimError> {
    if bits.len() != usize::from(width) {
        return Err(SimError::Invalid(format!(
            "expected {width} binary bits, got {bits}"
        )));
    }
    Ok(Signal::from_bits(
        bits.chars()
            .rev()
            .map(|bit| match bit {
                '0' => Ok(Logic::Low),
                '1' => Ok(Logic::High),
                'x' | 'X' => Ok(Logic::Unknown),
                'z' | 'Z' => Ok(Logic::HighZ),
                _ => Err(SimError::Invalid(format!("invalid binary bit {bit}"))),
            })
            .collect::<Result<_, _>>()?,
    ))
}
