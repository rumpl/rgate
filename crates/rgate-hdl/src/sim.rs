//! Stratified event-driven four-state Verilog simulator.

use crate::ast::{BinaryOp, CaseKind, UnaryOp};
use crate::elaborate::*;
use crate::value::BitVec;
use rgate_core::Logic;
use std::cmp::Reverse;
use std::collections::{BTreeSet, BinaryHeap, HashMap, VecDeque};

#[derive(Clone, Debug)]
enum SimEventKind {
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
enum ExecFrame {
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

enum StepOutcome {
    Done,
    Suspended,
    Return,
}

pub struct VerilogSim {
    pub design: Design,
    pub now: u64,
    values: Vec<BitVec>,
    /// Underlying value for forced signals (so `release` can restore it).
    unforced: Vec<BitVec>,
    forced: Vec<Option<BitVec>>,
    /// Per-signal continuous-assign drivers: driver_id -> value.
    drivers: Vec<HashMap<usize, BitVec>>,
    /// Unpacked memories.
    memories: Vec<Vec<BitVec>>,

    /// Fanout tables
    sig_to_assigns: Vec<Vec<usize>>,
    mem_to_assigns: Vec<Vec<usize>>,
    sig_to_procs: Vec<Vec<usize>>,
    mem_to_procs: Vec<Vec<usize>>,

    /// Previous LSB value for each edge-event on each process: proc_idx -> event_idx -> Logic
    edge_prev: Vec<Vec<Logic>>,
    /// Thread frame stacks for processes that can suspend on `#delay`
    proc_frames: Vec<Vec<ExecFrame>>,
    /// Whether a process is currently suspended waiting on a scheduled `#delay`
    proc_sleeping: Vec<bool>,

    /// Future event queue
    queue: BinaryHeap<Reverse<ScheduledEvent>>,
    next_seq: u64,

    /// Current-timestep delta queues
    active_assigns: VecDeque<usize>,
    in_active_assigns: Vec<bool>,
    active_procs: VecDeque<usize>,
    in_active_procs: Vec<bool>,
    pending_nba: Vec<(ElabLValue, BitVec)>,

    /// Signals changed during the current settle/call (for value-change callbacks)
    changed_log: Vec<(u64, SigId, BitVec)>,
    watched_sigs: Vec<bool>,
    finished: bool,
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

    fn schedule(&mut self, time: u64, kind: SimEventKind) {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.queue.push(Reverse(ScheduledEvent { time, seq, kind }));
    }

    fn enqueue_assign(&mut self, aidx: usize) {
        if !self.in_active_assigns[aidx] {
            self.in_active_assigns[aidx] = true;
            self.active_assigns.push_back(aidx);
        }
    }

    fn enqueue_proc(&mut self, pidx: usize) {
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

    fn run_process(&mut self, pidx: usize) -> Result<(), String> {
        if self.proc_frames[pidx].is_empty() {
            let body = self.design.processes[pidx].body.clone();
            self.proc_frames[pidx].push(ExecFrame::Seq {
                stmts: vec![body],
                idx: 0,
            });
        }

        let mut steps = 0usize;
        loop {
            steps += 1;
            if steps > 1_000_000 {
                return Err("process execution exceeded step limit (possible infinite loop)".into());
            }
            let Some(frame) = self.proc_frames[pidx].pop() else {
                // Process finished one full execution
                if self.design.processes[pidx].kind == ProcessKind::Always
                    && self.design.processes[pidx].sensitivity.is_empty()
                {
                    // Loop `always #5 clk = ~clk;`
                    let body = self.design.processes[pidx].body.clone();
                    self.proc_frames[pidx].push(ExecFrame::Seq {
                        stmts: vec![body],
                        idx: 0,
                    });
                    continue;
                }
                return Ok(());
            };

            match frame {
                ExecFrame::Seq { stmts, idx } => {
                    if idx >= stmts.len() {
                        continue;
                    }
                    let stmt = stmts[idx].clone();
                    if idx + 1 < stmts.len() {
                        self.proc_frames[pidx].push(ExecFrame::Seq {
                            stmts,
                            idx: idx + 1,
                        });
                    }
                    match self.exec_stmt(pidx, &stmt)? {
                        StepOutcome::Done => {}
                        StepOutcome::Suspended => return Ok(()),
                        StepOutcome::Return => {
                            self.proc_frames[pidx].clear();
                            return Ok(());
                        }
                    }
                }
                ExecFrame::Forever(body) => {
                    self.proc_frames[pidx].push(ExecFrame::Forever(body.clone()));
                    self.proc_frames[pidx].push(ExecFrame::Seq {
                        stmts: vec![body],
                        idx: 0,
                    });
                }
                ExecFrame::Repeat { remaining, body } => {
                    if remaining > 0 {
                        self.proc_frames[pidx].push(ExecFrame::Repeat {
                            remaining: remaining - 1,
                            body: body.clone(),
                        });
                        self.proc_frames[pidx].push(ExecFrame::Seq {
                            stmts: vec![body],
                            idx: 0,
                        });
                    }
                }
                ExecFrame::While { cond, body } => {
                    if self.eval_expr(&cond, None).is_true() {
                        self.proc_frames[pidx].push(ExecFrame::While {
                            cond,
                            body: body.clone(),
                        });
                        self.proc_frames[pidx].push(ExecFrame::Seq {
                            stmts: vec![body],
                            idx: 0,
                        });
                    }
                }
                ExecFrame::For {
                    cond,
                    step,
                    body,
                    run_step,
                } => {
                    if run_step
                        && let Some(ref st) = step
                    {
                        let _ = self.exec_stmt(pidx, st)?;
                    }
                    let ok = cond
                        .as_ref()
                        .is_none_or(|c| self.eval_expr(c, None).is_true());
                    if ok {
                        self.proc_frames[pidx].push(ExecFrame::For {
                            cond,
                            step,
                            body: body.clone(),
                            run_step: true,
                        });
                        self.proc_frames[pidx].push(ExecFrame::Seq {
                            stmts: vec![body],
                            idx: 0,
                        });
                    }
                }
            }
        }
    }

    fn exec_stmt(&mut self, pidx: usize, stmt: &ElabStmt) -> Result<StepOutcome, String> {
        match stmt {
            ElabStmt::Empty => Ok(StepOutcome::Done),
            ElabStmt::Block(stmts) => {
                if !stmts.is_empty() {
                    self.proc_frames[pidx].push(ExecFrame::Seq {
                        stmts: stmts.clone(),
                        idx: 0,
                    });
                }
                Ok(StepOutcome::Done)
            }
            ElabStmt::Assign {
                blocking,
                lhs,
                delay_ticks,
                rhs,
            } => {
                let ctx_w = self.lvalue_width(lhs);
                let val = self.eval_expr(rhs, Some(ctx_w));
                if *blocking {
                    if let Some(d) = delay_ticks
                        && *d > 0
                    {
                        self.proc_sleeping[pidx] = true;
                        let assign_now = ElabStmt::Assign {
                            blocking: true,
                            lhs: lhs.clone(),
                            delay_ticks: None,
                            rhs: ElabExpr::Const(val),
                        };
                        self.proc_frames[pidx].push(ExecFrame::Seq {
                            stmts: vec![assign_now],
                            idx: 0,
                        });
                        self.schedule(self.now + d, SimEventKind::ResumeProc(pidx));
                        return Ok(StepOutcome::Suspended);
                    }
                    self.write_lvalue(lhs, &val);
                } else if let Some(d) = delay_ticks
                    && *d > 0
                {
                    self.schedule(
                        self.now + d,
                        SimEventKind::DelayedNba {
                            lhs: lhs.clone(),
                            value: val,
                        },
                    );
                } else {
                    self.pending_nba.push((lhs.clone(), val));
                }
                Ok(StepOutcome::Done)
            }
            ElabStmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                if self.eval_expr(cond, None).is_true() {
                    self.exec_stmt(pidx, then_branch)
                } else if let Some(eb) = else_branch {
                    self.exec_stmt(pidx, eb)
                } else {
                    Ok(StepOutcome::Done)
                }
            }
            ElabStmt::Case {
                kind,
                expr,
                items,
                default,
            } => {
                let target = self.eval_expr(expr, None);
                for (pats, body) in items {
                    for pat in pats {
                        let pval = self.eval_expr(pat, Some(target.width()));
                        let matched = match kind {
                            CaseKind::Case => target.case_eq(&pval).is_true(),
                            CaseKind::Casez => target.casez_match(&pval),
                            CaseKind::Casex => target.casex_match(&pval),
                        };
                        if matched {
                            return self.exec_stmt(pidx, body);
                        }
                    }
                }
                if let Some(d) = default {
                    self.exec_stmt(pidx, d)
                } else {
                    Ok(StepOutcome::Done)
                }
            }
            ElabStmt::For {
                init,
                cond,
                step,
                body,
            } => {
                if let Some(init_stmt) = init {
                    let _ = self.exec_stmt(pidx, init_stmt)?;
                }
                self.proc_frames[pidx].push(ExecFrame::For {
                    cond: cond.clone(),
                    step: step.as_deref().cloned(),
                    body: (**body).clone(),
                    run_step: false,
                });
                Ok(StepOutcome::Done)
            }
            ElabStmt::While { cond, body } => {
                self.proc_frames[pidx].push(ExecFrame::While {
                    cond: cond.clone(),
                    body: (**body).clone(),
                });
                Ok(StepOutcome::Done)
            }
            ElabStmt::Repeat { count, body } => {
                let n = self.eval_expr(count, None).to_u64().unwrap_or(0);
                if n > 0 {
                    self.proc_frames[pidx].push(ExecFrame::Repeat {
                        remaining: n,
                        body: (**body).clone(),
                    });
                }
                Ok(StepOutcome::Done)
            }
            ElabStmt::Forever(body) => {
                self.proc_frames[pidx].push(ExecFrame::Forever((**body).clone()));
                Ok(StepOutcome::Done)
            }
            ElabStmt::DelayControl { delay_ticks, body } => {
                if *delay_ticks == 0 {
                    self.exec_stmt(pidx, body)
                } else {
                    self.proc_sleeping[pidx] = true;
                    self.proc_frames[pidx].push(ExecFrame::Seq {
                        stmts: vec![(**body).clone()],
                        idx: 0,
                    });
                    self.schedule(self.now + delay_ticks, SimEventKind::ResumeProc(pidx));
                    Ok(StepOutcome::Suspended)
                }
            }
            ElabStmt::EventControl { body, .. } => {
                // Nested event control: run body immediately if reached
                self.exec_stmt(pidx, body)
            }
            ElabStmt::Force { lhs, rhs } => {
                let ctx_w = self.lvalue_width(lhs);
                let val = self.eval_expr(rhs, Some(ctx_w));
                if let ElabLValue::Signal(sig) = lhs {
                    let info = &self.design.signals[sig.0];
                    let v = val.resized(info.width, info.signed);
                    self.forced[sig.0] = Some(v.clone());
                    if !info.is_net && self.drivers[sig.0].is_empty() {
                        self.unforced[sig.0] = v.clone();
                    }
                    self.commit_visible_value(*sig, v);
                }
                Ok(StepOutcome::Done)
            }
            ElabStmt::Release { lhs } => {
                if let ElabLValue::Signal(sig) = lhs {
                    let prev_forced = self.forced[sig.0].take();
                    let restored = if !self.drivers[sig.0].is_empty() {
                        self.resolve_drivers(*sig)
                    } else if self.design.signals[sig.0].is_net {
                        self.unforced[sig.0].clone()
                    } else {
                        // IEEE 1364-2005 9.3.2: releasing a reg retains its forced value
                        // until the next procedural assignment.
                        prev_forced.unwrap_or_else(|| self.unforced[sig.0].clone())
                    };
                    self.unforced[sig.0] = restored.clone();
                    self.commit_visible_value(*sig, restored);
                }
                Ok(StepOutcome::Done)
            }
            ElabStmt::TaskCall { task_id, args } => {
                let task = self.design.tasks[*task_id].clone();
                for (psig, aexpr) in task.param_sigs.iter().zip(args.iter()) {
                    let w = self.design.signals[psig.0].width;
                    let val = self.eval_expr(aexpr, Some(w));
                    self.write_signal_direct(*psig, val);
                }
                if !task.body.is_empty() {
                    self.proc_frames[pidx].push(ExecFrame::Seq {
                        stmts: task.body,
                        idx: 0,
                    });
                }
                Ok(StepOutcome::Done)
            }
            ElabStmt::SysTaskCall { name, .. } => {
                if name == "$finish" || name == "$stop" {
                    self.finished = true;
                }
                Ok(StepOutcome::Done)
            }
            ElabStmt::Return(_) => Ok(StepOutcome::Return),
        }
    }

    fn apply_driver_lvalue(&mut self, lhs: &ElabLValue, driver_id: usize, value: &BitVec) {
        match lhs {
            ElabLValue::Signal(sig) => {
                let info = &self.design.signals[sig.0];
                let v = value.resized(info.width, info.signed);
                self.drivers[sig.0].insert(driver_id, v);
                let resolved = self.resolve_drivers(*sig);
                self.unforced[sig.0] = resolved.clone();
                if self.forced[sig.0].is_none() {
                    self.commit_visible_value(*sig, resolved);
                }
            }
            ElabLValue::Concat(parts) => {
                let mut bit_pos = 0usize;
                for part in parts.iter().rev() {
                    let w = self.lvalue_width(part);
                    let slice = value.slice(bit_pos, w);
                    self.apply_driver_lvalue(part, driver_id, &slice);
                    bit_pos += w;
                }
            }
            other => {
                // Bit/part select continuous assign: merge into driver's current value
                self.write_lvalue(other, value);
            }
        }
    }

    fn resolve_drivers(&self, sig: SigId) -> BitVec {
        let info = &self.design.signals[sig.0];
        let mut acc = BitVec::filled(info.width, Logic::HighZ);
        for driven in self.drivers[sig.0].values() {
            acc = acc.resolve(driven);
        }
        acc.signed = info.signed;
        acc
    }

    fn write_signal_direct(&mut self, sig: SigId, value: BitVec) {
        let info = &self.design.signals[sig.0];
        let v = value.resized(info.width, info.signed);
        self.unforced[sig.0] = v.clone();
        if self.forced[sig.0].is_none() {
            self.commit_visible_value(sig, v);
        }
    }

    fn commit_visible_value(&mut self, sig: SigId, new_val: BitVec) {
        if self.values[sig.0] == new_val {
            return;
        }
        self.values[sig.0] = new_val.clone();
        if self.watched_sigs[sig.0] {
            self.changed_log.push((self.now, sig, new_val));
        }
        // Wake dependent continuous assigns
        let assigns = self.sig_to_assigns[sig.0].clone();
        for aidx in assigns {
            self.enqueue_assign(aidx);
        }
        // Wake dependent processes (check edge vs level sensitivity)
        let procs = self.sig_to_procs[sig.0].clone();
        for pidx in procs {
            let mut triggered = false;
            let sens_len = self.design.processes[pidx].sensitivity.len();
            for eidx in 0..sens_len {
                match self.design.processes[pidx].sensitivity[eidx].clone() {
                    ElabEvent::Posedge(e) => {
                        let new_bit = self.eval_expr(&e, None).bit(0);
                        let old_bit = self.edge_prev[pidx][eidx];
                        self.edge_prev[pidx][eidx] = new_bit;
                        if (old_bit == Logic::Low && new_bit != Logic::Low)
                            || (old_bit != Logic::High && new_bit == Logic::High)
                        {
                            triggered = true;
                        }
                    }
                    ElabEvent::Negedge(e) => {
                        let new_bit = self.eval_expr(&e, None).bit(0);
                        let old_bit = self.edge_prev[pidx][eidx];
                        self.edge_prev[pidx][eidx] = new_bit;
                        if (old_bit == Logic::High && new_bit != Logic::High)
                            || (old_bit != Logic::Low && new_bit == Logic::Low)
                        {
                            triggered = true;
                        }
                    }
                    ElabEvent::Level(s) => {
                        if s == sig {
                            triggered = true;
                        }
                    }
                }
            }
            if triggered {
                self.enqueue_proc(pidx);
            }
        }
    }

    fn write_lvalue(&mut self, lhs: &ElabLValue, value: &BitVec) {
        match lhs {
            ElabLValue::Signal(sig) => {
                self.write_signal_direct(*sig, value.clone());
            }
            ElabLValue::MemElem { mem, index } => {
                let idx_val = self.eval_expr(index, None);
                if let Some(addr) = idx_val.to_i64_signed()
                    && let Some(slot) = self.design.memories[mem.0].elem_index(addr)
                {
                    let m = &self.design.memories[mem.0];
                    let v = value.resized(m.elem_width, m.elem_signed);
                    if self.memories[mem.0][slot] != v {
                        self.memories[mem.0][slot] = v;
                        let assigns = self.mem_to_assigns[mem.0].clone();
                        for aidx in assigns {
                            self.enqueue_assign(aidx);
                        }
                        let procs = self.mem_to_procs[mem.0].clone();
                        for pidx in procs {
                            self.enqueue_proc(pidx);
                        }
                    }
                }
            }
            ElabLValue::BitSelect { sig, index } => {
                let idx_val = self.eval_expr(index, None);
                if let Some(bit_idx) = idx_val.to_i64_signed()
                    && let Some(off) = self.design.signals[sig.0].bit_offset(bit_idx)
                {
                    let mut cur = self.unforced[sig.0].clone();
                    cur.set_bit(off, value.bit(0));
                    self.write_signal_direct(*sig, cur);
                }
            }
            ElabLValue::PartSelect { sig, msb, lsb } => {
                let info = &self.design.signals[sig.0];
                let lo = (*msb).min(*lsb);
                let w = ((*msb - *lsb).unsigned_abs() as usize + 1).max(1);
                if let Some(off) = info.bit_offset(lo) {
                    let mut cur = self.unforced[sig.0].clone();
                    cur.set_slice(off, &value.resized(w, false));
                    self.write_signal_direct(*sig, cur);
                }
            }
            ElabLValue::IndexedPart {
                sig,
                base,
                width,
                ascending,
            } => {
                let bval = self.eval_expr(base, None);
                if let Some(b) = bval.to_i64_signed() {
                    let lo = if *ascending {
                        b
                    } else {
                        b - (*width as i64) + 1
                    };
                    if let Some(off) = self.design.signals[sig.0].bit_offset(lo) {
                        let mut cur = self.unforced[sig.0].clone();
                        cur.set_slice(off, &value.resized(*width, false));
                        self.write_signal_direct(*sig, cur);
                    }
                }
            }
            ElabLValue::Concat(parts) => {
                let mut bit_pos = 0usize;
                for part in parts.iter().rev() {
                    let w = self.lvalue_width(part);
                    let slice = value.slice(bit_pos, w);
                    self.write_lvalue(part, &slice);
                    bit_pos += w;
                }
            }
        }
    }

    fn lvalue_width(&self, lv: &ElabLValue) -> usize {
        match lv {
            ElabLValue::Signal(s) => self.design.signals[s.0].width,
            ElabLValue::MemElem { mem, .. } => self.design.memories[mem.0].elem_width,
            ElabLValue::BitSelect { .. } => 1,
            ElabLValue::PartSelect { msb, lsb, .. } => {
                ((*msb - *lsb).unsigned_abs() as usize + 1).max(1)
            }
            ElabLValue::IndexedPart { width, .. } => *width,
            ElabLValue::Concat(parts) => parts.iter().map(|p| self.lvalue_width(p)).sum(),
        }
    }

    pub fn eval_expr(&mut self, expr: &ElabExpr, ctx_width: Option<usize>) -> BitVec {
        match expr {
            ElabExpr::Const(bv) => bv.clone(),
            ElabExpr::UnbasedUnsized(bit) => BitVec::filled(ctx_width.unwrap_or(1), *bit),
            ElabExpr::Real(r) => BitVec::from_i64(*r as i64, 32),
            ElabExpr::String(s) => {
                let w = (s.len() * 8).max(8);
                let mut bv = BitVec::zeros(w);
                for (idx, b) in s.bytes().rev().enumerate() {
                    for bit in 0..8 {
                        bv.set_bit(idx * 8 + bit, Logic::from_bool((b >> bit) & 1 != 0));
                    }
                }
                bv
            }
            ElabExpr::Signal(sig) => self.values[sig.0].clone(),
            ElabExpr::MemRead { mem, index } => {
                let idx = self.eval_expr(index, None);
                let m = &self.design.memories[mem.0];
                if let Some(addr) = idx.to_i64_signed()
                    && let Some(slot) = m.elem_index(addr)
                {
                    self.memories[mem.0][slot].clone()
                } else {
                    BitVec::filled(m.elem_width, Logic::Unknown)
                }
            }
            ElabExpr::BitSelect {
                target,
                index,
                sig_hint,
            } => {
                let tv = self.eval_expr(target, None);
                let iv = self.eval_expr(index, None);
                let Some(idx) = iv.to_i64_signed() else {
                    return BitVec::from_logic(Logic::Unknown);
                };
                let off = if let Some(s) = sig_hint {
                    self.design.signals[s.0].bit_offset(idx)
                } else if idx >= 0 && (idx as usize) < tv.width() {
                    Some(idx as usize)
                } else {
                    None
                };
                match off {
                    Some(o) => BitVec::from_logic(tv.bit(o)),
                    None => BitVec::from_logic(Logic::Unknown),
                }
            }
            ElabExpr::PartSelect {
                target,
                msb,
                lsb,
                sig_hint,
            } => {
                let tv = self.eval_expr(target, None);
                let lo = (*msb).min(*lsb);
                let w = ((*msb - *lsb).unsigned_abs() as usize + 1).max(1);
                let off = if let Some(s) = sig_hint {
                    self.design.signals[s.0].bit_offset(lo)
                } else if lo >= 0 {
                    Some(lo as usize)
                } else {
                    None
                };
                match off {
                    Some(o) => tv.slice(o, w),
                    None => BitVec::filled(w, Logic::Unknown),
                }
            }
            ElabExpr::IndexedPart {
                target,
                base,
                width,
                ascending,
                sig_hint,
            } => {
                let tv = self.eval_expr(target, None);
                let bv = self.eval_expr(base, None);
                let Some(b) = bv.to_i64_signed() else {
                    return BitVec::filled(*width, Logic::Unknown);
                };
                let lo = if *ascending {
                    b
                } else {
                    b - (*width as i64) + 1
                };
                let off = if let Some(s) = sig_hint {
                    self.design.signals[s.0].bit_offset(lo)
                } else if lo >= 0 {
                    Some(lo as usize)
                } else {
                    None
                };
                match off {
                    Some(o) => tv.slice(o, *width),
                    None => BitVec::filled(*width, Logic::Unknown),
                }
            }
            ElabExpr::Concat(parts) => {
                let vals: Vec<BitVec> = parts.iter().map(|p| self.eval_expr(p, None)).collect();
                BitVec::concat(&vals)
            }
            ElabExpr::Replicate { count, expr } => {
                let v = self.eval_expr(expr, None);
                v.replicate(*count)
            }
            ElabExpr::Unary { op, expr } => {
                let child_ctx = match op {
                    UnaryOp::Plus | UnaryOp::Minus | UnaryOp::BitNot => ctx_width,
                    _ => None,
                };
                let v = self.eval_expr(expr, child_ctx);
                let v = if let Some(cw) = child_ctx
                    && cw > v.width()
                {
                    v.resized(cw, v.signed)
                } else {
                    v
                };
                eval_unary(*op, &v)
            }
            ElabExpr::Binary { op, lhs, rhs } => {
                let self_det = matches!(
                    op,
                    BinaryOp::LogAnd
                        | BinaryOp::LogOr
                        | BinaryOp::Eq
                        | BinaryOp::Ne
                        | BinaryOp::CaseEq
                        | BinaryOp::CaseNe
                        | BinaryOp::WildEq
                        | BinaryOp::WildNe
                        | BinaryOp::Lt
                        | BinaryOp::Le
                        | BinaryOp::Gt
                        | BinaryOp::Ge
                );
                let is_shift = matches!(
                    op,
                    BinaryOp::Shl | BinaryOp::Shr | BinaryOp::Ashl | BinaryOp::Ashr | BinaryOp::Pow
                );
                if self_det {
                    let l = self.eval_expr(lhs, None);
                    let r = self.eval_expr(rhs, None);
                    eval_binary(*op, &l, &r, None)
                } else if is_shift {
                    let l = self.eval_expr(lhs, ctx_width);
                    let r = self.eval_expr(rhs, None);
                    eval_binary(*op, &l, &r, ctx_width)
                } else {
                    let l = self.eval_expr(lhs, ctx_width);
                    let r = self.eval_expr(rhs, ctx_width);
                    eval_binary(*op, &l, &r, ctx_width)
                }
            }
            ElabExpr::Ternary {
                cond,
                then_expr,
                else_expr,
            } => {
                let c = self.eval_expr(cond, None);
                match c.to_bool_logic() {
                    Logic::High => {
                        let t = self.eval_expr(then_expr, ctx_width);
                        let w = ctx_width.unwrap_or(t.width()).max(t.width());
                        t.resized(w, t.signed)
                    }
                    Logic::Low => {
                        let e = self.eval_expr(else_expr, ctx_width);
                        let w = ctx_width.unwrap_or(e.width()).max(e.width());
                        e.resized(w, e.signed)
                    }
                    _ => {
                        // IEEE 1364: ambiguous condition merges bit-by-bit (equal bits preserved, differing -> X)
                        let t = self.eval_expr(then_expr, ctx_width);
                        let e = self.eval_expr(else_expr, ctx_width);
                        let w = t.width().max(e.width()).max(ctx_width.unwrap_or(0));
                        let signed = t.signed && e.signed;
                        let ta = t.resized(w, signed);
                        let ea = e.resized(w, signed);
                        let mut out = BitVec::zeros(w);
                        out.signed = signed;
                        for i in 0..w {
                            let bt = ta.bit(i);
                            let be = ea.bit(i);
                            out.set_bit(i, if bt == be { bt } else { Logic::Unknown });
                        }
                        out
                    }
                }
            }
            ElabExpr::SysCall { name, args } => match name.as_str() {
                "$signed" if args.len() == 1 => {
                    let mut v = self.eval_expr(&args[0], ctx_width);
                    v.signed = true;
                    v
                }
                "$unsigned" if args.len() == 1 => {
                    let mut v = self.eval_expr(&args[0], ctx_width);
                    v.signed = false;
                    v
                }
                "$time" | "$stime" => BitVec::from_u64(self.now / self.design.ticks_per_ns, 64),
                "$bits" if args.len() == 1 => {
                    let v = self.eval_expr(&args[0], None);
                    BitVec::from_i64(v.width() as i64, 32)
                }
                _ => BitVec::zeros(ctx_width.unwrap_or(32)),
            },
            ElabExpr::FuncCall { func_id, args } => {
                let func = self.design.functions[*func_id].clone();
                for (psig, aexpr) in func.param_sigs.iter().zip(args.iter()) {
                    let w = self.design.signals[psig.0].width;
                    let val = self.eval_expr(aexpr, Some(w));
                    self.write_signal_direct(*psig, val);
                }
                // Run function body synchronously
                let mut ret = None;
                for st in &func.body {
                    if let Some(v) = self.exec_func_stmt(st) {
                        ret = Some(v);
                        break;
                    }
                }
                ret.unwrap_or_else(|| self.values[func.return_sig.0].clone())
            }
        }
    }

    fn exec_func_stmt(&mut self, stmt: &ElabStmt) -> Option<BitVec> {
        match stmt {
            ElabStmt::Empty => None,
            ElabStmt::Block(stmts) => {
                for s in stmts {
                    if let Some(v) = self.exec_func_stmt(s) {
                        return Some(v);
                    }
                }
                None
            }
            ElabStmt::Assign { lhs, rhs, .. } => {
                let w = self.lvalue_width(lhs);
                let v = self.eval_expr(rhs, Some(w));
                self.write_lvalue(lhs, &v);
                None
            }
            ElabStmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                if self.eval_expr(cond, None).is_true() {
                    self.exec_func_stmt(then_branch)
                } else if let Some(eb) = else_branch {
                    self.exec_func_stmt(eb)
                } else {
                    None
                }
            }
            ElabStmt::Case {
                kind,
                expr,
                items,
                default,
            } => {
                let target = self.eval_expr(expr, None);
                for (pats, body) in items {
                    for pat in pats {
                        let pval = self.eval_expr(pat, Some(target.width()));
                        let matched = match kind {
                            CaseKind::Case => target.case_eq(&pval).is_true(),
                            CaseKind::Casez => target.casez_match(&pval),
                            CaseKind::Casex => target.casex_match(&pval),
                        };
                        if matched {
                            return self.exec_func_stmt(body);
                        }
                    }
                }
                default.as_ref().and_then(|d| self.exec_func_stmt(d))
            }
            ElabStmt::For {
                init,
                cond,
                step,
                body,
            } => {
                if let Some(i) = init {
                    self.exec_func_stmt(i);
                }
                let mut iters = 0;
                while cond
                    .as_ref()
                    .is_none_or(|c| self.eval_expr(c, None).is_true())
                {
                    iters += 1;
                    if iters > 65536 {
                        break;
                    }
                    if let Some(v) = self.exec_func_stmt(body) {
                        return Some(v);
                    }
                    if let Some(s) = step {
                        self.exec_func_stmt(s);
                    }
                }
                None
            }
            ElabStmt::Return(val) => Some(
                val.as_ref()
                    .map(|e| self.eval_expr(e, None))
                    .unwrap_or_else(|| BitVec::zeros(1)),
            ),
            _ => None,
        }
    }
}
