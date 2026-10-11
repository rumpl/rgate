//! Persistent Icarus runtime in a child process, controlled at settled VPI boundaries.
use crate::HdlSignal;
use rgate_core::{GateId, Logic, NetId, Signal};
use rgate_sim::{SimError, SimulationBackend, Trace, Transition};
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

#[derive(Clone, Debug)]
pub struct DiscoveredSignal {
    pub path: String,
    pub width: u16,
}

pub struct IcarusBackend {
    child: Child,
    input: ChildStdin,
    output: mpsc::Receiver<String>,
    _directory: tempfile::TempDir,
    signals: Vec<HdlSignal>,
    discovered: Vec<DiscoveredSignal>,
    values: BTreeMap<NetId, Signal>,
    traces: BTreeMap<NetId, Trace>,
    ticks: u64,
    scale: u64,
    failed: bool,
    top: String,
}

impl IcarusBackend {
    pub fn new(sources: Vec<String>, top: &str, signals: Vec<HdlSignal>) -> Result<Self, SimError> {
        Self::new_cancellable(sources, top, signals, &Arc::new(AtomicBool::new(false)))
    }
    pub fn new_cancellable(
        sources: Vec<String>,
        top: &str,
        signals: Vec<HdlSignal>,
        cancel: &Arc<AtomicBool>,
    ) -> Result<Self, SimError> {
        if cancel.load(Ordering::Relaxed) {
            return Err(error("Compilation cancelled"));
        }
        if !identifier(top)
            || signals.len() > 256
            || signals
                .iter()
                .any(|s| s.width == 0 || s.width > 4096 || !s.path.split('.').all(identifier))
        {
            return Err(error(
                "invalid HDL top, signal path, width, or binding count",
            ));
        }
        let mut nets = std::collections::BTreeSet::new();
        let mut gates = std::collections::BTreeSet::new();
        let mut paths = std::collections::BTreeSet::new();
        for signal in &signals {
            if !nets.insert(signal.net)
                || !paths.insert(&signal.path)
                || signal.input.is_some_and(|gate| !gates.insert(gate))
            {
                return Err(error("duplicate HDL binding"));
            }
        }
        let directory = tempfile::tempdir().map_err(|e| error(e.to_string()))?;
        let root = directory.path();
        std::fs::write(
            root.join("rgate_bridge.c"),
            include_str!("../vpi/rgate_bridge.c"),
        )
        .map_err(|e| error(e.to_string()))?;
        run_tool(
            Command::new(tool("iverilog-vpi"))
                .current_dir(root)
                .arg("rgate_bridge.c"),
            cancel,
        )?;
        let mut compiler = Command::new(tool("iverilog"));
        compiler
            .current_dir(root)
            .args(["-g2012", "-s", top, "-o", "design.vvp"]);
        for (index, source) in sources.iter().enumerate() {
            let name = format!("source{index}.sv");
            std::fs::write(root.join(&name), source).map_err(|e| error(e.to_string()))?;
            compiler.arg(name);
        }
        run_tool(&mut compiler, cancel)?;
        if cancel.load(Ordering::Relaxed) {
            return Err(error("Compilation cancelled"));
        }
        let mut child = Command::new(tool("vvp"))
            .current_dir(root)
            .args(["-M.", "-mrgate_bridge", "design.vvp"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| error(format!("start vvp: {e}")))?;
        let input = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, output) = mpsc::sync_channel(4096);
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                match line {
                    Ok(line) => {
                        if sender.send(line).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
        let mut backend = Self {
            child,
            input,
            output,
            _directory: directory,
            signals,
            discovered: Vec::new(),
            values: BTreeMap::new(),
            traces: BTreeMap::new(),
            ticks: 0,
            scale: 1,
            failed: false,
            top: top.into(),
        };
        backend.receive()?;
        backend.request("LIST")?;
        for index in 0..backend.signals.len() {
            let signal = &backend.signals[index];
            let path = format!("{top}.{}", signal.path);
            let discovered = backend
                .discovered
                .iter()
                .find(|s| s.path == path)
                .ok_or_else(|| error(format!("missing HDL signal {path}")))?;
            if discovered.width != signal.width {
                return Err(error(format!(
                    "width mismatch for {path}: expected {}, got {}",
                    signal.width, discovered.width
                )));
            }
            backend.request(&format!("BIND {path}"))?;
        }
        Ok(backend)
    }
    /// Bind a discovered packed signal without recompiling or resetting time.
    pub fn bind_discovered(
        &mut self,
        path: &str,
        input: bool,
    ) -> Result<(NetId, Option<GateId>), SimError> {
        let relative = path
            .strip_prefix(&format!("{}.", self.top))
            .ok_or_else(|| error("signal belongs to a different top"))?;
        if let Some(signal) = self.signals.iter().find(|s| s.path == relative) {
            return Ok((signal.net, signal.input));
        }
        if self.signals.len() >= 256 {
            return Err(error("live signal binding limit is 256"));
        }
        let discovered = self
            .discovered
            .iter()
            .find(|s| s.path == path)
            .ok_or_else(|| error("unknown HDL signal"))?;
        if discovered.width == 0 || discovered.width > 4096 || path.chars().any(char::is_whitespace)
        {
            return Err(error("unsupported signal path or width"));
        }
        let net = NetId(self.signals.iter().map(|s| s.net.0).max().unwrap_or(0) + 1);
        let gate = input.then_some(GateId(3_000_000 + net.0));
        self.signals.push(HdlSignal {
            net,
            path: relative.into(),
            width: discovered.width,
            input: gate,
        });
        if let Err(error) = self.request(&format!("BIND {path}")) {
            self.signals.pop();
            return Err(error);
        }
        Ok((net, gate))
    }
    pub fn unprobe(&mut self, net: NetId) {
        self.traces.remove(&net);
    }
    pub fn label_probe(&mut self, net: NetId, name: String) {
        if let Some(trace) = self.traces.get_mut(&net) {
            trace.name = name;
        }
    }
    pub fn signals(&self) -> &[HdlSignal] {
        &self.signals
    }
    pub fn discovered_signals(&self) -> &[DiscoveredSignal] {
        &self.discovered
    }
    /// Exact engine time for sub-nanosecond stepping; trait time is rounded down.
    pub fn time_ticks(&self) -> u64 {
        self.ticks
    }
    pub fn ticks_per_nanosecond(&self) -> u64 {
        self.scale
    }
    fn request(&mut self, command: &str) -> Result<usize, SimError> {
        if self.failed {
            return Err(error("Icarus runtime is no longer usable"));
        }
        if let Err(e) = writeln!(self.input, "{command}").and_then(|_| self.input.flush()) {
            self.fail();
            return Err(error(format!("VPI write: {e}")));
        }
        match self.receive() {
            Ok(work) => Ok(work),
            Err(e) => {
                self.fail();
                Err(e)
            }
        }
    }
    fn fail(&mut self) {
        self.failed = true;
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
    fn receive(&mut self) -> Result<usize, SimError> {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut work = 0;
        loop {
            let line = self
                .output
                .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
                .map_err(|e| error(format!("VPI response ended or timed out: {e}")))?;
            let Some(message) = line.strip_prefix("RGATE ") else {
                continue;
            };
            if message == "END" {
                return Ok(work);
            }
            let fields = message.split_whitespace().collect::<Vec<_>>();
            match fields.as_slice() {
                ["ERROR", detail] => return Err(error(format!("VPI: {detail}"))),
                ["SCALE", scale] => {
                    self.scale = scale.parse().map_err(|_| error("invalid VPI time scale"))?;
                    if self.scale == 0 {
                        return Err(error("zero VPI time scale"));
                    }
                }
                ["TIME", time] => {
                    self.ticks = time.parse().map_err(|_| error("invalid VPI timestamp"))?
                }
                ["META", width, path] => {
                    let width: u16 = width.parse().map_err(|_| error("unsupported VPI width"))?;
                    self.discovered.push(DiscoveredSignal {
                        path: (*path).into(),
                        width,
                    });
                }
                [kind @ ("VALUE" | "CHANGE"), time, index, bits] => {
                    let ticks: u64 = time.parse().map_err(|_| error("invalid VPI timestamp"))?;
                    let index: usize = index.parse().map_err(|_| error("invalid VPI index"))?;
                    let signal = self
                        .signals
                        .get(index)
                        .ok_or_else(|| error("unbound VPI index"))?;
                    let value = parse_bits(bits, signal.width)?;
                    if *kind == "VALUE" {
                        self.values.insert(signal.net, value);
                    } else if let Some(trace) = self.traces.get_mut(&signal.net)
                        && trace
                            .transitions
                            .last()
                            .is_none_or(|last| last.value != value)
                    {
                        trace.transitions.push(Transition {
                            time: ticks / self.scale,
                            value,
                        });
                        work += 1;
                        if trace.transitions.len() > 4096 {
                            trace.transitions.remove(0);
                        }
                    }
                }
                _ => return Err(error(format!("invalid VPI response {message}"))),
            }
        }
    }
}
impl Drop for IcarusBackend {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl SimulationBackend for IcarusBackend {
    fn time(&self) -> u64 {
        self.ticks / self.scale
    }
    fn advance(&mut self, ns: u64) -> Result<usize, SimError> {
        let delay = ns.checked_mul(self.scale).ok_or(SimError::TimeOverflow)?;
        self.ticks
            .checked_add(delay)
            .ok_or(SimError::TimeOverflow)?;
        self.request(&format!("RUN {ns}"))
    }
    /// Continue the same runtime to its next scheduled timestep, then settle it.
    fn step(&mut self) -> Result<usize, SimError> {
        self.request("STEP")
    }
    fn value(&self, net: NetId) -> Option<&Signal> {
        self.values.get(&net)
    }
    fn set_input(&mut self, gate: GateId, value: Signal) -> Result<(), SimError> {
        let index = self
            .signals
            .iter()
            .position(|s| s.input == Some(gate))
            .ok_or(SimError::NotInput(gate))?;
        if value.width() != self.signals[index].width {
            return Err(error("HDL input width mismatch"));
        }
        let bits = value
            .bits()
            .iter()
            .rev()
            .map(ToString::to_string)
            .collect::<String>();
        self.request(&format!("SET {index} {bits}"))?;
        Ok(())
    }
    fn probe(&mut self, net: NetId) -> Result<(), SimError> {
        if self.traces.contains_key(&net) {
            return Ok(());
        }
        let index = self
            .signals
            .iter()
            .position(|s| s.net == net)
            .ok_or(SimError::UnknownNet(net))?;
        let signal = &self.signals[index];
        self.traces.insert(
            net,
            Trace {
                net,
                name: signal.path.clone(),
                width: signal.width,
                transitions: Vec::new(),
            },
        );
        if let Err(e) = self.request(&format!("PROBE {index}")) {
            self.traces.remove(&net);
            return Err(e);
        }
        Ok(())
    }
    fn traces(&self) -> &BTreeMap<NetId, Trace> {
        &self.traces
    }
}
fn run_tool(command: &mut Command, cancel: &Arc<AtomicBool>) -> Result<(), SimError> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| error(format!("{command:?}: {e}; install icarus-verilog")))?;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let out = std::thread::spawn(move || {
        let mut text = String::new();
        std::io::Read::read_to_string(&mut BufReader::new(stdout), &mut text).map(|_| text)
    });
    let err = std::thread::spawn(move || {
        let mut text = String::new();
        std::io::Read::read_to_string(&mut BufReader::new(stderr), &mut text).map(|_| text)
    });
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    let result = loop {
        if cancel.load(Ordering::Relaxed) || std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            break Err(error(if cancel.load(Ordering::Relaxed) {
                "Compilation cancelled"
            } else {
                "Compiler exceeded 60 seconds"
            }));
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(error(e.to_string()));
            }
        }
    };
    // Drainers own their pipes; cancellation does not wait on a descendant
    // compiler that may retain them. Completed runs collect both diagnostics.
    let status = result?;
    let stdout = out
        .join()
        .map_err(|_| error("compiler stdout thread failed"))?
        .map_err(|e| error(e.to_string()))?;
    let stderr = err
        .join()
        .map_err(|_| error("compiler stderr thread failed"))?
        .map_err(|e| error(e.to_string()))?;
    if !status.success() {
        return Err(error(format!("{command:?}: {stdout}{stderr}")));
    }
    Ok(())
}
fn error(message: impl Into<String>) -> SimError {
    SimError::Invalid(message.into())
}
fn identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}
fn parse_bits(bits: &str, width: u16) -> Result<Signal, SimError> {
    if bits.len() != usize::from(width) {
        return Err(error(format!("VPI width mismatch {bits}")));
    }
    Ok(Signal::from_bits(
        bits.chars()
            .rev()
            .map(|c| match c {
                '0' => Ok(Logic::Low),
                '1' => Ok(Logic::High),
                'x' | 'X' => Ok(Logic::Unknown),
                'z' | 'Z' => Ok(Logic::HighZ),
                _ => Err(error("invalid VPI bit")),
            })
            .collect::<Result<_, _>>()?,
    ))
}

fn tool(name: &str) -> std::path::PathBuf {
    if let Some(paths) = std::env::var_os("PATH") {
        for path in std::env::split_paths(&paths) {
            let candidate = path.join(name);
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    // Finder-launched macOS applications do not inherit Homebrew's shell PATH.
    #[cfg(target_os = "macos")]
    for directory in ["/opt/homebrew/bin", "/usr/local/bin"] {
        let candidate = std::path::Path::new(directory).join(name);
        if candidate.is_file() {
            return candidate;
        }
    }
    name.into()
}

/// Check source on a worker without elaborating or launching simulation.
pub fn check_sources(sources: &[(String, String)]) -> Result<(), SimError> {
    let directory = tempfile::tempdir().map_err(|e| error(e.to_string()))?;
    let mut command = Command::new(tool("iverilog"));
    command
        .current_dir(directory.path())
        .args(["-g2012", "-i", "-tnull"]);
    for (index, (name, source)) in sources.iter().enumerate() {
        let filename = format!("lint{index}.sv");
        let named = format!("`line 1 \"{name}.sv\" 0\n{source}");
        std::fs::write(directory.path().join(&filename), named)
            .map_err(|e| error(e.to_string()))?;
        command.arg(filename);
    }
    run_tool(&mut command, &Arc::new(AtomicBool::new(false)))
}
