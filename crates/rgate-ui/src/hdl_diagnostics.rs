//! Compiler source locations, shared by Messages and editor annotations.
#[derive(Clone, Debug, PartialEq)]
pub struct HdlDiagnostic {
    pub module: String,
    pub line: u32,
    pub message: String,
}
pub fn parse(message: &str) -> Vec<HdlDiagnostic> {
    message
        .lines()
        .filter_map(|line| {
            let marker = line.find(".sv:")?;
            let file = &line[..marker + 3];
            let rest = &line[marker + 4..];
            let module = file
                .rsplit('/')
                .next()?
                .split_whitespace()
                .last()?
                .strip_suffix(".sv")?;
            let (number, message) = rest.split_once(":")?;
            let line = number.parse::<u32>().ok()?;
            Some(HdlDiagnostic {
                module: module.into(),
                line: line.max(1),
                message: message.trim().into(),
            })
        })
        .collect()
}
