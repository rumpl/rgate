use super::{ExecFrame, SimEventKind, VerilogSim};
use crate::ast::CaseKind;
use crate::elaborate::{ElabExpr, ElabLValue, ElabStmt, ProcessKind};

enum StepOutcome {
    Done,
    Suspended,
    Return,
}

impl VerilogSim {
    pub(super) fn run_process(&mut self, pidx: usize) -> Result<(), String> {
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
}
