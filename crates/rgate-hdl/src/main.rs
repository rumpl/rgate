use rgate_hdl::HdlProject;
use rgate_sim::SimulationBackend;
use std::path::Path;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() || args == ["--help"] {
        println!(
            "Icarus Verilog runner (native only)\nUsage: rgate-hdl <project.json> [--vcd <output.vcd>]\nWARNING: trusted HDL only; system tasks can write files or invoke commands."
        );
        return Ok(());
    }
    if args.len() != 1 && !(args.len() == 3 && args[1] == "--vcd") {
        return Err("expected a project file and optional --vcd output".into());
    }
    let path = Path::new(&args[0]);
    let project = HdlProject::load(path)?;
    let directory = path.parent().unwrap_or(Path::new("."));
    let start = std::time::Instant::now();
    let mut backend = project.instantiate(directory)?;
    project.execute(&mut backend)?;
    println!(
        "{} — {} ns, checks passed ({:.3}s wall time; persistent Icarus VPI)",
        project.top,
        backend.time(),
        start.elapsed().as_secs_f64()
    );
    for (index, signal) in project.signals.iter().enumerate() {
        println!(
            "{:<32} {}",
            signal.path,
            backend.value(rgate_core::NetId(index as u64 + 1)).unwrap()
        );
    }
    if args.len() == 3 {
        std::fs::write(
            &args[2],
            rgate_sim::export_vcd(
                &backend.traces().values().cloned().collect::<Vec<_>>(),
                backend.time(),
            )?,
        )?;
        println!("Saved {}", args[2]);
    }
    Ok(())
}
