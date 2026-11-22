use rgate_core::Circuit;

const HISTORY_LIMIT: usize = 128;

/// Owns undo/redo snapshots, dirty-state tracking, and active gesture rollback.
#[derive(Clone, Debug)]
pub struct History {
    saved: Circuit,
    recovered: bool,
    undo: Vec<Circuit>,
    redo: Vec<Circuit>,
    gesture: Option<Circuit>,
}

impl History {
    pub fn new(initial: Circuit) -> Self {
        Self {
            saved: initial,
            recovered: false,
            undo: Vec::new(),
            redo: Vec::new(),
            gesture: None,
        }
    }

    pub fn is_dirty(&self, current: &Circuit) -> bool {
        self.recovered || current != &self.saved
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn in_gesture(&self) -> bool {
        self.gesture.is_some()
    }

    pub fn mark_recovered(&mut self) {
        self.recovered = true;
    }

    pub fn mark_saved(&mut self, current: &Circuit) {
        self.saved = current.clone();
        self.recovered = false;
    }

    pub fn begin_gesture(&mut self, current: &Circuit) {
        self.finish_gesture(current);
        self.gesture = Some(current.clone());
    }

    pub fn finish_gesture(&mut self, current: &Circuit) {
        if let Some(before) = self.gesture.take()
            && &before != current
        {
            self.record(before);
        }
    }

    pub fn cancel_gesture(&mut self) -> Option<Circuit> {
        self.gesture.take()
    }

    pub fn record(&mut self, before: Circuit) {
        self.undo.push(before);
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn undo(&mut self, current: &mut Circuit) -> bool {
        self.finish_gesture(current);
        if let Some(previous) = self.undo.pop() {
            self.redo.push(std::mem::replace(current, previous));
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self, current: &mut Circuit) -> bool {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(current, next));
            true
        } else {
            false
        }
    }
}
