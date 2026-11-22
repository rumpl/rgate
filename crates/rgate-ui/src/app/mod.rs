mod canvas_input;
mod commands;
mod dialogs;
mod io;
mod property_dialog;
mod simulation;

pub use dialogs::{Dialog, NetDialog};
pub use property_dialog::PropertyDialog;

use crate::{
    commands::Command,
    live_simulation::LiveSimulation,
    theme::{Appearance, Theme},
};
use gpui::{Bounds, Context, Entity, FocusHandle, Pixels, Subscription, Task, Window};
use gpui_component::input::{InputEvent, InputState, TextareaState};
use gpui_component::menu::PopupMenu;
use rgate_core::{Circuit, GateId, NetId, Point};
use rgate_editor::{Editor, Hit};
use std::{collections::BTreeSet, path::PathBuf, time::Duration};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WorkspaceTab {
    Edit,
    Interface,
    Simulate,
}

pub struct ContextMenu {
    pub position: gpui::Point<Pixels>,
    pub menu: Entity<PopupMenu>,
    pub _dismiss: Subscription,
}

pub struct OpenMenu {
    pub name: &'static str,
    pub menu: Entity<PopupMenu>,
    pub _dismiss: Subscription,
}

/// The GPUI Kit dialog currently mirroring `GateApp::dialog`.
pub struct OpenDialog {
    pub title: &'static str,
    pub width: f32,
}

#[derive(Clone, Debug)]
pub enum Drag {
    FrameResize {
        gate: GateId,
        anchor: Point,
        width: f32,
        height: f32,
    },
    Wire(rgate_editor::WireSegmentDrag),
    Gates {
        last: Point,
    },
    Pan {
        last: Point,
    },
    Marquee {
        start: Point,
        end: Point,
        additive: bool,
    },
}

pub struct GateApp {
    #[cfg(not(target_family = "wasm"))]
    pub lint_due: Option<std::time::Instant>,
    #[cfg(not(target_family = "wasm"))]
    pub lint_result: Option<(
        Circuit,
        std::sync::mpsc::Receiver<Result<(), rgate_sim::SimError>>,
    )>,
    #[cfg_attr(target_family = "wasm", allow(dead_code))]
    pub background_compilation: bool,
    #[cfg(not(target_family = "wasm"))]
    pub compilation: Option<crate::compilation::Compilation>,
    pub hdl_diagnostics: Vec<crate::hdl_diagnostics::HdlDiagnostic>,
    #[cfg_attr(target_family = "wasm", allow(dead_code))]
    pub diagnostic_line: Option<u32>,
    #[cfg(not(target_family = "wasm"))]
    pub source_editor: Option<crate::source_editor::SourceEditor>,
    pub theme: Theme,
    pub appearance: Appearance,
    pub editor: Editor,
    pub file: Option<PathBuf>,
    pub focus: FocusHandle,
    pub simulation: Option<LiveSimulation>,
    pub debug_path: Option<String>,
    pub running: bool,
    pub tab: WorkspaceTab,
    pub bottom_scope: bool,
    pub waveform: crate::waveform::WaveformSettings,
    pub waveform_search: Entity<InputState>,
    pub wave_bounds: Bounds<Pixels>,
    pub wave_drag: Option<crate::waveform_view::WaveDrag>,
    pub nets_ports: bool,
    pub module_list: bool,
    pub collapsed_modules: std::collections::HashSet<String>,
    pub probes: BTreeSet<NetId>,
    pub messages: Vec<String>,
    pub message_log: Entity<TextareaState>,
    pub status: String,
    pub hover: Option<Hit>,
    pub input_swap: Option<(GateId, Option<String>)>,
    pub mouse: Point,
    pub context_menu: Option<ContextMenu>,
    pub open_menu: Option<OpenMenu>,
    pub dialog: Option<Dialog>,
    pub open_dialog: Option<OpenDialog>,
    pub drag: Option<Drag>,
    pub canvas_bounds: Bounds<Pixels>,
    pub sidebar_width: f32,
    pub components_width: f32,
    pub component_search: Entity<InputState>,
    pub modules_height: f32,
    pub bottom_height: f32,
    pub layout: crate::view::PanelLayout,
    pub need_fit: bool,
    /// The viewport produced by the last automatic fit, while the user has not moved it.
    pub auto_fit: Option<rgate_editor::Viewport>,
    pub closing: bool,
    pub prompting: bool,
    pub persistence: Option<crate::workspace::Persistence>,
    pub workspace_views: std::collections::BTreeMap<String, crate::workspace::ViewState>,
    pub saved_probes: std::collections::BTreeSet<crate::workspace::SavedProbe>,
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
        let component_search = crate::input::search_field("Search components…", window, cx);
        cx.subscribe(&component_search, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        })
        .detach();
        let waveform_search = crate::input::search_field("Filter signals…", window, cx);
        cx.subscribe(&waveform_search, |this, field, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.waveform.filter = field.read(cx).value().to_string();
                this.waveform.row_offset = 0;
                cx.notify();
            }
        })
        .detach();
        let message_log = crate::input::text_area("", window, cx);
        let layout = crate::view::PanelLayout::new(cx);
        let mut this = Self {
            background_compilation: false,
            #[cfg(not(target_family = "wasm"))]
            compilation: None,
            #[cfg(not(target_family = "wasm"))]
            lint_due: None,
            #[cfg(not(target_family = "wasm"))]
            lint_result: None,
            hdl_diagnostics: Vec::new(),
            diagnostic_line: None,
            #[cfg(not(target_family = "wasm"))]
            source_editor: None,
            waveform_search,
            component_search,
            waveform: crate::waveform::WaveformSettings::default(),
            wave_bounds: Bounds::default(),
            wave_drag: None,
            persistence: None,
            workspace_views: std::collections::BTreeMap::new(),
            saved_probes: std::collections::BTreeSet::new(),
            message_log,
            layout,
            theme: Theme::default(),
            appearance: Appearance::default(),
            editor: Editor::new(circuit).expect("startup circuit is validated"),
            file,
            focus,
            simulation: None,
            debug_path: None,
            running: false,
            tab: WorkspaceTab::Edit,
            bottom_scope: false,
            nets_ports: false,
            module_list: false,
            collapsed_modules: std::collections::HashSet::new(),
            probes: BTreeSet::new(),
            messages: vec![
                "RGate 0.1 — TkGate-style circuit editing, powered by Rust + GPUI.".into(),
                "Click a tool to place gates. W connects pins; Space runs; double-click a wire probes it.".into(),
            ],
            status: "Ready".into(),
            hover: None,
            input_swap: None,
            mouse: Point::ZERO,
            context_menu: None,
            open_menu: None,
            dialog: None,
            open_dialog: None,
            drag: None,
            canvas_bounds: Bounds::default(),
            sidebar_width: 210.0,
            components_width: 205.0,
            modules_height: 196.0,
            bottom_height: 176.0,
            need_fit: true,
            auto_fit: None,
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
                    {
                        this.poll_compilation(cx);
                        this.poll_source_check(cx);
                    }
                    if this.running {
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
        self.status = message.clone();
        self.messages.push(message);
        if self.messages.len() > 200 {
            self.messages.remove(0);
        }
    }

    pub(crate) fn error(&mut self, error: impl std::fmt::Display) {
        let message = error.to_string();
        self.hdl_diagnostics = crate::hdl_diagnostics::parse(&message);
        if !self.hdl_diagnostics.is_empty() {
            self.bottom_scope = false;
        }
        self.log(format!("Error: {message}"));
    }

    pub fn changed(&mut self) {
        self.input_swap = None;
        #[cfg(not(target_family = "wasm"))]
        self.cancel_compilation();
        self.simulation = None;
        self.debug_path = None;
        self.probes.clear();
        self.running = false;
        if self.tab == WorkspaceTab::Simulate {
            self.tab = WorkspaceTab::Edit;
        }
    }
}
