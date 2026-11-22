//! Verilog / SystemVerilog preprocessor: comments, timescales, `line, `define, `ifdef.

use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timescale {
    /// Time unit in femtoseconds (e.g. 1ns = 1_000_000 fs).
    pub unit_fs: u64,
    /// Time precision in femtoseconds (e.g. 1ps = 1_000 fs).
    pub precision_fs: u64,
}

impl Default for Timescale {
    fn default() -> Self {
        Self {
            unit_fs: 1_000_000,    // 1ns
            precision_fs: 1_000,   // 1ps
        }
    }
}

impl Timescale {
    pub fn ticks_per_ns(&self) -> u64 {
        (1_000_000 / self.precision_fs).max(1)
    }
}

#[derive(Clone, Debug)]
pub struct Preprocessed {
    pub text: String,
    pub line_map: Vec<(String, u32)>,
    pub timescale: Timescale,
}

#[derive(Clone, Debug)]
struct MacroDef {
    params: Option<Vec<String>>,
    body: String,
}

#[derive(Clone, Copy, Debug)]
struct IfFrame {
    parent_active: bool,
    branch_active: bool,
    any_taken: bool,
}

pub struct Preprocessor {
    macros: HashMap<String, MacroDef>,
    timescale: Option<Timescale>,
}

impl Default for Preprocessor {
    fn default() -> Self {
        Self::new()
    }
}

impl Preprocessor {
    pub fn new() -> Self {
        Self {
            macros: HashMap::new(),
            timescale: None,
        }
    }

    pub fn timescale(&self) -> Timescale {
        self.timescale.unwrap_or_default()
    }

    pub fn process(&mut self, default_file: &str, input: &str) -> Result<Preprocessed, String> {
        PreprocessCursor::new(self, default_file, input).run()
    }

    fn expand_text(&self, text: &str, depth: usize) -> String {
        if depth > 16 || !text.contains('`') {
            return text.to_string();
        }
        let mut out = String::with_capacity(text.len());
        let bytes = text.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'`' {
                i += 1;
                let start = i;
                while i < bytes.len() && is_ident_cont(bytes[i]) {
                    i += 1;
                }
                let name = &text[start..i];
                if let Some(mdef) = self.macros.get(name) {
                    let expanded = if let Some(params) = &mdef.params {
                        skip_hspace(bytes, &mut i);
                        let args = if i < bytes.len() && bytes[i] == b'(' {
                            let mut dummy_line = 1;
                            let mut dummy_out = String::new();
                            let mut dummy_map = Vec::new();
                            read_macro_args(text, &mut i, &mut dummy_line, &mut dummy_out, &mut dummy_map, "")
                        } else {
                            Vec::new()
                        };
                        substitute_macro(&mdef.body, params, &args)
                    } else {
                        mdef.body.clone()
                    };
                    out.push_str(&self.expand_text(&expanded, depth + 1));
                }
            } else {
                out.push(bytes[i] as char);
                i += 1;
            }
        }
        out
    }
}

struct PreprocessCursor<'a> {
    pp: &'a mut Preprocessor,
    input: &'a str,
    bytes: &'a [u8],
    pos: usize,
    out: String,
    line_map: Vec<(String, u32)>,
    file: String,
    line: u32,
    if_stack: Vec<IfFrame>,
    local_timescale: Option<Timescale>,
}

impl<'a> PreprocessCursor<'a> {
    fn new(pp: &'a mut Preprocessor, default_file: &str, input: &'a str) -> Self {
        let file = default_file.to_string();
        let line_map = vec![(file.clone(), 1)];
        Self {
            pp,
            input,
            bytes: input.as_bytes(),
            pos: 0,
            out: String::with_capacity(input.len()),
            line_map,
            file,
            line: 1,
            if_stack: Vec::new(),
            local_timescale: None,
        }
    }

    fn is_active(&self) -> bool {
        self.if_stack.last().is_none_or(|f| f.branch_active)
    }

    fn emit_newline(&mut self) {
        self.out.push('\n');
        self.line += 1;
        self.line_map.push((self.file.clone(), self.line));
    }

    fn run(mut self) -> Result<Preprocessed, String> {
        while self.pos < self.bytes.len() {
            let active = self.is_active();
            let b = self.bytes[self.pos];
            if b == b'\n' {
                self.emit_newline();
                self.pos += 1;
            } else if b == b'\r' {
                self.pos += 1;
            } else if b == b'/'
                && self.pos + 1 < self.bytes.len()
                && self.bytes[self.pos + 1] == b'/'
            {
                self.skip_line_comment();
            } else if b == b'/'
                && self.pos + 1 < self.bytes.len()
                && self.bytes[self.pos + 1] == b'*'
            {
                self.skip_block_comment(active)?;
            } else if b == b'"' {
                self.scan_string(active);
            } else if b == b'`' {
                self.handle_directive(active)?;
            } else {
                if active {
                    self.out.push(b as char);
                }
                self.pos += 1;
            }
        }
        let timescale = self.local_timescale.or(self.pp.timescale).unwrap_or_default();
        Ok(Preprocessed {
            text: self.out,
            line_map: self.line_map,
            timescale,
        })
    }

    fn skip_line_comment(&mut self) {
        self.pos += 2;
        while self.pos < self.bytes.len() && self.bytes[self.pos] != b'\n' {
            self.pos += 1;
        }
    }

    fn skip_block_comment(&mut self, active: bool) -> Result<(), String> {
        self.pos += 2;
        while self.pos + 1 < self.bytes.len()
            && !(self.bytes[self.pos] == b'*' && self.bytes[self.pos + 1] == b'/')
        {
            if self.bytes[self.pos] == b'\n' {
                self.emit_newline();
            }
            self.pos += 1;
        }
        if self.pos + 1 < self.bytes.len() {
            self.pos += 2;
        } else {
            return Err(format!(
                "{}:{}: unterminated block comment",
                self.file, self.line
            ));
        }
        if active {
            self.out.push(' ');
        }
        Ok(())
    }

    fn scan_string(&mut self, active: bool) {
        let start = self.pos;
        self.pos += 1;
        while self.pos < self.bytes.len() && self.bytes[self.pos] != b'"' {
            if self.bytes[self.pos] == b'\\' && self.pos + 1 < self.bytes.len() {
                self.pos += 2;
            } else if self.bytes[self.pos] == b'\n' {
                break;
            } else {
                self.pos += 1;
            }
        }
        if self.pos < self.bytes.len() && self.bytes[self.pos] == b'"' {
            self.pos += 1;
        }
        if active {
            self.out.push_str(&self.input[start..self.pos]);
        }
    }

    fn handle_directive(&mut self, active: bool) -> Result<(), String> {
        self.pos += 1;
        let name_start = self.pos;
        while self.pos < self.bytes.len() && is_ident_cont(self.bytes[self.pos]) {
            self.pos += 1;
        }
        let name = &self.input[name_start..self.pos];
        match name {
            "ifdef" | "ifndef" => {
                skip_hspace(self.bytes, &mut self.pos);
                let sym = read_ident(self.input, &mut self.pos);
                let defined = self.pp.macros.contains_key(sym);
                let cond = if name == "ifdef" { defined } else { !defined };
                self.if_stack.push(IfFrame {
                    parent_active: active,
                    branch_active: active && cond,
                    any_taken: cond,
                });
            }
            "elsif" => {
                skip_hspace(self.bytes, &mut self.pos);
                let sym = read_ident(self.input, &mut self.pos);
                let defined = self.pp.macros.contains_key(sym);
                let frame = self
                    .if_stack
                    .last_mut()
                    .ok_or_else(|| format!("{}:{}: unexpected `elsif", self.file, self.line))?;
                if frame.parent_active && !frame.any_taken && defined {
                    frame.branch_active = true;
                    frame.any_taken = true;
                } else {
                    frame.branch_active = false;
                }
            }
            "else" => {
                let frame = self
                    .if_stack
                    .last_mut()
                    .ok_or_else(|| format!("{}:{}: unexpected `else", self.file, self.line))?;
                frame.branch_active = frame.parent_active && !frame.any_taken;
                frame.any_taken = true;
            }
            "endif" => {
                if self.if_stack.pop().is_none() {
                    return Err(format!("{}:{}: unexpected `endif", self.file, self.line));
                }
            }
            "define" => self.handle_define(active),
            "undef" => {
                skip_hspace(self.bytes, &mut self.pos);
                let mname = read_ident(self.input, &mut self.pos);
                if active {
                    self.pp.macros.remove(mname);
                }
            }
            "timescale" => self.handle_timescale(active)?,
            "line" => self.handle_line(),
            "default_nettype"
            | "resetall"
            | "celldefine"
            | "endcelldefine"
            | "begin_keywords"
            | "end_keywords"
            | "pragma"
            | "nounconnected_drive"
            | "unconnected_drive" => {
                while self.pos < self.bytes.len() && self.bytes[self.pos] != b'\n' {
                    self.pos += 1;
                }
            }
            _ => {
                if active {
                    self.expand_macro_invocation(name);
                }
            }
        }
        Ok(())
    }

    fn handle_define(&mut self, active: bool) {
        skip_hspace(self.bytes, &mut self.pos);
        let mname = read_ident(self.input, &mut self.pos).to_string();
        let mut params = None;
        // Function-like macro iff '(' immediately follows name (no whitespace)
        if self.pos < self.bytes.len() && self.bytes[self.pos] == b'(' {
            self.pos += 1;
            let mut plist = Vec::new();
            loop {
                skip_hspace(self.bytes, &mut self.pos);
                if self.pos >= self.bytes.len() || self.bytes[self.pos] == b')' {
                    if self.pos < self.bytes.len() {
                        self.pos += 1;
                    }
                    break;
                }
                let p = read_ident(self.input, &mut self.pos).to_string();
                // Skip optional default value `= ...`
                skip_hspace(self.bytes, &mut self.pos);
                if self.pos < self.bytes.len() && self.bytes[self.pos] == b'=' {
                    while self.pos < self.bytes.len()
                        && self.bytes[self.pos] != b','
                        && self.bytes[self.pos] != b')'
                    {
                        self.pos += 1;
                    }
                }
                if !p.is_empty() {
                    plist.push(p);
                }
                skip_hspace(self.bytes, &mut self.pos);
                if self.pos < self.bytes.len() && self.bytes[self.pos] == b',' {
                    self.pos += 1;
                }
            }
            params = Some(plist);
        }
        skip_hspace(self.bytes, &mut self.pos);
        let mut body = String::new();
        while self.pos < self.bytes.len() && self.bytes[self.pos] != b'\n' {
            if self.bytes[self.pos] == b'\\'
                && self.pos + 1 < self.bytes.len()
                && self.bytes[self.pos + 1] == b'\n'
            {
                body.push(' ');
                self.emit_newline();
                self.pos += 2;
                continue;
            }
            if self.bytes[self.pos] == b'\\'
                && self.pos + 2 < self.bytes.len()
                && self.bytes[self.pos + 1] == b'\r'
                && self.bytes[self.pos + 2] == b'\n'
            {
                body.push(' ');
                self.emit_newline();
                self.pos += 3;
                continue;
            }
            if self.bytes[self.pos] == b'/'
                && self.pos + 1 < self.bytes.len()
                && self.bytes[self.pos + 1] == b'/'
            {
                while self.pos < self.bytes.len() && self.bytes[self.pos] != b'\n' {
                    self.pos += 1;
                }
                break;
            }
            if self.bytes[self.pos] != b'\r' {
                body.push(self.bytes[self.pos] as char);
            }
            self.pos += 1;
        }
        if active && !mname.is_empty() {
            self.pp.macros.insert(
                mname,
                MacroDef {
                    params,
                    body: body.trim().to_string(),
                },
            );
        }
    }

    fn handle_timescale(&mut self, active: bool) -> Result<(), String> {
        let start = self.pos;
        while self.pos < self.bytes.len() && self.bytes[self.pos] != b'\n' {
            if self.bytes[self.pos] == b'/'
                && self.pos + 1 < self.bytes.len()
                && self.bytes[self.pos + 1] == b'/'
            {
                break;
            }
            self.pos += 1;
        }
        if active {
            let spec = self.input[start..self.pos].trim();
            let ts = parse_timescale(spec)
                .ok_or_else(|| format!("{}:{}: invalid timescale `{spec}`", self.file, self.line))?;
            self.local_timescale = Some(ts);
            self.pp.timescale = Some(match self.pp.timescale {
                Some(prev) => Timescale {
                    unit_fs: ts.unit_fs,
                    precision_fs: prev.precision_fs.min(ts.precision_fs),
                },
                None => ts,
            });
        }
        Ok(())
    }

    fn handle_line(&mut self) {
        let start = self.pos;
        while self.pos < self.bytes.len() && self.bytes[self.pos] != b'\n' {
            self.pos += 1;
        }
        let rest = self.input[start..self.pos].trim();
        if let Some((num, fname)) = parse_line_directive(rest) {
            self.file = fname;
            // Subtract 1 because the trailing `\n` will increment `line`
            self.line = num.saturating_sub(1);
        }
    }

    fn expand_macro_invocation(&mut self, name: &str) {
        let Some(mdef) = self.pp.macros.get(name).cloned() else {
            return;
        };
        let expanded = if let Some(params) = &mdef.params {
            skip_hspace(self.bytes, &mut self.pos);
            let args = if self.pos < self.bytes.len() && self.bytes[self.pos] == b'(' {
                read_macro_args(
                    self.input,
                    &mut self.pos,
                    &mut self.line,
                    &mut self.out,
                    &mut self.line_map,
                    &self.file,
                )
            } else {
                Vec::new()
            };
            substitute_macro(&mdef.body, params, &args)
        } else {
            mdef.body.clone()
        };
        let resolved = self.pp.expand_text(&expanded, 0);
        self.out.push_str(&resolved);
    }
}

fn is_ident_cont(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

fn skip_hspace(bytes: &[u8], i: &mut usize) {
    while *i < bytes.len() && (bytes[*i] == b' ' || bytes[*i] == b'\t') {
        *i += 1;
    }
}

fn read_ident<'a>(input: &'a str, i: &mut usize) -> &'a str {
    let bytes = input.as_bytes();
    let start = *i;
    while *i < bytes.len() && is_ident_cont(bytes[*i]) {
        *i += 1;
    }
    &input[start..*i]
}

fn read_macro_args(
    input: &str,
    i: &mut usize,
    line: &mut u32,
    out: &mut String,
    line_map: &mut Vec<(String, u32)>,
    file: &str,
) -> Vec<String> {
    let bytes = input.as_bytes();
    if *i >= bytes.len() || bytes[*i] != b'(' {
        return Vec::new();
    }
    *i += 1;
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut depth = 1usize;
    let mut in_str = false;
    while *i < bytes.len() && depth > 0 {
        let ch = bytes[*i];
        if in_str {
            if ch == b'\\' && *i + 1 < bytes.len() {
                cur.push(ch as char);
                cur.push(bytes[*i + 1] as char);
                *i += 2;
                continue;
            }
            if ch == b'"' {
                in_str = false;
            }
            cur.push(ch as char);
            *i += 1;
            continue;
        }
        match ch {
            b'"' => {
                in_str = true;
                cur.push('"');
            }
            b'(' => {
                depth += 1;
                cur.push('(');
            }
            b')' => {
                depth -= 1;
                if depth == 0 {
                    args.push(cur.trim().to_string());
                    *i += 1;
                    break;
                }
                cur.push(')');
            }
            b',' if depth == 1 => {
                args.push(cur.trim().to_string());
                cur.clear();
            }
            b'\n' => {
                out.push('\n');
                *line += 1;
                line_map.push((file.to_string(), *line));
                cur.push(' ');
            }
            _ => cur.push(ch as char),
        }
        *i += 1;
    }
    args
}

fn substitute_macro(body: &str, params: &[String], args: &[String]) -> String {
    let mut out = String::with_capacity(body.len());
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' {
            let start = i;
            while i < bytes.len() && is_ident_cont(bytes[i]) {
                i += 1;
            }
            let ident = &body[start..i];
            if let Some(pos) = params.iter().position(|p| p == ident) {
                if let Some(arg) = args.get(pos) {
                    out.push_str(arg);
                }
            } else {
                out.push_str(ident);
            }
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

fn parse_timescale(spec: &str) -> Option<Timescale> {
    let (lhs, rhs) = spec.split_once('/')?;
    let unit_fs = parse_time_unit(lhs.trim())?;
    let precision_fs = parse_time_unit(rhs.trim())?;
    if precision_fs > unit_fs {
        return None;
    }
    Some(Timescale {
        unit_fs,
        precision_fs,
    })
}

fn parse_time_unit(s: &str) -> Option<u64> {
    let s = s.trim();
    let num_end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    let mult: u64 = s[..num_end].parse().ok()?;
    if !matches!(mult, 1 | 10 | 100) {
        return None;
    }
    let unit = s[num_end..].trim();
    let base: u64 = match unit {
        "s" => 1_000_000_000_000_000,
        "ms" => 1_000_000_000_000,
        "us" => 1_000_000_000,
        "ns" => 1_000_000,
        "ps" => 1_000,
        "fs" => 1,
        _ => return None,
    };
    Some(mult * base)
}

fn parse_line_directive(rest: &str) -> Option<(u32, String)> {
    let mut parts = rest.splitn(2, char::is_whitespace);
    let num: u32 = parts.next()?.trim().parse().ok()?;
    let remainder = parts.next()?.trim();
    let q1 = remainder.find('"')?;
    let q2 = remainder[q1 + 1..].find('"')?;
    let fname = remainder[q1 + 1..q1 + 1 + q2].to_string();
    Some((num, fname))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_timescale_ifdef_and_macros() {
        let mut pp = Preprocessor::new();
        let src = r#"
`timescale 1ns / 100ps
`define WIDTH 8
`define ADD(a, b) ((a) + (b))
`ifdef WIDTH
wire [`WIDTH-1:0] x = `ADD(1, 2);
`else
wire bad;
`endif
"#;
        let out = pp.process("test.sv", src).unwrap();
        assert_eq!(out.timescale.unit_fs, 1_000_000);
        assert_eq!(out.timescale.precision_fs, 100_000);
        assert_eq!(out.timescale.ticks_per_ns(), 10);
        assert!(out.text.contains("wire [8-1:0] x = ((1) + (2));"));
        assert!(!out.text.contains("bad"));
    }
}
