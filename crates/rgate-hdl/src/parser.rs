//! Recursive-descent + Pratt parser for Verilog-2005 / SystemVerilog-2012 subset.

use crate::ast::*;
use crate::lexer::{Span, Token, TokenKind};
use crate::preprocess::Timescale;

pub fn parse_modules(tokens: Vec<Token>, timescale: Timescale) -> Result<Vec<ModuleDecl>, String> {
    let mut parser = Parser {
        tokens,
        pos: 0,
        timescale,
    };
    let mut modules = Vec::new();
    while !parser.at_eof() {
        modules.push(parser.parse_module()?);
    }
    Ok(modules)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    timescale: Timescale,
}

impl Parser {
    fn peek(&self) -> &Token {
        self.tokens
            .get(self.pos)
            .unwrap_or_else(|| self.tokens.last().unwrap())
    }

    fn peek_kind(&self) -> &TokenKind {
        &self.peek().kind
    }

    fn peek_at(&self, offset: usize) -> &TokenKind {
        self.tokens
            .get(self.pos + offset)
            .map(|t| &t.kind)
            .unwrap_or(&TokenKind::Eof)
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Eof)
    }

    fn span(&self) -> Span {
        self.peek().span.clone()
    }

    fn err(&self, msg: impl Into<String>) -> String {
        let s = self.span();
        format!("{}:{}: {}", s.file, s.line, msg.into())
    }

    fn advance(&mut self) -> Token {
        let tok = self.peek().clone();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        tok
    }

    fn eat(&mut self, expected: &TokenKind) -> bool {
        if self.peek_kind() == expected {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, expected: &TokenKind, label: &str) -> Result<Token, String> {
        if self.peek_kind() == expected {
            Ok(self.advance())
        } else {
            Err(self.err(format!(
                "expected {label}, found {:?}",
                self.peek_kind()
            )))
        }
    }

    fn expect_ident(&mut self) -> Result<String, String> {
        match self.peek_kind().clone() {
            TokenKind::Ident(name) => {
                self.advance();
                Ok(name)
            }
            other => Err(self.err(format!("expected identifier, found {other:?}"))),
        }
    }

    fn parse_module(&mut self) -> Result<ModuleDecl, String> {
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

    fn parse_optional_param_type(
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

    fn try_parse_port_dir(&mut self) -> Option<PortDir> {
        let dir = match self.peek_kind() {
            TokenKind::Input => PortDir::Input,
            TokenKind::Output => PortDir::Output,
            TokenKind::Inout => PortDir::Inout,
            _ => return None,
        };
        self.advance();
        Some(dir)
    }

    fn try_parse_var_kind(&mut self) -> Option<VarKind> {
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

    fn parse_range(&mut self) -> Result<Range, String> {
        self.expect(&TokenKind::LBracket, "`[`")?;
        let msb = self.parse_expr()?;
        self.expect(&TokenKind::Colon, "`:`")?;
        let lsb = self.parse_expr()?;
        self.expect(&TokenKind::RBracket, "`]`")?;
        Ok(Range { msb, lsb })
    }

    fn parse_module_items_into(&mut self, items: &mut Vec<ModuleItem>) -> Result<(), String> {
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

    fn parse_var_decl(&mut self) -> Result<VarDecl, String> {
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

    fn parse_function(&mut self) -> Result<FunctionDecl, String> {
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

    fn parse_task(&mut self) -> Result<TaskDecl, String> {
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

    fn try_parse_delay(&mut self) -> Result<Option<Expr>, String> {
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

    fn parse_stmt(&mut self) -> Result<Stmt, String> {
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

    fn parse_lvalue(&mut self) -> Result<LValue, String> {
        if self.eat(&TokenKind::LBrace) {
            let mut parts = Vec::new();
            loop {
                parts.push(self.parse_lvalue()?);
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
            self.expect(&TokenKind::RBrace, "`}`")?;
            return Ok(LValue::Concat(parts));
        }
        let mut path = vec![self.expect_ident()?];
        while self.eat(&TokenKind::Dot) {
            path.push(self.expect_ident()?);
        }
        let mut lv = LValue::Ident(path);
        while self.eat(&TokenKind::LBracket) {
            let first = self.parse_expr()?;
            if self.eat(&TokenKind::Colon) {
                let lsb = self.parse_expr()?;
                self.expect(&TokenKind::RBracket, "`]`")?;
                lv = LValue::Range {
                    target: Box::new(lv),
                    msb: first,
                    lsb,
                };
            } else if self.eat(&TokenKind::PlusColon) {
                let width = self.parse_expr()?;
                self.expect(&TokenKind::RBracket, "`]`")?;
                lv = LValue::IndexedPart {
                    target: Box::new(lv),
                    base: first,
                    width,
                    ascending: true,
                };
            } else if self.eat(&TokenKind::MinusColon) {
                let width = self.parse_expr()?;
                self.expect(&TokenKind::RBracket, "`]`")?;
                lv = LValue::IndexedPart {
                    target: Box::new(lv),
                    base: first,
                    width,
                    ascending: false,
                };
            } else {
                self.expect(&TokenKind::RBracket, "`]`")?;
                lv = LValue::Index {
                    target: Box::new(lv),
                    index: first,
                };
            }
        }
        Ok(lv)
    }

    fn parse_expr(&mut self) -> Result<Expr, String> {
        self.parse_ternary()
    }

    fn parse_ternary(&mut self) -> Result<Expr, String> {
        let cond = self.parse_binary(1)?;
        if self.eat(&TokenKind::Question) {
            let then_expr = self.parse_expr()?;
            self.expect(&TokenKind::Colon, "`:`")?;
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
        while let Some((op, prec)) = binary_op_prec(self.peek_kind()) {
            if prec < min_prec {
                break;
            }
            self.advance();
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
        let op = match self.peek_kind() {
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
            self.advance();
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
        while self.eat(&TokenKind::LBracket) {
            let first = self.parse_expr()?;
            if self.eat(&TokenKind::Colon) {
                let lsb = self.parse_expr()?;
                self.expect(&TokenKind::RBracket, "`]`")?;
                expr = Expr::Range {
                    target: Box::new(expr),
                    msb: Box::new(first),
                    lsb: Box::new(lsb),
                };
            } else if self.eat(&TokenKind::PlusColon) {
                let width = self.parse_expr()?;
                self.expect(&TokenKind::RBracket, "`]`")?;
                expr = Expr::IndexedPart {
                    target: Box::new(expr),
                    base: Box::new(first),
                    width: Box::new(width),
                    ascending: true,
                };
            } else if self.eat(&TokenKind::MinusColon) {
                let width = self.parse_expr()?;
                self.expect(&TokenKind::RBracket, "`]`")?;
                expr = Expr::IndexedPart {
                    target: Box::new(expr),
                    base: Box::new(first),
                    width: Box::new(width),
                    ascending: false,
                };
            } else {
                self.expect(&TokenKind::RBracket, "`]`")?;
                expr = Expr::Index {
                    target: Box::new(expr),
                    index: Box::new(first),
                };
            }
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        match self.peek_kind().clone() {
            TokenKind::IntLiteral(n) => {
                self.advance();
                Ok(Expr::Int(n))
            }
            TokenKind::BasedLiteral { value, sized } => {
                self.advance();
                Ok(Expr::Based { value, sized })
            }
            TokenKind::UnbasedUnsized(bit) => {
                self.advance();
                Ok(Expr::UnbasedUnsized(bit))
            }
            TokenKind::RealLiteral(r) => {
                self.advance();
                Ok(Expr::Real(r))
            }
            TokenKind::StringLiteral(s) => {
                self.advance();
                Ok(Expr::String(s))
            }
            TokenKind::LParen => {
                self.advance();
                let e = self.parse_expr()?;
                self.expect(&TokenKind::RParen, "`)`")?;
                Ok(e)
            }
            TokenKind::LBrace => {
                self.advance();
                let first = self.parse_expr()?;
                if self.eat(&TokenKind::LBrace) {
                    // Replication `{count{a, b, ...}}`
                    let mut inner = Vec::new();
                    loop {
                        inner.push(self.parse_expr()?);
                        if !self.eat(&TokenKind::Comma) {
                            break;
                        }
                    }
                    self.expect(&TokenKind::RBrace, "`}`")?;
                    self.expect(&TokenKind::RBrace, "`}`")?;
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
                    while self.eat(&TokenKind::Comma) {
                        parts.push(self.parse_expr()?);
                    }
                    self.expect(&TokenKind::RBrace, "`}`")?;
                    Ok(Expr::Concat(parts))
                }
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
                Ok(Expr::SysCall { name, args })
            }
            TokenKind::Ident(first) => {
                self.advance();
                if self.eat(&TokenKind::LParen) {
                    let mut args = Vec::new();
                    while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
                        args.push(self.parse_expr()?);
                        if !self.eat(&TokenKind::Comma) {
                            break;
                        }
                    }
                    self.expect(&TokenKind::RParen, "`)`")?;
                    return Ok(Expr::Call { name: first, args });
                }
                let mut path = vec![first];
                while self.eat(&TokenKind::Dot) {
                    path.push(self.expect_ident()?);
                }
                Ok(Expr::Ident(path))
            }
            other => Err(self.err(format!("expected expression, found {other:?}"))),
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

fn lvalue_to_expr(lv: &LValue) -> Result<Expr, String> {
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
