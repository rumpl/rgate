use anyhow::{Context, Result, bail};
use rgate_core::demo;
use std::path::PathBuf;

fn main() -> Result<()> {
    let mut arguments = std::env::args().skip(1);
    let argument = arguments.next();
    if argument
        .as_deref()
        .is_some_and(|argument| argument == "--help" || argument == "-h")
    {
        println!(
            "RGate — Rust / GPUI digital circuit editor\n\nUsage:\n  rgate [circuit.rgate | tkgate.v]\n  rgate --simulate <file> [nanoseconds]\n  rgate --export <input> <output.v | output.rgate>\n\nWith no file, opens a working full-adder example.\nOpen .rgate files from the examples directory with File → Open."
        );
        return Ok(());
    }
    if argument.as_deref() == Some("--simulate") {
        let path = PathBuf::from(
            arguments
                .next()
                .context("--simulate requires a circuit file")?,
        );
        let time = arguments
            .next()
            .map(|time| time.parse::<u64>())
            .transpose()?
            .unwrap_or(200);
        let result = rgate_format::load(&path)?;
        for warning in result.warnings {
            eprintln!("Warning: {warning}");
        }
        let module = result
            .circuit
            .module(&result.circuit.root)
            .context("missing root module")?;
        let mut simulator =
            rgate_sim::Simulator::from_circuit(&result.circuit, &result.circuit.root)?;
        for warning in simulator.warnings() {
            eprintln!("Warning: {warning}");
        }
        simulator.advance(time)?;
        println!("{} — {} ns", result.circuit.title, simulator.time());
        for net in &module.nets {
            println!("{:<24} {}", net.name, simulator.value(net.id).unwrap());
        }
        return Ok(());
    }
    if argument.as_deref() == Some("--export") {
        let input = PathBuf::from(
            arguments
                .next()
                .context("--export requires an input file")?,
        );
        let output = PathBuf::from(
            arguments
                .next()
                .context("--export requires an output file")?,
        );
        let result = rgate_format::load(&input)?;
        rgate_format::save(&output, &result.circuit)?;
        println!("Saved {}", output.display());
        return Ok(());
    }
    if arguments.next().is_some() {
        bail!("unexpected extra argument; use --help");
    }
    let (circuit, path, warnings) = if let Some(argument) = argument {
        if argument.starts_with('-') {
            bail!("unknown option {argument}; use --help");
        }
        let path = PathBuf::from(argument);
        let result = rgate_format::load(&path)?;
        (result.circuit, Some(path), result.warnings)
    } else {
        (demo::full_adder(), None, Vec::new())
    };
    rgate_ui::run(circuit, path, warnings)
}
