use crate::elaborate::ElabLValue;
use crate::value::BitVec;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

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

/// Owns the stratified Verilog event queues: future heap, deduplicated active queues, and NBA queue.
pub(super) struct EventScheduler {
    queue: BinaryHeap<Reverse<ScheduledEvent>>,
    next_seq: u64,
    active_assigns: VecDeque<usize>,
    in_active_assigns: Vec<bool>,
    active_procs: VecDeque<usize>,
    in_active_procs: Vec<bool>,
    pending_nba: Vec<(ElabLValue, BitVec)>,
}

impl EventScheduler {
    pub(super) fn new(num_assigns: usize, num_procs: usize) -> Self {
        Self {
            queue: BinaryHeap::new(),
            next_seq: 0,
            active_assigns: VecDeque::new(),
            in_active_assigns: vec![false; num_assigns],
            active_procs: VecDeque::new(),
            in_active_procs: vec![false; num_procs],
            pending_nba: Vec::new(),
        }
    }

    pub(super) fn schedule(&mut self, time: u64, kind: SimEventKind) {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.queue.push(Reverse(ScheduledEvent { time, seq, kind }));
    }

    pub(super) fn next_time(&self) -> Option<u64> {
        self.queue.peek().map(|Reverse(ev)| ev.time)
    }

    pub(super) fn has_due(&self, now: u64) -> bool {
        self.queue.peek().is_some_and(|Reverse(ev)| ev.time <= now)
    }

    pub(super) fn pop_due(&mut self, now: u64) -> Option<SimEventKind> {
        if self.has_due(now) {
            Some(self.queue.pop().unwrap().0.kind)
        } else {
            None
        }
    }

    pub(super) fn enqueue_assign(&mut self, aidx: usize) {
        if !self.in_active_assigns[aidx] {
            self.in_active_assigns[aidx] = true;
            self.active_assigns.push_back(aidx);
        }
    }

    pub(super) fn pop_assign(&mut self) -> Option<usize> {
        let aidx = self.active_assigns.pop_front()?;
        self.in_active_assigns[aidx] = false;
        Some(aidx)
    }

    pub(super) fn enqueue_proc(&mut self, pidx: usize, sleeping: bool) {
        if !self.in_active_procs[pidx] && !sleeping {
            self.in_active_procs[pidx] = true;
            self.active_procs.push_back(pidx);
        }
    }

    pub(super) fn pop_proc(&mut self) -> Option<usize> {
        let pidx = self.active_procs.pop_front()?;
        self.in_active_procs[pidx] = false;
        Some(pidx)
    }

    pub(super) fn has_active(&self) -> bool {
        !self.active_assigns.is_empty() || !self.active_procs.is_empty()
    }

    pub(super) fn push_nba(&mut self, lhs: ElabLValue, value: BitVec) {
        self.pending_nba.push((lhs, value));
    }

    pub(super) fn has_nbas(&self) -> bool {
        !self.pending_nba.is_empty()
    }

    pub(super) fn take_nbas(&mut self) -> Vec<(ElabLValue, BitVec)> {
        std::mem::take(&mut self.pending_nba)
    }
}
