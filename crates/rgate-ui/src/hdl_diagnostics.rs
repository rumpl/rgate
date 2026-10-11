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
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_source_lines() {
        let ds = parse(
            "Error: compiler\nalu.sv:12: syntax error\nalu.sv:13: Invalid module instantiation",
        );
        assert_eq!(ds.len(), 2);
        assert_eq!(ds[0].line, 12);
        assert!(parse("unknown net").is_empty());
    }
}

#[cfg(all(test, feature = "icarus"))]
mod compiler_tests {
    use super::*;
    #[test]
    fn compiler_identifies_body_typo_instead_of_module_header() {
        let source = include_str!("../../../examples/verilog/led-chaser.sv");
        assert!(rgate_hdl::check_sources(&[("led_chaser".into(), source.into())]).is_ok());
        let invalid = source.replacen("end else if (enable)", "nd else if (enable)", 1);
        let error = rgate_hdl::check_sources(&[("led_chaser".into(), invalid)])
            .unwrap_err()
            .to_string();
        let errors = parse(&error);
        assert!(errors.iter().any(|d| d.line == 17), "{error}");
        assert!(errors.iter().all(|d| d.line != 3));
    }
}
