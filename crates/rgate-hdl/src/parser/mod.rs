//! Recursive-descent + Pratt parser for Verilog-2005 / SystemVerilog-2012 subset.

mod cursor;
mod expr;
mod module;
mod stmt;

use crate::ast::{Expr, LValue, ModuleDecl, Range};
use crate::lexer::Token;
use crate::preprocess::Timescale;
use cursor::TokenCursor;
use expr::ExprParser;
use std::ops::{Deref, DerefMut};

pub fn parse_modules(tokens: Vec<Token>, timescale: Timescale) -> Result<Vec<ModuleDecl>, String> {
    let mut parser = Parser {
        cursor: TokenCursor::new(tokens),
        timescale,
    };
    let mut modules = Vec::new();
    while !parser.at_eof() {
        modules.push(parser.parse_module()?);
    }
    Ok(modules)
}

struct Parser {
    cursor: TokenCursor,
    timescale: Timescale,
}

impl Deref for Parser {
    type Target = TokenCursor;
    fn deref(&self) -> &Self::Target {
        &self.cursor
    }
}

impl DerefMut for Parser {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.cursor
    }
}

impl Parser {
    pub(super) fn parse_expr(&mut self) -> Result<Expr, String> {
        ExprParser::new(&mut self.cursor).parse_expr()
    }

    pub(super) fn parse_lvalue(&mut self) -> Result<LValue, String> {
        ExprParser::new(&mut self.cursor).parse_lvalue()
    }

    pub(super) fn parse_range(&mut self) -> Result<Range, String> {
        ExprParser::new(&mut self.cursor).parse_range()
    }
}
