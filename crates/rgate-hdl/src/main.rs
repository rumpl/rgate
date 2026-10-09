use rgate_hdl::HdlProject;
use rgate_sim::SimulationBackend;
use std::path::Path;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    let xezim = args.first().is_some_and(|arg| arg == "--xezim");
    if xezim {
        args.remove(0);
    }
    if args.is_empty() || args == ["--help"] {
        println!(
            "Experimental HDL evaluation (native only)\nUsage: rgate-hdl [--xezim] <project.json> [--vcd <output.vcd>]\nWARNING: trusted HDL only; system tasks can write files or invoke commands.\nDefault: persistent Icarus VPI runtime. --xezim: replay with 1 ns quantum step."
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
    let mut backend: Box<dyn SimulationBackend> = if xezim {
        #[cfg(feature = "xezim")]
        {
            Box::new(project.instantiate(directory)?)
        }
        #[cfg(not(feature = "xezim"))]
        {
            return Err("enable the xezim feature for --xezim".into());
        }
    } else {
        Box::new(project.instantiate_icarus(directory)?)
    };
    project.execute(&mut *backend)?;
    println!(
        "{} — {} ns, checks passed ({:.3}s wall time; {})",
        project.top,
        backend.time(),
        start.elapsed().as_secs_f64(),
        if xezim {
            "xezim replay"
        } else {
            "persistent Icarus VPI"
        }
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
