use super::const_eval::{clog2, eval_const_expr, eval_const_i64};
use super::ir::{ElabExpr, ElabLValue, ElabStmt, SigId};
use super::Elaborator;
use crate::ast::{Expr, LValue, Stmt};
use crate::value::BitVec;
use std::collections::HashMap;

impl<'a> Elaborator<'a> {
    pub(super) fn elab_stmt(
        &mut self,
        scope_idx: usize,
        stmt: &Stmt,
        unit_ticks: u64,
        locals: &mut HashMap<String, SigId>,
    ) -> Result<ElabStmt, String> {
        Ok(match stmt {
            Stmt::Empty => ElabStmt::Empty,
            Stmt::Block { decls, stmts, .. } => {
                let mut inner_locals = locals.clone();
                self.elab_local_decls(scope_idx, decls, &mut inner_locals)?;
                let mut out = Vec::with_capacity(stmts.len());
                for s in stmts {
                    out.push(self.elab_stmt(scope_idx, s, unit_ticks, &mut inner_locals)?);
                }
                ElabStmt::Block(out)
            }
            Stmt::Assign {
                blocking,
                lhs,
                delay,
                rhs,
            } => {
                let delay_ticks = if delay.is_some() {
                    Some(self.eval_delay(scope_idx, delay.as_ref(), unit_ticks)?)
                } else {
                    None
                };
                ElabStmt::Assign {
                    blocking: *blocking,
                    lhs: self.elab_lvalue(scope_idx, lhs, locals)?,
                    delay_ticks,
                    rhs: self.elab_expr(scope_idx, rhs, locals)?,
                }
            }
            Stmt::If {
                cond,
                then_branch,
                else_branch,
            } => ElabStmt::If {
                cond: self.elab_expr(scope_idx, cond, locals)?,
                then_branch: Box::new(self.elab_stmt(scope_idx, then_branch, unit_ticks, locals)?),
                else_branch: else_branch
                    .as_ref()
                    .map(|e| self.elab_stmt(scope_idx, e, unit_ticks, locals).map(Box::new))
                    .transpose()?,
            },
            Stmt::Case {
                kind,
                expr,
                items,
                default,
            } => {
                let expr = self.elab_expr(scope_idx, expr, locals)?;
                let mut eitems = Vec::with_capacity(items.len());
                for (pats, body) in items {
                    let epats = pats
                        .iter()
                        .map(|p| self.elab_expr(scope_idx, p, locals))
                        .collect::<Result<Vec<_>, _>>()?;
                    let ebody = self.elab_stmt(scope_idx, body, unit_ticks, locals)?;
                    eitems.push((epats, ebody));
                }
                let default = default
                    .as_ref()
                    .map(|d| self.elab_stmt(scope_idx, d, unit_ticks, locals).map(Box::new))
                    .transpose()?;
                ElabStmt::Case {
                    kind: *kind,
                    expr,
                    items: eitems,
                    default,
                }
            }
            Stmt::For {
                init,
                cond,
                step,
                body,
            } => {
                let mut inner_locals = locals.clone();
                // Support `for (int i = 0; ...)` where `i` is not declared in outer scope
                if let Some(init_stmt) = init
                    && let Stmt::Assign {
                        lhs: LValue::Ident(p),
                        ..
                    } = init_stmt.as_ref()
                    && p.len() == 1
                    && !inner_locals.contains_key(&p[0])
                    && !self.scopes[scope_idx].sigs.contains_key(&p[0])
                {
                    let s = self.alloc_signal(
                        format!("{}.__for_{}", self.scopes[scope_idx].path, p[0]),
                        31,
                        0,
                        true,
                        false,
                        BitVec::zeros(32),
                        false,
                    );
                    inner_locals.insert(p[0].clone(), s);
                }
                ElabStmt::For {
                    init: init
                        .as_ref()
                        .map(|s| {
                            self.elab_stmt(scope_idx, s, unit_ticks, &mut inner_locals)
                                .map(Box::new)
                        })
                        .transpose()?,
                    cond: cond
                        .as_ref()
                        .map(|c| self.elab_expr(scope_idx, c, &inner_locals))
                        .transpose()?,
                    step: step
                        .as_ref()
                        .map(|s| {
                            self.elab_stmt(scope_idx, s, unit_ticks, &mut inner_locals)
                                .map(Box::new)
                        })
                        .transpose()?,
                    body: Box::new(self.elab_stmt(
                        scope_idx,
                        body,
                        unit_ticks,
                        &mut inner_locals,
                    )?),
                }
            }
            Stmt::While { cond, body } => ElabStmt::While {
                cond: self.elab_expr(scope_idx, cond, locals)?,
                body: Box::new(self.elab_stmt(scope_idx, body, unit_ticks, locals)?),
            },
            Stmt::Repeat { count, body } => ElabStmt::Repeat {
                count: self.elab_expr(scope_idx, count, locals)?,
                body: Box::new(self.elab_stmt(scope_idx, body, unit_ticks, locals)?),
            },
            Stmt::Forever(body) => ElabStmt::Forever(Box::new(
                self.elab_stmt(scope_idx, body, unit_ticks, locals)?,
            )),
            Stmt::DelayControl { delay, body } => ElabStmt::DelayControl {
                delay_ticks: self.eval_delay(scope_idx, Some(delay), unit_ticks)?,
                body: Box::new(self.elab_stmt(scope_idx, body, unit_ticks, locals)?),
            },
            Stmt::EventControl { events, body } => {
                let mut sens = Vec::new();
                for ev in events {
                    self.elab_event(scope_idx, ev, locals, &mut sens)?;
                }
                ElabStmt::EventControl {
                    events: sens,
                    body: Box::new(self.elab_stmt(scope_idx, body, unit_ticks, locals)?),
                }
            }
            Stmt::Force { lhs, rhs } => ElabStmt::Force {
                lhs: self.elab_lvalue(scope_idx, lhs, locals)?,
                rhs: self.elab_expr(scope_idx, rhs, locals)?,
            },
            Stmt::Release { lhs } => ElabStmt::Release {
                lhs: self.elab_lvalue(scope_idx, lhs, locals)?,
            },
            Stmt::TaskCall { name, args } => {
                let eargs = args
                    .iter()
                    .map(|a| self.elab_expr(scope_idx, a, locals))
                    .collect::<Result<Vec<_>, _>>()?;
                if let Some(&task_id) = self.scopes[scope_idx].tasks.get(name) {
                    ElabStmt::TaskCall {
                        task_id,
                        args: eargs,
                    }
                } else {
                    ElabStmt::Empty
                }
            }
            Stmt::SysTaskCall { name, args } => {
                let eargs = args
                    .iter()
                    .map(|a| self.elab_expr(scope_idx, a, locals))
                    .collect::<Result<Vec<_>, _>>()?;
                ElabStmt::SysTaskCall {
                    name: name.clone(),
                    args: eargs,
                }
            }
            Stmt::Return(val) => ElabStmt::Return(
                val.as_ref()
                    .map(|e| self.elab_expr(scope_idx, e, locals))
                    .transpose()?,
            ),
        })
    }

    pub(super) fn elab_lvalue(
        &self,
        scope_idx: usize,
        lv: &LValue,
        locals: &HashMap<String, SigId>,
    ) -> Result<ElabLValue, String> {
        match lv {
            LValue::Ident(path) => {
                let sig = self.resolve_sig(scope_idx, path, locals).ok_or_else(|| {
                    format!(
                        "unknown signal `{}` in `{}`",
                        path.join("."),
                        self.scopes[scope_idx].path
                    )
                })?;
                Ok(ElabLValue::Signal(sig))
            }
            LValue::Index { target, index } => {
                if let LValue::Ident(path) = target.as_ref()
                    && let Some(mem) = self.resolve_mem(scope_idx, path)
                {
                    return Ok(ElabLValue::MemElem {
                        mem,
                        index: self.elab_expr(scope_idx, index, locals)?,
                    });
                }
                let ElabLValue::Signal(sig) = self.elab_lvalue(scope_idx, target, locals)? else {
                    return Err("unsupported nested index in lvalue".into());
                };
                Ok(ElabLValue::BitSelect {
                    sig,
                    index: self.elab_expr(scope_idx, index, locals)?,
                })
            }
            LValue::Range { target, msb, lsb } => {
                let ElabLValue::Signal(sig) = self.elab_lvalue(scope_idx, target, locals)? else {
                    return Err("unsupported nested range in lvalue".into());
                };
                let m = eval_const_i64(msb, &self.scopes[scope_idx].params)?;
                let l = eval_const_i64(lsb, &self.scopes[scope_idx].params)?;
                Ok(ElabLValue::PartSelect {
                    sig,
                    msb: m,
                    lsb: l,
                })
            }
            LValue::IndexedPart {
                target,
                base,
                width,
                ascending,
            } => {
                let ElabLValue::Signal(sig) = self.elab_lvalue(scope_idx, target, locals)? else {
                    return Err("unsupported nested indexed part-select in lvalue".into());
                };
                let w = eval_const_i64(width, &self.scopes[scope_idx].params)?.max(1) as usize;
                Ok(ElabLValue::IndexedPart {
                    sig,
                    base: self.elab_expr(scope_idx, base, locals)?,
                    width: w,
                    ascending: *ascending,
                })
            }
            LValue::Concat(parts) => Ok(ElabLValue::Concat(
                parts
                    .iter()
                    .map(|p| self.elab_lvalue(scope_idx, p, locals))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
        }
    }

    pub(super) fn elab_expr(
        &self,
        scope_idx: usize,
        expr: &Expr,
        locals: &HashMap<String, SigId>,
    ) -> Result<ElabExpr, String> {
        let params = &self.scopes[scope_idx].params;
        Ok(match expr {
            Expr::Int(n) => ElabExpr::Const(BitVec::from_i64(*n as i64, 32)),
            Expr::Based { value, .. } => ElabExpr::Const(value.clone()),
            Expr::UnbasedUnsized(bit) => ElabExpr::UnbasedUnsized(*bit),
            Expr::Real(r) => ElabExpr::Real(*r),
            Expr::String(s) => ElabExpr::String(s.clone()),
            Expr::Ident(path) => {
                if path.len() == 1
                    && let Some(val) = params.get(&path[0])
                {
                    return Ok(ElabExpr::Const(val.clone()));
                }
                if let Some(sig) = self.resolve_sig(scope_idx, path, locals) {
                    return Ok(ElabExpr::Signal(sig));
                }
                return Err(format!(
                    "unknown identifier `{}` in `{}`",
                    path.join("."),
                    self.scopes[scope_idx].path
                ));
            }
            Expr::Index { target, index } => {
                if let Expr::Ident(path) = target.as_ref()
                    && let Some(mem) = self.resolve_mem(scope_idx, path)
                {
                    return Ok(ElabExpr::MemRead {
                        mem,
                        index: Box::new(self.elab_expr(scope_idx, index, locals)?),
                    });
                }
                let et = self.elab_expr(scope_idx, target, locals)?;
                let sig_hint = if let ElabExpr::Signal(s) = et {
                    Some(s)
                } else {
                    None
                };
                ElabExpr::BitSelect {
                    target: Box::new(et),
                    index: Box::new(self.elab_expr(scope_idx, index, locals)?),
                    sig_hint,
                }
            }
            Expr::Range { target, msb, lsb } => {
                let et = self.elab_expr(scope_idx, target, locals)?;
                let sig_hint = if let ElabExpr::Signal(s) = et {
                    Some(s)
                } else {
                    None
                };
                let m = eval_const_i64(msb, params)?;
                let l = eval_const_i64(lsb, params)?;
                ElabExpr::PartSelect {
                    target: Box::new(et),
                    msb: m,
                    lsb: l,
                    sig_hint,
                }
            }
            Expr::IndexedPart {
                target,
                base,
                width,
                ascending,
            } => {
                let et = self.elab_expr(scope_idx, target, locals)?;
                let sig_hint = if let ElabExpr::Signal(s) = et {
                    Some(s)
                } else {
                    None
                };
                let w = eval_const_i64(width, params)?.max(1) as usize;
                ElabExpr::IndexedPart {
                    target: Box::new(et),
                    base: Box::new(self.elab_expr(scope_idx, base, locals)?),
                    width: w,
                    ascending: *ascending,
                    sig_hint,
                }
            }
            Expr::Concat(parts) => ElabExpr::Concat(
                parts
                    .iter()
                    .map(|p| self.elab_expr(scope_idx, p, locals))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            Expr::Replicate { count, expr } => {
                let c = eval_const_i64(count, params)?.max(0) as usize;
                ElabExpr::Replicate {
                    count: c,
                    expr: Box::new(self.elab_expr(scope_idx, expr, locals)?),
                }
            }
            Expr::Unary { op, expr } => ElabExpr::Unary {
                op: *op,
                expr: Box::new(self.elab_expr(scope_idx, expr, locals)?),
            },
            Expr::Binary { op, lhs, rhs } => ElabExpr::Binary {
                op: *op,
                lhs: Box::new(self.elab_expr(scope_idx, lhs, locals)?),
                rhs: Box::new(self.elab_expr(scope_idx, rhs, locals)?),
            },
            Expr::Ternary {
                cond,
                then_expr,
                else_expr,
            } => ElabExpr::Ternary {
                cond: Box::new(self.elab_expr(scope_idx, cond, locals)?),
                then_expr: Box::new(self.elab_expr(scope_idx, then_expr, locals)?),
                else_expr: Box::new(self.elab_expr(scope_idx, else_expr, locals)?),
            },
            Expr::Call { name, args } => {
                let func_id = *self.scopes[scope_idx]
                    .funcs
                    .get(name)
                    .ok_or_else(|| format!("unknown function `{name}`"))?;
                let eargs = args
                    .iter()
                    .map(|a| self.elab_expr(scope_idx, a, locals))
                    .collect::<Result<Vec<_>, _>>()?;
                ElabExpr::FuncCall {
                    func_id,
                    args: eargs,
                }
            }
            Expr::SysCall { name, args } => {
                if name == "$clog2" && args.len() == 1 {
                    let v = eval_const_expr(&args[0], params)?;
                    let n = v.to_u64().unwrap_or(1);
                    let res = clog2(n);
                    return Ok(ElabExpr::Const(BitVec::from_i64(res as i64, 32)));
                }
                let eargs = args
                    .iter()
                    .map(|a| self.elab_expr(scope_idx, a, locals))
                    .collect::<Result<Vec<_>, _>>()?;
                ElabExpr::SysCall {
                    name: name.clone(),
                    args: eargs,
                }
            }
        })
    }
}

pub(super) fn expr_to_lvalue(expr: &Expr) -> Result<LValue, String> {
    Ok(match expr {
        Expr::Ident(p) => LValue::Ident(p.clone()),
        Expr::Index { target, index } => LValue::Index {
            target: Box::new(expr_to_lvalue(target)?),
            index: (**index).clone(),
        },
        Expr::Range { target, msb, lsb } => LValue::Range {
            target: Box::new(expr_to_lvalue(target)?),
            msb: (**msb).clone(),
            lsb: (**lsb).clone(),
        },
        Expr::IndexedPart {
            target,
            base,
            width,
            ascending,
        } => LValue::IndexedPart {
            target: Box::new(expr_to_lvalue(target)?),
            base: (**base).clone(),
            width: (**width).clone(),
            ascending: *ascending,
        },
        Expr::Concat(parts) => LValue::Concat(
            parts
                .iter()
                .map(expr_to_lvalue)
                .collect::<Result<Vec<_>, _>>()?,
        ),
        _ => return Err("port output connection must be a valid lvalue".into()),
    })
}
