//! Icarus Verilog integration, independent of the schematic simulator and UI.
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

#[cfg(all(feature = "icarus", target_arch = "wasm32"))]
compile_error!("Icarus is native-only; disable the icarus feature for WASM.");
#[cfg(feature = "icarus")]
mod icarus_backend;
#[cfg(feature = "icarus")]
pub use icarus_backend::{DiscoveredSignal, IcarusBackend, check_sources};
#[cfg(feature = "icarus")]
mod project;
#[cfg(feature = "icarus")]
pub use project::{Action, HdlProject, ProjectSignal};
