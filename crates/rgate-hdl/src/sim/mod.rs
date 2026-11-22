//! Stratified event-driven four-state Verilog simulator.

mod eval;
mod exec;
mod process;
mod scheduler;
mod signals;
mod store;

use crate::elaborate::*;
use crate::value::BitVec;
use process::ProcessState;
use scheduler::{EventScheduler, SimEventKind};
use store::SignalStore;

pub struct VerilogSim {
    pub design: Design,
    pub now: u64,
    store: SignalStore,
    scheduler: EventScheduler,
    procs: ProcessState,
}

impl VerilogSim {
    pub fn new(design: Design) -> Result<Self, String> {
        let num_assigns = design.assigns.len();
        let num_procs = design.processes.len();
        let store = SignalStore::from_design(&design);
        let scheduler = EventScheduler::new(num_assigns, num_procs);
        let procs = ProcessState::new(num_procs);

        let mut sim = Self {
            design,
            now: 0,
            store,
            scheduler,
            procs,
        };

        // Snapshot initial edge values before t=0 evaluation
        sim.refresh_all_edge_prev();

        // Schedule all continuous assigns at t=0
        for aidx in 0..num_assigns {
            sim.scheduler.enqueue_assign(aidx);
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
        sim.store.clear_changes();
        Ok(sim)
    }

    pub fn watch(&mut self, sig: SigId) {
        self.store.watch(sig);
    }

    pub fn unwatch(&mut self, sig: SigId) {
        self.store.unwatch(sig);
    }

    pub fn value(&self, sig: SigId) -> &BitVec {
        self.store.value(sig)
    }

    pub fn take_changes(&mut self) -> Vec<(u64, SigId, BitVec)> {
        self.store.take_changes()
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
        if self.procs.finished {
            return Ok(0);
        }
        let Some(next_time) = self.scheduler.next_time() else {
            return Err("no scheduled future events".into());
        };
        self.now = next_time;
        self.settle()?;
        Ok(self.store.changed_len())
    }

    /// Advance by `delta_ticks`, settling every intermediate event up to `now + delta_ticks`.
    pub fn advance_ticks(&mut self, delta_ticks: u64) -> Result<usize, String> {
        let target = self
            .now
            .checked_add(delta_ticks)
            .ok_or_else(|| "simulation time overflow".to_string())?;
        while !self.procs.finished {
            let Some(next_time) = self.scheduler.next_time() else {
                break;
            };
            if next_time > target {
                break;
            }
            self.now = next_time;
            self.settle()?;
        }
        self.now = target;
        Ok(self.store.changed_len())
    }

    pub(super) fn enqueue_proc(&mut self, pidx: usize) {
        self.scheduler
            .enqueue_proc(pidx, self.procs.is_sleeping(pidx));
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
                    ElabEvent::Level(s) => self.store.value(*s).bit(0),
                };
                self.store.edge_prev[pidx][eidx] = bit;
            }
        }
    }

    fn settle(&mut self) -> Result<(), String> {
        let mut deltas = 0usize;
        loop {
            // Drain all scheduled events at `self.now`
            while let Some(kind) = self.scheduler.pop_due(self.now) {
                match kind {
                    SimEventKind::DriverUpdate { assign_idx, value } => {
                        let driver_id = self.design.assigns[assign_idx].driver_id;
                        let lhs = self.design.assigns[assign_idx].lhs.clone();
                        self.apply_driver_lvalue(&lhs, driver_id, &value);
                    }
                    SimEventKind::DelayedNba { lhs, value } => {
                        self.write_lvalue(&lhs, &value);
                    }
                    SimEventKind::ResumeProc(pidx) => {
                        self.procs.wake(pidx);
                        self.enqueue_proc(pidx);
                    }
                }
            }

            // Active region: continuous assigns + processes
            while self.scheduler.has_active() {
                deltas += 1;
                if deltas > 200_000 {
                    return Err("zero-delay simulation loop exceeded delta limit".into());
                }
                while let Some(aidx) = self.scheduler.pop_assign() {
                    let delay = self.design.assigns[aidx].delay_ticks;
                    let lhs = self.design.assigns[aidx].lhs.clone();
                    let rhs = self.design.assigns[aidx].rhs.clone();
                    let driver_id = self.design.assigns[aidx].driver_id;
                    let ctx_w = self.lvalue_width(&lhs);
                    let val = self.eval_expr(&rhs, Some(ctx_w));
                    if delay == 0 {
                        self.apply_driver_lvalue(&lhs, driver_id, &val);
                    } else {
                        self.scheduler.schedule(
                            self.now + delay,
                            SimEventKind::DriverUpdate {
                                assign_idx: aidx,
                                value: val,
                            },
                        );
                    }
                }
                if let Some(pidx) = self.scheduler.pop_proc() {
                    self.run_process(pidx)?;
                }
            }

            // NBA region
            if !self.scheduler.has_nbas() {
                if self.scheduler.has_due(self.now) {
                    continue;
                }
                break;
            }
            for (lhs, val) in self.scheduler.take_nbas() {
                self.write_lvalue(&lhs, &val);
            }
        }
        Ok(())
    }
}
