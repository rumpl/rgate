//! Owns Verilog background compilation, debounced source linting, and HDL diagnostics.
use crate::commands::Command;
use crate::hdl_diagnostics::HdlDiagnostic;
#[cfg(not(target_family = "wasm"))]
use crate::live_simulation::LiveSimulation;
#[cfg(not(target_family = "wasm"))]
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

#[cfg(not(target_family = "wasm"))]
struct Compilation {
    cancel: Arc<AtomicBool>,
    result: mpsc::Receiver<Result<LiveSimulation, rgate_sim::SimError>>,
    circuit: rgate_core::Circuit,
    command: Command,
}

#[cfg(not(target_family = "wasm"))]
impl Drop for Compilation {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

#[cfg(not(target_family = "wasm"))]
pub struct CompletedCompilation {
    pub command: Command,
    pub result: Result<LiveSimulation, rgate_sim::SimError>,
}

/// Self-contained component owning HDL diagnostics, background compilation, and debounced linting.
#[derive(Default)]
pub struct HdlCompiler {
    #[cfg(not(target_family = "wasm"))]
    compilation: Option<Compilation>,
    #[cfg(not(target_family = "wasm"))]
    lint_due: Option<std::time::Instant>,
    #[cfg(not(target_family = "wasm"))]
    lint_result: Option<(
        rgate_core::Circuit,
        mpsc::Receiver<Result<(), rgate_sim::SimError>>,
    )>,
    #[cfg_attr(target_family = "wasm", allow(dead_code))]
    pub background_compilation: bool,
    pub diagnostics: Vec<HdlDiagnostic>,
    #[cfg_attr(target_family = "wasm", allow(dead_code))]
    pub diagnostic_line: Option<u32>,
}

impl HdlCompiler {
    pub fn is_compiling(&self) -> bool {
        #[cfg(not(target_family = "wasm"))]
        {
            self.compilation.is_some()
        }
        #[cfg(target_family = "wasm")]
        {
            false
        }
    }

    pub fn record_error_diagnostics(&mut self, message: &str) -> bool {
        self.diagnostics = crate::hdl_diagnostics::parse(message);
        !self.diagnostics.is_empty()
    }

    pub fn clear_diagnostics(&mut self) {
        self.diagnostics.clear();
    }

    pub fn cancel(&mut self) -> bool {
        #[cfg(not(target_family = "wasm"))]
        {
            self.compilation.take().is_some()
        }
        #[cfg(target_family = "wasm")]
        {
            false
        }
    }

    #[cfg(not(target_family = "wasm"))]
    pub fn schedule_lint(&mut self) {
        self.diagnostics.clear();
        self.lint_due = Some(std::time::Instant::now() + std::time::Duration::from_millis(350));
    }

    #[cfg(not(target_family = "wasm"))]
    pub fn should_compile_in_background(
        &self,
        sim_active: bool,
        circuit: &rgate_core::Circuit,
        command: &Command,
    ) -> bool {
        self.background_compilation
            && !sim_active
            && circuit.modules.iter().any(|m| m.verilog.is_some())
            && matches!(
                command,
                Command::PlayPause | Command::Step | Command::ClockStep
            )
    }

    #[cfg(not(target_family = "wasm"))]
    pub fn start(
        &mut self,
        circuit: rgate_core::Circuit,
        root: String,
        command: Command,
    ) -> bool {
        if self.compilation.is_some() {
            return false;
        }
        self.lint_due = None;
        self.lint_result = None;
        let cancel = Arc::new(AtomicBool::new(false));
        let token = cancel.clone();
        let snapshot = circuit.clone();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let result = LiveSimulation::verilog_cancellable(&snapshot, &root, &token);
            let _ = sender.send(result);
        });
        // Drop cancels compilation when the document changes or the window closes.
        self.compilation = Some(Compilation {
            cancel,
            result: receiver,
            circuit,
            command,
        });
        true
    }

    #[cfg(not(target_family = "wasm"))]
    pub fn poll_compile(&mut self, current_circuit: &rgate_core::Circuit) -> Option<CompletedCompilation> {
        let pending = self.compilation.as_ref()?;
        let result = match pending.result.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => Err(rgate_sim::SimError::Invalid(
                "Compiler worker ended unexpectedly".into(),
            )),
        };
        let pending = self.compilation.take().unwrap();
        if current_circuit != &pending.circuit {
            return None;
        }
        Some(CompletedCompilation {
            command: pending.command.clone(),
            result,
        })
    }

    /// Returns `true` if diagnostics changed and the UI should re-render.
    #[cfg(not(target_family = "wasm"))]
    pub fn poll_lint(&mut self, current_circuit: &rgate_core::Circuit, sim_active: bool) -> bool {
        let mut changed = false;
        if let Some((snapshot, receiver)) = &self.lint_result {
            match receiver.try_recv() {
                Ok(result) => {
                    let matches = current_circuit == snapshot;
                    self.lint_result = None;
                    if matches {
                        self.diagnostics = match result {
                            Ok(()) => Vec::new(),
                            Err(error) => crate::hdl_diagnostics::parse(&error.to_string()),
                        };
                        changed = true;
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.lint_result = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.lint_result.is_none()
            && !sim_active
            && self.compilation.is_none()
            && self
                .lint_due
                .is_some_and(|due| std::time::Instant::now() >= due)
        {
            self.lint_due = None;
            let snapshot = current_circuit.clone();
            let sources = snapshot
                .modules
                .iter()
                .filter_map(|m| {
                    m.verilog
                        .as_ref()
                        .map(|source| (m.name.clone(), source.clone()))
                })
                .collect::<Vec<_>>();
            let (sender, receiver) = mpsc::channel();
            std::thread::spawn(move || {
                let _ = sender.send(rgate_hdl::check_sources(&sources));
            });
            self.lint_result = Some((snapshot, receiver));
        }
        changed
    }
}
