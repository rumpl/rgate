use crate::ast::{BinaryOp, Expr, Range, UnaryOp};
use crate::value::BitVec;
use rgate_core::Logic;
use std::collections::HashMap;

pub fn clog2(n: u64) -> u64 {
    if n <= 1 {
        0
    } else {
        64 - (n - 1).leading_zeros() as u64
    }
}

pub(super) fn eval_range(r: &Range, params: &HashMap<String, BitVec>) -> Result<(i64, i64), String> {
    Ok((
        eval_const_i64(&r.msb, params)?,
        eval_const_i64(&r.lsb, params)?,
    ))
}

pub(super) fn eval_const_i64(
    expr: &Expr,
    params: &HashMap<String, BitVec>,
) -> Result<i64, String> {
    let v = eval_const_expr(expr, params)?;
    if v.signed {
        v.to_i64_signed()
            .ok_or_else(|| "constant expression has X/Z bits".into())
    } else {
        v.to_u64()
            .map(|u| u as i64)
            .ok_or_else(|| "constant expression has X/Z bits".into())
    }
}

pub fn eval_const_expr(expr: &Expr, params: &HashMap<String, BitVec>) -> Result<BitVec, String> {
    Ok(match expr {
        Expr::Int(n) => BitVec::from_i64(*n as i64, 32),
        Expr::Based { value, .. } => value.clone(),
        Expr::UnbasedUnsized(bit) => BitVec::filled(32, *bit),
        Expr::Real(r) => BitVec::from_i64(*r as i64, 32),
        Expr::String(s) => {
            let w = (s.len() * 8).max(8);
            let mut bv = BitVec::zeros(w);
            for (idx, b) in s.bytes().rev().enumerate() {
                for bit in 0..8 {
                    bv.set_bit(idx * 8 + bit, Logic::from_bool((b >> bit) & 1 != 0));
                }
            }
            bv
        }
        Expr::Ident(path) => {
            if path.len() == 1
                && let Some(v) = params.get(&path[0])
            {
                return Ok(v.clone());
            }
            return Err(format!("non-constant identifier `{}`", path.join(".")));
        }
        Expr::Concat(parts) => {
            let vals = parts
                .iter()
                .map(|p| eval_const_expr(p, params))
                .collect::<Result<Vec<_>, _>>()?;
            BitVec::concat(&vals)
        }
        Expr::Replicate { count, expr } => {
            let c = eval_const_i64(count, params)?.max(0) as usize;
            let v = eval_const_expr(expr, params)?;
            v.replicate(c)
        }
        Expr::Unary { op, expr } => {
            let v = eval_const_expr(expr, params)?;
            eval_unary(*op, &v)
        }
        Expr::Binary { op, lhs, rhs } => {
            let l = eval_const_expr(lhs, params)?;
            let r = eval_const_expr(rhs, params)?;
            eval_binary(*op, &l, &r, None)
        }
        Expr::Ternary {
            cond,
            then_expr,
            else_expr,
        } => {
            let c = eval_const_expr(cond, params)?;
            if c.is_true() {
                eval_const_expr(then_expr, params)?
            } else {
                eval_const_expr(else_expr, params)?
            }
        }
        Expr::SysCall { name, args } => match name.as_str() {
            "$clog2" if args.len() == 1 => {
                let v = eval_const_expr(&args[0], params)?;
                BitVec::from_i64(clog2(v.to_u64().unwrap_or(1)) as i64, 32)
            }
            "$signed" if args.len() == 1 => {
                let mut v = eval_const_expr(&args[0], params)?;
                v.signed = true;
                v
            }
            "$unsigned" if args.len() == 1 => {
                let mut v = eval_const_expr(&args[0], params)?;
                v.signed = false;
                v
            }
            _ => return Err(format!("unsupported constant system call `{name}`")),
        },
        _ => return Err("expression is not a compile-time constant".into()),
    })
}

pub fn eval_unary(op: UnaryOp, v: &BitVec) -> BitVec {
    match op {
        UnaryOp::Plus => v.clone(),
        UnaryOp::Minus => v.neg(),
        UnaryOp::BitNot => v.bit_not(),
        UnaryOp::LogNot => v.logical_not(),
        UnaryOp::RedAnd => v.reduce_and(),
        UnaryOp::RedNand => v.reduce_and().bit_not(),
        UnaryOp::RedOr => v.reduce_or(),
        UnaryOp::RedNor => v.reduce_or().bit_not(),
        UnaryOp::RedXor => v.reduce_xor(),
        UnaryOp::RedXnor => v.reduce_xor().bit_not(),
    }
}

pub fn eval_binary(op: BinaryOp, l: &BitVec, r: &BitVec, ctx_width: Option<usize>) -> BitVec {
    let signed = l.signed && r.signed;
    match op {
        BinaryOp::Add => {
            let w = l.width().max(r.width()).max(ctx_width.unwrap_or(0));
            l.add(r, w, signed)
        }
        BinaryOp::Sub => {
            let w = l.width().max(r.width()).max(ctx_width.unwrap_or(0));
            l.sub(r, w, signed)
        }
        BinaryOp::Mul => {
            let w = l.width().max(r.width()).max(ctx_width.unwrap_or(0));
            l.mul(r, w, signed)
        }
        BinaryOp::Div => {
            let w = l.width().max(r.width()).max(ctx_width.unwrap_or(0));
            l.div(r, w, signed)
        }
        BinaryOp::Rem => {
            let w = l.width().max(r.width()).max(ctx_width.unwrap_or(0));
            l.rem(r, w, signed)
        }
        BinaryOp::Pow => {
            let w = l.width().max(ctx_width.unwrap_or(0));
            l.pow(r, w, l.signed)
        }
        BinaryOp::BitAnd => {
            let w = l.width().max(r.width()).max(ctx_width.unwrap_or(0));
            l.resized(w, signed).bit_and(&r.resized(w, signed))
        }
        BinaryOp::BitOr => {
            let w = l.width().max(r.width()).max(ctx_width.unwrap_or(0));
            l.resized(w, signed).bit_or(&r.resized(w, signed))
        }
        BinaryOp::BitXor => {
            let w = l.width().max(r.width()).max(ctx_width.unwrap_or(0));
            l.resized(w, signed).bit_xor(&r.resized(w, signed))
        }
        BinaryOp::BitXnor => {
            let w = l.width().max(r.width()).max(ctx_width.unwrap_or(0));
            l.resized(w, signed).bit_xnor(&r.resized(w, signed))
        }
        BinaryOp::LogAnd => l.logical_and(r),
        BinaryOp::LogOr => l.logical_or(r),
        BinaryOp::Eq => l.logic_eq(r),
        BinaryOp::Ne => l.logic_eq(r).bit_not(),
        BinaryOp::CaseEq => l.case_eq(r),
        BinaryOp::CaseNe => l.case_eq(r).bit_not(),
        BinaryOp::WildEq => l.wild_eq(r),
        BinaryOp::WildNe => l.wild_eq(r).bit_not(),
        BinaryOp::Lt => match l.cmp_ordering(r) {
            Some(ord) => BitVec::from_bool(ord == std::cmp::Ordering::Less),
            None => BitVec::from_logic(Logic::Unknown),
        },
        BinaryOp::Le => match l.cmp_ordering(r) {
            Some(ord) => BitVec::from_bool(ord != std::cmp::Ordering::Greater),
            None => BitVec::from_logic(Logic::Unknown),
        },
        BinaryOp::Gt => match l.cmp_ordering(r) {
            Some(ord) => BitVec::from_bool(ord == std::cmp::Ordering::Greater),
            None => BitVec::from_logic(Logic::Unknown),
        },
        BinaryOp::Ge => match l.cmp_ordering(r) {
            Some(ord) => BitVec::from_bool(ord != std::cmp::Ordering::Less),
            None => BitVec::from_logic(Logic::Unknown),
        },
        BinaryOp::Shl | BinaryOp::Ashl => {
            let w = l.width().max(ctx_width.unwrap_or(0));
            l.resized(w, l.signed).shl(r)
        }
        BinaryOp::Shr => {
            let w = l.width().max(ctx_width.unwrap_or(0));
            l.resized(w, l.signed).shr(r, false)
        }
        BinaryOp::Ashr => {
            let w = l.width().max(ctx_width.unwrap_or(0));
            l.resized(w, l.signed).shr(r, true)
        }
    }
}
