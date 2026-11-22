use super::VerilogSim;
use crate::ast::{BinaryOp, CaseKind, UnaryOp};
use crate::elaborate::{ElabExpr, ElabStmt, eval_binary, eval_unary};
use crate::value::BitVec;
use rgate_core::Logic;

impl VerilogSim {
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
