pub mod canvas_controller;
mod canvas_input;
mod commands;
pub mod console;
pub mod dialog_host;
mod dialogs;
mod io;
pub mod property_dialog;
pub mod sim_session;
mod simulation;

pub use crate::view::layout::{ContextMenu, OpenMenu, WorkspaceTab};
pub use canvas_controller::{CanvasController, Drag};
pub use console::ConsoleLog;
pub use dialog_host::{Dialog, DialogHost, NetDialog, OpenDialog};
pub use property_dialog::PropertyDialog;
pub use sim_session::SimSession;

use crate::{
    commands::Command,
    compilation::HdlCompiler,
    theme::{Appearance, Theme},
    view::layout::ChromeLayout,
    waveform_view::WaveformController,
    workspace::WorkspaceStore,
};
use gpui::{Context, FocusHandle, Task, Window};
use rgate_core::Circuit;
use rgate_editor::Editor;
use std::{path::PathBuf, time::Duration};

pub struct GateApp {
    pub editor: Editor,
    pub file: Option<PathBuf>,
    pub focus: FocusHandle,
    pub theme: Theme,
    pub appearance: Appearance,
    pub sim: SimSession,
    pub hdl: HdlCompiler,
    #[cfg(not(target_family = "wasm"))]
    pub source_editor: Option<crate::source_editor::SourceEditor>,
    pub canvas: CanvasController,
    pub waveform: WaveformController,
    pub console: ConsoleLog,
    pub chrome: ChromeLayout,
    pub dialogs: DialogHost,
    pub workspace: WorkspaceStore,
    pub closing: bool,
    pub prompting: bool,
    timer: Option<Task<()>>,
}

impl GateApp {
    pub fn new(
        circuit: Circuit,
        file: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let waveform = WaveformController::new(window, cx);
        let console = ConsoleLog::new(window, cx);
        let chrome = ChromeLayout::new(window, cx);
        let mut this = Self {
            editor: Editor::new(circuit).expect("startup circuit is validated"),
            file,
            focus,
            theme: Theme::default(),
            appearance: Appearance::default(),
            sim: SimSession::default(),
            hdl: HdlCompiler::default(),
            #[cfg(not(target_family = "wasm"))]
            source_editor: None,
            canvas: CanvasController::default(),
            waveform,
            console,
            chrome,
            dialogs: DialogHost::default(),
            workspace: WorkspaceStore::default(),
            closing: false,
            prompting: false,
            timer: None,
        };
        let timer = cx.spawn(async move |entity, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(40))
                    .await;
                let result = entity.update(cx, |this, cx| {
                    #[cfg(not(target_family = "wasm"))]
                    this.poll_hdl_tasks(cx);
                    if this.sim.running {
                        this.advance_running(cx);
                    }
                    this.persistence_tick();
                });
                if result.is_err() {
                    break;
                }
            }
        });
        this.timer = Some(timer);
        let entity = cx.weak_entity();
        window.on_window_should_close(cx, move |window, cx| {
            entity
                .update(cx, |this, cx| {
                    if this.closing || !this.editor.is_dirty() {
                        this.flush_workspace();
                        true
                    } else {
                        this.command(Command::Quit, window, cx);
                        false
                    }
                })
                .unwrap_or(true)
        });
        this
    }

    pub fn palette(&self) -> crate::theme::Palette {
        self.theme.palette(self.appearance)
    }

    pub fn set_appearance(
        &mut self,
        appearance: Appearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.appearance = appearance;
        crate::view::apply_appearance(appearance, Some(window), cx);
        window.refresh();
    }

    pub fn log(&mut self, message: String) {
        self.console.log(message);
    }

    pub(crate) fn error(&mut self, error: impl std::fmt::Display) {
        let message = error.to_string();
        if self.hdl.record_error_diagnostics(&message) {
            self.chrome.bottom_scope = false;
        }
        self.log(format!("Error: {message}"));
    }

    pub fn changed(&mut self) {
        self.canvas.input_swap = None;
        if self.hdl.cancel() {
            self.log("Compilation cancelled.".into());
        }
        self.sim.reset();
        if self.chrome.tab == WorkspaceTab::Simulate {
            self.chrome.tab = WorkspaceTab::Edit;
        }
    }
}
