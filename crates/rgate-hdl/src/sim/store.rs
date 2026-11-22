use crate::elaborate::{Design, ElabEvent, MemId, SigId, collect_expr_deps};
use crate::value::BitVec;
use rgate_core::Logic;
use std::collections::{BTreeSet, HashMap};

/// Static dependency graph mapping signals and memories to dependent assigns and processes.
pub(super) struct FanoutIndex {
    pub(super) sig_to_assigns: Vec<Vec<usize>>,
    pub(super) mem_to_assigns: Vec<Vec<usize>>,
    pub(super) sig_to_procs: Vec<Vec<usize>>,
    pub(super) mem_to_procs: Vec<Vec<usize>>,
}

impl FanoutIndex {
    fn from_design(design: &Design) -> Self {
        let num_sigs = design.signals.len();
        let num_mems = design.memories.len();

        let mut sig_to_assigns = vec![Vec::new(); num_sigs];
        let mut mem_to_assigns = vec![Vec::new(); num_mems];
        for (aidx, a) in design.assigns.iter().enumerate() {
            for &s in &a.sig_deps {
                sig_to_assigns[s.0].push(aidx);
            }
            for &m in &a.mem_deps {
                mem_to_assigns[m.0].push(aidx);
            }
        }

        let mut sig_to_procs = vec![Vec::new(); num_sigs];
        let mut mem_to_procs = vec![Vec::new(); num_mems];
        for (pidx, p) in design.processes.iter().enumerate() {
            let mut dep_sigs = BTreeSet::new();
            for ev in &p.sensitivity {
                match ev {
                    ElabEvent::Posedge(e) | ElabEvent::Negedge(e) => {
                        let (sigs, _) = collect_expr_deps(e);
                        dep_sigs.extend(sigs);
                    }
                    ElabEvent::Level(s) => {
                        dep_sigs.insert(*s);
                    }
                }
            }
            for s in dep_sigs {
                sig_to_procs[s.0].push(pidx);
            }
            for &m in &p.mem_sensitivity {
                mem_to_procs[m.0].push(pidx);
            }
        }

        Self {
            sig_to_assigns,
            mem_to_assigns,
            sig_to_procs,
            mem_to_procs,
        }
    }
}

/// Owns runtime signal values, multi-driver resolution, force/release state,
/// unpacked memories, edge-sensitivity history, fanout tables, and value-change watch log.
pub(super) struct SignalStore {
    pub(super) values: Vec<BitVec>,
    pub(super) unforced: Vec<BitVec>,
    pub(super) forced: Vec<Option<BitVec>>,
    pub(super) drivers: Vec<HashMap<usize, BitVec>>,
    pub(super) memories: Vec<Vec<BitVec>>,
    pub(super) fanout: FanoutIndex,
    pub(super) edge_prev: Vec<Vec<Logic>>,
    watched_sigs: Vec<bool>,
    changed_log: Vec<(u64, SigId, BitVec)>,
}

impl SignalStore {
    pub(super) fn from_design(design: &Design) -> Self {
        let num_sigs = design.signals.len();
        let values: Vec<BitVec> = design.signals.iter().map(|s| s.init.clone()).collect();
        let unforced = values.clone();
        let memories: Vec<Vec<BitVec>> = design
            .memories
            .iter()
            .map(|m| vec![BitVec::filled(m.elem_width, Logic::Unknown); m.len()])
            .collect();
        let edge_prev = design
            .processes
            .iter()
            .map(|p| vec![Logic::Unknown; p.sensitivity.len()])
            .collect();
        let fanout = FanoutIndex::from_design(design);
        Self {
            values,
            unforced,
            forced: vec![None; num_sigs],
            drivers: vec![HashMap::new(); num_sigs],
            memories,
            fanout,
            edge_prev,
            watched_sigs: vec![false; num_sigs],
            changed_log: Vec::new(),
        }
    }

    pub(super) fn watch(&mut self, sig: SigId) {
        self.watched_sigs[sig.0] = true;
    }

    pub(super) fn unwatch(&mut self, sig: SigId) {
        self.watched_sigs[sig.0] = false;
    }

    pub(super) fn value(&self, sig: SigId) -> &BitVec {
        &self.values[sig.0]
    }

    pub(super) fn changed_len(&self) -> usize {
        self.changed_log.len()
    }

    pub(super) fn clear_changes(&mut self) {
        self.changed_log.clear();
    }

    pub(super) fn take_changes(&mut self) -> Vec<(u64, SigId, BitVec)> {
        std::mem::take(&mut self.changed_log)
    }

    pub(super) fn resolve_drivers(&self, design: &Design, sig: SigId) -> BitVec {
        let info = &design.signals[sig.0];
        let mut acc = BitVec::filled(info.width, Logic::HighZ);
        for driven in self.drivers[sig.0].values() {
            acc = acc.resolve(driven);
        }
        acc.signed = info.signed;
        acc
    }

    /// Commits a new visible signal value and records it in `changed_log` if watched.
    /// Returns `true` if the visible value actually changed.
    pub(super) fn set_visible(&mut self, now: u64, sig: SigId, new_val: BitVec) -> bool {
        if self.values[sig.0] == new_val {
            return false;
        }
        self.values[sig.0] = new_val.clone();
        if self.watched_sigs[sig.0] {
            self.changed_log.push((now, sig, new_val));
        }
        true
    }

    pub(super) fn write_memory(&mut self, design: &Design, mem: MemId, addr: i64, value: &BitVec) -> bool {
        let m = &design.memories[mem.0];
        let Some(slot) = m.elem_index(addr) else {
            return false;
        };
        let v = value.resized(m.elem_width, m.elem_signed);
        if self.memories[mem.0][slot] != v {
            self.memories[mem.0][slot] = v;
            true
        } else {
            false
        }
    }
}
