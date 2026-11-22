use crate::lexer::{Span, Token, TokenKind};

/// Owns the token stream and cursor primitives (lookahead, consumption, error formatting).
pub(super) struct TokenCursor {
    tokens: Vec<Token>,
    pos: usize,
}

impl TokenCursor {
    pub(super) fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    pub(super) fn peek(&self) -> &Token {
        self.tokens
            .get(self.pos)
            .unwrap_or_else(|| self.tokens.last().unwrap())
    }

    pub(super) fn peek_kind(&self) -> &TokenKind {
        &self.peek().kind
    }

    pub(super) fn peek_at(&self, offset: usize) -> &TokenKind {
        self.tokens
            .get(self.pos + offset)
            .map(|t| &t.kind)
            .unwrap_or(&TokenKind::Eof)
    }

    pub(super) fn at_eof(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Eof)
    }

    pub(super) fn span(&self) -> Span {
        self.peek().span.clone()
    }

    pub(super) fn err(&self, msg: impl Into<String>) -> String {
        let s = self.span();
        format!("{}:{}: {}", s.file, s.line, msg.into())
    }

    pub(super) fn advance(&mut self) -> Token {
        let tok = self.peek().clone();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        tok
    }

    pub(super) fn eat(&mut self, expected: &TokenKind) -> bool {
        if self.peek_kind() == expected {
            self.advance();
            true
        } else {
            false
        }
    }

    pub(super) fn expect(&mut self, expected: &TokenKind, label: &str) -> Result<Token, String> {
        if self.peek_kind() == expected {
            Ok(self.advance())
        } else {
            Err(self.err(format!(
                "expected {label}, found {:?}",
                self.peek_kind()
            )))
        }
    }

    pub(super) fn expect_ident(&mut self) -> Result<String, String> {
        match self.peek_kind().clone() {
            TokenKind::Ident(name) => {
                self.advance();
                Ok(name)
            }
            other => Err(self.err(format!("expected identifier, found {other:?}"))),
        }
    }
}
