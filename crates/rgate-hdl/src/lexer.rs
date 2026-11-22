//! Verilog / SystemVerilog lexer with source location tracking.

use crate::preprocess::Preprocessed;
use crate::value::BitVec;
use rgate_core::Logic;

#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    pub file: String,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TokenKind {
    Ident(String),
    SysIdent(String),
    IntLiteral(u64),
    BasedLiteral {
        value: BitVec,
        sized: bool,
    },
    UnbasedUnsized(Logic), // '0, '1, 'x, 'z
    RealLiteral(f64),
    StringLiteral(String),

    // Keywords
    Module,
    EndModule,
    Input,
    Output,
    Inout,
    Wire,
    Reg,
    Logic,
    Tri,
    Supply0,
    Supply1,
    Integer,
    Int,
    Signed,
    Unsigned,
    Parameter,
    Localparam,
    Genvar,
    Assign,
    Always,
    AlwaysComb,
    AlwaysFf,
    AlwaysLatch,
    Initial,
    Final,
    Begin,
    End,
    If,
    Else,
    Case,
    Casex,
    Casez,
    EndCase,
    Default,
    For,
    While,
    Repeat,
    Forever,
    Posedge,
    Negedge,
    Or,
    Generate,
    EndGenerate,
    Function,
    EndFunction,
    Task,
    EndTask,
    Force,
    Release,
    Return,
    Typedef,
    Enum,
    Bit,
    Byte,
    Shortint,
    Longint,
    Automatic,
    // Gate primitives
    GateAnd,
    GateNand,
    GateOr,
    GateNor,
    GateXor,
    GateXnor,
    GateBuf,
    GateNot,
    GateBufif0,
    GateBufif1,
    GateNotif0,
    GateNotif1,

    // Punctuation & operators
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Semi,
    Comma,
    Colon,
    Dot,
    Hash,
    At,
    Question,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    StarStar,
    Tilde,
    Bang,
    Amp,
    Pipe,
    Caret,
    TildeAmp,
    TildePipe,
    TildeCaret,
    AmpAmp,
    PipePipe,
    Eq,
    EqEq,
    BangEq,
    EqEqEq,
    BangEqEq,
    EqEqQ,
    BangEqQ,
    Lt,
    Le,
    Gt,
    Ge,
    Shl,
    Shr,
    Ashl,
    Ashr,
    PlusColon,
    MinusColon,
    PlusPlus,
    MinusMinus,
    PlusEq,
    MinusEq,
    Eof,
}

pub fn tokenize(pre: &Preprocessed) -> Result<Vec<Token>, String> {
    Lexer::new(pre).tokenize()
}

struct Lexer<'a> {
    text: &'a str,
    bytes: &'a [u8],
    line_map: &'a [(String, u32)],
    pos: usize,
    line_idx: usize,
}

impl<'a> Lexer<'a> {
    fn new(pre: &'a Preprocessed) -> Self {
        Self {
            text: &pre.text,
            bytes: pre.text.as_bytes(),
            line_map: &pre.line_map,
            pos: 0,
            line_idx: 0,
        }
    }

    fn span(&self) -> Span {
        let (file, line) = self
            .line_map
            .get(self.line_idx)
            .cloned()
            .unwrap_or_else(|| ("source.sv".into(), 1));
        Span { file, line }
    }

    fn tokenize(mut self) -> Result<Vec<Token>, String> {
        let mut tokens = Vec::new();
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            if b == b'\n' {
                self.line_idx += 1;
                self.pos += 1;
                continue;
            }
            if b.is_ascii_whitespace() {
                self.pos += 1;
                continue;
            }
            if b == b'(' && self.pos + 1 < self.bytes.len() && self.bytes[self.pos + 1] == b'*' {
                self.skip_attribute();
                continue;
            }

            let span = self.span();
            let kind = match b {
                b'\\' => self.scan_escaped_ident(),
                b'"' => self.scan_string(),
                b'$' => self.scan_sys_ident(),
                b'\'' => {
                    if let Some(tok) = try_lex_based(self.text, &mut self.pos, None, &span)? {
                        tok
                    } else {
                        return Err(format!("{}:{}: unexpected `'`", span.file, span.line));
                    }
                }
                b if b.is_ascii_digit() => self.scan_number(&span)?,
                b if b.is_ascii_alphabetic() || b == b'_' => self.scan_ident_or_keyword(),
                _ => self.scan_operator(&span)?,
            };
            tokens.push(Token { kind, span });
        }

        tokens.push(Token {
            kind: TokenKind::Eof,
            span: self.span(),
        });
        Ok(tokens)
    }

    fn skip_attribute(&mut self) {
        self.pos += 2;
        while self.pos + 1 < self.bytes.len()
            && !(self.bytes[self.pos] == b'*' && self.bytes[self.pos + 1] == b')')
        {
            if self.bytes[self.pos] == b'\n' {
                self.line_idx += 1;
            }
            self.pos += 1;
        }
        if self.pos + 1 < self.bytes.len() {
            self.pos += 2;
        }
    }

    fn scan_escaped_ident(&mut self) -> TokenKind {
        self.pos += 1;
        let start = self.pos;
        while self.pos < self.bytes.len() && !self.bytes[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
        TokenKind::Ident(self.text[start..self.pos].to_string())
    }

    fn scan_string(&mut self) -> TokenKind {
        self.pos += 1;
        let mut s = String::new();
        while self.pos < self.bytes.len() && self.bytes[self.pos] != b'"' {
            if self.bytes[self.pos] == b'\\' && self.pos + 1 < self.bytes.len() {
                self.pos += 1;
                match self.bytes[self.pos] {
                    b'n' => s.push('\n'),
                    b't' => s.push('\t'),
                    b'\\' => s.push('\\'),
                    b'"' => s.push('"'),
                    other => s.push(other as char),
                }
                self.pos += 1;
            } else {
                if self.bytes[self.pos] == b'\n' {
                    self.line_idx += 1;
                }
                s.push(self.bytes[self.pos] as char);
                self.pos += 1;
            }
        }
        if self.pos < self.bytes.len() && self.bytes[self.pos] == b'"' {
            self.pos += 1;
        }
        TokenKind::StringLiteral(s)
    }

    fn scan_sys_ident(&mut self) -> TokenKind {
        let start = self.pos;
        self.pos += 1;
        while self.pos < self.bytes.len()
            && (self.bytes[self.pos].is_ascii_alphanumeric()
                || self.bytes[self.pos] == b'_'
                || self.bytes[self.pos] == b'$')
        {
            self.pos += 1;
        }
        TokenKind::SysIdent(self.text[start..self.pos].to_string())
    }

    fn scan_number(&mut self, span: &Span) -> Result<TokenKind, String> {
        let start = self.pos;
        while self.pos < self.bytes.len()
            && (self.bytes[self.pos].is_ascii_digit() || self.bytes[self.pos] == b'_')
        {
            self.pos += 1;
        }
        // Check for based literal: optional horizontal spaces then `'`
        let mut look = self.pos;
        while look < self.bytes.len() && (self.bytes[look] == b' ' || self.bytes[look] == b'\t') {
            look += 1;
        }
        if look < self.bytes.len() && self.bytes[look] == b'\'' {
            let width_str: String = self.text[start..self.pos]
                .chars()
                .filter(|&c| c != '_')
                .collect();
            let width: usize = width_str
                .parse()
                .map_err(|_| format!("{}:{}: invalid literal width", span.file, span.line))?;
            self.pos = look;
            if let Some(tok) = try_lex_based(self.text, &mut self.pos, Some(width), span)? {
                return Ok(tok);
            }
        }
        // Check for real literal (decimal point followed by digit, or exponent)
        if self.pos < self.bytes.len()
            && ((self.bytes[self.pos] == b'.'
                && self.pos + 1 < self.bytes.len()
                && self.bytes[self.pos + 1].is_ascii_digit())
                || self.bytes[self.pos] == b'e'
                || self.bytes[self.pos] == b'E')
        {
            if self.bytes[self.pos] == b'.' {
                self.pos += 1;
                while self.pos < self.bytes.len()
                    && (self.bytes[self.pos].is_ascii_digit() || self.bytes[self.pos] == b'_')
                {
                    self.pos += 1;
                }
            }
            if self.pos < self.bytes.len()
                && (self.bytes[self.pos] == b'e' || self.bytes[self.pos] == b'E')
            {
                self.pos += 1;
                if self.pos < self.bytes.len()
                    && (self.bytes[self.pos] == b'+' || self.bytes[self.pos] == b'-')
                {
                    self.pos += 1;
                }
                while self.pos < self.bytes.len()
                    && (self.bytes[self.pos].is_ascii_digit() || self.bytes[self.pos] == b'_')
                {
                    self.pos += 1;
                }
            }
            let cleaned: String = self.text[start..self.pos]
                .chars()
                .filter(|&c| c != '_')
                .collect();
            let val: f64 = cleaned
                .parse()
                .map_err(|_| format!("{}:{}: invalid real literal", span.file, span.line))?;
            return Ok(TokenKind::RealLiteral(val));
        }
        let cleaned: String = self.text[start..self.pos]
            .chars()
            .filter(|&c| c != '_')
            .collect();
        let val: u64 = cleaned
            .parse()
            .map_err(|_| format!("{}:{}: integer literal overflow", span.file, span.line))?;
        Ok(TokenKind::IntLiteral(val))
    }

    fn scan_ident_or_keyword(&mut self) -> TokenKind {
        let start = self.pos;
        while self.pos < self.bytes.len()
            && (self.bytes[self.pos].is_ascii_alphanumeric()
                || self.bytes[self.pos] == b'_'
                || self.bytes[self.pos] == b'$')
        {
            self.pos += 1;
        }
        keyword_or_ident(&self.text[start..self.pos])
    }

    fn scan_operator(&mut self, span: &Span) -> Result<TokenKind, String> {
        let rest = &self.text[self.pos..];
        let b = self.bytes[self.pos];
        let (kind, len) = if rest.starts_with("===") {
            (TokenKind::EqEqEq, 3)
        } else if rest.starts_with("!==") {
            (TokenKind::BangEqEq, 3)
        } else if rest.starts_with("==?") {
            (TokenKind::EqEqQ, 3)
        } else if rest.starts_with("!=?") {
            (TokenKind::BangEqQ, 3)
        } else if rest.starts_with("<<<") {
            (TokenKind::Ashl, 3)
        } else if rest.starts_with(">>>") {
            (TokenKind::Ashr, 3)
        } else if rest.starts_with("**") {
            (TokenKind::StarStar, 2)
        } else if rest.starts_with("<<") {
            (TokenKind::Shl, 2)
        } else if rest.starts_with(">>") {
            (TokenKind::Shr, 2)
        } else if rest.starts_with("<=") {
            (TokenKind::Le, 2)
        } else if rest.starts_with(">=") {
            (TokenKind::Ge, 2)
        } else if rest.starts_with("==") {
            (TokenKind::EqEq, 2)
        } else if rest.starts_with("!=") {
            (TokenKind::BangEq, 2)
        } else if rest.starts_with("&&") {
            (TokenKind::AmpAmp, 2)
        } else if rest.starts_with("||") {
            (TokenKind::PipePipe, 2)
        } else if rest.starts_with("~&") {
            (TokenKind::TildeAmp, 2)
        } else if rest.starts_with("~|") {
            (TokenKind::TildePipe, 2)
        } else if rest.starts_with("~^") || rest.starts_with("^~") {
            (TokenKind::TildeCaret, 2)
        } else if rest.starts_with("+:") {
            (TokenKind::PlusColon, 2)
        } else if rest.starts_with("-:") {
            (TokenKind::MinusColon, 2)
        } else if rest.starts_with("++") {
            (TokenKind::PlusPlus, 2)
        } else if rest.starts_with("--") {
            (TokenKind::MinusMinus, 2)
        } else if rest.starts_with("+=") {
            (TokenKind::PlusEq, 2)
        } else if rest.starts_with("-=") {
            (TokenKind::MinusEq, 2)
        } else {
            match b {
                b'(' => (TokenKind::LParen, 1),
                b')' => (TokenKind::RParen, 1),
                b'[' => (TokenKind::LBracket, 1),
                b']' => (TokenKind::RBracket, 1),
                b'{' => (TokenKind::LBrace, 1),
                b'}' => (TokenKind::RBrace, 1),
                b';' => (TokenKind::Semi, 1),
                b',' => (TokenKind::Comma, 1),
                b':' => (TokenKind::Colon, 1),
                b'.' => (TokenKind::Dot, 1),
                b'#' => (TokenKind::Hash, 1),
                b'@' => (TokenKind::At, 1),
                b'?' => (TokenKind::Question, 1),
                b'+' => (TokenKind::Plus, 1),
                b'-' => (TokenKind::Minus, 1),
                b'*' => (TokenKind::Star, 1),
                b'/' => (TokenKind::Slash, 1),
                b'%' => (TokenKind::Percent, 1),
                b'~' => (TokenKind::Tilde, 1),
                b'!' => (TokenKind::Bang, 1),
                b'&' => (TokenKind::Amp, 1),
                b'|' => (TokenKind::Pipe, 1),
                b'^' => (TokenKind::Caret, 1),
                b'=' => (TokenKind::Eq, 1),
                b'<' => (TokenKind::Lt, 1),
                b'>' => (TokenKind::Gt, 1),
                _ => {
                    return Err(format!(
                        "{}:{}: unexpected character `{}`",
                        span.file, span.line, b as char
                    ));
                }
            }
        };
        self.pos += len;
        Ok(kind)
    }
}

fn keyword_or_ident(word: &str) -> TokenKind {
    match word {
        "module" | "macromodule" => TokenKind::Module,
        "endmodule" => TokenKind::EndModule,
        "input" => TokenKind::Input,
        "output" => TokenKind::Output,
        "inout" => TokenKind::Inout,
        "wire" => TokenKind::Wire,
        "reg" => TokenKind::Reg,
        "logic" => TokenKind::Logic,
        "tri" => TokenKind::Tri,
        "supply0" => TokenKind::Supply0,
        "supply1" => TokenKind::Supply1,
        "integer" => TokenKind::Integer,
        "int" => TokenKind::Int,
        "signed" => TokenKind::Signed,
        "unsigned" => TokenKind::Unsigned,
        "parameter" => TokenKind::Parameter,
        "localparam" => TokenKind::Localparam,
        "genvar" => TokenKind::Genvar,
        "assign" => TokenKind::Assign,
        "always" => TokenKind::Always,
        "always_comb" => TokenKind::AlwaysComb,
        "always_ff" => TokenKind::AlwaysFf,
        "always_latch" => TokenKind::AlwaysLatch,
        "initial" => TokenKind::Initial,
        "final" => TokenKind::Final,
        "begin" => TokenKind::Begin,
        "end" => TokenKind::End,
        "if" => TokenKind::If,
        "else" => TokenKind::Else,
        "case" => TokenKind::Case,
        "casex" => TokenKind::Casex,
        "casez" => TokenKind::Casez,
        "endcase" => TokenKind::EndCase,
        "default" => TokenKind::Default,
        "for" => TokenKind::For,
        "while" => TokenKind::While,
        "repeat" => TokenKind::Repeat,
        "forever" => TokenKind::Forever,
        "posedge" => TokenKind::Posedge,
        "negedge" => TokenKind::Negedge,
        "or" => TokenKind::Or,
        "generate" => TokenKind::Generate,
        "endgenerate" => TokenKind::EndGenerate,
        "function" => TokenKind::Function,
        "endfunction" => TokenKind::EndFunction,
        "task" => TokenKind::Task,
        "endtask" => TokenKind::EndTask,
        "force" => TokenKind::Force,
        "release" => TokenKind::Release,
        "return" => TokenKind::Return,
        "typedef" => TokenKind::Typedef,
        "enum" => TokenKind::Enum,
        "bit" => TokenKind::Bit,
        "byte" => TokenKind::Byte,
        "shortint" => TokenKind::Shortint,
        "longint" => TokenKind::Longint,
        "automatic" => TokenKind::Automatic,
        "and" => TokenKind::GateAnd,
        "nand" => TokenKind::GateNand,
        "nor" => TokenKind::GateNor,
        "xor" => TokenKind::GateXor,
        "xnor" => TokenKind::GateXnor,
        "buf" => TokenKind::GateBuf,
        "not" => TokenKind::GateNot,
        "bufif0" => TokenKind::GateBufif0,
        "bufif1" => TokenKind::GateBufif1,
        "notif0" => TokenKind::GateNotif0,
        "notif1" => TokenKind::GateNotif1,
        _ => TokenKind::Ident(word.to_string()),
    }
}

fn try_lex_based(
    text: &str,
    i: &mut usize,
    explicit_width: Option<usize>,
    span: &Span,
) -> Result<Option<TokenKind>, String> {
    let bytes = text.as_bytes();
    if *i >= bytes.len() || bytes[*i] != b'\'' {
        return Ok(None);
    }
    let save = *i;
    *i += 1;
    if *i >= bytes.len() {
        *i = save;
        return Ok(None);
    }

    // SystemVerilog unbased unsized fill literals: '0, '1, 'x, 'z
    if explicit_width.is_none()
        && matches!(bytes[*i], b'0' | b'1' | b'x' | b'X' | b'z' | b'Z')
        && (*i + 1 >= bytes.len() || !bytes[*i + 1].is_ascii_alphanumeric())
    {
        let bit = match bytes[*i] {
            b'0' => Logic::Low,
            b'1' => Logic::High,
            b'x' | b'X' => Logic::Unknown,
            _ => Logic::HighZ,
        };
        *i += 1;
        return Ok(Some(TokenKind::UnbasedUnsized(bit)));
    }

    let mut signed = false;
    if matches!(bytes[*i], b's' | b'S') {
        signed = true;
        *i += 1;
        if *i >= bytes.len() {
            return Err(format!("{}:{}: incomplete based literal", span.file, span.line));
        }
    }

    let base = match bytes[*i] {
        b'b' | b'B' => 2u32,
        b'o' | b'O' => 8u32,
        b'd' | b'D' => 10u32,
        b'h' | b'H' => 16u32,
        _ => {
            *i = save;
            return Ok(None);
        }
    };
    *i += 1;

    // Verilog allows horizontal whitespace between base specifier and digits (e.g. `4'b 0001`)
    while *i < bytes.len() && (bytes[*i] == b' ' || bytes[*i] == b'\t') {
        *i += 1;
    }

    let start = *i;
    while *i < bytes.len() {
        let c = bytes[*i];
        let ok = match base {
            2 => matches!(c, b'0' | b'1' | b'x' | b'X' | b'z' | b'Z' | b'?' | b'_'),
            8 => matches!(c, b'0'..=b'7' | b'x' | b'X' | b'z' | b'Z' | b'?' | b'_'),
            10 => matches!(c, b'0'..=b'9' | b'x' | b'X' | b'z' | b'Z' | b'?' | b'_'),
            16 => c.is_ascii_hexdigit() || matches!(c, b'x' | b'X' | b'z' | b'Z' | b'?' | b'_'),
            _ => false,
        };
        if ok {
            *i += 1;
        } else {
            break;
        }
    }
    let digits: String = text[start..*i].chars().filter(|&c| c != '_').collect();
    if digits.is_empty() {
        return Err(format!("{}:{}: empty based literal", span.file, span.line));
    }

    let width = explicit_width.unwrap_or(32).max(1);
    let mut bv = parse_based_digits(&digits, base, width, span)?;
    bv.signed = signed;
    Ok(Some(TokenKind::BasedLiteral {
        value: bv,
        sized: explicit_width.is_some(),
    }))
}

fn parse_based_digits(digits: &str, base: u32, width: usize, span: &Span) -> Result<BitVec, String> {
    if base == 10 {
        if digits.len() == 1 {
            match digits.as_bytes()[0] {
                b'x' | b'X' => return Ok(BitVec::filled(width, Logic::Unknown)),
                b'z' | b'Z' | b'?' => return Ok(BitVec::filled(width, Logic::HighZ)),
                _ => {}
            }
        }
        // Arbitrary precision decimal -> BitVec
        let mut out = BitVec::zeros(width);
        let ten = BitVec::from_u64(10, width);
        for ch in digits.chars() {
            let d = ch
                .to_digit(10)
                .ok_or_else(|| format!("{}:{}: invalid decimal digit `{ch}`", span.file, span.line))?;
            out = out.mul(&ten, width, false);
            out = out.add(&BitVec::from_u64(u64::from(d), width), width, false);
        }
        return Ok(out);
    }

    let bits_per_digit = match base {
        2 => 1,
        8 => 3,
        16 => 4,
        _ => unreachable!(),
    };
    let mut bits: Vec<Logic> = Vec::with_capacity(digits.len() * bits_per_digit);
    for ch in digits.chars().rev() {
        match ch {
            'x' | 'X' => {
                for _ in 0..bits_per_digit {
                    bits.push(Logic::Unknown);
                }
            }
            'z' | 'Z' | '?' => {
                for _ in 0..bits_per_digit {
                    bits.push(Logic::HighZ);
                }
            }
            _ => {
                let d = ch.to_digit(base).ok_or_else(|| {
                    format!("{}:{}: invalid digit `{ch}`", span.file, span.line)
                })?;
                for b in 0..bits_per_digit {
                    bits.push(Logic::from_bool((d >> b) & 1 != 0));
                }
            }
        }
    }
    // Verilog left-extension rule: MSB X extends X, MSB Z extends Z, otherwise 0
    let fill = match bits.last().copied().unwrap_or(Logic::Low) {
        Logic::Unknown => Logic::Unknown,
        Logic::HighZ => Logic::HighZ,
        _ => Logic::Low,
    };
    let mut bv = BitVec::filled(width, fill);
    for (idx, &bit) in bits.iter().enumerate().take(width) {
        bv.set_bit(idx, bit);
    }
    Ok(bv)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preprocess::Preprocessor;

    #[test]
    fn lexes_based_literals_and_reals() {
        let mut pp = Preprocessor::new();
        let pre = pp
            .process("t.sv", "4'b 10xz 32'h0050_0093 #0.5 '0 (* keep *) \\esc ")
            .unwrap();
        let toks = tokenize(&pre).unwrap();
        assert!(matches!(toks[0].kind, TokenKind::BasedLiteral { sized: true, .. }));
        assert!(matches!(toks[1].kind, TokenKind::BasedLiteral { sized: true, .. }));
        assert_eq!(toks[2].kind, TokenKind::Hash);
        assert_eq!(toks[3].kind, TokenKind::RealLiteral(0.5));
        assert_eq!(toks[4].kind, TokenKind::UnbasedUnsized(Logic::Low));
        assert_eq!(toks[5].kind, TokenKind::Ident("esc".into()));
    }
}
