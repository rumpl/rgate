//! Stratified event-driven four-state Verilog simulator.

mod eval;
mod exec;
mod signals;

use crate::elaborate::*;
use crate::value::BitVec;
use rgate_core::Logic;
use std::cmp::Reverse;
use std::collections::{BTreeSet, BinaryHeap, HashMap, VecDeque};

#[derive(Clone, Debug)]
pub(super) enum SimEventKind {
    DriverUpdate {
        assign_idx: usize,
        value: BitVec,
    },
    DelayedNba {
        lhs: ElabLValue,
        value: BitVec,
    },
    ResumeProc(usize),
}

#[derive(Clone, Debug)]
struct ScheduledEvent {
    time: u64,
    seq: u64,
    kind: SimEventKind,
}

impl PartialEq for ScheduledEvent {
    fn eq(&self, other: &Self) -> bool {
        self.time == other.time && self.seq == other.seq
    }
}
impl Eq for ScheduledEvent {}
impl PartialOrd for ScheduledEvent {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for ScheduledEvent {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.time.cmp(&other.time).then(self.seq.cmp(&other.seq))
    }
}

#[derive(Clone, Debug)]
pub(super) enum ExecFrame {
    Seq {
        stmts: Vec<ElabStmt>,
        idx: usize,
    },
    Forever(ElabStmt),
    Repeat {
        remaining: u64,
        body: ElabStmt,
    },
    While {
        cond: ElabExpr,
        body: ElabStmt,
    },
    For {
        cond: Option<ElabExpr>,
        step: Option<ElabStmt>,
        body: ElabStmt,
        run_step: bool,
    },
}

pub struct VerilogSim {
    pub design: Design,
    pub now: u64,
    pub(super) values: Vec<BitVec>,
    /// Underlying value for forced signals (so `release` can restore it).
    pub(super) unforced: Vec<BitVec>,
    pub(super) forced: Vec<Option<BitVec>>,
    /// Per-signal continuous-assign drivers: driver_id -> value.
    pub(super) drivers: Vec<HashMap<usize, BitVec>>,
    /// Unpacked memories.
    pub(super) memories: Vec<Vec<BitVec>>,

    /// Fanout tables
    pub(super) sig_to_assigns: Vec<Vec<usize>>,
    pub(super) mem_to_assigns: Vec<Vec<usize>>,
    pub(super) sig_to_procs: Vec<Vec<usize>>,
    pub(super) mem_to_procs: Vec<Vec<usize>>,

    /// Previous LSB value for each edge-event on each process: proc_idx -> event_idx -> Logic
    pub(super) edge_prev: Vec<Vec<Logic>>,
    /// Thread frame stacks for processes that can suspend on `#delay`
    pub(super) proc_frames: Vec<Vec<ExecFrame>>,
    /// Whether a process is currently suspended waiting on a scheduled `#delay`
    pub(super) proc_sleeping: Vec<bool>,

    /// Future event queue
    queue: BinaryHeap<Reverse<ScheduledEvent>>,
    next_seq: u64,

    /// Current-timestep delta queues
    active_assigns: VecDeque<usize>,
    in_active_assigns: Vec<bool>,
    active_procs: VecDeque<usize>,
    in_active_procs: Vec<bool>,
    pub(super) pending_nba: Vec<(ElabLValue, BitVec)>,

    /// Signals changed during the current settle/call (for value-change callbacks)
    pub(super) changed_log: Vec<(u64, SigId, BitVec)>,
    pub(super) watched_sigs: Vec<bool>,
    pub(super) finished: bool,
}

impl VerilogSim {
    pub fn new(design: Design) -> Result<Self, String> {
        let num_sigs = design.signals.len();
        let num_mems = design.memories.len();
        let num_assigns = design.assigns.len();
        let num_procs = design.processes.len();

        let values: Vec<BitVec> = design.signals.iter().map(|s| s.init.clone()).collect();
        let unforced = values.clone();
        let memories: Vec<Vec<BitVec>> = design
            .memories
            .iter()
            .map(|m| vec![BitVec::filled(m.elem_width, Logic::Unknown); m.len()])
            .collect();

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
        let mut edge_prev = Vec::with_capacity(num_procs);
        for (pidx, p) in design.processes.iter().enumerate() {
            let mut prevs = Vec::with_capacity(p.sensitivity.len());
            let mut dep_sigs = BTreeSet::new();
            for ev in &p.sensitivity {
                match ev {
                    ElabEvent::Posedge(e) | ElabEvent::Negedge(e) => {
                        let (sigs, _) = collect_expr_deps(e);
                        for s in sigs {
                            dep_sigs.insert(s);
                        }
                        prevs.push(Logic::Unknown);
                    }
                    ElabEvent::Level(s) => {
                        dep_sigs.insert(*s);
                        prevs.push(Logic::Unknown);
                    }
                }
            }
            for s in dep_sigs {
                sig_to_procs[s.0].push(pidx);
            }
            for &m in &p.mem_sensitivity {
                mem_to_procs[m.0].push(pidx);
            }
            edge_prev.push(prevs);
        }

        let mut sim = Self {
            design,
            now: 0,
            values,
            unforced,
            forced: vec![None; num_sigs],
            drivers: vec![HashMap::new(); num_sigs],
            memories,
            sig_to_assigns,
            mem_to_assigns,
            sig_to_procs,
            mem_to_procs,
            edge_prev,
            proc_frames: vec![Vec::new(); num_procs],
            proc_sleeping: vec![false; num_procs],
            queue: BinaryHeap::new(),
            next_seq: 0,
            active_assigns: VecDeque::new(),
            in_active_assigns: vec![false; num_assigns],
            active_procs: VecDeque::new(),
            in_active_procs: vec![false; num_procs],
            pending_nba: Vec::new(),
            changed_log: Vec::new(),
            watched_sigs: vec![false; num_sigs],
            finished: false,
        };

        // Snapshot initial edge values before t=0 evaluation
        sim.refresh_all_edge_prev();

        // Schedule all continuous assigns at t=0
        for aidx in 0..num_assigns {
            sim.enqueue_assign(aidx);
        }

        // Schedule all Initial, AlwaysComb, and unsensitive Always processes at t=0
        for pidx in 0..num_procs {
            match sim.design.processes[pidx].kind {
                ProcessKind::Initial | ProcessKind::AlwaysComb => {
                    sim.enqueue_proc(pidx);
                }
                ProcessKind::Always if sim.design.processes[pidx].sensitivity.is_empty() => {
                    sim.enqueue_proc(pidx);
                }
                _ => {}
            }
        }

        sim.settle()?;
        sim.changed_log.clear();
        Ok(sim)
    }

    pub fn watch(&mut self, sig: SigId) {
        self.watched_sigs[sig.0] = true;
    }

    pub fn unwatch(&mut self, sig: SigId) {
        self.watched_sigs[sig.0] = false;
    }

    pub fn value(&self, sig: SigId) -> &BitVec {
        &self.values[sig.0]
    }

    pub fn take_changes(&mut self) -> Vec<(u64, SigId, BitVec)> {
        std::mem::take(&mut self.changed_log)
    }

    /// Deposit an external input value and settle combinational/delta propagation.
    pub fn set_signal(&mut self, sig: SigId, value: BitVec) -> Result<(), String> {
        let info = &self.design.signals[sig.0];
        let val = value.resized(info.width, info.signed);
        self.write_signal_direct(sig, val);
        self.settle()
    }

    /// Advance to the next scheduled timestep and settle it.
    pub fn step(&mut self) -> Result<usize, String> {
        if self.finished {
            return Ok(0);
        }
        let Some(Reverse(next)) = self.queue.peek() else {
            return Err("no scheduled future events".into());
        };
        self.now = next.time;
        self.settle()?;
        Ok(self.changed_log.len())
    }

    /// Advance by `delta_ticks`, settling every intermediate event up to `now + delta_ticks`.
    pub fn advance_ticks(&mut self, delta_ticks: u64) -> Result<usize, String> {
        let target = self
            .now
            .checked_add(delta_ticks)
            .ok_or_else(|| "simulation time overflow".to_string())?;
        while !self.finished {
            let Some(Reverse(next)) = self.queue.peek() else {
                break;
            };
            if next.time > target {
                break;
            }
            self.now = next.time;
            self.settle()?;
        }
        self.now = target;
        Ok(self.changed_log.len())
    }

    pub(super) fn schedule(&mut self, time: u64, kind: SimEventKind) {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.queue.push(Reverse(ScheduledEvent { time, seq, kind }));
    }

    pub(super) fn enqueue_assign(&mut self, aidx: usize) {
        if !self.in_active_assigns[aidx] {
            self.in_active_assigns[aidx] = true;
            self.active_assigns.push_back(aidx);
        }
    }

    pub(super) fn enqueue_proc(&mut self, pidx: usize) {
        if !self.in_active_procs[pidx] && !self.proc_sleeping[pidx] {
            self.in_active_procs[pidx] = true;
            self.active_procs.push_back(pidx);
        }
    }

    fn refresh_all_edge_prev(&mut self) {
        for pidx in 0..self.design.processes.len() {
            let sens_len = self.design.processes[pidx].sensitivity.len();
            for eidx in 0..sens_len {
                let bit = match &self.design.processes[pidx].sensitivity[eidx] {
                    ElabEvent::Posedge(e) | ElabEvent::Negedge(e) => {
                        let e_clone = e.clone();
                        self.eval_expr(&e_clone, None).bit(0)
                    }
                    ElabEvent::Level(s) => self.values[s.0].bit(0),
                };
                self.edge_prev[pidx][eidx] = bit;
            }
        }
    }

    fn settle(&mut self) -> Result<(), String> {
        let mut deltas = 0usize;
        loop {
            // Drain all scheduled events at `self.now`
            while let Some(Reverse(ev)) = self.queue.peek() {
                if ev.time > self.now {
                    break;
                }
                let Reverse(ev) = self.queue.pop().unwrap();
                match ev.kind {
                    SimEventKind::DriverUpdate { assign_idx, value } => {
                        let driver_id = self.design.assigns[assign_idx].driver_id;
                        let lhs = self.design.assigns[assign_idx].lhs.clone();
                        self.apply_driver_lvalue(&lhs, driver_id, &value);
                    }
                    SimEventKind::DelayedNba { lhs, value } => {
                        self.write_lvalue(&lhs, &value);
                    }
                    SimEventKind::ResumeProc(pidx) => {
                        self.proc_sleeping[pidx] = false;
                        self.enqueue_proc(pidx);
                    }
                }
            }

            // Active region: continuous assigns + processes
            while !self.active_assigns.is_empty() || !self.active_procs.is_empty() {
                deltas += 1;
                if deltas > 200_000 {
                    return Err("zero-delay simulation loop exceeded delta limit".into());
                }
                while let Some(aidx) = self.active_assigns.pop_front() {
                    self.in_active_assigns[aidx] = false;
                    let delay = self.design.assigns[aidx].delay_ticks;
                    let lhs = self.design.assigns[aidx].lhs.clone();
                    let rhs = self.design.assigns[aidx].rhs.clone();
                    let driver_id = self.design.assigns[aidx].driver_id;
                    let ctx_w = self.lvalue_width(&lhs);
                    let val = self.eval_expr(&rhs, Some(ctx_w));
                    if delay == 0 {
                        self.apply_driver_lvalue(&lhs, driver_id, &val);
                    } else {
                        self.schedule(
                            self.now + delay,
                            SimEventKind::DriverUpdate {
                                assign_idx: aidx,
                                value: val,
                            },
                        );
                    }
                }
                if let Some(pidx) = self.active_procs.pop_front() {
                    self.in_active_procs[pidx] = false;
                    self.run_process(pidx)?;
                }
            }

            // NBA region
            if self.pending_nba.is_empty() {
                // Check if any zero-delay event was scheduled for `self.now`
                if self.queue.peek().is_some_and(|Reverse(e)| e.time <= self.now) {
                    continue;
                }
                break;
            }
            let nbas = std::mem::take(&mut self.pending_nba);
            for (lhs, val) in nbas {
                self.write_lvalue(&lhs, &val);
            }
        }
        Ok(())
    }
}
