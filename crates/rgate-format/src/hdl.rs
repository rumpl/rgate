//! Source definitions and a conservative, compiler-independent port reader.
//! Unresolved widths are errors: silently guessing a bus width corrupts wiring.
use anyhow::{Context, Result, bail};
use rgate_core::{Direction, Module, Net, NetId};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct Definition {
    pub name: String,
    pub source: String,
    pub ports: Vec<Net>,
}

#[derive(Clone, Debug)]
struct Token {
    text: String,
    start: usize,
    end: usize,
}

fn tokens(source: &str) -> Result<Vec<Token>> {
    let bytes = source.as_bytes();
    let mut result = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if source[i..].starts_with("//") {
            i = source[i..].find('\n').map_or(bytes.len(), |n| i + n);
            continue;
        }
        if source[i..].starts_with("/*") {
            i += 2;
            i += source[i..].find("*/").context("unterminated comment")? + 2;
            continue;
        }
        let start = i;
        if bytes[i] == b'"' {
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i = (i + 2).min(bytes.len());
                } else if bytes[i] == b'"' {
                    i += 1;
                    break;
                } else {
                    i += 1;
                }
            }
        } else if bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'$' {
            i += 1;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || b"_$'".contains(&bytes[i]))
            {
                i += 1;
            }
        } else {
            let ch = source[i..].chars().next().unwrap();
            i += ch.len_utf8();
        }
        result.push(Token {
            text: source[start..i].into(),
            start,
            end: i,
        });
    }
    Ok(result)
}
fn identifier(s: &str) -> bool {
    let mut chars = s.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}
fn direction(s: &str) -> Option<Direction> {
    match s {
        "input" => Some(Direction::Input),
        "output" => Some(Direction::Output),
        "inout" => Some(Direction::InOut),
        _ => None,
    }
}
fn matching(ts: &[Token], start: usize, open: &str, close: &str) -> Result<usize> {
    let mut depth = 0;
    for (i, t) in ts.iter().enumerate().skip(start) {
        if t.text == open {
            depth += 1;
        }
        if t.text == close {
            depth -= 1;
            if depth == 0 {
                return Ok(i);
            }
        }
    }
    bail!("unclosed {open}")
}
fn split<'a>(ts: &'a [Token], separator: &str) -> Vec<&'a [Token]> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut depth = 0;
    for (i, t) in ts.iter().enumerate() {
        match t.text.as_str() {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" => depth -= 1,
            _ => {}
        }
        if depth == 0 && t.text == separator {
            result.push(&ts[start..i]);
            start = i + 1;
        }
    }
    result.push(&ts[start..]);
    result
}
fn number(s: &str) -> Result<i64> {
    let s = s.replace('_', "");
    if let Some((_, literal)) = s.split_once('\'') {
        let literal = literal.trim_start_matches(['s', 'S']);
        let (base, digits) = literal.split_at(1);
        let radix = match base.to_ascii_lowercase().as_str() {
            "h" => 16,
            "b" => 2,
            "o" => 8,
            "d" => 10,
            _ => bail!("invalid constant {s}"),
        };
        return Ok(i64::from_str_radix(digits, radix)?);
    }
    Ok(s.parse()?)
}
struct Expr<'a> {
    ts: &'a [Token],
    pos: usize,
    params: &'a BTreeMap<String, i64>,
}
impl Expr<'_> {
    fn parse(&mut self, min: u8) -> Result<i64> {
        let t = self
            .ts
            .get(self.pos)
            .context("expected constant expression")?
            .text
            .clone();
        self.pos += 1;
        let mut lhs = match t.as_str() {
            "+" => self.parse(3)?,
            "-" => self.parse(3)?.checked_neg().context("constant overflow")?,
            "(" => {
                let n = self.parse(0)?;
                if self.ts.get(self.pos).is_none_or(|t| t.text != ")") {
                    bail!("expected )");
                }
                self.pos += 1;
                n
            }
            "$clog2" => {
                if self.ts.get(self.pos).is_none_or(|t| t.text != "(") {
                    bail!("expected $clog2 argument");
                }
                self.pos += 1;
                let n = self.parse(0)?;
                if n <= 0 {
                    bail!("$clog2 requires a positive value");
                }
                if self.ts.get(self.pos).is_none_or(|t| t.text != ")") {
                    bail!("expected )");
                }
                self.pos += 1;
                i64::from(64 - (n as u64 - 1).leading_zeros())
            }
            _ => self.params.get(&t).copied().map(Ok).unwrap_or_else(|| {
                number(&t).with_context(|| format!("unresolved constant {t}"))
            })?,
        };
        while let Some(op) = self.ts.get(self.pos).map(|t| t.text.as_str()) {
            let prec = match op {
                "+" | "-" => 1,
                "*" | "/" | "%" => 2,
                _ => break,
            };
            if prec < min {
                break;
            }
            self.pos += 1;
            let rhs = self.parse(prec + 1)?;
            lhs = match op {
                "+" => lhs.checked_add(rhs),
                "-" => lhs.checked_sub(rhs),
                "*" => lhs.checked_mul(rhs),
                "/" => lhs.checked_div(rhs),
                "%" => lhs.checked_rem(rhs),
                _ => None,
            }
            .context("constant overflow or division by zero")?;
        }
        Ok(lhs)
    }
}
fn eval(ts: &[Token], params: &BTreeMap<String, i64>) -> Result<i64> {
    let mut expr = Expr { ts, pos: 0, params };
    let n = expr.parse(0)?;
    if expr.pos != ts.len() {
        bail!("unsupported constant expression");
    }
    Ok(n)
}
fn parameters(ts: &[Token], params: &mut BTreeMap<String, i64>) -> Result<()> {
    for part in split(ts, ",") {
        if part.is_empty() {
            continue;
        }
        let eq = part
            .iter()
            .position(|t| t.text == "=")
            .context("parameter needs a default")?;
        let name = &part
            .get(eq.wrapping_sub(1))
            .context("missing parameter name")?
            .text;
        params.insert(name.clone(), eval(&part[eq + 1..], params)?);
    }
    Ok(())
}
fn declaration(
    ts: &[Token],
    params: &BTreeMap<String, i64>,
    inherited: Option<(Direction, u16)>,
) -> Result<(String, Direction, u16)> {
    let mut i = 0;
    let explicit = ts.first().and_then(|t| direction(&t.text));
    let (dir, mut width) = explicit
        .map(|d| (d, 1))
        .or(inherited)
        .context("port direction missing")?;
    if explicit.is_some() {
        i += 1;
    }
    while ts.get(i).is_some_and(|t| {
        matches!(
            t.text.as_str(),
            "wire" | "reg" | "logic" | "bit" | "signed" | "unsigned" | "var" | "tri"
        )
    }) {
        i += 1;
    }
    if ts.get(i).is_some_and(|t| t.text == "[") {
        let end = matching(ts, i, "[", "]")?;
        let range = &ts[i + 1..end];
        let colon = range
            .iter()
            .position(|t| t.text == ":")
            .context("packed range needs :")?;
        let high = eval(&range[..colon], params)?;
        let low = eval(&range[colon + 1..], params)?;
        let n = high
            .abs_diff(low)
            .checked_add(1)
            .context("range overflow")?;
        if !(1..=4096).contains(&n) {
            bail!("port width must be 1–4096 bits");
        }
        width = n as u16;
        i = end + 1;
    }
    let name = ts.get(i).context("missing port name")?.text.clone();
    if !identifier(&name) {
        bail!("unsupported port name {name}");
    }
    i += 1;
    if ts.get(i).is_some_and(|t| t.text != "=") {
        bail!("unsupported declaration for {name} (use scalar or packed-vector ports)");
    }
    Ok((name, dir, width))
}

pub fn definitions(source: &str) -> Result<Vec<Definition>> {
    if source.len() > 4_000_000 {
        bail!("Verilog source exceeds 4 MB");
    }
    let ts = tokens(source)?;
    let mut result = Vec::new();
    let mut cursor = 0;
    while cursor < ts.len() {
        if ts[cursor].text != "module" {
            cursor += 1;
            continue;
        }
        let begin = cursor;
        cursor += 1;
        if ts
            .get(cursor)
            .is_some_and(|t| matches!(t.text.as_str(), "automatic" | "static"))
        {
            cursor += 1;
        }
        let name = ts.get(cursor).context("missing module name")?.text.clone();
        if !identifier(&name) {
            bail!("unsupported module name {name}");
        }
        cursor += 1;
        let mut params = BTreeMap::new();
        if ts.get(cursor).is_some_and(|t| t.text == "#") {
            cursor += 1;
            if ts.get(cursor).is_none_or(|t| t.text != "(") {
                bail!("expected parameter list");
            }
            let end = matching(&ts, cursor, "(", ")")?;
            parameters(&ts[cursor + 1..end], &mut params)?;
            cursor = end + 1;
        }
        let ports_start = cursor;
        let ports_end = if ts.get(cursor).is_some_and(|t| t.text == "(") {
            let end = matching(&ts, cursor, "(", ")")?;
            cursor = end + 1;
            Some(end)
        } else {
            None
        };
        if ts.get(cursor).is_none_or(|t| t.text != ";") {
            bail!("incomplete header for {name}");
        }
        cursor += 1;
        let body_start = cursor;
        let end = ts[cursor..]
            .iter()
            .position(|t| t.text == "endmodule")
            .map(|i| i + cursor)
            .context("missing endmodule")?;
        for (i, t) in ts[body_start..end].iter().enumerate() {
            if matches!(t.text.as_str(), "parameter" | "localparam") {
                let start = body_start + i;
                let stop = ts[start..end]
                    .iter()
                    .position(|t| t.text == ";")
                    .context("incomplete parameter")?
                    + start;
                parameters(&ts[start..stop], &mut params)?;
            }
        }
        let mut ports = Vec::new();
        if let Some(ports_end) = ports_end {
            let header = &ts[ports_start + 1..ports_end];
            let ansi = header.iter().any(|t| direction(&t.text).is_some());
            let mut inherited = None;
            let mut declarations = BTreeMap::new();
            if !ansi {
                for (i, t) in ts[body_start..end].iter().enumerate() {
                    if direction(&t.text).is_some() {
                        let start = body_start + i;
                        let stop = ts[start..end]
                            .iter()
                            .position(|t| t.text == ";")
                            .context("incomplete port declaration")?
                            + start;
                        let mut inherit = None;
                        for part in split(&ts[start..stop], ",") {
                            let (n, d, w) = declaration(part, &params, inherit)?;
                            inherit = Some((d, w));
                            declarations.insert(n, (d, w));
                        }
                    }
                }
            }
            for part in split(header, ",") {
                if part.is_empty() {
                    continue;
                }
                let (n, d, w) = if ansi {
                    let item = declaration(part, &params, inherited)?;
                    inherited = Some((item.1, item.2));
                    item
                } else {
                    if part.len() != 1 {
                        bail!("unsupported non-ANSI port expression");
                    }
                    let n = part[0].text.clone();
                    let (d, w) = declarations
                        .get(&n)
                        .copied()
                        .with_context(|| format!("missing direction for {n}"))?;
                    (n, d, w)
                };
                if ports.iter().any(|p: &Net| p.name == n) {
                    bail!("duplicate port {n}");
                }
                let mut net = Net::new(NetId(ports.len() as u64 + 1), n, w);
                net.port = Some(d);
                ports.push(net);
            }
        }
        if result.iter().any(|d: &Definition| d.name == name) {
            bail!("duplicate source module {name}");
        }
        let mut finish = ts[end].end;
        if ts.get(end + 1).is_some_and(|t| t.text == ":") {
            finish = ts.get(end + 2).context("missing end label")?.end;
            cursor = end + 3;
        } else {
            cursor = end + 1;
        }
        // Keep directives at file scope with the first definition. Other modules
        // start at their own declaration so imported source isn't duplicated.
        let start = if result.is_empty() {
            0
        } else {
            ts[begin].start
        };
        result.push(Definition {
            name,
            source: source[start..finish].into(),
            ports,
        });
    }
    if result.is_empty() {
        bail!("no Verilog module definitions found");
    }
    Ok(result)
}

pub fn inferred_nets(source: &str, name: &str, existing: &[Net]) -> Result<Vec<Net>> {
    let definition = definitions(source)?
        .into_iter()
        .find(|d| d.name == name)
        .with_context(|| format!("source must declare module {name}"))?;
    let mut next = existing.iter().map(|n| n.id.0).max().unwrap_or(0) + 1;
    let mut nets = definition.ports;
    for net in &mut nets {
        net.id = existing.iter().find(|n| n.name == net.name).map_or_else(
            || {
                let id = NetId(next);
                next += 1;
                id
            },
            |n| n.id,
        );
    }
    nets.extend(
        existing
            .iter()
            .filter(|n| n.port.is_none() && !nets.iter().any(|p| p.name == n.name))
            .cloned()
            .collect::<Vec<_>>(),
    );
    Ok(nets)
}

pub fn module_from_definition(definition: Definition) -> Module {
    let mut module = Module::new(definition.name);
    module.verilog = Some(definition.source);
    module.nets = definition.ports;
    module
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ansi_ports_and_parameters() {
        let source = "module demo #(parameter W=8, parameter N=$clog2(W))(input logic clk, reset, input [W-1:0] a,b, output signed [0:W-1] y, inout [N-1:0] bus); endmodule";
        let d = definitions(source).unwrap().remove(0);
        assert_eq!(
            d.ports
                .iter()
                .map(|p| (p.name.as_str(), p.width))
                .collect::<Vec<_>>(),
            vec![
                ("clk", 1),
                ("reset", 1),
                ("a", 8),
                ("b", 8),
                ("y", 8),
                ("bus", 3)
            ]
        );
    }
    #[test]
    fn old_style_and_comments() {
        let source = "// module fake\nmodule test(a,b,y); parameter W=4; input [W*2-1:0] a,b; output [7:0] y; endmodule";
        let d = definitions(source).unwrap().remove(0);
        assert_eq!(d.ports.len(), 3);
        assert!(d.ports.iter().all(|p| p.width == 8));
    }
    #[test]
    fn unknown_width_is_error() {
        assert!(definitions("module x(input [MISSING-1:0] a);endmodule").is_err());
        assert!(definitions("module x(input a[4]);endmodule").is_err());
    }
    #[test]
    fn preserves_ids_and_observations() {
        let mut old = Net::new(NetId(90), "clk", 1);
        old.port = Some(Direction::Input);
        let internal = Net::new(NetId(91), "state", 8);
        let nets = inferred_nets(
            "module x(input clk,output [7:0] q);endmodule",
            "x",
            &[old, internal],
        )
        .unwrap();
        assert_eq!(nets[0].id, NetId(90));
        assert_eq!(nets[1].id, NetId(92));
        assert_eq!(nets[2].id, NetId(91));
    }
    #[test]
    fn imported_sources_are_separate() {
        let ds =
            definitions("`timescale 1ns/1ps\nmodule a; b child();endmodule\nmodule b;endmodule")
                .unwrap();
        assert_eq!(ds.len(), 2);
        assert!(!ds[0].source.contains("module b;"));
        assert!(ds[0].source.contains("`timescale"));
    }
}

/// Import a source file as editable definitions; the caller may choose any root.
pub fn import_source(source: &str) -> Result<crate::ImportResult> {
    let definitions = definitions(source)?;
    let root = definitions[0].name.clone();
    let circuit = rgate_core::Circuit {
        title: format!("Verilog: {root}"),
        root,
        modules: definitions
            .into_iter()
            .map(module_from_definition)
            .collect(),
    };
    circuit.validate()?;
    Ok(crate::ImportResult {
        circuit,
        warnings: Vec::new(),
    })
}

#[cfg(test)]
mod import_tests {
    #[test]
    fn imports_multiple_definitions_and_roundtrips() {
        let result=crate::parse("module top(input a,output y);child u(a,y);endmodule\nmodule child(input a,output y);assign y=~a;endmodule").unwrap();
        assert_eq!(result.circuit.modules.len(), 2);
        assert_eq!(result.circuit.root, "top");
        assert_eq!(
            crate::native::decode(&crate::native::encode(&result.circuit).unwrap()).unwrap(),
            result.circuit
        );
    }
}
