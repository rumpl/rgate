use super::Parser;
use crate::ast::*;
use crate::lexer::TokenKind;

impl Parser {
    pub(super) fn parse_module(&mut self) -> Result<ModuleDecl, String> {
        let span = self.span();
        self.expect(&TokenKind::Module, "`module`")?;
        let name = self.expect_ident()?;

        // Optional `#(parameter ...)` list
        let mut params = Vec::new();
        if self.eat(&TokenKind::Hash) {
            self.expect(&TokenKind::LParen, "`(`")?;
            let mut sticky_signed = false;
            let mut sticky_range: Option<Range> = None;
            let mut sticky_local = false;
            while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
                let pspan = self.span();
                if self.eat(&TokenKind::Parameter) {
                    sticky_local = false;
                    sticky_signed = false;
                    sticky_range = None;
                } else if self.eat(&TokenKind::Localparam) {
                    sticky_local = true;
                    sticky_signed = false;
                    sticky_range = None;
                }
                self.parse_optional_param_type(&mut sticky_signed, &mut sticky_range)?;
                let pname = self.expect_ident()?;
                self.expect(&TokenKind::Eq, "`=`")?;
                let value = self.parse_expr()?;
                params.push(ParamDecl {
                    is_local: sticky_local,
                    signed: sticky_signed,
                    range: sticky_range.clone(),
                    name: pname,
                    value,
                    span: pspan,
                });
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
            self.expect(&TokenKind::RParen, "`)`")?;
        }

        // Optional port list `(...)`
        let mut ports = Vec::new();
        if self.eat(&TokenKind::LParen) {
            let mut sticky_dir: Option<PortDir> = None;
            let mut sticky_kind: Option<VarKind> = None;
            let mut sticky_signed = false;
            let mut sticky_range: Option<Range> = None;
            while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
                let pspan = self.span();
                let mut saw_header = false;
                if let Some(dir) = self.try_parse_port_dir() {
                    sticky_dir = Some(dir);
                    sticky_kind = None;
                    sticky_signed = false;
                    sticky_range = None;
                    saw_header = true;
                }
                if let Some(kind) = self.try_parse_var_kind() {
                    sticky_kind = Some(kind);
                    saw_header = true;
                }
                if self.eat(&TokenKind::Signed) {
                    sticky_signed = true;
                    saw_header = true;
                } else if self.eat(&TokenKind::Unsigned) {
                    sticky_signed = false;
                    saw_header = true;
                }
                if matches!(self.peek_kind(), TokenKind::LBracket) {
                    sticky_range = Some(self.parse_range()?);
                } else if saw_header && sticky_kind == Some(VarKind::Integer) {
                    sticky_signed = true;
                    sticky_range = Some(Range {
                        msb: Expr::Int(31),
                        lsb: Expr::Int(0),
                    });
                }
                let pname = self.expect_ident()?;
                let init = if self.eat(&TokenKind::Eq) {
                    Some(self.parse_expr()?)
                } else {
                    None
                };
                ports.push(PortDecl {
                    name: pname,
                    dir: sticky_dir,
                    kind: sticky_kind,
                    signed: sticky_signed,
                    range: sticky_range.clone(),
                    init,
                    span: pspan,
                });
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
            self.expect(&TokenKind::RParen, "`)`")?;
        }
        self.expect(&TokenKind::Semi, "`;`")?;

        let mut items = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::EndModule | TokenKind::Eof) {
            self.parse_module_items_into(&mut items)?;
        }
        self.expect(&TokenKind::EndModule, "`endmodule`")?;
        // Optional `: module_name`
        if self.eat(&TokenKind::Colon) {
            let _ = self.expect_ident()?;
        }

        Ok(ModuleDecl {
            name,
            params,
            ports,
            items,
            timescale: self.timescale,
            span,
        })
    }

    pub(super) fn parse_optional_param_type(
        &mut self,
        signed: &mut bool,
        range: &mut Option<Range>,
    ) -> Result<(), String> {
        loop {
            match self.peek_kind() {
                TokenKind::Integer | TokenKind::Int => {
                    self.advance();
                    *signed = true;
                    *range = Some(Range {
                        msb: Expr::Int(31),
                        lsb: Expr::Int(0),
                    });
                }
                TokenKind::Logic | TokenKind::Reg | TokenKind::Bit | TokenKind::Wire => {
                    self.advance();
                }
                TokenKind::Signed => {
                    self.advance();
                    *signed = true;
                }
                TokenKind::Unsigned => {
                    self.advance();
                    *signed = false;
                }
                TokenKind::LBracket => {
                    *range = Some(self.parse_range()?);
                }
                _ => break,
            }
        }
        Ok(())
    }

    pub(super) fn try_parse_port_dir(&mut self) -> Option<PortDir> {
        let dir = match self.peek_kind() {
            TokenKind::Input => PortDir::Input,
            TokenKind::Output => PortDir::Output,
            TokenKind::Inout => PortDir::Inout,
            _ => return None,
        };
        self.advance();
        Some(dir)
    }

    pub(super) fn try_parse_var_kind(&mut self) -> Option<VarKind> {
        let mut kind = None;
        loop {
            let next = match self.peek_kind() {
                TokenKind::Wire | TokenKind::Tri => VarKind::Wire,
                TokenKind::Reg => VarKind::Reg,
                TokenKind::Logic | TokenKind::Bit => VarKind::Logic,
                TokenKind::Integer | TokenKind::Int => VarKind::Integer,
                TokenKind::Supply0 => VarKind::Supply0,
                TokenKind::Supply1 => VarKind::Supply1,
                _ => break,
            };
            self.advance();
            // Handle `input wire logic` -> treat as Wire if wire appeared, else Logic
            kind = Some(match (kind, next) {
                (Some(VarKind::Wire), VarKind::Logic) => VarKind::Wire,
                _ => next,
            });
        }
        kind
    }

    pub(super) fn parse_range(&mut self) -> Result<Range, String> {
        self.expect(&TokenKind::LBracket, "`[`")?;
        let msb = self.parse_expr()?;
        self.expect(&TokenKind::Colon, "`:`")?;
        let lsb = self.parse_expr()?;
        self.expect(&TokenKind::RBracket, "`]`")?;
        Ok(Range { msb, lsb })
    }

    pub(super) fn parse_module_items_into(
        &mut self,
        items: &mut Vec<ModuleItem>,
    ) -> Result<(), String> {
        let span = self.span();
        match self.peek_kind().clone() {
            TokenKind::Semi => {
                self.advance();
            }
            TokenKind::Parameter | TokenKind::Localparam => {
                let is_local = matches!(self.advance().kind, TokenKind::Localparam);
                let mut signed = false;
                let mut range = None;
                self.parse_optional_param_type(&mut signed, &mut range)?;
                loop {
                    let pspan = self.span();
                    let name = self.expect_ident()?;
                    self.expect(&TokenKind::Eq, "`=`")?;
                    let value = self.parse_expr()?;
                    items.push(ModuleItem::ParamDecl(ParamDecl {
                        is_local,
                        signed,
                        range: range.clone(),
                        name,
                        value,
                        span: pspan,
                    }));
                    if !self.eat(&TokenKind::Comma) {
                        break;
                    }
                }
                self.expect(&TokenKind::Semi, "`;`")?;
            }
            TokenKind::Genvar => {
                self.advance();
                let mut names = Vec::new();
                loop {
                    names.push(self.expect_ident()?);
                    if !self.eat(&TokenKind::Comma) {
                        break;
                    }
                }
                self.expect(&TokenKind::Semi, "`;`")?;
                items.push(ModuleItem::GenvarDecl(names));
            }
            TokenKind::Input
            | TokenKind::Output
            | TokenKind::Inout
            | TokenKind::Wire
            | TokenKind::Tri
            | TokenKind::Reg
            | TokenKind::Logic
            | TokenKind::Bit
            | TokenKind::Integer
            | TokenKind::Int
            | TokenKind::Supply0
            | TokenKind::Supply1 => {
                let decl = self.parse_var_decl()?;
                items.push(ModuleItem::VarDecl(decl));
            }
            TokenKind::Assign => {
                self.advance();
                let delay = self.try_parse_delay()?;
                loop {
                    let aspan = self.span();
                    let lhs = self.parse_lvalue()?;
                    self.expect(&TokenKind::Eq, "`=`")?;
                    let rhs = self.parse_expr()?;
                    items.push(ModuleItem::Assign {
                        delay: delay.clone(),
                        lhs,
                        rhs,
                        span: aspan,
                    });
                    if !self.eat(&TokenKind::Comma) {
                        break;
                    }
                }
                self.expect(&TokenKind::Semi, "`;`")?;
            }
            TokenKind::Always
            | TokenKind::AlwaysComb
            | TokenKind::AlwaysFf
            | TokenKind::AlwaysLatch => {
                let kind = match self.advance().kind {
                    TokenKind::AlwaysComb => AlwaysKind::Comb,
                    TokenKind::AlwaysFf => AlwaysKind::Ff,
                    TokenKind::AlwaysLatch => AlwaysKind::Latch,
                    _ => AlwaysKind::Always,
                };
                let body = self.parse_stmt()?;
                items.push(ModuleItem::Always { kind, body, span });
            }
            TokenKind::Initial => {
                self.advance();
                let body = self.parse_stmt()?;
                items.push(ModuleItem::Initial { body, span });
            }
            TokenKind::Final => {
                self.advance();
                let body = self.parse_stmt()?;
                items.push(ModuleItem::Final { body, span });
            }
            TokenKind::Generate => {
                self.advance();
                while !matches!(self.peek_kind(), TokenKind::EndGenerate | TokenKind::Eof) {
                    self.parse_module_items_into(items)?;
                }
                self.expect(&TokenKind::EndGenerate, "`endgenerate`")?;
            }
            TokenKind::If => {
                items.push(self.parse_gen_if()?);
            }
            TokenKind::For => {
                items.push(self.parse_gen_for()?);
            }
            TokenKind::Function => {
                items.push(ModuleItem::Function(self.parse_function()?));
            }
            TokenKind::Task => {
                items.push(ModuleItem::Task(self.parse_task()?));
            }
            TokenKind::Typedef => {
                self.parse_typedef_into(items)?;
            }
            TokenKind::GateAnd
            | TokenKind::GateNand
            | TokenKind::Or
            | TokenKind::GateNor
            | TokenKind::GateXor
            | TokenKind::GateXnor
            | TokenKind::GateBuf
            | TokenKind::GateNot
            | TokenKind::GateBufif0
            | TokenKind::GateBufif1
            | TokenKind::GateNotif0
            | TokenKind::GateNotif1 => {
                items.push(self.parse_gate_primitive()?);
            }
            TokenKind::Ident(_) => {
                items.push(self.parse_instantiation()?);
            }
            other => {
                return Err(self.err(format!("unexpected token {other:?} in module")));
            }
        }
        Ok(())
    }

    pub(super) fn parse_var_decl(&mut self) -> Result<VarDecl, String> {
        let span = self.span();
        let dir = self.try_parse_port_dir();
        let kind = self.try_parse_var_kind().unwrap_or(VarKind::Wire);
        let mut signed = kind == VarKind::Integer;
        if self.eat(&TokenKind::Signed) {
            signed = true;
        } else if self.eat(&TokenKind::Unsigned) {
            signed = false;
        }
        let range = if matches!(self.peek_kind(), TokenKind::LBracket) {
            Some(self.parse_range()?)
        } else if kind == VarKind::Integer {
            Some(Range {
                msb: Expr::Int(31),
                lsb: Expr::Int(0),
            })
        } else {
            None
        };
        // Optional delay on net decl `wire #1 a = b;`
        let _ = self.try_parse_delay()?;

        let mut items = Vec::new();
        loop {
            let ispan = self.span();
            let name = self.expect_ident()?;
            let mut array_dims = Vec::new();
            while matches!(self.peek_kind(), TokenKind::LBracket) {
                array_dims.push(self.parse_range()?);
            }
            let init = if self.eat(&TokenKind::Eq) {
                Some(self.parse_expr()?)
            } else {
                None
            };
            items.push(VarDeclItem {
                name,
                array_dims,
                init,
                span: ispan,
            });
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::Semi, "`;`")?;
        Ok(VarDecl {
            dir,
            kind,
            signed,
            range,
            items,
            span,
        })
    }

    fn parse_typedef_into(&mut self, items: &mut Vec<ModuleItem>) -> Result<(), String> {
        self.expect(&TokenKind::Typedef, "`typedef`")?;
        if self.eat(&TokenKind::Enum) {
            let mut signed = false;
            let mut range = None;
            self.parse_optional_param_type(&mut signed, &mut range)?;
            self.expect(&TokenKind::LBrace, "`{`")?;
            let mut next_val: u64 = 0;
            while !matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                let espan = self.span();
                let name = self.expect_ident()?;
                let value = if self.eat(&TokenKind::Eq) {
                    self.parse_expr()?
                } else {
                    Expr::Int(next_val)
                };
                if let Expr::Int(v) = value {
                    next_val = v + 1;
                } else {
                    next_val += 1;
                }
                items.push(ModuleItem::ParamDecl(ParamDecl {
                    is_local: true,
                    signed,
                    range: range.clone(),
                    name,
                    value,
                    span: espan,
                }));
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
            self.expect(&TokenKind::RBrace, "`}`")?;
            let _type_name = self.expect_ident()?;
            self.expect(&TokenKind::Semi, "`;`")?;
        } else {
            while !matches!(self.peek_kind(), TokenKind::Semi | TokenKind::Eof) {
                self.advance();
            }
            self.eat(&TokenKind::Semi);
        }
        Ok(())
    }

    fn parse_gate_primitive(&mut self) -> Result<ModuleItem, String> {
        let span = self.span();
        let kind = match self.advance().kind {
            TokenKind::GateAnd => PrimitiveKind::And,
            TokenKind::GateNand => PrimitiveKind::Nand,
            TokenKind::Or => PrimitiveKind::Or,
            TokenKind::GateNor => PrimitiveKind::Nor,
            TokenKind::GateXor => PrimitiveKind::Xor,
            TokenKind::GateXnor => PrimitiveKind::Xnor,
            TokenKind::GateBuf => PrimitiveKind::Buf,
            TokenKind::GateNot => PrimitiveKind::Not,
            TokenKind::GateBufif0 => PrimitiveKind::Bufif0,
            TokenKind::GateBufif1 => PrimitiveKind::Bufif1,
            TokenKind::GateNotif0 => PrimitiveKind::Notif0,
            TokenKind::GateNotif1 => PrimitiveKind::Notif1,
            _ => unreachable!(),
        };
        let delay = self.try_parse_delay()?;
        let name = if matches!(self.peek_kind(), TokenKind::Ident(_)) {
            Some(self.expect_ident()?)
        } else {
            None
        };
        let range = if matches!(self.peek_kind(), TokenKind::LBracket) {
            Some(self.parse_range()?)
        } else {
            None
        };
        self.expect(&TokenKind::LParen, "`(`")?;
        let mut args = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
            args.push(self.parse_expr()?);
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::RParen, "`)`")?;
        self.expect(&TokenKind::Semi, "`;`")?;
        Ok(ModuleItem::GatePrimitive {
            kind,
            delay,
            name,
            range,
            args,
            span,
        })
    }

    fn parse_instantiation(&mut self) -> Result<ModuleItem, String> {
        let span = self.span();
        let module = self.expect_ident()?;
        let mut params = Vec::new();
        if self.eat(&TokenKind::Hash) {
            self.expect(&TokenKind::LParen, "`(`")?;
            while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
                if self.eat(&TokenKind::Dot) {
                    let pname = self.expect_ident()?;
                    self.expect(&TokenKind::LParen, "`(`")?;
                    let val = self.parse_expr()?;
                    self.expect(&TokenKind::RParen, "`)`")?;
                    params.push((Some(pname), val));
                } else {
                    params.push((None, self.parse_expr()?));
                }
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
            self.expect(&TokenKind::RParen, "`)`")?;
        }

        let mut instances = Vec::new();
        loop {
            let ispan = self.span();
            let name = self.expect_ident()?;
            let range = if matches!(self.peek_kind(), TokenKind::LBracket) {
                Some(self.parse_range()?)
            } else {
                None
            };
            self.expect(&TokenKind::LParen, "`(`")?;
            let mut connections = Vec::new();
            while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
                if self.eat(&TokenKind::Dot) {
                    let pname = self.expect_ident()?;
                    let expr = if self.eat(&TokenKind::LParen) {
                        let e = if matches!(self.peek_kind(), TokenKind::RParen) {
                            None
                        } else {
                            Some(self.parse_expr()?)
                        };
                        self.expect(&TokenKind::RParen, "`)`")?;
                        e
                    } else {
                        // SystemVerilog `.name` implicit connection
                        Some(Expr::Ident(vec![pname.clone()]))
                    };
                    connections.push((Some(pname), expr));
                } else {
                    let e = if matches!(self.peek_kind(), TokenKind::Comma | TokenKind::RParen) {
                        None
                    } else {
                        Some(self.parse_expr()?)
                    };
                    connections.push((None, e));
                }
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
            self.expect(&TokenKind::RParen, "`)`")?;
            instances.push(InstanceDecl {
                name,
                range,
                connections,
                span: ispan,
            });
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::Semi, "`;`")?;
        Ok(ModuleItem::Instantiation {
            module,
            params,
            instances,
            span,
        })
    }

    fn parse_gen_block(&mut self) -> Result<(Option<String>, Vec<ModuleItem>), String> {
        if self.eat(&TokenKind::Begin) {
            let label = if self.eat(&TokenKind::Colon) {
                Some(self.expect_ident()?)
            } else {
                None
            };
            let mut items = Vec::new();
            while !matches!(self.peek_kind(), TokenKind::End | TokenKind::Eof) {
                self.parse_module_items_into(&mut items)?;
            }
            self.expect(&TokenKind::End, "`end`")?;
            Ok((label, items))
        } else {
            let mut items = Vec::new();
            self.parse_module_items_into(&mut items)?;
            Ok((None, items))
        }
    }

    fn parse_gen_if(&mut self) -> Result<ModuleItem, String> {
        let span = self.span();
        self.expect(&TokenKind::If, "`if`")?;
        self.expect(&TokenKind::LParen, "`(`")?;
        let cond = self.parse_expr()?;
        self.expect(&TokenKind::RParen, "`)`")?;
        let (label, then_items) = self.parse_gen_block()?;
        let else_items = if self.eat(&TokenKind::Else) {
            self.parse_gen_block()?.1
        } else {
            Vec::new()
        };
        Ok(ModuleItem::GenIf {
            cond,
            then_items,
            else_items,
            label,
            span,
        })
    }

    fn parse_gen_for(&mut self) -> Result<ModuleItem, String> {
        let span = self.span();
        self.expect(&TokenKind::For, "`for`")?;
        self.expect(&TokenKind::LParen, "`(`")?;
        let _ = self.eat(&TokenKind::Genvar);
        let var = self.expect_ident()?;
        self.expect(&TokenKind::Eq, "`=`")?;
        let init = self.parse_expr()?;
        self.expect(&TokenKind::Semi, "`;`")?;
        let cond = self.parse_expr()?;
        self.expect(&TokenKind::Semi, "`;`")?;
        let step_var = self.expect_ident()?;
        let step_expr = if self.eat(&TokenKind::Eq) {
            self.parse_expr()?
        } else if self.eat(&TokenKind::PlusPlus) {
            Expr::Binary {
                op: BinaryOp::Add,
                lhs: Box::new(Expr::Ident(vec![step_var.clone()])),
                rhs: Box::new(Expr::Int(1)),
            }
        } else if self.eat(&TokenKind::MinusMinus) {
            Expr::Binary {
                op: BinaryOp::Sub,
                lhs: Box::new(Expr::Ident(vec![step_var.clone()])),
                rhs: Box::new(Expr::Int(1)),
            }
        } else if self.eat(&TokenKind::PlusEq) {
            let rhs = self.parse_expr()?;
            Expr::Binary {
                op: BinaryOp::Add,
                lhs: Box::new(Expr::Ident(vec![step_var.clone()])),
                rhs: Box::new(rhs),
            }
        } else {
            return Err(self.err("expected assignment in generate for step"));
        };
        self.expect(&TokenKind::RParen, "`)`")?;
        let (label, body) = self.parse_gen_block()?;
        Ok(ModuleItem::GenFor {
            var,
            init,
            cond,
            step_var,
            step_expr,
            body,
            label,
            span,
        })
    }
}
