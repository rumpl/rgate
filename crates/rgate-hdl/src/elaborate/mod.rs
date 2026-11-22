//! Two-pass Verilog hierarchy elaborator:
//! Pass 1: parameter resolution, generate expansion, scope & signal allocation.
//! Pass 2: port wiring, continuous assigns, gate primitives, and processes.

mod behavior;
mod const_eval;
mod deps;
mod expr;
mod hierarchy;
mod ir;

pub use const_eval::{clog2, eval_binary, eval_const_expr, eval_unary};
pub use deps::collect_expr_deps;
pub use ir::*;

use crate::ast::{Expr, ModuleDecl, ModuleItem, PortDir};
use crate::preprocess::Timescale;
use crate::value::BitVec;
use std::collections::HashMap;

pub(super) type PortConnList = Vec<(Option<String>, Option<Expr>)>;

pub(super) struct Scope {
    pub(super) parent: Option<usize>,
    pub(super) path: String,
    pub(super) module_name: String,
    pub(super) timescale: Timescale,
    pub(super) params: HashMap<String, BitVec>,
    pub(super) sigs: HashMap<String, SigId>,
    pub(super) mems: HashMap<String, MemId>,
    pub(super) funcs: HashMap<String, usize>,
    pub(super) tasks: HashMap<String, usize>,
    pub(super) children: HashMap<String, usize>,
    /// Expanded module items for Pass 2
    pub(super) items: Vec<ModuleItem>,
    /// Port list of this module in declaration order
    pub(super) ports: Vec<(String, PortDir)>,
    /// Pending port connections from parent: (child_scope_idx, connections)
    pub(super) child_conns: Vec<(usize, PortConnList)>,
    /// Pending wire initializers: (sig_id, expr)
    pub(super) wire_inits: Vec<(SigId, Expr)>,
    /// Pending reg initializers: (sig_id, expr)
    pub(super) reg_inits: Vec<(SigId, Expr)>,
}

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

    let mut elab = Elaborator {
        mod_map,
        precision_fs,
        scopes: Vec::new(),
        signals: Vec::new(),
        memories: Vec::new(),
        path_to_sig: HashMap::new(),
        assigns: Vec::new(),
        processes: Vec::new(),
        functions: Vec::new(),
        tasks: Vec::new(),
        next_driver_id: 0,
    };

    // Pass 1: build scope tree & allocate all signals/memories
    elab.instantiate_module(top_name, top_name, &[], 0)?;

    // Pass 2: elaborate behavior, port connections, continuous assigns, processes
    for scope_idx in 0..elab.scopes.len() {
        elab.elaborate_scope_behavior(scope_idx)?;
    }

    Ok(Design {
        top: top_name.to_string(),
        precision_fs,
        ticks_per_ns,
        signals: elab.signals,
        memories: elab.memories,
        path_to_sig: elab.path_to_sig,
        assigns: elab.assigns,
        processes: elab.processes,
        functions: elab.functions,
        tasks: elab.tasks,
    })
}

pub(super) struct Elaborator<'a> {
    pub(super) mod_map: HashMap<&'a str, &'a ModuleDecl>,
    pub(super) precision_fs: u64,
    pub(super) scopes: Vec<Scope>,
    pub(super) signals: Vec<SignalInfo>,
    pub(super) memories: Vec<MemoryInfo>,
    pub(super) path_to_sig: HashMap<String, SigId>,
    pub(super) assigns: Vec<ContinuousAssign>,
    pub(super) processes: Vec<ProcessInfo>,
    pub(super) functions: Vec<ElabFunc>,
    pub(super) tasks: Vec<ElabTask>,
    pub(super) next_driver_id: usize,
}

impl<'a> Elaborator<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn alloc_signal(
        &mut self,
        path: String,
        msb: i64,
        lsb: i64,
        signed: bool,
        is_net: bool,
        init: BitVec,
        visible: bool,
    ) -> SigId {
        let id = SigId(self.signals.len());
        let width = ((msb - lsb).unsigned_abs() as usize + 1).max(1);
        self.signals.push(SignalInfo {
            id,
            path: path.clone(),
            width,
            msb,
            lsb,
            signed,
            is_net,
            init: init.resized(width, signed),
            visible,
        });
        if visible {
            self.path_to_sig.insert(path, id);
        }
        id
    }

    pub(super) fn alloc_memory(
        &mut self,
        path: String,
        elem_width: usize,
        elem_signed: bool,
        lo: i64,
        hi: i64,
    ) -> MemId {
        let id = MemId(self.memories.len());
        self.memories.push(MemoryInfo {
            id,
            path,
            elem_width,
            elem_signed,
            lo,
            hi,
        });
        id
    }

    pub(super) fn resolve_sig(
        &self,
        scope_idx: usize,
        path: &[String],
        locals: &HashMap<String, SigId>,
    ) -> Option<SigId> {
        if path.len() == 1 {
            if let Some(&s) = locals.get(&path[0]) {
                return Some(s);
            }
            let mut cur_scope = Some(scope_idx);
            while let Some(sc) = cur_scope {
                if let Some(&s) = self.scopes[sc].sigs.get(&path[0]) {
                    return Some(s);
                }
                cur_scope = self.scopes[sc].parent;
            }
        }
        // Hierarchical lookup relative to current scope (and lexical parents)
        let mut start_scope = Some(scope_idx);
        while let Some(mut cur) = start_scope {
            let mut found = true;
            for (i, seg) in path.iter().enumerate() {
                if i + 1 == path.len() {
                    if let Some(&s) = self.scopes[cur].sigs.get(seg) {
                        return Some(s);
                    }
                    found = false;
                } else if let Some(&child) = self.scopes[cur].children.get(seg) {
                    cur = child;
                } else {
                    found = false;
                    break;
                }
            }
            if found {
                break;
            }
            start_scope = self.scopes[start_scope.unwrap()].parent;
        }
        // Try absolute path lookup
        let joined = path.join(".");
        if let Some(&s) = self.path_to_sig.get(&joined) {
            return Some(s);
        }
        let prefixed = format!("{}.{joined}", self.scopes[scope_idx].path);
        self.path_to_sig.get(&prefixed).copied()
    }

    pub(super) fn resolve_mem(&self, scope_idx: usize, path: &[String]) -> Option<MemId> {
        if path.len() == 1 {
            let mut cur_scope = Some(scope_idx);
            while let Some(sc) = cur_scope {
                if let Some(&m) = self.scopes[sc].mems.get(&path[0]) {
                    return Some(m);
                }
                cur_scope = self.scopes[sc].parent;
            }
            return None;
        }
        let mut cur = scope_idx;
        for (i, seg) in path.iter().enumerate() {
            if i + 1 == path.len() {
                return self.scopes[cur].mems.get(seg).copied();
            } else if let Some(&child) = self.scopes[cur].children.get(seg) {
                cur = child;
            } else {
                return None;
            }
        }
        None
    }
}
