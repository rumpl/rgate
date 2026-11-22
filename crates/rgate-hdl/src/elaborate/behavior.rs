use super::const_eval::{eval_const_expr, eval_range};
use super::deps::{collect_expr_deps, collect_stmt_reads};
use super::expr::expr_to_lvalue;
use super::ir::{
    ContinuousAssign, ElabEvent, ElabExpr, ElabLValue, ElabStmt, ProcessInfo, ProcessKind, SigId,
};
use super::Elaborator;
use crate::ast::{
    AlwaysKind, BinaryOp, EventExpr, Expr, ModuleItem, PortDir, PrimitiveKind, Stmt, UnaryOp,
};
use crate::value::BitVec;
use rgate_core::Logic;
use std::collections::HashMap;

impl<'a> Elaborator<'a> {
    pub(super) fn elaborate_scope_behavior(&mut self, scope_idx: usize) -> Result<(), String> {
        let unit_ticks = (self.scopes[scope_idx].timescale.unit_fs / self.precision_fs).max(1);
        let wire_inits = self.scopes[scope_idx].wire_inits.clone();
        let reg_inits = self.scopes[scope_idx].reg_inits.clone();
        let child_conns = self.scopes[scope_idx].child_conns.clone();
        let items = self.scopes[scope_idx].items.clone();

        // 1. Wire initializers -> continuous assignments
        for (sig_id, expr) in wire_inits {
            let rhs = self.elab_expr(scope_idx, &expr, &HashMap::new())?;
            self.push_assign(ElabLValue::Signal(sig_id), rhs, 0);
        }

        // 2. Reg initializers -> evaluate constant or schedule at t=0
        if !reg_inits.is_empty() {
            let mut stmts = Vec::new();
            for (sig_id, expr) in reg_inits {
                if let Ok(val) = eval_const_expr(&expr, &self.scopes[scope_idx].params) {
                    let sig = &mut self.signals[sig_id.0];
                    sig.init = val.resized(sig.width, sig.signed);
                } else {
                    let rhs = self.elab_expr(scope_idx, &expr, &HashMap::new())?;
                    stmts.push(ElabStmt::Assign {
                        blocking: true,
                        lhs: ElabLValue::Signal(sig_id),
                        delay_ticks: None,
                        rhs,
                    });
                }
            }
            if !stmts.is_empty() {
                self.processes.push(ProcessInfo {
                    kind: ProcessKind::Initial,
                    body: ElabStmt::Block(stmts),
                    unit_ticks,
                    sensitivity: Vec::new(),
                    mem_sensitivity: Vec::new(),
                });
            }
        }

        // 3. Port connections to child instances
        for (child_idx, conns) in child_conns {
            let child_ports = self.scopes[child_idx].ports.clone();
            for (pos, (pname, expr_opt)) in conns.into_iter().enumerate() {
                let Some(expr) = expr_opt else { continue };
                let (port_name, dir) = if let Some(n) = pname {
                    let d = child_ports
                        .iter()
                        .find(|(pn, _)| pn == &n)
                        .map(|(_, d)| *d)
                        .ok_or_else(|| {
                            format!(
                                "unknown port `.{n}` on module `{}`",
                                self.scopes[child_idx].module_name
                            )
                        })?;
                    (n, d)
                } else {
                    child_ports.get(pos).cloned().ok_or_else(|| {
                        format!(
                            "too many positional ports on `{}`",
                            self.scopes[child_idx].path
                        )
                    })?
                };
                let Some(&child_sig) = self.scopes[child_idx].sigs.get(&port_name) else {
                    continue;
                };
                match dir {
                    PortDir::Input => {
                        let rhs = self.elab_expr(scope_idx, &expr, &HashMap::new())?;
                        self.push_assign(ElabLValue::Signal(child_sig), rhs, 0);
                    }
                    PortDir::Output => {
                        let lv = expr_to_lvalue(&expr)?;
                        let lhs = self.elab_lvalue(scope_idx, &lv, &HashMap::new())?;
                        self.push_assign(lhs, ElabExpr::Signal(child_sig), 0);
                    }
                    PortDir::Inout => {
                        // Wire both directions via continuous assigns (multi-driver resolution handles Z)
                        let rhs = self.elab_expr(scope_idx, &expr, &HashMap::new())?;
                        self.push_assign(ElabLValue::Signal(child_sig), rhs, 0);
                        if let Ok(lv) = expr_to_lvalue(&expr)
                            && let Ok(lhs) = self.elab_lvalue(scope_idx, &lv, &HashMap::new())
                        {
                            self.push_assign(lhs, ElabExpr::Signal(child_sig), 0);
                        }
                    }
                }
            }
        }

        // 4. Module items: Assign, GatePrimitive, Always, Initial, Final, Function, Task
        for item in items {
            match item {
                ModuleItem::Assign {
                    delay, lhs, rhs, ..
                } => {
                    let delay_ticks = self.eval_delay(scope_idx, delay.as_ref(), unit_ticks)?;
                    let lhs = self.elab_lvalue(scope_idx, &lhs, &HashMap::new())?;
                    let rhs = self.elab_expr(scope_idx, &rhs, &HashMap::new())?;
                    self.push_assign(lhs, rhs, delay_ticks);
                }
                ModuleItem::GatePrimitive {
                    kind,
                    delay,
                    args,
                    ..
                } => {
                    let delay_ticks = self.eval_delay(scope_idx, delay.as_ref(), unit_ticks)?;
                    self.elab_gate_primitive(scope_idx, kind, delay_ticks, &args)?;
                }
                ModuleItem::Always { kind, body, .. } => {
                    let mut locals = HashMap::new();
                    let (proc_kind, sensitivity, mem_sens, elab_body) = match kind {
                        AlwaysKind::Comb | AlwaysKind::Latch => {
                            let body = self.elab_stmt(scope_idx, &body, unit_ticks, &mut locals)?;
                            let (sig_deps, mem_deps) = collect_stmt_reads(&body);
                            let sens = sig_deps.into_iter().map(ElabEvent::Level).collect();
                            (ProcessKind::AlwaysComb, sens, mem_deps, body)
                        }
                        AlwaysKind::Always | AlwaysKind::Ff => {
                            if let Stmt::EventControl {
                                events,
                                body: inner,
                            } = body
                            {
                                let elab_inner =
                                    self.elab_stmt(scope_idx, &inner, unit_ticks, &mut locals)?;
                                if events.iter().any(|e| matches!(e, EventExpr::Star)) {
                                    let (sig_deps, mem_deps) = collect_stmt_reads(&elab_inner);
                                    let sens = sig_deps.into_iter().map(ElabEvent::Level).collect();
                                    (ProcessKind::AlwaysComb, sens, mem_deps, elab_inner)
                                } else {
                                    let mut sens = Vec::new();
                                    for ev in &events {
                                        self.elab_event(scope_idx, ev, &locals, &mut sens)?;
                                    }
                                    (ProcessKind::Always, sens, Vec::new(), elab_inner)
                                }
                            } else {
                                let elab_body =
                                    self.elab_stmt(scope_idx, &body, unit_ticks, &mut locals)?;
                                (ProcessKind::Always, Vec::new(), Vec::new(), elab_body)
                            }
                        }
                    };
                    self.processes.push(ProcessInfo {
                        kind: proc_kind,
                        body: elab_body,
                        unit_ticks,
                        sensitivity,
                        mem_sensitivity: mem_sens,
                    });
                }
                ModuleItem::Initial { body, .. } => {
                    let mut locals = HashMap::new();
                    let body = self.elab_stmt(scope_idx, &body, unit_ticks, &mut locals)?;
                    self.processes.push(ProcessInfo {
                        kind: ProcessKind::Initial,
                        body,
                        unit_ticks,
                        sensitivity: Vec::new(),
                        mem_sensitivity: Vec::new(),
                    });
                }
                ModuleItem::Final { body, .. } => {
                    let mut locals = HashMap::new();
                    let body = self.elab_stmt(scope_idx, &body, unit_ticks, &mut locals)?;
                    self.processes.push(ProcessInfo {
                        kind: ProcessKind::Final,
                        body,
                        unit_ticks,
                        sensitivity: Vec::new(),
                        mem_sensitivity: Vec::new(),
                    });
                }
                ModuleItem::Function(f) => {
                    let func_id = self.scopes[scope_idx].funcs[&f.name];
                    let ret_sig = self.functions[func_id].return_sig;
                    let mut locals = HashMap::new();
                    locals.insert(f.name.clone(), ret_sig);
                    let mut param_sigs = Vec::new();
                    for inp in &f.inputs {
                        let (msb, lsb) = if let Some(r) = &inp.range {
                            eval_range(r, &self.scopes[scope_idx].params)?
                        } else {
                            (0, 0)
                        };
                        let w = ((msb - lsb).unsigned_abs() as usize + 1).max(1);
                        for v in &inp.items {
                            let s = self.alloc_signal(
                                format!("{}.__fn_{}_{}", self.scopes[scope_idx].path, f.name, v.name),
                                msb,
                                lsb,
                                inp.signed,
                                false,
                                BitVec::filled(w, Logic::Unknown),
                                false,
                            );
                            locals.insert(v.name.clone(), s);
                            param_sigs.push(s);
                        }
                    }
                    self.elab_local_decls(scope_idx, &f.decls, &mut locals)?;
                    let mut body = Vec::new();
                    for st in &f.body {
                        body.push(self.elab_stmt(scope_idx, st, unit_ticks, &mut locals)?);
                    }
                    self.functions[func_id].param_sigs = param_sigs;
                    self.functions[func_id].body = body;
                }
                ModuleItem::Task(t) => {
                    let task_id = self.scopes[scope_idx].tasks[&t.name];
                    let mut locals = HashMap::new();
                    let mut param_sigs = Vec::new();
                    for inp in &t.inputs {
                        let (msb, lsb) = if let Some(r) = &inp.range {
                            eval_range(r, &self.scopes[scope_idx].params)?
                        } else {
                            (0, 0)
                        };
                        let w = ((msb - lsb).unsigned_abs() as usize + 1).max(1);
                        for v in &inp.items {
                            let s = self.alloc_signal(
                                format!("{}.__tk_{}_{}", self.scopes[scope_idx].path, t.name, v.name),
                                msb,
                                lsb,
                                inp.signed,
                                false,
                                BitVec::filled(w, Logic::Unknown),
                                false,
                            );
                            locals.insert(v.name.clone(), s);
                            param_sigs.push(s);
                        }
                    }
                    self.elab_local_decls(scope_idx, &t.decls, &mut locals)?;
                    let mut body = Vec::new();
                    for st in &t.body {
                        body.push(self.elab_stmt(scope_idx, st, unit_ticks, &mut locals)?);
                    }
                    self.tasks[task_id].param_sigs = param_sigs;
                    self.tasks[task_id].body = body;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn push_assign(&mut self, lhs: ElabLValue, rhs: ElabExpr, delay_ticks: u64) {
        let (sig_deps, mem_deps) = collect_expr_deps(&rhs);
        let driver_id = self.next_driver_id;
        self.next_driver_id += 1;
        self.assigns.push(ContinuousAssign {
            driver_id,
            lhs,
            rhs,
            delay_ticks,
            sig_deps,
            mem_deps,
        });
    }

    fn elab_gate_primitive(
        &mut self,
        scope_idx: usize,
        kind: PrimitiveKind,
        delay_ticks: u64,
        args: &[Expr],
    ) -> Result<(), String> {
        if args.len() < 2 {
            return Err("gate primitive requires at least 2 arguments".into());
        }
        let lhs_lv = expr_to_lvalue(&args[0])?;
        let lhs = self.elab_lvalue(scope_idx, &lhs_lv, &HashMap::new())?;
        let inputs: Vec<ElabExpr> = args[1..]
            .iter()
            .map(|a| self.elab_expr(scope_idx, a, &HashMap::new()))
            .collect::<Result<_, _>>()?;

        let rhs = match kind {
            PrimitiveKind::Buf => inputs[0].clone(),
            PrimitiveKind::Not => ElabExpr::Unary {
                op: UnaryOp::BitNot,
                expr: Box::new(inputs[0].clone()),
            },
            PrimitiveKind::And | PrimitiveKind::Nand => {
                let mut acc = inputs[0].clone();
                for inp in &inputs[1..] {
                    acc = ElabExpr::Binary {
                        op: BinaryOp::BitAnd,
                        lhs: Box::new(acc),
                        rhs: Box::new(inp.clone()),
                    };
                }
                if kind == PrimitiveKind::Nand {
                    ElabExpr::Unary {
                        op: UnaryOp::BitNot,
                        expr: Box::new(acc),
                    }
                } else {
                    acc
                }
            }
            PrimitiveKind::Or | PrimitiveKind::Nor => {
                let mut acc = inputs[0].clone();
                for inp in &inputs[1..] {
                    acc = ElabExpr::Binary {
                        op: BinaryOp::BitOr,
                        lhs: Box::new(acc),
                        rhs: Box::new(inp.clone()),
                    };
                }
                if kind == PrimitiveKind::Nor {
                    ElabExpr::Unary {
                        op: UnaryOp::BitNot,
                        expr: Box::new(acc),
                    }
                } else {
                    acc
                }
            }
            PrimitiveKind::Xor | PrimitiveKind::Xnor => {
                let mut acc = inputs[0].clone();
                for inp in &inputs[1..] {
                    acc = ElabExpr::Binary {
                        op: BinaryOp::BitXor,
                        lhs: Box::new(acc),
                        rhs: Box::new(inp.clone()),
                    };
                }
                if kind == PrimitiveKind::Xnor {
                    ElabExpr::Unary {
                        op: UnaryOp::BitNot,
                        expr: Box::new(acc),
                    }
                } else {
                    acc
                }
            }
            PrimitiveKind::Bufif1
            | PrimitiveKind::Bufif0
            | PrimitiveKind::Notif1
            | PrimitiveKind::Notif0 => {
                if inputs.len() < 2 {
                    return Err("tri-state primitive requires (out, in, enable)".into());
                }
                let data = if matches!(kind, PrimitiveKind::Notif0 | PrimitiveKind::Notif1) {
                    ElabExpr::Unary {
                        op: UnaryOp::BitNot,
                        expr: Box::new(inputs[0].clone()),
                    }
                } else {
                    inputs[0].clone()
                };
                let cond = if matches!(kind, PrimitiveKind::Bufif0 | PrimitiveKind::Notif0) {
                    ElabExpr::Unary {
                        op: UnaryOp::LogNot,
                        expr: Box::new(inputs[1].clone()),
                    }
                } else {
                    inputs[1].clone()
                };
                ElabExpr::Ternary {
                    cond: Box::new(cond),
                    then_expr: Box::new(data),
                    else_expr: Box::new(ElabExpr::UnbasedUnsized(Logic::HighZ)),
                }
            }
        };
        self.push_assign(lhs, rhs, delay_ticks);
        Ok(())
    }

    pub(super) fn elab_local_decls(
        &mut self,
        scope_idx: usize,
        decls: &[ModuleItem],
        locals: &mut HashMap<String, SigId>,
    ) -> Result<(), String> {
        for d in decls {
            if let ModuleItem::VarDecl(vd) = d {
                let (msb, lsb) = if let Some(r) = &vd.range {
                    eval_range(r, &self.scopes[scope_idx].params)?
                } else {
                    (0, 0)
                };
                let w = ((msb - lsb).unsigned_abs() as usize + 1).max(1);
                for v in &vd.items {
                    let s = self.alloc_signal(
                        format!("{}.__local_{}", self.scopes[scope_idx].path, v.name),
                        msb,
                        lsb,
                        vd.signed,
                        false,
                        BitVec::filled(w, Logic::Unknown),
                        false,
                    );
                    locals.insert(v.name.clone(), s);
                }
            }
        }
        Ok(())
    }

    pub(super) fn eval_delay(
        &self,
        scope_idx: usize,
        delay: Option<&Expr>,
        unit_ticks: u64,
    ) -> Result<u64, String> {
        let Some(expr) = delay else {
            return Ok(0);
        };
        match expr {
            Expr::Real(r) => Ok((*r * unit_ticks as f64).round().max(0.0) as u64),
            other => {
                let v = eval_const_expr(other, &self.scopes[scope_idx].params)?;
                let n = v.to_u64().unwrap_or(0);
                Ok(n.saturating_mul(unit_ticks))
            }
        }
    }

    pub(super) fn elab_event(
        &self,
        scope_idx: usize,
        ev: &EventExpr,
        locals: &HashMap<String, SigId>,
        out: &mut Vec<ElabEvent>,
    ) -> Result<(), String> {
        match ev {
            EventExpr::Star => {}
            EventExpr::Posedge(e) => {
                out.push(ElabEvent::Posedge(self.elab_expr(scope_idx, e, locals)?));
            }
            EventExpr::Negedge(e) => {
                out.push(ElabEvent::Negedge(self.elab_expr(scope_idx, e, locals)?));
            }
            EventExpr::Level(e) => {
                let ee = self.elab_expr(scope_idx, e, locals)?;
                let (sigs, _) = collect_expr_deps(&ee);
                for s in sigs {
                    out.push(ElabEvent::Level(s));
                }
            }
        }
        Ok(())
    }
}
