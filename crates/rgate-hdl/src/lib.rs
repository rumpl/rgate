//! In-process pure-Rust Verilog / SystemVerilog parser, elaborator, and event-driven simulator.
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

pub mod ast;
mod backend;
pub mod elaborate;
pub mod lexer;
pub mod parser;
pub mod preprocess;
mod project;
pub mod sim;
pub mod value;

pub use backend::{DiscoveredSignal, VerilogBackend, check_sources};
pub use project::{Action, HdlProject, ProjectSignal};
