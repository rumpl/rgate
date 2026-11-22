//! Two-pass Verilog hierarchy elaborator:
//! Pass 1 (`HierarchyBuilder`): parameter resolution, generate expansion, scope & signal allocation.
//! Pass 2 (`BehaviorLowerer`): port wiring, continuous assigns, gate primitives, and processes.

mod behavior;
mod const_eval;
mod deps;
mod expr;
mod hierarchy;
mod ir;
mod symbols;

pub use const_eval::{clog2, eval_binary, eval_const_expr, eval_unary};
pub use deps::collect_expr_deps;
pub use ir::*;

use crate::ast::ModuleDecl;
use behavior::BehaviorLowerer;
use hierarchy::HierarchyBuilder;
use std::collections::HashMap;

pub fn elaborate(modules: &[ModuleDecl], top_name: &str) -> Result<Design, String> {
    let mod_map: HashMap<&str, &ModuleDecl> =
        modules.iter().map(|m| (m.name.as_str(), m)).collect();
    if !mod_map.contains_key(top_name) {
        return Err(format!("unknown top module `{top_name}`"));
    }

    let precision_fs = modules
        .iter()
        .map(|m| m.timescale.precision_fs)
        .min()
        .unwrap_or(1_000);
    let ticks_per_ns = (1_000_000 / precision_fs).max(1);

    // Pass 1: build scope tree & allocate all signals/memories
    let symbols = HierarchyBuilder::new(mod_map).build(top_name)?;

    // Pass 2: lower port connections, continuous assigns, gate primitives, and processes
    BehaviorLowerer::new(symbols, precision_fs).lower(top_name, ticks_per_ns)
}
