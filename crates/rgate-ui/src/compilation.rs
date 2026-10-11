//! Own a compile request independently of the UI and discard stale results.
use crate::{
    app::{GateApp, WorkspaceTab},
    commands::Command,
    live_simulation::LiveSimulation,
};
use gpui::{Context, Window};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

pub struct Compilation {
    cancel: Arc<AtomicBool>,
    result: mpsc::Receiver<Result<LiveSimulation, rgate_sim::SimError>>,
    circuit: rgate_core::Circuit,
    command: Command,
}
impl Drop for Compilation {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
impl GateApp {
    pub fn start_compilation(&mut self, command: Command, cx: &mut Context<Self>) {
        if self.compilation.is_some() {
            return;
        }
        if let Err(error) = self.refresh_verilog_ports() {
            self.error(error);
            return;
        }
        let circuit = self.editor.circuit().clone();
        let root = self.editor.active_module().to_owned();
        let cancel = Arc::new(AtomicBool::new(false));
        let token = cancel.clone();
        let snapshot = circuit.clone();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let result = LiveSimulation::verilog_cancellable(&snapshot, &root, &token);
            let _ = sender.send(result);
        });
        // Move compilation state into the UI only after the worker owns its snapshot.
        // Drop cancels compilation when the document changes or the window closes.
        // The receiver is polled by the existing UI timer.
        self.compilation = Some(Compilation {
            cancel,
            result: receiver,
            circuit,
            command,
        });
        self.running = false;
        self.log("Compiling Verilog… Stop cancels.".into());
        cx.notify();
    }
    pub fn poll_compilation(&mut self, cx: &mut Context<Self>) {
        let Some(pending) = &self.compilation else {
            return;
        };
        let result = match pending.result.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Err(rgate_sim::SimError::Invalid(
                "Compiler worker ended unexpectedly".into(),
            )),
        };
        let pending = self.compilation.take().unwrap();
        if self.editor.circuit() != &pending.circuit {
            return;
        }
        match result {
            Ok(mut sim) => {
                for net in &self.probes {
                    if let Err(error) = sim.probe(*net) {
                        self.messages.push(error.to_string());
                    }
                }
                self.hdl_diagnostics.clear();
                self.debug_path = Some(sim.root_path().into());
                self.simulation = Some(sim);
                self.tab = WorkspaceTab::Simulate;
                self.bottom_scope = true;
                self.running = matches!(pending.command, Command::PlayPause);
                self.restore_saved_probes();
                if matches!(pending.command, Command::Step | Command::ClockStep) {
                    let sim = self.simulation.as_mut().unwrap();
                    let result = if matches!(pending.command, Command::Step) {
                        sim.step()
                    } else {
                        sim.advance(sim.clock_period())
                    };
                    if let Err(error) = result {
                        self.error(error);
                    }
                }
                self.log("Verilog ready.".into());
            }
            Err(error) => self.error(error),
        }
        cx.notify();
    }
    pub fn cancel_compilation(&mut self) {
        if self.compilation.take().is_some() {
            self.log("Compilation cancelled.".into());
        }
    }
    pub fn compile_before_command(
        &mut self,
        command: &Command,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.background_compilation
            && self.simulation.is_none()
            && self
                .editor
                .circuit()
                .modules
                .iter()
                .any(|m| m.verilog.is_some())
            && matches!(
                command,
                Command::PlayPause | Command::Step | Command::ClockStep
            )
        {
            self.start_compilation(command.clone(), cx);
            return true;
        }
        false
    }
}
