//! Experimental HDL backends, independent of the schematic simulator and UI.
use rgate_core::{GateId, NetId};

#[derive(Clone, Debug)]
pub struct HdlSignal {
    pub net: NetId,
    /// Path relative to the supplied top (e.g. `timing.x`).
    pub path: String,
    pub width: u16,
    /// Interactive variables belong in a harness; this is deposit, not force.
    pub input: Option<GateId>,
}

#[cfg(all(any(feature = "xezim", feature = "icarus"), target_arch = "wasm32"))]
compile_error!("HDL backends are native-only; disable their features for WASM.");
#[cfg(feature = "xezim")]
mod xezim_backend;
#[cfg(feature = "xezim")]
pub use xezim_backend::XezimBackend;
#[cfg(feature = "icarus")]
mod icarus_backend;
#[cfg(feature = "icarus")]
pub use icarus_backend::{DiscoveredSignal, IcarusBackend};
#[cfg(any(feature = "xezim", feature = "icarus"))]
mod project;
#[cfg(any(feature = "xezim", feature = "icarus"))]
pub use project::{Action, HdlProject, ProjectSignal};
