mod component_verilog;
pub mod native;
pub mod tkgate;
pub mod verilog;

use anyhow::{Context, Result};
use rgate_core::Circuit;
use std::{fs, path::Path};

pub use tkgate::ImportResult;
pub use verilog::export_interactive;

pub fn parse(source: &str) -> Result<ImportResult> {
    if source.trim_start().starts_with('{') {
        return Ok(ImportResult {
            circuit: native::decode(source)?,
            warnings: Vec::new(),
        });
    }
    if let Some(circuit) = verilog::embedded_document(source)? {
        return Ok(ImportResult {
            circuit,
            warnings: Vec::new(),
        });
    }
    tkgate::import(source)
}

pub fn load(path: &Path) -> Result<ImportResult> {
    let bytes = fs::read(path).with_context(|| format!("could not read {}", path.display()))?;
    // TkGate 2 ships examples in ISO-8859-1 as well as UTF-8.
    let source = String::from_utf8(bytes)
        .unwrap_or_else(|error| error.into_bytes().into_iter().map(char::from).collect());
    parse(&source).with_context(|| format!("could not load {}", path.display()))
}

pub fn save(path: &Path, circuit: &Circuit) -> Result<()> {
    let content = if path.extension().is_some_and(|extension| extension == "v") {
        verilog::export(circuit)?
    } else {
        native::encode(circuit)?
    };
    native::save_atomic(path, &content)
}

#[cfg(test)]
mod tests {
    #[test]
    fn source_modules_roundtrip_native_and_metadata_export() {
        let circuit = super::parse(include_str!("../../../examples/pwm-verilog.rgate"))
            .unwrap()
            .circuit;
        let source = circuit.module("pwm").unwrap().verilog.as_ref().unwrap();
        let exported = super::verilog::export(&circuit).unwrap();
        assert!(exported.contains(source));
        assert_eq!(super::parse(&exported).unwrap().circuit, circuit);
        let interactive = super::export_interactive(&circuit).unwrap();
        assert!(interactive.contains("__rgate_input_2"));
    }
    #[test]
    fn shipped_nested_adder_matches_demo_and_round_trips() {
        let circuit = super::parse(include_str!("../../../examples/hierarchical-adder8.rgate"))
            .unwrap()
            .circuit;
        assert_eq!(circuit, rgate_core::demo::hierarchical_adder());
        let exported = super::verilog::export(&circuit).unwrap();
        assert_eq!(super::parse(&exported).unwrap().circuit, circuit);
    }

    #[test]
    fn shipped_full_adder_matches_startup_graph() {
        let source = include_str!("../../../examples/full-adder.rgate");
        assert_eq!(
            super::parse(source).unwrap().circuit,
            rgate_core::demo::full_adder()
        );
    }
}
