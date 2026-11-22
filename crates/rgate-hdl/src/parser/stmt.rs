use super::expr::lvalue_to_expr;
use super::Parser;
use crate::ast::*;
use crate::lexer::TokenKind;

impl Parser {
    pub(super) fn parse_function(&mut self) -> Result<FunctionDecl, String> {
        let span = self.span();
        self.expect(&TokenKind::Function, "`function`")?;
        let _ = self.eat(&TokenKind::Automatic);
        let mut signed = false;
        let mut return_range = None;
        self.parse_optional_param_type(&mut signed, &mut return_range)?;
        let name = self.expect_ident()?;
        let mut inputs = Vec::new();
        if self.eat(&TokenKind::LParen) {
            while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
                let ispan = self.span();
                let dir = self.try_parse_port_dir().or(Some(PortDir::Input));
                let kind = self.try_parse_var_kind().unwrap_or(VarKind::Logic);
                let mut isigned = kind == VarKind::Integer;
                if self.eat(&TokenKind::Signed) {
                    isigned = true;
                }
                let irange = if matches!(self.peek_kind(), TokenKind::LBracket) {
                    Some(self.parse_range()?)
                } else if kind == VarKind::Integer {
                    Some(Range {
                        msb: Expr::Int(31),
                        lsb: Expr::Int(0),
                    })
                } else {
                    None
                };
                let pname = self.expect_ident()?;
                inputs.push(VarDecl {
                    dir,
                    kind,
                    signed: isigned,
                    range: irange,
                    items: vec![VarDeclItem {
                        name: pname,
                        array_dims: Vec::new(),
                        init: None,
                        span: ispan.clone(),
                    }],
                    span: ispan,
                });
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
            self.expect(&TokenKind::RParen, "`)`")?;
        }
        self.expect(&TokenKind::Semi, "`;`")?;
        let mut decls = Vec::new();
        let mut body = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::EndFunction | TokenKind::Eof) {
            if matches!(
                self.peek_kind(),
                TokenKind::Input
                    | TokenKind::Output
                    | TokenKind::Inout
                    | TokenKind::Reg
                    | TokenKind::Logic
                    | TokenKind::Integer
                    | TokenKind::Int
                    | TokenKind::Parameter
                    | TokenKind::Localparam
            ) {
                if matches!(self.peek_kind(), TokenKind::Input) {
                    inputs.push(self.parse_var_decl()?);
                } else {
                    self.parse_module_items_into(&mut decls)?;
                }
            } else {
                body.push(self.parse_stmt()?);
            }
        }
        self.expect(&TokenKind::EndFunction, "`endfunction`")?;
        Ok(FunctionDecl {
            name,
            signed,
            return_range,
            inputs,
            decls,
            body,
            span,
        })
    }

    pub(super) fn parse_task(&mut self) -> Result<TaskDecl, String> {
        let span = self.span();
        self.expect(&TokenKind::Task, "`task`")?;
        let _ = self.eat(&TokenKind::Automatic);
        let name = self.expect_ident()?;
        let mut inputs = Vec::new();
        if self.eat(&TokenKind::LParen) {
            while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
                let ispan = self.span();
                let dir = self.try_parse_port_dir().or(Some(PortDir::Input));
                let kind = self.try_parse_var_kind().unwrap_or(VarKind::Logic);
                let mut isigned = kind == VarKind::Integer;
                if self.eat(&TokenKind::Signed) {
                    isigned = true;
                }
                let irange = if matches!(self.peek_kind(), TokenKind::LBracket) {
                    Some(self.parse_range()?)
                } else {
                    None
                };
                let pname = self.expect_ident()?;
                inputs.push(VarDecl {
                    dir,
                    kind,
                    signed: isigned,
                    range: irange,
                    items: vec![VarDeclItem {
                        name: pname,
                        array_dims: Vec::new(),
                        init: None,
                        span: ispan.clone(),
                    }],
                    span: ispan,
                });
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
            self.expect(&TokenKind::RParen, "`)`")?;
        }
        self.expect(&TokenKind::Semi, "`;`")?;
        let mut decls = Vec::new();
        let mut body = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::EndTask | TokenKind::Eof) {
            if matches!(
                self.peek_kind(),
                TokenKind::Input
                    | TokenKind::Output
                    | TokenKind::Inout
                    | TokenKind::Reg
                    | TokenKind::Logic
                    | TokenKind::Integer
                    | TokenKind::Int
                    | TokenKind::Parameter
                    | TokenKind::Localparam
            ) {
                if matches!(self.peek_kind(), TokenKind::Input) {
                    inputs.push(self.parse_var_decl()?);
                } else {
                    self.parse_module_items_into(&mut decls)?;
                }
            } else {
                body.push(self.parse_stmt()?);
            }
        }
        self.expect(&TokenKind::EndTask, "`endtask`")?;
        Ok(TaskDecl {
            name,
            inputs,
            decls,
            body,
            span,
        })
    }

    pub(super) fn try_parse_delay(&mut self) -> Result<Option<Expr>, String> {
        if !self.eat(&TokenKind::Hash) {
            return Ok(None);
        }
        if self.eat(&TokenKind::LParen) {
            let e = self.parse_expr()?;
            self.expect(&TokenKind::RParen, "`)`")?;
            Ok(Some(e))
        } else {
            let e = match self.peek_kind().clone() {
                TokenKind::IntLiteral(n) => {
                    self.advance();
                    Expr::Int(n)
                }
                TokenKind::RealLiteral(r) => {
                    self.advance();
                    Expr::Real(r)
                }
                TokenKind::Ident(name) => {
                    self.advance();
                    Expr::Ident(vec![name])
                }
                _ => return Err(self.err("expected delay value after `#`")),
            };
            Ok(Some(e))
        }
    }

    fn parse_event_control(&mut self) -> Result<Vec<EventExpr>, String> {
        self.expect(&TokenKind::At, "`@`")?;
        if self.eat(&TokenKind::Star) {
            return Ok(vec![EventExpr::Star]);
        }
        if self.eat(&TokenKind::LParen) {
            if self.eat(&TokenKind::Star) {
                self.expect(&TokenKind::RParen, "`)`")?;
                return Ok(vec![EventExpr::Star]);
            }
            let mut events = Vec::new();
            loop {
                if self.eat(&TokenKind::Posedge) {
                    events.push(EventExpr::Posedge(self.parse_expr()?));
                } else if self.eat(&TokenKind::Negedge) {
                    events.push(EventExpr::Negedge(self.parse_expr()?));
                } else {
                    events.push(EventExpr::Level(self.parse_expr()?));
                }
                if !self.eat(&TokenKind::Or) && !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
            self.expect(&TokenKind::RParen, "`)`")?;
            Ok(events)
        } else {
            let name = self.expect_ident()?;
            Ok(vec![EventExpr::Level(Expr::Ident(vec![name]))])
        }
    }

    pub(super) fn parse_stmt(&mut self) -> Result<Stmt, String> {
        match self.peek_kind().clone() {
            TokenKind::Semi => {
                self.advance();
                Ok(Stmt::Empty)
            }
            TokenKind::Begin => {
                self.advance();
                let name = if self.eat(&TokenKind::Colon) {
                    Some(self.expect_ident()?)
                } else {
                    None
                };
                let mut decls = Vec::new();
                let mut stmts = Vec::new();
                while !matches!(self.peek_kind(), TokenKind::End | TokenKind::Eof) {
                    if matches!(
                        self.peek_kind(),
                        TokenKind::Reg
                            | TokenKind::Logic
                            | TokenKind::Integer
                            | TokenKind::Int
                            | TokenKind::Parameter
                            | TokenKind::Localparam
                    ) {
                        self.parse_module_items_into(&mut decls)?;
                    } else {
                        stmts.push(self.parse_stmt()?);
                    }
                }
                self.expect(&TokenKind::End, "`end`")?;
                if self.eat(&TokenKind::Colon) {
                    let _ = self.expect_ident()?;
                }
                Ok(Stmt::Block { name, decls, stmts })
            }
            TokenKind::If => {
                self.advance();
                self.expect(&TokenKind::LParen, "`(`")?;
                let cond = self.parse_expr()?;
                self.expect(&TokenKind::RParen, "`)`")?;
                let then_branch = Box::new(self.parse_stmt()?);
                let else_branch = if self.eat(&TokenKind::Else) {
                    Some(Box::new(self.parse_stmt()?))
                } else {
                    None
                };
                Ok(Stmt::If {
                    cond,
                    then_branch,
                    else_branch,
                })
            }
            TokenKind::Case | TokenKind::Casex | TokenKind::Casez => {
                let kind = match self.advance().kind {
                    TokenKind::Casex => CaseKind::Casex,
                    TokenKind::Casez => CaseKind::Casez,
                    _ => CaseKind::Case,
                };
                self.expect(&TokenKind::LParen, "`(`")?;
                let expr = self.parse_expr()?;
                self.expect(&TokenKind::RParen, "`)`")?;
                let mut items = Vec::new();
                let mut default = None;
                while !matches!(self.peek_kind(), TokenKind::EndCase | TokenKind::Eof) {
                    if self.eat(&TokenKind::Default) {
                        let _ = self.eat(&TokenKind::Colon);
                        default = Some(Box::new(self.parse_stmt()?));
                    } else {
                        let mut patterns = Vec::new();
                        loop {
                            patterns.push(self.parse_expr()?);
                            if !self.eat(&TokenKind::Comma) {
                                break;
                            }
                        }
                        self.expect(&TokenKind::Colon, "`:`")?;
                        let stmt = self.parse_stmt()?;
                        items.push((patterns, stmt));
                    }
                }
                self.expect(&TokenKind::EndCase, "`endcase`")?;
                Ok(Stmt::Case {
                    kind,
                    expr,
                    items,
                    default,
                })
            }
            TokenKind::For => {
                self.advance();
                self.expect(&TokenKind::LParen, "`(`")?;
                // Allow `for (int i = 0; ...)`
                let _ = self.eat(&TokenKind::Int) || self.eat(&TokenKind::Integer);
                let init = if matches!(self.peek_kind(), TokenKind::Semi) {
                    None
                } else {
                    Some(Box::new(self.parse_simple_assign()?))
                };
                self.expect(&TokenKind::Semi, "`;`")?;
                let cond = if matches!(self.peek_kind(), TokenKind::Semi) {
                    None
                } else {
                    Some(self.parse_expr()?)
                };
                self.expect(&TokenKind::Semi, "`;`")?;
                let step = if matches!(self.peek_kind(), TokenKind::RParen) {
                    None
                } else {
                    Some(Box::new(self.parse_simple_assign()?))
                };
                self.expect(&TokenKind::RParen, "`)`")?;
                let body = Box::new(self.parse_stmt()?);
                Ok(Stmt::For {
                    init,
                    cond,
                    step,
                    body,
                })
            }
            TokenKind::While => {
                self.advance();
                self.expect(&TokenKind::LParen, "`(`")?;
                let cond = self.parse_expr()?;
                self.expect(&TokenKind::RParen, "`)`")?;
                let body = Box::new(self.parse_stmt()?);
                Ok(Stmt::While { cond, body })
            }
            TokenKind::Repeat => {
                self.advance();
                self.expect(&TokenKind::LParen, "`(`")?;
                let count = self.parse_expr()?;
                self.expect(&TokenKind::RParen, "`)`")?;
                let body = Box::new(self.parse_stmt()?);
                Ok(Stmt::Repeat { count, body })
            }
            TokenKind::Forever => {
                self.advance();
                let body = Box::new(self.parse_stmt()?);
                Ok(Stmt::Forever(body))
            }
            TokenKind::Hash => {
                let delay = self.try_parse_delay()?.unwrap();
                let body = Box::new(self.parse_stmt()?);
                Ok(Stmt::DelayControl { delay, body })
            }
            TokenKind::At => {
                let events = self.parse_event_control()?;
                let body = Box::new(self.parse_stmt()?);
                Ok(Stmt::EventControl { events, body })
            }
            TokenKind::Force => {
                self.advance();
                let lhs = self.parse_lvalue()?;
                self.expect(&TokenKind::Eq, "`=`")?;
                let rhs = self.parse_expr()?;
                self.expect(&TokenKind::Semi, "`;`")?;
                Ok(Stmt::Force { lhs, rhs })
            }
            TokenKind::Release => {
                self.advance();
                let lhs = self.parse_lvalue()?;
                self.expect(&TokenKind::Semi, "`;`")?;
                Ok(Stmt::Release { lhs })
            }
            TokenKind::Return => {
                self.advance();
                let val = if matches!(self.peek_kind(), TokenKind::Semi) {
                    None
                } else {
                    Some(self.parse_expr()?)
                };
                self.expect(&TokenKind::Semi, "`;`")?;
                Ok(Stmt::Return(val))
            }
            TokenKind::SysIdent(name) => {
                self.advance();
                let args = if self.eat(&TokenKind::LParen) {
                    let mut a = Vec::new();
                    while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
                        a.push(self.parse_expr()?);
                        if !self.eat(&TokenKind::Comma) {
                            break;
                        }
                    }
                    self.expect(&TokenKind::RParen, "`)`")?;
                    a
                } else {
                    Vec::new()
                };
                self.expect(&TokenKind::Semi, "`;`")?;
                Ok(Stmt::SysTaskCall { name, args })
            }
            TokenKind::Ident(_) | TokenKind::LBrace => {
                // Check for task call `ident;` or `ident(args);`
                if let TokenKind::Ident(name) = self.peek_kind().clone()
                    && matches!(self.peek_at(1), TokenKind::Semi | TokenKind::LParen)
                {
                    self.advance();
                    let args = if self.eat(&TokenKind::LParen) {
                        let mut a = Vec::new();
                        while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
                            a.push(self.parse_expr()?);
                            if !self.eat(&TokenKind::Comma) {
                                break;
                            }
                        }
                        self.expect(&TokenKind::RParen, "`)`")?;
                        a
                    } else {
                        Vec::new()
                    };
                    self.expect(&TokenKind::Semi, "`;`")?;
                    return Ok(Stmt::TaskCall { name, args });
                }
                let stmt = self.parse_simple_assign()?;
                self.expect(&TokenKind::Semi, "`;`")?;
                Ok(stmt)
            }
            other => Err(self.err(format!("unexpected token {other:?} in statement"))),
        }
    }

    fn parse_simple_assign(&mut self) -> Result<Stmt, String> {
        let lhs = self.parse_lvalue()?;
        if self.eat(&TokenKind::Eq) {
            let delay = self.try_parse_delay()?;
            let rhs = self.parse_expr()?;
            Ok(Stmt::Assign {
                blocking: true,
                lhs,
                delay,
                rhs,
            })
        } else if self.eat(&TokenKind::Le) {
            let delay = self.try_parse_delay()?;
            let rhs = self.parse_expr()?;
            Ok(Stmt::Assign {
                blocking: false,
                lhs,
                delay,
                rhs,
            })
        } else if self.eat(&TokenKind::PlusPlus) {
            let expr_lhs = lvalue_to_expr(&lhs)?;
            Ok(Stmt::Assign {
                blocking: true,
                lhs,
                delay: None,
                rhs: Expr::Binary {
                    op: BinaryOp::Add,
                    lhs: Box::new(expr_lhs),
                    rhs: Box::new(Expr::Int(1)),
                },
            })
        } else if self.eat(&TokenKind::MinusMinus) {
            let expr_lhs = lvalue_to_expr(&lhs)?;
            Ok(Stmt::Assign {
                blocking: true,
                lhs,
                delay: None,
                rhs: Expr::Binary {
                    op: BinaryOp::Sub,
                    lhs: Box::new(expr_lhs),
                    rhs: Box::new(Expr::Int(1)),
                },
            })
        } else if self.eat(&TokenKind::PlusEq) {
            let rhs = self.parse_expr()?;
            let expr_lhs = lvalue_to_expr(&lhs)?;
            Ok(Stmt::Assign {
                blocking: true,
                lhs,
                delay: None,
                rhs: Expr::Binary {
                    op: BinaryOp::Add,
                    lhs: Box::new(expr_lhs),
                    rhs: Box::new(rhs),
                },
            })
        } else if self.eat(&TokenKind::MinusEq) {
            let rhs = self.parse_expr()?;
            let expr_lhs = lvalue_to_expr(&lhs)?;
            Ok(Stmt::Assign {
                blocking: true,
                lhs,
                delay: None,
                rhs: Expr::Binary {
                    op: BinaryOp::Sub,
                    lhs: Box::new(expr_lhs),
                    rhs: Box::new(rhs),
                },
            })
        } else {
            Err(self.err(format!(
                "expected `=` or `<=` in assignment, found {:?}",
                self.peek_kind()
            )))
        }
    }
}
