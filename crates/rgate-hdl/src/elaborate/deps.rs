use super::ir::{ElabExpr, ElabLValue, ElabStmt, MemId, SigId};
use std::collections::BTreeSet;

pub fn collect_expr_deps(expr: &ElabExpr) -> (Vec<SigId>, Vec<MemId>) {
    let mut sigs = BTreeSet::new();
    let mut mems = BTreeSet::new();
    walk_expr_deps(expr, &mut sigs, &mut mems);
    (sigs.into_iter().collect(), mems.into_iter().collect())
}

fn walk_expr_deps(expr: &ElabExpr, sigs: &mut BTreeSet<SigId>, mems: &mut BTreeSet<MemId>) {
    match expr {
        ElabExpr::Signal(s) => {
            sigs.insert(*s);
        }
        ElabExpr::MemRead { mem, index } => {
            mems.insert(*mem);
            walk_expr_deps(index, sigs, mems);
        }
        ElabExpr::BitSelect { target, index, .. } => {
            walk_expr_deps(target, sigs, mems);
            walk_expr_deps(index, sigs, mems);
        }
        ElabExpr::PartSelect { target, .. } => {
            walk_expr_deps(target, sigs, mems);
        }
        ElabExpr::IndexedPart { target, base, .. } => {
            walk_expr_deps(target, sigs, mems);
            walk_expr_deps(base, sigs, mems);
        }
        ElabExpr::Concat(parts) => {
            for p in parts {
                walk_expr_deps(p, sigs, mems);
            }
        }
        ElabExpr::Replicate { expr, .. } | ElabExpr::Unary { expr, .. } => {
            walk_expr_deps(expr, sigs, mems);
        }
        ElabExpr::Binary { lhs, rhs, .. } => {
            walk_expr_deps(lhs, sigs, mems);
            walk_expr_deps(rhs, sigs, mems);
        }
        ElabExpr::Ternary {
            cond,
            then_expr,
            else_expr,
        } => {
            walk_expr_deps(cond, sigs, mems);
            walk_expr_deps(then_expr, sigs, mems);
            walk_expr_deps(else_expr, sigs, mems);
        }
        ElabExpr::SysCall { args, .. } | ElabExpr::FuncCall { args, .. } => {
            for a in args {
                walk_expr_deps(a, sigs, mems);
            }
        }
        ElabExpr::Const(_)
        | ElabExpr::UnbasedUnsized(_)
        | ElabExpr::Real(_)
        | ElabExpr::String(_) => {}
    }
}

pub(super) fn collect_stmt_reads(stmt: &ElabStmt) -> (Vec<SigId>, Vec<MemId>) {
    let mut sigs = BTreeSet::new();
    let mut mems = BTreeSet::new();
    walk_stmt_reads(stmt, &mut sigs, &mut mems);
    (sigs.into_iter().collect(), mems.into_iter().collect())
}

fn walk_lvalue_index_reads(
    lv: &ElabLValue,
    sigs: &mut BTreeSet<SigId>,
    mems: &mut BTreeSet<MemId>,
) {
    match lv {
        ElabLValue::MemElem { index, .. } | ElabLValue::BitSelect { index, .. } => {
            walk_expr_deps(index, sigs, mems);
        }
        ElabLValue::IndexedPart { base, .. } => walk_expr_deps(base, sigs, mems),
        ElabLValue::Concat(parts) => {
            for p in parts {
                walk_lvalue_index_reads(p, sigs, mems);
            }
        }
        _ => {}
    }
}

fn walk_stmt_reads(stmt: &ElabStmt, sigs: &mut BTreeSet<SigId>, mems: &mut BTreeSet<MemId>) {
    match stmt {
        ElabStmt::Empty | ElabStmt::Release { .. } => {}
        ElabStmt::Block(stmts) => {
            for s in stmts {
                walk_stmt_reads(s, sigs, mems);
            }
        }
        ElabStmt::Assign { lhs, rhs, .. } | ElabStmt::Force { lhs, rhs } => {
            walk_lvalue_index_reads(lhs, sigs, mems);
            walk_expr_deps(rhs, sigs, mems);
        }
        ElabStmt::If {
            cond,
            then_branch,
            else_branch,
        } => {
            walk_expr_deps(cond, sigs, mems);
            walk_stmt_reads(then_branch, sigs, mems);
            if let Some(e) = else_branch {
                walk_stmt_reads(e, sigs, mems);
            }
        }
        ElabStmt::Case {
            expr,
            items,
            default,
            ..
        } => {
            walk_expr_deps(expr, sigs, mems);
            for (pats, body) in items {
                for p in pats {
                    walk_expr_deps(p, sigs, mems);
                }
                walk_stmt_reads(body, sigs, mems);
            }
            if let Some(d) = default {
                walk_stmt_reads(d, sigs, mems);
            }
        }
        ElabStmt::For {
            init,
            cond,
            step,
            body,
        } => {
            if let Some(i) = init {
                walk_stmt_reads(i, sigs, mems);
            }
            if let Some(c) = cond {
                walk_expr_deps(c, sigs, mems);
            }
            if let Some(s) = step {
                walk_stmt_reads(s, sigs, mems);
            }
            walk_stmt_reads(body, sigs, mems);
        }
        ElabStmt::While { cond, body } => {
            walk_expr_deps(cond, sigs, mems);
            walk_stmt_reads(body, sigs, mems);
        }
        ElabStmt::Repeat { count, body } => {
            walk_expr_deps(count, sigs, mems);
            walk_stmt_reads(body, sigs, mems);
        }
        ElabStmt::Forever(b)
        | ElabStmt::DelayControl { body: b, .. }
        | ElabStmt::EventControl { body: b, .. } => {
            walk_stmt_reads(b, sigs, mems);
        }
        ElabStmt::TaskCall { args, .. } | ElabStmt::SysTaskCall { args, .. } => {
            for a in args {
                walk_expr_deps(a, sigs, mems);
            }
        }
        ElabStmt::Return(Some(e)) => walk_expr_deps(e, sigs, mems),
        ElabStmt::Return(None) => {}
    }
}
