use super::ir::{ElabFunc, ElabTask, MemId, MemoryInfo, SigId, SignalInfo};
use crate::ast::{Expr, ModuleItem, PortDir};
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

/// Owns the elaborated scope hierarchy, signal/memory tables, and hierarchical name resolution.
#[derive(Default)]
pub(super) struct SymbolTable {
    pub(super) scopes: Vec<Scope>,
    pub(super) signals: Vec<SignalInfo>,
    pub(super) memories: Vec<MemoryInfo>,
    pub(super) path_to_sig: HashMap<String, SigId>,
    pub(super) functions: Vec<ElabFunc>,
    pub(super) tasks: Vec<ElabTask>,
}

impl SymbolTable {
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
