use super::cursor::TokenCursor;
use crate::ast::{BinaryOp, Expr, LValue, Range, UnaryOp};
use crate::lexer::TokenKind;

/// Standalone Pratt expression, lvalue, and range parser operating over a `TokenCursor`.
pub(super) struct ExprParser<'a> {
    cursor: &'a mut TokenCursor,
}

impl<'a> ExprParser<'a> {
    pub(super) fn new(cursor: &'a mut TokenCursor) -> Self {
        Self { cursor }
    }

    pub(super) fn parse_range(&mut self) -> Result<Range, String> {
        self.cursor.expect(&TokenKind::LBracket, "`[`")?;
        let msb = self.parse_expr()?;
        self.cursor.expect(&TokenKind::Colon, "`:`")?;
        let lsb = self.parse_expr()?;
        self.cursor.expect(&TokenKind::RBracket, "`]`")?;
        Ok(Range { msb, lsb })
    }

    pub(super) fn parse_lvalue(&mut self) -> Result<LValue, String> {
        if self.cursor.eat(&TokenKind::LBrace) {
            let mut parts = Vec::new();
            loop {
                parts.push(self.parse_lvalue()?);
                if !self.cursor.eat(&TokenKind::Comma) {
                    break;
                }
            }
            self.cursor.expect(&TokenKind::RBrace, "`}`")?;
            return Ok(LValue::Concat(parts));
        }
        let mut path = vec![self.cursor.expect_ident()?];
        while self.cursor.eat(&TokenKind::Dot) {
            path.push(self.cursor.expect_ident()?);
        }
        let mut lv = LValue::Ident(path);
        while self.cursor.eat(&TokenKind::LBracket) {
            let first = self.parse_expr()?;
            if self.cursor.eat(&TokenKind::Colon) {
                let lsb = self.parse_expr()?;
                self.cursor.expect(&TokenKind::RBracket, "`]`")?;
                lv = LValue::Range {
                    target: Box::new(lv),
                    msb: first,
                    lsb,
                };
            } else if self.cursor.eat(&TokenKind::PlusColon) {
                let width = self.parse_expr()?;
                self.cursor.expect(&TokenKind::RBracket, "`]`")?;
                lv = LValue::IndexedPart {
                    target: Box::new(lv),
                    base: first,
                    width,
                    ascending: true,
                };
            } else if self.cursor.eat(&TokenKind::MinusColon) {
                let width = self.parse_expr()?;
                self.cursor.expect(&TokenKind::RBracket, "`]`")?;
                lv = LValue::IndexedPart {
                    target: Box::new(lv),
                    base: first,
                    width,
                    ascending: false,
                };
            } else {
                self.cursor.expect(&TokenKind::RBracket, "`]`")?;
                lv = LValue::Index {
                    target: Box::new(lv),
                    index: first,
                };
            }
        }
        Ok(lv)
    }

    pub(super) fn parse_expr(&mut self) -> Result<Expr, String> {
        self.parse_ternary()
    }

    fn parse_ternary(&mut self) -> Result<Expr, String> {
        let cond = self.parse_binary(1)?;
        if self.cursor.eat(&TokenKind::Question) {
            let then_expr = self.parse_expr()?;
            self.cursor.expect(&TokenKind::Colon, "`:`")?;
            let else_expr = self.parse_ternary()?;
            Ok(Expr::Ternary {
                cond: Box::new(cond),
                then_expr: Box::new(then_expr),
                else_expr: Box::new(else_expr),
            })
        } else {
            Ok(cond)
        }
    }

    fn parse_binary(&mut self, min_prec: u8) -> Result<Expr, String> {
        let mut lhs = self.parse_unary()?;
        while let Some((op, prec)) = binary_op_prec(self.cursor.peek_kind()) {
            if prec < min_prec {
                break;
            }
            self.cursor.advance();
            let rhs = self.parse_binary(prec + 1)?;
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        let op = match self.cursor.peek_kind() {
            TokenKind::Plus => Some(UnaryOp::Plus),
            TokenKind::Minus => Some(UnaryOp::Minus),
            TokenKind::Tilde => Some(UnaryOp::BitNot),
            TokenKind::Bang => Some(UnaryOp::LogNot),
            TokenKind::Amp => Some(UnaryOp::RedAnd),
            TokenKind::TildeAmp => Some(UnaryOp::RedNand),
            TokenKind::Pipe => Some(UnaryOp::RedOr),
            TokenKind::TildePipe => Some(UnaryOp::RedNor),
            TokenKind::Caret => Some(UnaryOp::RedXor),
            TokenKind::TildeCaret => Some(UnaryOp::RedXnor),
            _ => None,
        };
        if let Some(op) = op {
            self.cursor.advance();
            let expr = self.parse_unary()?;
            Ok(Expr::Unary {
                op,
                expr: Box::new(expr),
            })
        } else {
            self.parse_postfix()
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_primary()?;
        while self.cursor.eat(&TokenKind::LBracket) {
            let first = self.parse_expr()?;
            if self.cursor.eat(&TokenKind::Colon) {
                let lsb = self.parse_expr()?;
                self.cursor.expect(&TokenKind::RBracket, "`]`")?;
                expr = Expr::Range {
                    target: Box::new(expr),
                    msb: Box::new(first),
                    lsb: Box::new(lsb),
                };
            } else if self.cursor.eat(&TokenKind::PlusColon) {
                let width = self.parse_expr()?;
                self.cursor.expect(&TokenKind::RBracket, "`]`")?;
                expr = Expr::IndexedPart {
                    target: Box::new(expr),
                    base: Box::new(first),
                    width: Box::new(width),
                    ascending: true,
                };
            } else if self.cursor.eat(&TokenKind::MinusColon) {
                let width = self.parse_expr()?;
                self.cursor.expect(&TokenKind::RBracket, "`]`")?;
                expr = Expr::IndexedPart {
                    target: Box::new(expr),
                    base: Box::new(first),
                    width: Box::new(width),
                    ascending: false,
                };
            } else {
                self.cursor.expect(&TokenKind::RBracket, "`]`")?;
                expr = Expr::Index {
                    target: Box::new(expr),
                    index: Box::new(first),
                };
            }
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        match self.cursor.peek_kind().clone() {
            TokenKind::IntLiteral(n) => {
                self.cursor.advance();
                Ok(Expr::Int(n))
            }
            TokenKind::BasedLiteral { value, sized } => {
                self.cursor.advance();
                Ok(Expr::Based { value, sized })
            }
            TokenKind::UnbasedUnsized(bit) => {
                self.cursor.advance();
                Ok(Expr::UnbasedUnsized(bit))
            }
            TokenKind::RealLiteral(r) => {
                self.cursor.advance();
                Ok(Expr::Real(r))
            }
            TokenKind::StringLiteral(s) => {
                self.cursor.advance();
                Ok(Expr::String(s))
            }
            TokenKind::LParen => {
                self.cursor.advance();
                let e = self.parse_expr()?;
                self.cursor.expect(&TokenKind::RParen, "`)`")?;
                Ok(e)
            }
            TokenKind::LBrace => {
                self.cursor.advance();
                let first = self.parse_expr()?;
                if self.cursor.eat(&TokenKind::LBrace) {
                    // Replication `{count{a, b, ...}}`
                    let mut inner = Vec::new();
                    loop {
                        inner.push(self.parse_expr()?);
                        if !self.cursor.eat(&TokenKind::Comma) {
                            break;
                        }
                    }
                    self.cursor.expect(&TokenKind::RBrace, "`}`")?;
                    self.cursor.expect(&TokenKind::RBrace, "`}`")?;
                    let body = if inner.len() == 1 {
                        inner.pop().unwrap()
                    } else {
                        Expr::Concat(inner)
                    };
                    Ok(Expr::Replicate {
                        count: Box::new(first),
                        expr: Box::new(body),
                    })
                } else {
                    let mut parts = vec![first];
                    while self.cursor.eat(&TokenKind::Comma) {
                        parts.push(self.parse_expr()?);
                    }
                    self.cursor.expect(&TokenKind::RBrace, "`}`")?;
                    Ok(Expr::Concat(parts))
                }
            }
            TokenKind::SysIdent(name) => {
                self.cursor.advance();
                let args = if self.cursor.eat(&TokenKind::LParen) {
                    let mut a = Vec::new();
                    while !matches!(self.cursor.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
                        a.push(self.parse_expr()?);
                        if !self.cursor.eat(&TokenKind::Comma) {
                            break;
                        }
                    }
                    self.cursor.expect(&TokenKind::RParen, "`)`")?;
                    a
                } else {
                    Vec::new()
                };
                Ok(Expr::SysCall { name, args })
            }
            TokenKind::Ident(first) => {
                self.cursor.advance();
                if self.cursor.eat(&TokenKind::LParen) {
                    let mut args = Vec::new();
                    while !matches!(self.cursor.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
                        args.push(self.parse_expr()?);
                        if !self.cursor.eat(&TokenKind::Comma) {
                            break;
                        }
                    }
                    self.cursor.expect(&TokenKind::RParen, "`)`")?;
                    return Ok(Expr::Call { name: first, args });
                }
                let mut path = vec![first];
                while self.cursor.eat(&TokenKind::Dot) {
                    path.push(self.cursor.expect_ident()?);
                }
                Ok(Expr::Ident(path))
            }
            other => Err(self
                .cursor
                .err(format!("expected expression, found {other:?}"))),
        }
    }
}

fn binary_op_prec(tok: &TokenKind) -> Option<(BinaryOp, u8)> {
    match tok {
        TokenKind::PipePipe => Some((BinaryOp::LogOr, 1)),
        TokenKind::AmpAmp => Some((BinaryOp::LogAnd, 2)),
        TokenKind::Pipe => Some((BinaryOp::BitOr, 3)),
        TokenKind::Caret => Some((BinaryOp::BitXor, 4)),
        TokenKind::TildeCaret => Some((BinaryOp::BitXnor, 4)),
        TokenKind::Amp => Some((BinaryOp::BitAnd, 5)),
        TokenKind::EqEq => Some((BinaryOp::Eq, 6)),
        TokenKind::BangEq => Some((BinaryOp::Ne, 6)),
        TokenKind::EqEqEq => Some((BinaryOp::CaseEq, 6)),
        TokenKind::BangEqEq => Some((BinaryOp::CaseNe, 6)),
        TokenKind::EqEqQ => Some((BinaryOp::WildEq, 6)),
        TokenKind::BangEqQ => Some((BinaryOp::WildNe, 6)),
        TokenKind::Lt => Some((BinaryOp::Lt, 7)),
        TokenKind::Le => Some((BinaryOp::Le, 7)),
        TokenKind::Gt => Some((BinaryOp::Gt, 7)),
        TokenKind::Ge => Some((BinaryOp::Ge, 7)),
        TokenKind::Shl => Some((BinaryOp::Shl, 8)),
        TokenKind::Shr => Some((BinaryOp::Shr, 8)),
        TokenKind::Ashl => Some((BinaryOp::Ashl, 8)),
        TokenKind::Ashr => Some((BinaryOp::Ashr, 8)),
        TokenKind::Plus => Some((BinaryOp::Add, 9)),
        TokenKind::Minus => Some((BinaryOp::Sub, 9)),
        TokenKind::Star => Some((BinaryOp::Mul, 10)),
        TokenKind::Slash => Some((BinaryOp::Div, 10)),
        TokenKind::Percent => Some((BinaryOp::Rem, 10)),
        TokenKind::StarStar => Some((BinaryOp::Pow, 11)),
        _ => None,
    }
}

pub(super) fn lvalue_to_expr(lv: &LValue) -> Result<Expr, String> {
    Ok(match lv {
        LValue::Ident(p) => Expr::Ident(p.clone()),
        LValue::Index { target, index } => Expr::Index {
            target: Box::new(lvalue_to_expr(target)?),
            index: Box::new(index.clone()),
        },
        LValue::Range { target, msb, lsb } => Expr::Range {
            target: Box::new(lvalue_to_expr(target)?),
            msb: Box::new(msb.clone()),
            lsb: Box::new(lsb.clone()),
        },
        LValue::IndexedPart {
            target,
            base,
            width,
            ascending,
        } => Expr::IndexedPart {
            target: Box::new(lvalue_to_expr(target)?),
            base: Box::new(base.clone()),
            width: Box::new(width.clone()),
            ascending: *ascending,
        },
        LValue::Concat(parts) => Expr::Concat(
            parts
                .iter()
                .map(lvalue_to_expr)
                .collect::<Result<Vec<_>, _>>()?,
        ),
    })
}
