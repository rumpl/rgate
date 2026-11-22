use crate::SimError;
use rgate_core::Signal;
use std::{
    cmp::{Ordering, Reverse},
    collections::{BinaryHeap, HashMap},
};

#[derive(Clone, Debug)]
pub(crate) enum EventKind {
    Drive {
        gate: usize,
        pin: String,
        value: Signal,
        generation: u64,
    },
    Clock(usize),
    TerminalCapture {
        gate: usize,
        byte: u8,
    },
    TerminalConsume {
        gate: usize,
    },
}

#[derive(Clone, Debug)]
struct Event {
    time: u64,
    sequence: u64,
    kind: EventKind,
}

impl PartialEq for Event {
    fn eq(&self, other: &Self) -> bool {
        (self.time, self.sequence) == (other.time, other.sequence)
    }
}
impl Eq for Event {}
impl PartialOrd for Event {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Event {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.time, self.sequence).cmp(&(other.time, other.sequence))
    }
}

/// Deterministic event queue with per-output inertial-delay generation tracking.
#[derive(Clone, Debug, Default)]
pub(crate) struct EventQueue {
    sequence: u64,
    heap: BinaryHeap<Reverse<Event>>,
    intended: HashMap<(usize, String), Signal>,
    generations: HashMap<(usize, String), u64>,
}

impl EventQueue {
    pub(crate) fn push(&mut self, time: u64, kind: EventKind) {
        self.sequence += 1;
        self.heap.push(Reverse(Event {
            time,
            sequence: self.sequence,
            kind,
        }));
    }

    pub(crate) fn next_time(&self) -> Option<u64> {
        self.heap.peek().map(|event| event.0.time)
    }

    pub(crate) fn pop_at(&mut self, time: u64) -> Option<EventKind> {
        if self.heap.peek().is_some_and(|event| event.0.time == time) {
            Some(self.heap.pop().unwrap().0.kind)
        } else {
            None
        }
    }

    pub(crate) fn is_current_generation(&self, key: &(usize, String), generation: u64) -> bool {
        self.generations.get(key).copied().unwrap_or(0) == generation
    }

    /// Schedule an output pin drive with inertial delay filtering:
    /// if the intended value did not change, no event is scheduled;
    /// otherwise the generation increments, superseding earlier pending drives.
    pub(crate) fn schedule_drive(
        &mut self,
        now: u64,
        delay: u64,
        gate: usize,
        pin: String,
        value: Signal,
    ) -> Result<(), SimError> {
        let key = (gate, pin.clone());
        if self.intended.get(&key) == Some(&value) {
            return Ok(());
        }
        self.intended.insert(key.clone(), value.clone());
        let generation = self.generations.entry(key).or_default();
        *generation += 1;
        let generation = *generation;
        let time = now.checked_add(delay).ok_or(SimError::TimeOverflow)?;
        self.push(
            time,
            EventKind::Drive {
                gate,
                pin,
                value,
                generation,
            },
        );
        Ok(())
    }
}
