use crate::elaborate::{ElabExpr, ElabStmt};

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

/// Owns per-process coroutine execution stacks, sleep states, and simulation finish flag.
pub(super) struct ProcessState {
    pub(super) frames: Vec<Vec<ExecFrame>>,
    pub(super) sleeping: Vec<bool>,
    pub(super) finished: bool,
}

impl ProcessState {
    pub(super) fn new(num_procs: usize) -> Self {
        Self {
            frames: vec![Vec::new(); num_procs],
            sleeping: vec![false; num_procs],
            finished: false,
        }
    }

    pub(super) fn is_sleeping(&self, pidx: usize) -> bool {
        self.sleeping[pidx]
    }

    pub(super) fn sleep(&mut self, pidx: usize) {
        self.sleeping[pidx] = true;
    }

    pub(super) fn wake(&mut self, pidx: usize) {
        self.sleeping[pidx] = false;
    }
}
