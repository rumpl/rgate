use super::VerilogSim;
use crate::elaborate::{ElabEvent, ElabLValue, SigId};
use crate::value::BitVec;
use rgate_core::Logic;

impl VerilogSim {
    pub(super) fn apply_driver_lvalue(&mut self, lhs: &ElabLValue, driver_id: usize, value: &BitVec) {
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

    pub(super) fn resolve_drivers(&self, sig: SigId) -> BitVec {
        let info = &self.design.signals[sig.0];
        let mut acc = BitVec::filled(info.width, Logic::HighZ);
        for driven in self.drivers[sig.0].values() {
            acc = acc.resolve(driven);
        }
        acc.signed = info.signed;
        acc
    }

    pub(super) fn write_signal_direct(&mut self, sig: SigId, value: BitVec) {
        let info = &self.design.signals[sig.0];
        let v = value.resized(info.width, info.signed);
        self.unforced[sig.0] = v.clone();
        if self.forced[sig.0].is_none() {
            self.commit_visible_value(sig, v);
        }
    }

    pub(super) fn commit_visible_value(&mut self, sig: SigId, new_val: BitVec) {
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

    pub(super) fn write_lvalue(&mut self, lhs: &ElabLValue, value: &BitVec) {
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

    pub(super) fn lvalue_width(&self, lv: &ElabLValue) -> usize {
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
}
