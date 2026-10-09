use crate::live_simulation::LiveSimulation;
use crate::{commands::Command, input::TextField, theme::Theme};
use anyhow::{Context as _, Result};
use gpui::{
    AppContext, Bounds, ClipboardItem, Context, Entity, FocusHandle, KeyDownEvent, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, PinchEvent, Pixels, ScrollWheelEvent, Task, Window, px,
};
#[cfg(not(target_family = "wasm"))]
use gpui::{PathPromptOptions, PromptLevel};
use rgate_core::{Circuit, GateId, GateKind, Module, NetId, Point, Rect, Signal, demo, route_to};
use rgate_editor::{Editor, Hit, Tool};
use rgate_sim::Simulator;
use std::{collections::BTreeSet, path::PathBuf, time::Duration};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WorkspaceTab {
    Edit,
    Interface,
    Simulate,
}

#[derive(Clone, Debug)]
pub struct Popup {
    pub menu: String,
    pub position: Point,
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
    Components {
        start_x: f32,
        width: f32,
    },
    Sidebar,
    Modules {
        start_y: f32,
        height: f32,
    },
    Bottom,
}

pub struct PropertyDialog {
    pub gate: GateId,
    pub name: Entity<TextField>,
    pub value: Entity<TextField>,
    pub delay: Entity<TextField>,
    pub width: Entity<TextField>,
    pub period: Entity<TextField>,
    pub comment: Entity<TextField>,
    pub show_name: bool,
    pub inputs: Entity<TextField>,
    pub partitions: Entity<TextField>,
    pub tap_offset: Entity<TextField>,
    pub tap_width: Entity<TextField>,
    pub address_bits: Entity<TextField>,
    pub memory: Entity<TextField>,
    pub led_display: rgate_core::LedDisplay,
    pub tty_tkgate: bool,
    pub reduction: bool,
    pub enable_low: bool,
    pub invert_output: bool,
    pub phase: Entity<TextField>,
    pub duty: Entity<TextField>,
    pub operand_a: Entity<TextField>,
    pub operand_b: Entity<TextField>,
    pub frame_width: Entity<TextField>,
    pub frame_height: Entity<TextField>,
    pub module_width: Entity<TextField>,
    pub custom_ports: Entity<TextField>,
    pub error: Option<String>,
}

pub struct NetDialog {
    pub net: NetId,
    pub name: Entity<TextField>,
    pub width: Entity<TextField>,
    pub show_name: bool,
    pub error: Option<String>,
}

pub enum Dialog {
    Verilog {
        creating: bool,
        name: Entity<TextField>,
        source: Entity<TextField>,
        interface: Entity<TextField>,
        error: Option<String>,
    },
    #[cfg_attr(not(feature = "icarus"), allow(dead_code))]
    HdlInput {
        net: NetId,
        value: Entity<TextField>,
        error: Option<String>,
    },
    Vga {
        gate: GateId,
    },
    Find {
        query: Entity<TextField>,
    },
    Recovery,
    Symbol(crate::symbol_editor::SymbolDialog),
    Input {
        gate: GateId,
        value: Entity<TextField>,
        error: Option<String>,
    },
    Memory {
        gate: GateId,
        address: Entity<TextField>,
        value: Entity<TextField>,
        error: Option<String>,
    },
    Net(Box<NetDialog>),
    Terminal {
        gate: GateId,
        input: Entity<TextField>,
        error: Option<String>,
    },
    Properties(Box<PropertyDialog>),
    NewModule {
        name: Entity<TextField>,
        error: Option<String>,
    },
    Help,
    About,
}

pub struct GateApp {
    #[cfg(not(target_family = "wasm"))]
    pub source_editor: Option<crate::source_editor::SourceEditor>,
    pub theme: Theme,
    pub editor: Editor,
    pub file: Option<PathBuf>,
    pub focus: FocusHandle,
    pub simulation: Option<LiveSimulation>,
    pub icarus_pwm: bool,
    pub debug_path: Option<String>,
    pub running: bool,
    pub tab: WorkspaceTab,
    pub bottom_scope: bool,
    pub waveform: crate::waveform::WaveformSettings,
    pub waveform_search: Entity<TextField>,
    pub wave_bounds: Bounds<Pixels>,
    pub wave_drag: Option<crate::waveform_view::WaveDrag>,
    pub nets_ports: bool,
    pub module_list: bool,
    pub collapsed_modules: std::collections::HashSet<String>,
    pub probes: BTreeSet<NetId>,
    pub messages: Vec<String>,
    pub message_log: Entity<crate::message_log::MessageLog>,
    pub status: String,
    pub hover: Option<Hit>,
    pub mouse: Point,
    pub popup: Option<Popup>,
    pub menu_bounds: Vec<Bounds<Pixels>>,
    pub dialog: Option<Dialog>,
    pub drag: Option<Drag>,
    pub canvas_bounds: Bounds<Pixels>,
    pub sidebar_width: f32,
    pub components_width: f32,
    pub component_search: Entity<TextField>,
    pub modules_height: f32,
    pub sidebar_bounds: Bounds<Pixels>,
    pub bottom_height: f32,
    pub need_fit: bool,
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
        cx.set_global(Theme::default());
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let component_search =
            cx.new(|cx| TextField::new("", cx).with_placeholder("Search components…"));
        cx.observe(&component_search, |_, _, cx| cx.notify())
            .detach();
        let waveform_search =
            cx.new(|cx| TextField::new("", cx).with_placeholder("Filter signals…"));
        cx.observe(&waveform_search, |this, field, cx| {
            this.waveform.filter = field.read(cx).value.clone();
            this.waveform.row_offset = 0;
            cx.notify();
        })
        .detach();
        let mut this = Self {
            #[cfg(not(target_family="wasm"))]
            source_editor:None,
            waveform_search,
            component_search,
            waveform:crate::waveform::WaveformSettings::default(),wave_bounds:Bounds::default(),wave_drag:None,
            persistence:None,workspace_views:std::collections::BTreeMap::new(),saved_probes:std::collections::BTreeSet::new(),
            message_log:cx.new(crate::message_log::MessageLog::new),
            theme: Theme::default(), editor: Editor::new(circuit).expect("startup circuit is validated"), file, focus,
            simulation: None, icarus_pwm: false, debug_path: None, running: false, tab: WorkspaceTab::Edit, bottom_scope: false,
            nets_ports: false, module_list: false, collapsed_modules: std::collections::HashSet::new(), probes: BTreeSet::new(),
            messages: vec!["RGate 0.1 — TkGate-style circuit editing, powered by Rust + GPUI.".into(),
                "Click a tool to place gates. W connects pins; Space runs; double-click a wire probes it.".into()],
            status: "Ready".into(), hover: None, mouse: Point::ZERO, popup: None, menu_bounds: Vec::new(), dialog: None, drag: None,
            canvas_bounds: Bounds::default(), sidebar_width: 210.0, components_width:205.0, modules_height:196.0, sidebar_bounds:Bounds::default(), bottom_height: 176.0,
            need_fit: true, closing: false, prompting: false, timer: None,
        };
        let timer = cx.spawn(async move |entity, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(40))
                    .await;
                let result = entity.update(cx, |this, cx| {
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

    pub fn advance_running(&mut self, cx: &mut Context<Self>) {
        if let Some(simulator) = &mut self.simulation {
            let quantum = simulator.clock_period().clamp(10, 1000);
            if let Err(error) = simulator.advance(quantum) {
                self.running = false;
                self.log(format!("Simulation error: {error}"));
            }
        }
        cx.notify();
    }

    pub fn held_reset_input(&self) -> Option<(GateId, rgate_core::NetId)> {
        let sim = self.simulation.as_ref()?;
        let root = sim.module();
        let net = root.nets.iter().find(|net| net.name == "RESET_N")?;
        if sim.value(net.id)?.to_u64() != Some(0) {
            return None;
        }
        let gate = root.gates.iter().find(|gate| {
            gate.kind == GateKind::Switch
                && gate.pin("Z").is_some_and(|pin| pin.net == Some(net.id))
        })?;
        Some((gate.id, net.id))
    }

    pub fn release_terminal_reset(&mut self, cx: &mut Context<Self>) {
        if let Some((id, _)) = self.held_reset_input()
            && let Some(sim) = &mut self.simulation
            && let Err(error) = sim.set_input(id, Signal::from_u64(1, 1))
        {
            self.error(error);
        }
        cx.notify();
    }

    pub fn log(&mut self, message: String) {
        self.status = message.clone();
        self.messages.push(message);
        if self.messages.len() > 200 {
            self.messages.remove(0);
        }
    }

    fn error(&mut self, error: impl std::fmt::Display) {
        self.log(format!("Error: {error}"));
    }

    pub fn changed(&mut self) {
        self.simulation = None;
        self.debug_path = None;
        self.probes.clear();
        self.running = false;
        if self.tab == WorkspaceTab::Simulate {
            self.tab = WorkspaceTab::Edit;
        }
    }

    pub fn command(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        self.popup = None;
        if matches!(command, Command::LoadMemory)
            && matches!(self.dialog, Some(Dialog::Properties(_)))
        {
            self.load_memory_image(window, cx);
            return;
        }
        if self.simulation.is_some()
            && matches!(
                command,
                Command::Align(_)
                    | Command::Distribute(_)
                    | Command::AutoRoute
                    | Command::TidyLayout
            )
        {
            self.log("Stop simulation before arranging or routing circuit geometry.".into());
            cx.notify();
            return;
        }
        if self.prompting {
            return;
        }
        if matches!(
            self.dialog,
            Some(Dialog::Terminal { .. } | Dialog::Vga { .. })
        ) && matches!(
            command,
            Command::PlayPause | Command::Step | Command::ClockStep
        ) {
            self.execute(command, window, cx);
            return;
        }
        if self.dialog.is_some() {
            return;
        }
        if matches!(
            command,
            Command::New | Command::Open | Command::Example(_) | Command::Quit
        ) && self.editor.is_dirty()
        {
            #[cfg(target_family = "wasm")]
            {
                if crate::browser::confirm_discard() {
                    self.execute(command, window, cx);
                }
                return;
            }
            #[cfg(not(target_family = "wasm"))]
            {
                let answer = window.prompt(
                    PromptLevel::Warning,
                    "Discard unsaved circuit changes?",
                    Some("Save the circuit first if you want to keep your edits."),
                    &["Discard changes", "Cancel"],
                    cx,
                );
                self.prompting = true;
                cx.spawn_in(window, async move |entity, cx| {
                    let answer = answer.await;
                    if let Err(error) = entity.update_in(cx, |this, window, cx| {
                        this.prompting = false;
                        match answer {
                            Ok(0) => this.execute(command, window, cx),
                            Ok(_) => {}
                            Err(error) => this.error(error),
                        }
                        cx.notify();
                    }) {
                        eprintln!("discard prompt ended after window closed: {error}");
                    }
                })
                .detach();
                return;
            }
        }
        self.execute(command, window, cx);
    }

    fn execute(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        match command {
            Command::New => self.replace_circuit(Circuit::default(), None),
            Command::Example(name) => {
                let result = if name == "full-adder" {
                    Ok(rgate_format::ImportResult {
                        circuit: demo::full_adder(),
                        warnings: Vec::new(),
                    })
                } else if name == "adder8" {
                    Ok(rgate_format::ImportResult {
                        circuit: demo::hierarchical_adder(),
                        warnings: Vec::new(),
                    })
                } else if name == "clocked" {
                    Ok(rgate_format::ImportResult {
                        circuit: demo::clocked_flip_flop(),
                        warnings: Vec::new(),
                    })
                } else if name == "counter" {
                    rgate_format::parse(include_str!("../../../examples/adjustable-counter.rgate"))
                } else if name == "pwm-verilog" {
                    rgate_format::parse(include_str!("../../../examples/pwm-verilog.rgate"))
                } else if matches!(name, "pwm" | "pwm-icarus") {
                    rgate_format::parse(include_str!("../../../examples/pwm-dimmer.rgate"))
                } else if name == "tiny-vga" {
                    rgate_format::parse(include_str!("../../../examples/tiny-vga.rgate"))
                } else if name == "lc3" {
                    rgate_format::parse(include_str!("../../../examples/lc3.rgate"))
                } else if name == "bus-memory" {
                    Ok(rgate_format::ImportResult {
                        circuit: demo::bus_memory(),
                        warnings: Vec::new(),
                    })
                } else if name == "hierarchy" {
                    Ok(rgate_format::ImportResult {
                        circuit: demo::hierarchical_inverters(),
                        warnings: Vec::new(),
                    })
                } else {
                    Err(anyhow::anyhow!("unknown example {name}"))
                };
                match result {
                    Ok(result) => {
                        self.replace_circuit(result.circuit, None);
                        self.icarus_pwm = name == "pwm-icarus";
                        if self.icarus_pwm {
                            self.log("Experimental Icarus PWM selected. Start simulation to compile RTL; only the bundled unmodified circuit is supported.".into());
                        }
                        for warning in result.warnings {
                            self.log(warning);
                        }
                    }
                    Err(error) => self.error(error),
                }
            }
            Command::Open => self.open(window, cx),
            Command::Save => self.save(false, false, window, cx),
            Command::SaveAs => self.save(true, false, window, cx),
            Command::Export => self.save(true, true, window, cx),
            Command::Undo => {
                self.editor.undo();
                self.changed();
            }
            Command::Redo => {
                self.editor.redo();
                self.changed();
            }
            Command::SelectAll => self.editor.select_all(),
            Command::Delete => {
                let result = self.editor.delete_selection();
                if let Err(error) = result {
                    self.error(error);
                } else {
                    self.changed();
                }
            }
            Command::Copy | Command::Cut => {
                let copied = self.editor.copy_selection();
                if !copied.gates.is_empty() {
                    match serde_json::to_string(&copied) {
                        Ok(text) => cx.write_to_clipboard(ClipboardItem::new_string(format!(
                            "RGATE-CLIPBOARD\n{text}"
                        ))),
                        Err(error) => self.error(error),
                    }
                    if matches!(command, Command::Cut) {
                        if let Err(error) = self.editor.delete_selection() {
                            self.error(error);
                        } else {
                            self.changed();
                        }
                    }
                }
            }
            Command::Paste => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    if let Some(json) = text.strip_prefix("RGATE-CLIPBOARD\n") {
                        match serde_json::from_str::<Module>(json) {
                            Ok(module) => {
                                match self.editor.paste(&module, Point::new(20.0, 20.0)) {
                                    Ok(()) => self.changed(),
                                    Err(error) => self.error(error),
                                }
                            }
                            Err(error) => self.error(error),
                        }
                    } else {
                        self.log("Clipboard does not contain RGate components.".into());
                    }
                }
            }
            Command::Rotate(clockwise) => {
                if let Err(error) = self.editor.rotate_selection(clockwise) {
                    self.error(error);
                } else {
                    self.changed();
                }
            }
            Command::Tool(tool) => {
                if self.simulation.is_some() {
                    self.changed();
                }
                self.editor.set_tool(tool);
                self.status = self.tool_hint().into();
                self.tab = WorkspaceTab::Edit;
            }
            Command::PlayPause => {
                if self.ensure_simulator() {
                    self.running = !self.running;
                    self.tab = WorkspaceTab::Simulate;
                    self.bottom_scope = true;
                    self.log(
                        if self.running {
                            "Simulation running — click switches to change inputs."
                        } else {
                            "Simulation paused."
                        }
                        .into(),
                    );
                }
            }
            Command::Stop => {
                self.remember_view();
                self.simulation = None;
                self.debug_path = None;
                self.running = false;
                self.tab = WorkspaceTab::Edit;
                self.remember_view();
                self.log("Simulation stopped; circuit values reset.".into());
            }
            Command::Step | Command::ClockStep => {
                if self.ensure_simulator() {
                    self.running = false;
                    self.tab = WorkspaceTab::Simulate;
                    self.bottom_scope = true;
                    let period = self.simulation.as_ref().unwrap().clock_period();
                    let result = if matches!(command, Command::Step) {
                        self.simulation.as_mut().unwrap().step()
                    } else {
                        self.simulation.as_mut().unwrap().advance(period)
                    };
                    if let Err(error) = result {
                        self.error(error);
                    }
                }
            }
            Command::Zoom(factor) => {
                let center = Point::new(
                    f32::from(self.canvas_bounds.size.width) / 2.0,
                    f32::from(self.canvas_bounds.size.height) / 2.0,
                );
                self.editor.viewport.zoom_at(factor, center);
            }
            Command::Fit => self.need_fit = true,
            Command::SetTheme(theme) => {
                self.theme = theme;
                cx.set_global(theme);
                window.refresh();
            }
            Command::ToggleGrid => self.editor.show_grid = !self.editor.show_grid,
            Command::ToggleSnap => self.editor.snap = !self.editor.snap,
            Command::ToggleScope => self.bottom_scope = !self.bottom_scope,
            Command::EditSymbol => {
                self.running = false;
                let module = self.editor.active_module().to_owned();
                let shapes = self.editor.module().symbol.clone();
                self.dialog = Some(Dialog::Symbol(crate::symbol_editor::SymbolDialog {
                    module,
                    shapes,
                    tool: crate::symbol_editor::SymbolTool::Line,
                    start: None,
                    bounds: Bounds::default(),
                    text: cx.new(|cx| TextField::new("Label", cx)),
                }));
            }
            Command::Find => {
                let query = cx.new(|cx| {
                    TextField::new("", cx).with_placeholder("Gate/net name, type, or module…")
                });
                query.read(cx).focus_handle(cx).focus(window, cx);
                cx.observe(&query, |_, _, cx| cx.notify()).detach();
                self.dialog = Some(Dialog::Find { query });
            }
            Command::Align(alignment) => match self.editor.align_selection(alignment) {
                Ok(()) => self.changed(),
                Err(error) => self.error(error),
            },
            Command::Distribute(axis) => match self.editor.distribute_selection(axis) {
                Ok(()) => self.changed(),
                Err(error) => self.error(error),
            },
            Command::AutoRoute => match self.editor.autoroute_selection() {
                Ok((before, after)) => {
                    self.changed();
                    self.log(format!("Routed selected wires. Crossings: {before} → {after} (heuristic, not guaranteed minimum)."));
                }
                Err(error) => self.error(error),
            },
            Command::TidyLayout => {
                match self.editor.tidy_layout() {
                    Ok(()) => {
                        self.changed();
                        self.need_fit = true;
                        self.log("Applied layered layout. Route wires separately; Undo restores placement.".into());
                    }
                    Err(error) => self.error(error),
                }
            }
            Command::Properties => self.show_properties(window, cx),
            Command::LoadMemory => {
                self.log("Open RAM/ROM properties to load a memory image.".into())
            }
            Command::NewVerilog => self.edit_verilog(true, window, cx),
            Command::EditVerilog => {
                #[cfg(not(target_family = "wasm"))]
                {
                    if self.editor.module().verilog.is_some() {
                        self.set_tab(WorkspaceTab::Edit, window, cx);
                    } else {
                        self.log("Select a Verilog source module first.".into());
                    }
                }
                #[cfg(target_family = "wasm")]
                self.edit_verilog(false, window, cx);
            }
            Command::VerilogInterface => self.edit_verilog(false, window, cx),
            Command::SetHdlInput => {
                #[cfg(feature = "icarus")]
                if self.ensure_simulator()
                    && let Some(net) = self
                        .editor
                        .selected_net
                        .and_then(|n| self.simulation_net(n))
                    && self
                        .simulation
                        .as_ref()
                        .unwrap()
                        .source_input(net)
                        .is_some()
                {
                    self.running = false;
                    self.dialog = Some(Dialog::HdlInput {
                        net,
                        value: cx.new(|cx| TextField::new("0", cx)),
                        error: None,
                    });
                } else {
                    self.log("Select an input net in a live Verilog source module.".into());
                }
                #[cfg(not(feature = "icarus"))]
                self.log("Build native RGate with --features icarus to run HDL.".into());
            }
            Command::NewModule => {
                let mut index = 2;
                while self
                    .editor
                    .circuit()
                    .module(&format!("module{index}"))
                    .is_some()
                {
                    index += 1;
                }
                let field = cx.new(|cx| TextField::new(format!("module{index}"), cx));
                field.read(cx).focus_handle(cx).focus(window, cx);
                self.dialog = Some(Dialog::NewModule {
                    name: field,
                    error: None,
                });
            }
            Command::Port(direction) => {
                if let Some(net) = self.editor.selected_net {
                    if let Err(error) = self.editor.edit_module(|module| {
                        if let Some(net) = module
                            .nets
                            .iter_mut()
                            .find(|definition| definition.id == net)
                        {
                            net.port = direction;
                        }
                    }) {
                        self.error(error);
                    } else {
                        self.changed();
                    }
                }
            }
            Command::Help => self.dialog = Some(Dialog::Help),
            Command::About => self.dialog = Some(Dialog::About),
            Command::Quit => {
                #[cfg(not(target_family = "wasm"))]
                {
                    self.closing = true;
                    self.editor.mark_saved();
                    self.flush_workspace();
                    cx.quit();
                }
                #[cfg(target_family = "wasm")]
                self.log("To leave the web app, close this browser tab.".into());
            }
        }
        if !matches!(self.dialog, Some(Dialog::Recovery)) {
            self.flush_workspace();
        }
        cx.notify();
    }

    fn replace_circuit(&mut self, circuit: Circuit, path: Option<PathBuf>) {
        self.icarus_pwm = false;
        self.flush_workspace();
        match Editor::new(circuit) {
            Ok(editor) => {
                self.editor = editor;
                self.file = path;
                self.changed();
                self.probes.clear();
                self.dialog = None;
                self.hover = None;
                self.drag = None;
                self.workspace_views.clear();
                self.saved_probes.clear();
                if let Some(workspace) = self.persistence.as_ref().and_then(|store| {
                    store
                        .preferences
                        .documents
                        .get(&self.workspace_key_public())
                        .cloned()
                }) {
                    self.apply_workspace(workspace);
                } else {
                    self.need_fit = true;
                }
                self.flush_workspace();
                self.log("Circuit loaded.".into());
            }
            Err(error) => self.error(error),
        }
    }

    #[cfg(target_family = "wasm")]
    fn open(&mut self, window: &Window, cx: &mut Context<Self>) {
        let picker = crate::browser::picker(".rgate,.v,.sv");
        self.prompting = true;
        cx.spawn_in(window, async move |entity, cx| {
            let result = crate::browser::read_file(picker).await;
            if let Err(error) = entity.update_in(cx, |this, _, cx| {
                this.prompting = false;
                match result {
                    Ok(Some((name, source))) => match rgate_format::parse(&source) {
                        Ok(result) => {
                            this.replace_circuit(result.circuit, Some(PathBuf::from(name)));
                            for warning in result.warnings {
                                this.log(warning);
                            }
                        }
                        Err(error) => this.error(format!("{error:#}")),
                    },
                    Ok(None) => {}
                    Err(error) => this.error(error),
                }
                cx.notify();
            }) {
                eprintln!("Upload completed after app closed: {error}");
            }
        })
        .detach();
    }

    #[cfg(target_family = "wasm")]
    fn save(&mut self, _save_as: bool, export: bool, _window: &Window, cx: &mut Context<Self>) {
        let content = if export {
            rgate_format::verilog::export(self.editor.circuit())
        } else {
            rgate_format::native::encode(self.editor.circuit())
        };
        let extension = if export { "v" } else { "rgate" };
        let base = self
            .file
            .as_ref()
            .and_then(|path| path.file_stem())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| {
                self.editor
                    .circuit()
                    .title
                    .trim_end_matches(".v")
                    .to_owned()
            });
        let filename = format!("{}.{}", base.replace(['/', '\\'], "-"), extension);
        let result = content.and_then(|content| {
            crate::browser::download(&filename, &content).map_err(crate::browser::js_error)
        });
        match result {
            Ok(()) => {
                if !export {
                    self.file = Some(PathBuf::from(&filename));
                    self.editor.mark_saved();
                }
                self.flush_workspace();
                self.log(format!("Downloaded {filename}"));
            }
            Err(error) => self.error(format!("{error:#}")),
        }
        cx.notify();
    }

    #[cfg(target_family = "wasm")]
    fn load_memory_image(&mut self, window: &Window, cx: &mut Context<Self>) {
        let picker = crate::browser::picker(".hex,.mem,.txt");
        self.prompting = true;
        cx.spawn_in(window, async move |entity, cx| {
            let result = crate::browser::read_file(picker).await;
            if let Err(error) = entity.update_in(cx, |this, _, cx| {
                this.prompting = false;
                let result: Result<()> = (|| {
                    if let Some((_, image)) = result?
                        && let Some(Dialog::Properties(dialog)) = &mut this.dialog
                    {
                        let width = dialog.width.read(cx).value.parse()?;
                        let address_bits = dialog.address_bits.read(cx).value.parse()?;
                        let words = rgate_core::parse_sparse_memory(&image, width, address_bits)
                            .map_err(anyhow::Error::msg)?;
                        let text = words
                            .iter()
                            .map(|(address, word)| {
                                format!(
                                    "@{address:X} {}",
                                    word.to_biguint()
                                        .map(|value| value.to_str_radix(16))
                                        .unwrap_or_else(|| "x".into())
                                )
                            })
                            .collect::<Vec<_>>()
                            .join(" ");
                        dialog
                            .memory
                            .update(cx, |field, cx| field.set_value(text, cx));
                    }
                    Ok(())
                })();
                if let Err(error) = result
                    && let Some(Dialog::Properties(dialog)) = &mut this.dialog
                {
                    dialog.error = Some(format!("{error:#}"));
                }
                cx.notify();
            }) {
                eprintln!("Memory upload completed after app closed: {error}");
            }
        })
        .detach();
    }

    #[cfg(not(target_family = "wasm"))]
    fn open(&mut self, window: &Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open a .rgate or TkGate .v circuit".into()),
        });
        self.prompting = true;
        cx.spawn_in(window, async move |entity, cx| {
            let result = paths.await;
            if let Err(error) = entity.update_in(cx, |this, _, cx| {
                this.prompting = false;
                match result {
                    Ok(Ok(Some(paths))) => {
                        if let Some(path) = paths.into_iter().next() {
                            match rgate_format::load(&path) {
                                Ok(result) => {
                                    this.replace_circuit(result.circuit, Some(path));
                                    for warning in result.warnings {
                                        this.log(warning);
                                    }
                                }
                                Err(error) => this.error(format!("{error:#}")),
                            }
                        }
                    }
                    Ok(Ok(None)) => {}
                    Ok(Err(error)) => this.error(error),
                    Err(error) => this.error(error),
                }
                cx.notify();
            }) {
                eprintln!("open dialog ended after window closed: {error}");
            }
        })
        .detach();
    }

    #[cfg(not(target_family = "wasm"))]
    fn save(&mut self, save_as: bool, export: bool, window: &Window, cx: &mut Context<Self>) {
        if !save_as
            && let Some(path) = &self.file
            && path
                .extension()
                .is_some_and(|extension| extension == "rgate")
        {
            let path = path.clone();
            self.save_to(path, false);
            return;
        }
        let directory = self
            .file
            .as_ref()
            .and_then(|path| path.parent())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").unwrap_or_else(|| ".".into()))
            });
        let extension = if export { "v" } else { "rgate" };
        let name = format!(
            "{}.{}",
            self.editor
                .circuit()
                .title
                .trim_end_matches(".v")
                .replace('/', "-"),
            extension
        );
        let path = cx.prompt_for_new_path(&directory, Some(&name));
        self.prompting = true;
        cx.spawn_in(window, async move |entity, cx| {
            let result = path.await;
            if let Err(error) = entity.update_in(cx, |this, _, cx| {
                this.prompting = false;
                match result {
                    Ok(Ok(Some(mut path))) => {
                        if export || path.extension().is_none() {
                            path.set_extension(extension);
                        }
                        this.save_to(path, export);
                    }
                    Ok(Ok(None)) => {}
                    Ok(Err(error)) => this.error(error),
                    Err(error) => this.error(error),
                }
                cx.notify();
            }) {
                eprintln!("save dialog ended after window closed: {error}");
            }
        })
        .detach();
    }

    #[cfg(not(target_family = "wasm"))]
    fn save_to(&mut self, path: PathBuf, export: bool) {
        match rgate_format::save(&path, self.editor.circuit()) {
            Ok(()) => {
                if !export {
                    self.file = Some(path.clone());
                    self.editor.mark_saved();
                }
                self.flush_workspace();
                self.log(format!(
                    "{} {}",
                    if export { "Exported" } else { "Saved" },
                    path.display()
                ));
            }
            Err(error) => self.error(format!("{error:#}")),
        }
    }

    pub fn ensure_simulator(&mut self) -> bool {
        if self.simulation.is_some() {
            return true;
        }
        let result: std::result::Result<LiveSimulation, rgate_sim::SimError> = if self
            .editor
            .circuit()
            .modules
            .iter()
            .any(|m| m.verilog.is_some())
        {
            #[cfg(feature = "icarus")]
            {
                LiveSimulation::verilog(self.editor.circuit(), self.editor.active_module())
            }
            #[cfg(not(feature = "icarus"))]
            {
                Err(rgate_sim::SimError::Invalid(
                    "Verilog simulation requires the native icarus feature.".into(),
                ))
            }
        } else if self.icarus_pwm {
            #[cfg(feature = "icarus")]
            {
                if self.editor.active_module() != "main" {
                    Err(rgate_sim::SimError::Invalid(
                        "Start Icarus PWM from main, then navigate into dimmer.".into(),
                    ))
                } else {
                    LiveSimulation::pwm(self.editor.circuit())
                }
            }
            #[cfg(not(feature = "icarus"))]
            {
                Err(rgate_sim::SimError::Invalid(
                    "This build does not include Icarus support.".into(),
                ))
            }
        } else {
            Simulator::from_circuit(self.editor.circuit(), self.editor.active_module())
                .map(Into::into)
        };
        match result {
            Ok(mut simulator) => {
                for net in &self.probes {
                    if let Err(error) = simulator.probe(*net) {
                        self.messages.push(error.to_string());
                    }
                }
                for warning in simulator.warnings() {
                    self.messages.push(warning.clone());
                }
                self.remember_view();
                self.debug_path = Some(simulator.root_path().into());
                self.simulation = Some(simulator);
                self.restore_saved_probes();
                self.remember_view();
                self.editor.set_tool(Tool::Select);
                true
            }
            Err(error) => {
                self.error(error);
                false
            }
        }
    }

    pub fn simulation_net(&self, net: NetId) -> Option<NetId> {
        self.simulation
            .as_ref()?
            .scoped_net(self.debug_path.as_deref()?, net)
    }
    pub fn simulation_gate(&self, gate: GateId) -> Option<GateId> {
        self.simulation
            .as_ref()?
            .scoped_gate(self.debug_path.as_deref()?, gate)
    }
    pub fn displayed_value(&self, net: NetId) -> Option<&Signal> {
        self.simulation.as_ref()?.value(self.simulation_net(net)?)
    }
    pub fn is_probed(&self, net: NetId) -> bool {
        self.probes
            .contains(&self.simulation_net(net).unwrap_or(net))
    }
    pub fn toggle_probe(&mut self, net: NetId, cx: &mut Context<Self>) {
        if !self.ensure_simulator() {
            return;
        }
        let Some(actual) = self.simulation_net(net) else {
            self.log(
                "Choose a live instance in the hierarchy tree to probe this definition.".into(),
            );
            return;
        };
        let saved = crate::workspace::SavedProbe {
            root: self.simulation.as_ref().unwrap().root_path().into(),
            path: self.debug_path.clone().unwrap_or_default(),
            net: self.editor.module().net(net).unwrap().name.clone(),
        };
        if self.probes.remove(&actual) {
            self.saved_probes.remove(&saved);
            self.simulation.as_mut().unwrap().unprobe(actual);
        } else {
            self.probes.insert(actual);
            self.saved_probes.insert(saved);
            let result = self.simulation.as_mut().unwrap().probe(actual);
            if let Err(error) = result {
                self.error(error);
            } else {
                let path = self.debug_path.as_deref().unwrap_or("");
                let name = self
                    .editor
                    .module()
                    .net(net)
                    .map(|net| net.name.as_str())
                    .unwrap_or("");
                self.simulation
                    .as_mut()
                    .unwrap()
                    .label_probe(actual, format!("{path}/{name}"));
            }
        }
        self.bottom_scope = true;
        cx.notify();
    }

    pub fn navigate_scope(&mut self, path: String, name: String, cx: &mut Context<Self>) {
        if self
            .simulation
            .as_ref()
            .is_some_and(|sim| sim.scope_module(&path) == Some(name.as_str()))
        {
            self.remember_view();
            if let Err(error) = self.editor.switch_module(&name) {
                self.error(error);
                return;
            }
            self.debug_path = Some(path.clone());
            self.hover = None;
            self.drag = None;
            self.editor.set_tool(Tool::Select);
            self.restore_view();
            self.tab = WorkspaceTab::Simulate;
            self.status = format!("Live instance: {path}");
            cx.notify();
        } else if self.simulation.is_some() {
            self.log(format!("{path} is not in the running simulation. Stop simulation to edit or run this definition independently."));
            cx.notify();
        } else {
            self.switch_module(name, cx);
        }
    }

    pub fn switch_module(&mut self, name: String, cx: &mut Context<Self>) {
        if let Some(sim) = &self.simulation {
            let paths = sim
                .scopes()
                .iter()
                .filter(|(_, scope)| scope.module == name)
                .map(|(path, _)| path.clone())
                .collect::<Vec<_>>();
            if paths.len() == 1 {
                self.navigate_scope(paths[0].clone(), name, cx);
            } else {
                self.log("Choose the specific instance in Tree view; this definition has multiple or no running instances.".into());
                cx.notify();
            }
            return;
        }
        self.remember_view();
        if let Err(error) = self.editor.switch_module(&name) {
            self.error(error);
        } else {
            self.changed();
            self.probes.clear();
            self.hover = None;
            self.drag = None;
            self.editor.set_tool(Tool::Select);
            self.restore_view();
            self.tab = WorkspaceTab::Edit;
        }
        cx.notify();
    }

    pub fn set_tab(&mut self, tab: WorkspaceTab, window: &mut Window, cx: &mut Context<Self>) {
        match tab {
            WorkspaceTab::Edit => {
                self.remember_view();
                self.running = false;
                self.simulation = None;
                self.debug_path = None;
                self.probes.clear();
                self.tab = tab;
                self.remember_view();
            }
            WorkspaceTab::Interface => {
                self.running = false;
                self.tab = tab;
            }
            WorkspaceTab::Simulate => {
                if self.ensure_simulator() {
                    self.tab = tab;
                    self.bottom_scope = true;
                }
            }
        }
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub fn tool_hint(&self) -> &'static str {
        match self.editor.tool {
            Tool::Select => {
                "Select / move: drag gates or wire segments; Shift-click adds gates; double-click opens properties."
            }
            Tool::Wire => {
                "Wire: click a pin, click to add corners, then click a target pin or wire. Esc cancels."
            }
            Tool::Pan => {
                "Scroll: drag to pan the circuit. Scroll wheel also pans; ⌘/Ctrl-scroll zooms."
            }
            Tool::Delete => "Cut: click a wire to disconnect it, or a gate to delete it.",
            Tool::Place(_) => {
                "Place: click the canvas to add a component. Esc returns to selection."
            }
        }
    }

    #[cfg(not(target_family = "wasm"))]
    fn load_memory_image(&mut self, window: &Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Load hex memory image".into()),
        });
        self.prompting = true;
        cx.spawn_in(window, async move |entity, cx| {
            let result = paths.await;
            if let Err(error) = entity.update_in(cx, |this, _, cx| {
                this.prompting = false;
                let result: Result<()> = (|| {
                    let paths = result??;
                    if let Some(path) = paths.and_then(|paths| paths.into_iter().next()) {
                        let image = std::fs::read_to_string(path)?;
                        if let Some(Dialog::Properties(dialog)) = &mut this.dialog {
                            let width = dialog.width.read(cx).value.parse()?;
                            let address_bits = dialog.address_bits.read(cx).value.parse()?;
                            let words =
                                rgate_core::parse_sparse_memory(&image, width, address_bits)
                                    .map_err(anyhow::Error::msg)?;
                            let contents = words
                                .iter()
                                .map(|(address, word)| {
                                    format!(
                                        "@{address:X} {}",
                                        word.to_biguint()
                                            .map(|value| value.to_str_radix(16))
                                            .unwrap_or_else(|| "x".into())
                                    )
                                })
                                .collect::<Vec<_>>()
                                .join(" ");
                            dialog.memory.update(cx, |field, cx| {
                                field.set_value(contents, cx);
                            });
                        }
                    }
                    Ok(())
                })();
                if let Err(error) = result
                    && let Some(Dialog::Properties(dialog)) = &mut this.dialog
                {
                    dialog.error = Some(format!("{error:#}"));
                }
                cx.notify();
            }) {
                eprintln!("memory image dialog closed: {error}");
            }
        })
        .detach();
    }

    pub fn select_led_display(&mut self, mode: rgate_core::LedDisplay, cx: &mut Context<Self>) {
        if let Some(Dialog::Properties(dialog)) = &mut self.dialog {
            let connected = self
                .editor
                .module()
                .gate(dialog.gate)
                .is_some_and(|gate| gate.pins.iter().any(|pin| pin.net.is_some()));
            dialog.led_display = mode;
            if !connected {
                dialog.width.update(cx, |field, cx| {
                    field.set_value(mode.recommended_width().to_string(), cx)
                });
            }
            dialog.error = None;
        }
        cx.notify();
    }

    pub fn select_search_result(
        &mut self,
        result: rgate_editor::SearchResult,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dialog = None;
        self.switch_module(result.module.clone(), cx);
        if self.editor.active_module() != result.module.as_str() {
            self.log(
                "Select the exact live instance in Tree view before locating this result.".into(),
            );
            cx.notify();
            return;
        }
        let center = match result.target {
            rgate_editor::SearchTarget::Gate(id) => {
                self.editor.select(id, false);
                self.editor.module().gate(id).map(|gate| gate.position)
            }
            rgate_editor::SearchTarget::Net(id) => {
                self.editor.selection.clear();
                self.editor.selected_wires.clear();
                self.editor.selected_net = Some(id);
                self.editor
                    .module()
                    .wires
                    .iter()
                    .find(|wire| wire.net == id)
                    .and_then(|wire| wire.points.first().copied())
            }
        };
        if let Some(center) = center {
            self.editor.viewport.pan = Point::new(
                f32::from(self.canvas_bounds.size.width) / 2.0,
                f32::from(self.canvas_bounds.size.height) / 2.0,
            ) - center * self.editor.viewport.zoom;
            self.need_fit = false;
        }
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub fn show_properties(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(net) = self.editor.selected_net {
            if let Some(definition) = self.editor.module().net(net) {
                let name = cx.new(|cx| TextField::new(&definition.name, cx));
                name.read(cx).focus_handle(cx).focus(window, cx);
                self.dialog = Some(Dialog::Net(Box::new(NetDialog {
                    net,
                    name,
                    width: cx.new(|cx| TextField::new(definition.width.to_string(), cx)),
                    show_name: definition.show_name,
                    error: None,
                })));
            }
            return;
        }
        let Some(id) = self.editor.selection.iter().next().copied() else {
            self.log("Select a gate first; module ports are edited in the Interface tab.".into());
            return;
        };
        let Some(gate) = self.editor.module().gate(id) else {
            return;
        };
        let name = cx.new(|cx| TextField::new(&gate.name, cx));
        name.read(cx).focus_handle(cx).focus(window, cx);
        self.dialog = Some(Dialog::Properties(Box::new(PropertyDialog {
            gate: id,
            name,
            value: cx.new(|cx| TextField::new(gate.initial.display_value(), cx)),
            delay: cx.new(|cx| TextField::new(gate.delay.to_string(), cx)),
            width: cx.new(|cx| TextField::new(gate.width.to_string(), cx)),
            period: cx.new(|cx| TextField::new(gate.period.to_string(), cx)),
            comment: cx.new(|cx| TextField::new(gate.text.replace('\n', "\\n"), cx)),
            show_name: gate.show_name,
            led_display: gate.config.led_display,
            tty_tkgate: gate.config.tty_tkgate,
            reduction: gate.config.reduction || gate.kind.is_reduction(),
            enable_low: gate.config.enable_low,
            invert_output: gate.config.invert_output,
            phase: cx.new(|cx| TextField::new(gate.config.clock_phase.to_string(), cx)),
            duty: cx.new(|cx| TextField::new(gate.config.clock_duty.to_string(), cx)),
            operand_a: cx.new(|cx| {
                TextField::new(
                    gate.config
                        .operand_a_width
                        .unwrap_or(gate.width)
                        .to_string(),
                    cx,
                )
            }),
            operand_b: cx.new(|cx| {
                TextField::new(
                    gate.config
                        .operand_b_width
                        .unwrap_or(gate.width)
                        .to_string(),
                    cx,
                )
            }),
            frame_width: cx.new(|cx| TextField::new(gate.config.frame_width.to_string(), cx)),
            module_width: cx.new(|cx| TextField::new(gate.config.module_width.to_string(), cx)),
            custom_ports: cx.new(|cx| {
                TextField::new(
                    serde_json::to_string(&gate.config.custom_ports)
                        .unwrap_or_else(|_| "{}".into()),
                    cx,
                )
            }),
            frame_height: cx.new(|cx| TextField::new(gate.config.frame_height.to_string(), cx)),
            inputs: cx.new(|cx| TextField::new(gate.input_count.to_string(), cx)),
            partitions: cx.new(|cx| {
                TextField::new(
                    gate.config
                        .partitions
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(","),
                    cx,
                )
            }),
            tap_offset: cx.new(|cx| TextField::new(gate.config.tap_offset.to_string(), cx)),
            tap_width: cx.new(|cx| TextField::new(gate.config.tap_width.to_string(), cx)),
            address_bits: cx.new(|cx| TextField::new(gate.config.address_bits.to_string(), cx)),
            memory: cx.new(|cx| {
                TextField::new(
                    gate.config
                        .memory
                        .iter()
                        .enumerate()
                        .map(|(address, word)| (address as u64, word))
                        .chain(
                            gate.config
                                .sparse_memory
                                .iter()
                                .map(|(address, word)| (u64::from(*address), word)),
                        )
                        .map(|(address, word)| {
                            format!(
                                "@{address:X} {}",
                                word.to_biguint()
                                    .map(|value| value.to_str_radix(16))
                                    .unwrap_or_else(|| "x".into())
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(" "),
                    cx,
                )
            }),
            error: None,
        })));
        self.running = false;
    }

    pub fn apply_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(Dialog::Find { query }) = &self.dialog {
            if let Some(result) = rgate_editor::search(self.editor.circuit(), &query.read(cx).value)
                .into_iter()
                .next()
            {
                self.select_search_result(result, window, cx);
            }
            return;
        }
        if matches!(self.dialog, Some(Dialog::Recovery)) {
            self.restore_recovery(window, cx);
            return;
        }
        if matches!(
            self.dialog,
            Some(Dialog::Help | Dialog::About | Dialog::Vga { .. })
        ) {
            self.dialog = None;
            self.focus.focus(window, cx);
            cx.notify();
            return;
        }
        let terminal_dialog = matches!(
            self.dialog,
            Some(
                Dialog::Terminal { .. }
                    | Dialog::Memory { .. }
                    | Dialog::Input { .. }
                    | Dialog::HdlInput { .. }
            )
        );
        let creating_module = matches!(self.dialog, Some(Dialog::NewModule { .. }));
        let runtime_dialog_gate = match self.dialog.as_ref() {
            Some(
                Dialog::Memory { gate, .. }
                | Dialog::Terminal { gate, .. }
                | Dialog::Input { gate, .. },
            ) => self.simulation_gate(*gate),
            _ => None,
        };
        let result: Result<()> = match self.dialog.as_ref() {
            Some(Dialog::Verilog {
                creating,
                name,
                source,
                interface,
                ..
            }) => (|| {
                let name = name.read(cx).value.trim().to_owned();
                if !crate::verilog_editor::identifier(&name) {
                    anyhow::bail!("Use a simple Verilog module name");
                }
                let source = source.read(cx).value.clone();
                let mut nets = crate::verilog_editor::interface(&interface.read(cx).value)
                    .map_err(anyhow::Error::msg)?;
                if source.trim().is_empty() || source.len() > 1_000_000 {
                    anyhow::bail!("Source must contain 1–1000000 bytes");
                }
                if !*creating
                    && self
                        .editor
                        .module()
                        .nets
                        .iter()
                        .filter(|n| n.port.is_some())
                        .map(|n| (&n.name, n.width, n.port))
                        .collect::<Vec<_>>()
                        != nets
                            .iter()
                            .filter(|n| n.port.is_some())
                            .map(|n| (&n.name, n.width, n.port))
                            .collect::<Vec<_>>()
                    && self.editor.circuit().modules.iter().any(|m| {
                        m.gates.iter().any(|g| {
                            g.kind == GateKind::Module(name.clone())
                                && g.pins.iter().any(|p| p.net.is_some())
                        })
                    })
                {
                    anyhow::bail!(
                        "Disconnect module instances before changing HDL interface ports"
                    );
                }
                if !*creating && name != self.editor.active_module() {
                    anyhow::bail!("Renaming source modules is not supported yet");
                }
                if *creating {
                    let mut definition = Module::new(name);
                    definition.verilog = Some(source);
                    definition.nets = nets;
                    self.editor.create_definition(definition)?;
                    return Ok(());
                } else {
                    let mut next = self.editor.module().next_net_id().0;
                    for net in &mut nets {
                        net.id = if let Some(old) = self
                            .editor
                            .module()
                            .nets
                            .iter()
                            .find(|n| n.name == net.name)
                        {
                            old.id
                        } else {
                            let id = NetId(next);
                            next += 1;
                            id
                        };
                    }
                }
                self.editor.edit_module(|module| {
                    module.verilog = Some(source);
                    module.nets = nets;
                })?;
                Ok(())
            })(),
            Some(Dialog::HdlInput { net, value, .. }) => (|| {
                #[cfg(feature = "icarus")]
                {
                    let sim = self.simulation.as_mut().context("Simulation inactive")?;
                    let gate = sim.source_input(*net).context("Not an interactive input")?;
                    let width = sim.value(*net).context("Missing input")?.width();
                    let value =
                        Signal::parse(value.read(cx).value.trim_start_matches("0x"), width, 16)
                            .map_err(anyhow::Error::msg)?;
                    sim.set_input(gate, value)?;
                    Ok(())
                }
                #[cfg(not(feature = "icarus"))]
                {
                    let _ = (net, value);
                    anyhow::bail!("Icarus support unavailable")
                }
            })(),
            Some(Dialog::Symbol(dialog)) => self
                .editor
                .set_module_symbol(&dialog.module, dialog.shapes.clone())
                .map_err(Into::into),
            Some(Dialog::Input { gate, value, .. }) => (|| {
                let width = self
                    .editor
                    .module()
                    .gate(*gate)
                    .context("Input no longer exists")?
                    .width;
                let word = Signal::parse(value.read(cx).value.trim_start_matches("0x"), width, 16)
                    .map_err(anyhow::Error::msg)?;
                self.simulation
                    .as_mut()
                    .context("Simulation inactive")?
                    .set_input(runtime_dialog_gate.context("No runtime input")?, word)?;
                Ok(())
            })(),
            Some(Dialog::Memory {
                gate,
                address,
                value,
                ..
            }) => (|| {
                let id = *gate;
                let address: usize = address
                    .read(cx)
                    .value
                    .parse()
                    .context("Address must be a decimal number.")?;

                let width = self
                    .editor
                    .module()
                    .gate(id)
                    .context("Memory no longer exists.")?
                    .width;
                let word = Signal::parse(value.read(cx).value.trim_start_matches("0x"), width, 16)
                    .map_err(anyhow::Error::msg)?;
                let runtime_id =
                    runtime_dialog_gate.context("Memory is not in this live instance.")?;
                self.simulation
                    .as_mut()
                    .context("Simulation is not active.")?
                    .set_memory_word(runtime_id, address, word)?;
                Ok(())
            })(),
            Some(Dialog::Terminal { input, .. }) => {
                let text = input.read(cx).value.clone();
                let runtime_id = runtime_dialog_gate;
                self.simulation
                    .as_mut()
                    .context("Start simulation before sending terminal input.")
                    .and_then(|sim| {
                        sim.send_terminal(
                            runtime_id.context("Terminal is not in this live instance.")?,
                            &text,
                        )
                        .map_err(Into::into)
                    })
            }
            Some(Dialog::Net(dialog)) => (|| {
                let id = dialog.net;
                let name = dialog.name.read(cx).value.trim().to_owned();
                if name.is_empty() {
                    anyhow::bail!("Net name must not be empty.");
                }
                let width: u16 = dialog
                    .width
                    .read(cx)
                    .value
                    .parse()
                    .context("Width must be 1–4096.")?;
                if !(1..=4096).contains(&width) {
                    anyhow::bail!("Width must be 1–4096.");
                }
                if self.editor.module().gates.iter().any(|gate| {
                    gate.pins
                        .iter()
                        .any(|pin| pin.net == Some(id) && gate.pin_width(&pin.name) != width)
                }) {
                    anyhow::bail!(
                        "Width conflicts with connected pins. Disconnect or resize the components first."
                    );
                }
                let show = dialog.show_name;
                self.editor.edit_module(|module| {
                    if let Some(net) = module.nets.iter_mut().find(|net| net.id == id) {
                        net.name = name;
                        net.width = width;
                        net.show_name = show;
                    }
                })?;
                Ok(())
            })(),
            Some(Dialog::Properties(dialog))
                if self.editor.module().gate(dialog.gate).is_some_and(|gate| {
                    matches!(gate.kind, GateKind::Frame | GateKind::Comment)
                }) =>
            {
                (|| {
                    let id = dialog.gate;
                    let name = dialog.name.read(cx).value.trim().to_owned();
                    if name.is_empty() {
                        anyhow::bail!("Name must not be empty.");
                    }
                    let is_frame = self.editor.module().gate(id).unwrap().kind == GateKind::Frame;
                    let dimensions = if is_frame {
                        let width: f32 = dialog
                            .frame_width
                            .read(cx)
                            .value
                            .parse()
                            .context("Frame width must be numeric.")?;
                        let height: f32 = dialog
                            .frame_height
                            .read(cx)
                            .value
                            .parse()
                            .context("Frame height must be numeric.")?;
                        if !width.is_finite()
                            || !height.is_finite()
                            || !(20.0..=10000.0).contains(&width)
                            || !(20.0..=10000.0).contains(&height)
                        {
                            anyhow::bail!("Frame dimensions must be between 20 and 10000.");
                        }
                        Some((width, height))
                    } else {
                        None
                    };
                    let title = dialog.comment.read(cx).value.replace("\\n", "\n");
                    let show_name = dialog.show_name;
                    self.editor.edit_module(|module| {
                        let gate = module.gate_mut(id).expect("selected annotation exists");
                        gate.name = name;
                        gate.text = title;
                        gate.show_name = show_name;
                        if let Some((width, height)) = dimensions {
                            gate.config.frame_width = width;
                            gate.config.frame_height = height;
                        }
                    })?;
                    Ok(())
                })()
            }
            Some(Dialog::Properties(dialog)) => (|| {
                let id = dialog.gate;
                let name = dialog.name.read(cx).value.trim().to_owned();
                if name.is_empty() {
                    anyhow::bail!("Name must not be empty.");
                }
                let gate = self
                    .editor
                    .module()
                    .gate(id)
                    .context("Selected gate no longer exists.")?;
                let fields = crate::properties::fields(&gate.kind);
                let width = if fields.width {
                    let width: u16 = dialog
                        .width
                        .read(cx)
                        .value
                        .parse()
                        .context("Width must be a number from 1 to 4096.")?;
                    if !(1..=4096).contains(&width) {
                        anyhow::bail!("Width must be from 1 to 4096.");
                    }
                    width
                } else {
                    gate.width
                };
                if gate.width != width && gate.pins.iter().any(|pin| pin.net.is_some()) {
                    anyhow::bail!(
                        "Disconnect this gate before changing its bit width; existing nets retain their width."
                    );
                }
                let value = if fields.initial {
                    let value = dialog.value.read(cx).value.clone();
                    if let Some(hex) = value.strip_prefix("0x") {
                        Signal::parse(hex, width, 16)
                    } else {
                        Signal::parse(&value, width, 10)
                    }
                    .map_err(anyhow::Error::msg)?
                } else {
                    gate.initial.resized(width)
                };
                let delay = if fields.delay {
                    dialog
                        .delay
                        .read(cx)
                        .value
                        .parse()
                        .context("Delay must be a nonnegative number.")?
                } else {
                    gate.delay
                };
                let period = if fields.clock {
                    dialog
                        .period
                        .read(cx)
                        .value
                        .parse()
                        .context("Period must be a number.")?
                } else {
                    gate.period
                };
                let comment = gate.text.clone();
                let mut config = gate.config.clone();
                if gate.kind == GateKind::Led {
                    config.led_display = dialog.led_display;
                }
                if gate.kind == GateKind::Tty {
                    config.tty_tkgate = dialog.tty_tkgate;
                }
                if gate.kind.is_logic() {
                    config.reduction = dialog.reduction;
                }
                if gate.kind == GateKind::TriState {
                    config.enable_low = dialog.enable_low;
                    config.invert_output = dialog.invert_output;
                }
                if fields.clock {
                    config.clock_phase = dialog
                        .phase
                        .read(cx)
                        .value
                        .parse()
                        .context("Phase must be nonnegative nanoseconds.")?;
                    config.clock_duty = dialog
                        .duty
                        .read(cx)
                        .value
                        .parse()
                        .context("Duty must be 1–99 percent.")?;
                }
                if matches!(gate.kind, GateKind::Module(_)) {
                    config.module_width = dialog
                        .module_width
                        .read(cx)
                        .value
                        .parse()
                        .context("Module width must be numeric")?;
                    config.custom_ports = serde_json::from_str(&dialog.custom_ports.read(cx).value)
                        .context("Port positions must be JSON: {\"A\":{\"x\":-50,\"y\":0}}")?;
                    for name in config.custom_ports.keys() {
                        if gate.pin(name).is_none() {
                            anyhow::bail!("Unknown port {name}");
                        }
                    }
                }
                if matches!(gate.kind, GateKind::Multiply | GateKind::Divide) {
                    config.operand_a_width = Some(dialog.operand_a.read(cx).value.parse()?);
                    config.operand_b_width = Some(dialog.operand_b.read(cx).value.parse()?);
                }
                let inputs = if fields.count {
                    dialog
                        .inputs
                        .read(cx)
                        .value
                        .parse()
                        .context("Input/output count must be 1–64.")?
                } else {
                    gate.input_count
                };
                if matches!(gate.kind, GateKind::Concat | GateKind::Splitter) {
                    config.partitions = dialog
                        .partitions
                        .read(cx)
                        .value
                        .split(',')
                        .filter(|part| !part.trim().is_empty())
                        .map(|part| part.trim().parse::<u16>())
                        .collect::<Result<Vec<_>, _>>()
                        .context("Partitions must be comma-separated widths.")?;
                }
                if gate.kind == GateKind::Tap {
                    config.tap_offset = dialog
                        .tap_offset
                        .read(cx)
                        .value
                        .parse()
                        .context("Tap offset must be a number.")?;
                    config.tap_width = dialog
                        .tap_width
                        .read(cx)
                        .value
                        .parse()
                        .context("Tap width must be a number.")?;
                }
                if matches!(gate.kind, GateKind::Ram | GateKind::Rom) {
                    config.address_bits = dialog
                        .address_bits
                        .read(cx)
                        .value
                        .parse()
                        .context("Address bits must be 1–32.")?;
                    config.sparse_memory = rgate_core::parse_sparse_memory(
                        &dialog.memory.read(cx).value,
                        width,
                        config.address_bits,
                    )
                    .map_err(anyhow::Error::msg)?;
                    config.memory.clear();
                }
                if gate.pins.iter().any(|pin| pin.net.is_some())
                    && (config.partitions != gate.config.partitions
                        || config.tap_offset != gate.config.tap_offset
                        || config.tap_width != gate.config.tap_width
                        || config.address_bits != gate.config.address_bits
                        || inputs != gate.input_count
                        || config.reduction != gate.config.reduction
                        || config.tty_tkgate != gate.config.tty_tkgate
                        || config.operand_a_width != gate.config.operand_a_width
                        || config.operand_b_width != gate.config.operand_b_width)
                {
                    anyhow::bail!(
                        "Disconnect this component before changing its pin layout or widths."
                    );
                }
                let old_gate = gate.clone();
                let show_name = dialog.show_name;
                self.editor.edit_module(|module| {
                    if let Some(gate) = module.gate_mut(id) {
                        gate.name = name;
                        gate.width = width;
                        gate.initial = value;
                        gate.delay = delay;
                        gate.period = period;
                        gate.text = comment;
                        gate.show_name = show_name;
                        gate.input_count = inputs;
                        gate.config = config;
                        if matches!(gate.kind, GateKind::Module(_)) {
                            for pin in &mut gate.pins {
                                if let Some(offset) = gate.config.custom_ports.get(&pin.name) {
                                    pin.offset = *offset;
                                } else if old_gate.config.module_width != gate.config.module_width {
                                    pin.offset.x =
                                        pin.offset.x.signum() * gate.config.module_width / 2.0;
                                }
                            }
                        }
                        if gate.kind.is_extended()
                            || inputs != old_gate.input_count
                            || width != old_gate.width
                            || gate.config.reduction != old_gate.config.reduction
                        {
                            let old = gate.pins.clone();
                            gate.reset_pins();
                            for pin in &mut gate.pins {
                                if let Some(previous) = old.iter().find(|old| old.name == pin.name)
                                {
                                    pin.net = previous.net;
                                    pin.offset = previous.offset;
                                }
                            }
                        }
                    }
                    let gate = module.gate(id).unwrap().clone();
                    for wire in &mut module.wires {
                        for (start, reference) in [(true, &wire.start), (false, &wire.end)] {
                            if let Some(reference) = reference
                                && reference.gate == id
                                && let Some(pin) = gate.pin(&reference.pin)
                            {
                                let target = gate.pin_position(pin);
                                let index = if start { 0 } else { wire.points.len() - 1 };
                                let old = wire.points[index];
                                if old != target {
                                    if start {
                                        wire.points[0] = target;
                                        wire.points.insert(1, Point::new(old.x, target.y));
                                        wire.points.insert(2, old);
                                    } else {
                                        wire.points.pop();
                                        wire.points.push(Point::new(target.x, old.y));
                                        wire.points.push(target);
                                    }
                                }
                            }
                        }
                    }
                })?;
                Ok(())
            })(),
            Some(Dialog::NewModule { name, .. }) => self
                .editor
                .create_module(name.read(cx).value.trim().to_owned())
                .map_err(Into::into),
            _ => Ok(()),
        };
        match result {
            Ok(()) => {
                self.dialog = None;
                if !terminal_dialog {
                    self.changed();
                }
                if creating_module {
                    self.probes.clear();
                    self.hover = None;
                    self.drag = None;
                    self.editor.set_tool(Tool::Select);
                    self.tab = WorkspaceTab::Edit;
                    self.need_fit = true;
                    self.log(format!(
                        "Created module {}. Place gates, then connect pins to create nets.",
                        self.editor.active_module()
                    ));
                }
                self.focus.focus(window, cx);
            }
            Err(error) => match &mut self.dialog {
                Some(
                    Dialog::Verilog { error: slot, .. } | Dialog::HdlInput { error: slot, .. },
                ) => *slot = Some(format!("{error:#}")),
                Some(Dialog::Input { error: slot, .. }) => *slot = Some(format!("{error:#}")),
                Some(Dialog::Net(dialog)) => dialog.error = Some(format!("{error:#}")),
                Some(Dialog::Memory { error: slot, .. }) => *slot = Some(format!("{error:#}")),
                Some(Dialog::Terminal { error: slot, .. }) => *slot = Some(format!("{error:#}")),
                Some(Dialog::Properties(dialog)) => dialog.error = Some(format!("{error:#}")),
                Some(Dialog::NewModule { error: slot, .. }) => *slot = Some(format!("{error:#}")),
                _ => self.error(error),
            },
        }
        cx.notify();
    }

    fn canvas_point(&self, position: gpui::Point<Pixels>) -> Point {
        self.editor.viewport.to_world(Point::new(
            f32::from(position.x) - f32::from(self.canvas_bounds.left()),
            f32::from(position.y) - f32::from(self.canvas_bounds.top()),
        ))
    }

    pub fn drop_component(
        &mut self,
        kind: GateKind,
        position: gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dialog.is_some()
            || self.popup.is_some()
            || self.prompting
            || self.tab == WorkspaceTab::Interface
            || !self.canvas_bounds.contains(&position)
        {
            return;
        }
        let point = self.canvas_point(position);
        self.editor.cancel();
        self.drag = None;
        match self.editor.place(kind, point) {
            Ok(_) => {
                self.changed();
                self.editor.tool = Tool::Select;
                self.focus.focus(window, cx);
            }
            Err(error) => self.error(error),
        }
        cx.notify();
    }

    pub fn mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus.focus(window, cx);
        self.popup = None;
        let point = self.canvas_point(event.position);
        let hit = self.editor.hit_test(point, 4.5 / self.editor.viewport.zoom);
        if event.modifiers.platform || event.modifiers.control || event.click_count >= 2 {
            let target = self
                .editor
                .module()
                .gates
                .iter()
                .filter(|gate| gate.kind == GateKind::Comment)
                .find_map(|gate| {
                    rgate_core::rich_comment(&gate.text)
                        .into_iter()
                        .find(|run| run.bounds().contains(point - gate.position))
                        .and_then(|run| run.style.link)
                });
            if let Some(target) = target {
                if target.starts_with("https://") || target.starts_with("http://") {
                    cx.open_url(&target);
                    return;
                }
                if let Some(name) = target.strip_prefix("module:") {
                    self.switch_module(name.into(), cx);
                    return;
                }
                if target.to_ascii_lowercase().ends_with(".v")
                    || target.to_ascii_lowercase().ends_with(".rgate")
                {
                    if self.editor.is_dirty() {
                        self.log("Save your current edits before following a circuit link.".into());
                        return;
                    }
                    #[cfg(not(target_family = "wasm"))]
                    if let Some(parent) = self.file.as_ref().and_then(|path| path.parent()) {
                        match rgate_format::load(&parent.join(&target)) {
                            Ok(result) => {
                                self.replace_circuit(result.circuit, Some(parent.join(&target)));
                            }
                            Err(error) => self.error(error),
                        }
                        return;
                    }
                    self.log(format!("Open tutorial link via File → Open: {target}"));
                    return;
                }
            }
        }
        if event.button == gpui::MouseButton::Right {
            if let Some(Hit::Gate(id) | Hit::Pin(rgate_core::PinRef { gate: id, .. })) = &hit {
                self.editor.select(*id, false);
            }
            self.popup = Some(Popup {
                menu: "Canvas".into(),
                position: Point::new(f32::from(event.position.x), f32::from(event.position.y)),
            });
            cx.notify();
            return;
        }
        if event.button == gpui::MouseButton::Middle || self.editor.tool == Tool::Pan {
            self.drag = Some(Drag::Pan {
                last: Point::new(f32::from(event.position.x), f32::from(event.position.y)),
            });
            cx.notify();
            return;
        }
        if self.tab == WorkspaceTab::Simulate {
            match hit {
                Some(Hit::Wire { net, .. }) if event.click_count >= 2 => self.toggle_probe(net, cx),
                Some(Hit::Gate(id)) => {
                    if event.click_count >= 2
                        && let Some(GateKind::Module(name)) =
                            self.editor.module().gate(id).map(|gate| gate.kind.clone())
                    {
                        let instance = self.editor.module().gate(id).unwrap().name.clone();
                        if let Some(path) = self.debug_path.clone() {
                            self.navigate_scope(format!("{path}/{instance}"), name, cx);
                        }
                        return;
                    }
                    if event.click_count >= 2
                        && self
                            .editor
                            .module()
                            .gate(id)
                            .is_some_and(|gate| gate.kind == GateKind::Vga)
                    {
                        self.dialog = Some(Dialog::Vga { gate: id });
                        cx.notify();
                        return;
                    }
                    if event.click_count >= 2
                        && self
                            .editor
                            .module()
                            .gate(id)
                            .is_some_and(|gate| matches!(gate.kind, GateKind::Ram | GateKind::Rom))
                    {
                        let address = cx.new(|cx| TextField::new("0", cx));
                        address.read(cx).focus_handle(cx).focus(window, cx);
                        self.dialog = Some(Dialog::Memory {
                            gate: id,
                            address,
                            value: cx.new(|cx| TextField::new("00", cx)),
                            error: None,
                        });
                        self.running = false;
                        cx.notify();
                        return;
                    }
                    if event.click_count >= 2
                        && self
                            .editor
                            .module()
                            .gate(id)
                            .is_some_and(|gate| gate.kind == GateKind::Tty)
                    {
                        let input = cx.new(|cx| TextField::new("", cx));
                        input.read(cx).focus_handle(cx).focus(window, cx);
                        self.dialog = Some(Dialog::Terminal {
                            gate: id,
                            input,
                            error: None,
                        });
                        cx.notify();
                        return;
                    }
                    if self
                        .editor
                        .module()
                        .gate(id)
                        .is_some_and(|gate| gate.kind == GateKind::Dip)
                    {
                        let runtime = self.simulation_gate(id).unwrap();
                        let value = self
                            .simulation
                            .as_ref()
                            .unwrap()
                            .gate_value(runtime)
                            .map(|value| value.display_value().trim_start_matches("0x").to_owned())
                            .unwrap_or_else(|| "0".into());
                        let field = cx.new(|cx| TextField::new(value, cx));
                        field.read(cx).focus_handle(cx).focus(window, cx);
                        self.dialog = Some(Dialog::Input {
                            gate: id,
                            value: field,
                            error: None,
                        });
                        self.running = false;
                        cx.notify();
                        return;
                    }
                    if self.editor.module().gate(id).is_some_and(|gate| {
                        matches!(
                            gate.kind,
                            GateKind::Switch | GateKind::Dip | GateKind::Peripheral
                        )
                    }) {
                        let Some(runtime_id) = self.simulation_gate(id) else {
                            self.log("Input is not in this live instance.".into());
                            return;
                        };
                        if let Err(error) =
                            self.simulation.as_mut().unwrap().toggle_input(runtime_id)
                        {
                            self.error(error);
                        }
                        if let Err(error) = self.simulation.as_mut().unwrap().advance(30) {
                            self.error(error);
                        }
                    } else {
                        self.editor.select(id, event.modifiers.shift);
                    }
                }
                Some(Hit::Pin(reference)) => {
                    self.editor.select(reference.gate, event.modifiers.shift)
                }
                Some(Hit::Wire { net, .. }) => self.editor.selected_net = Some(net),
                _ => {}
            }
            cx.notify();
            return;
        }
        if self.editor.tool == Tool::Select
            && self.simulation.is_none()
            && let Some(gate) = self
                .editor
                .module()
                .gates
                .iter()
                .find(|gate| {
                    gate.kind == GateKind::Frame
                        && gate.bounds().max.distance(point) < 10.0 / self.editor.viewport.zoom
                })
                .cloned()
        {
            self.editor.select(gate.id, false);
            self.editor.begin_gesture();
            self.drag = Some(Drag::FrameResize {
                gate: gate.id,
                anchor: point,
                width: gate.config.frame_width,
                height: gate.config.frame_height,
            });
            cx.notify();
            return;
        }
        match self.editor.tool.clone() {
            Tool::Place(kind) => match self.editor.place(kind, point) {
                Ok(_) => self.changed(),
                Err(error) => self.error(error),
            },
            Tool::Delete => match hit {
                Some(Hit::Wire { id, .. }) => {
                    if let Err(error) = self.editor.delete_wire(id) {
                        self.error(error);
                    } else {
                        self.changed();
                    }
                }
                Some(Hit::Gate(id) | Hit::Pin(rgate_core::PinRef { gate: id, .. })) => {
                    self.editor.select(id, false);
                    self.command(Command::Delete, window, cx);
                }
                _ => {}
            },
            Tool::Wire => {
                if self.editor.draft.is_some() {
                    if let Some(hit @ (Hit::Pin(_) | Hit::Wire { .. })) = hit {
                        match self.editor.finish_wire(hit) {
                            Ok(_) => self.changed(),
                            Err(error) => self.error(error),
                        }
                    } else if let Err(error) = self.editor.wire_corner(point) {
                        self.error(error);
                    }
                } else if let Some(hit) = hit {
                    if let Err(error) = self.editor.start_wire(hit) {
                        self.error(error);
                    }
                } else {
                    self.status = "Start a wire by clicking a pin or an existing wire.".into();
                }
            }
            Tool::Select => match hit {
                Some(Hit::Pin(reference)) if event.click_count < 2 => {
                    self.editor.set_tool(Tool::Wire);
                    if let Err(error) = self.editor.start_wire(Hit::Pin(reference)) {
                        self.error(error);
                    }
                }
                Some(Hit::Gate(id) | Hit::Pin(rgate_core::PinRef { gate: id, .. })) => {
                    if !self.editor.selection.contains(&id) || event.modifiers.shift {
                        self.editor.select(id, event.modifiers.shift);
                    }
                    if event.click_count >= 2 {
                        if let Some(GateKind::Module(name)) =
                            self.editor.module().gate(id).map(|gate| gate.kind.clone())
                        {
                            self.switch_module(name, cx);
                        } else {
                            self.show_properties(window, cx);
                        }
                    } else {
                        self.editor.begin_gesture();
                        self.drag = Some(Drag::Gates {
                            last: self.editor.snapped(point),
                        });
                    }
                }
                Some(Hit::Wire { id, net, .. }) => {
                    self.editor.select_wire(id, event.modifiers.shift);
                    if event.click_count >= 2 {
                        self.toggle_probe(net, cx);
                    } else if !event.modifiers.shift {
                        self.drag = self.editor.begin_wire_drag(id, point).map(Drag::Wire);
                        self.status =
                            "Drag wire segment; pins and endpoints stay attached. Esc cancels."
                                .into();
                    }
                }
                None => {
                    if !event.modifiers.shift {
                        self.editor.selection.clear();
                        self.editor.selected_wires.clear();
                        self.editor.selected_net = None;
                    }
                    self.drag = Some(Drag::Marquee {
                        start: point,
                        end: point,
                        additive: event.modifiers.shift,
                    });
                }
            },
            Tool::Pan => {}
        }
        cx.notify();
    }

    pub fn mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        let world = self.canvas_point(event.position);
        self.mouse = world;
        let screen = Point::new(f32::from(event.position.x), f32::from(event.position.y));
        match self.drag.clone() {
            Some(Drag::FrameResize {
                gate,
                anchor,
                width,
                height,
            }) if event.dragging() => self.editor.resize_frame(
                gate,
                width + (world.x - anchor.x) * 2.0,
                height + (world.y - anchor.y) * 2.0,
            ),
            Some(Drag::Wire(drag)) if event.dragging() => {
                self.editor.drag_wire_segment(&drag, world)
            }
            Some(Drag::Gates { last }) if event.dragging() => {
                let current = self.editor.snapped(world);
                self.editor.move_selection(current - last);
                self.drag = Some(Drag::Gates { last: current });
            }
            Some(Drag::Pan { last }) => {
                self.editor.viewport.pan += screen - last;
                self.drag = Some(Drag::Pan { last: screen });
            }
            Some(Drag::Marquee {
                start, additive, ..
            }) if event.dragging() => {
                self.drag = Some(Drag::Marquee {
                    start,
                    end: world,
                    additive,
                })
            }
            Some(Drag::Modules { start_y, height }) if event.dragging() => {
                let maximum = (f32::from(self.sidebar_bounds.size.height) - 110.0).max(80.0);
                self.modules_height = (height + screen.y - start_y).clamp(80.0, maximum);
            }
            Some(Drag::Components { start_x, width }) if event.dragging() => {
                let available = f32::from(self.canvas_bounds.size.width) + self.components_width;
                let maximum = (available - 200.0).clamp(150.0, 600.0);
                self.components_width = (width + start_x - screen.x).clamp(150.0, maximum);
            }
            Some(Drag::Sidebar) if event.dragging() => {
                self.sidebar_width = (screen.x - 8.0).clamp(150.0, 400.0)
            }
            Some(Drag::Bottom) if event.dragging() => {
                self.bottom_height = (f32::from(self.canvas_bounds.bottom()) + self.bottom_height
                    - screen.y)
                    .clamp(80.0, 380.0);
            }
            _ => {}
        }
        self.hover = self.editor.hit_test(world, 4.5 / self.editor.viewport.zoom);
        cx.notify();
    }

    pub fn mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        match self.drag.take() {
            Some(Drag::Gates { .. } | Drag::Wire(_) | Drag::FrameResize { .. }) => {
                self.editor.finish_gesture();
                if self
                    .simulation
                    .as_ref()
                    .is_some_and(|simulator| simulator.module() != self.editor.module())
                {
                    self.changed();
                }
            }
            Some(Drag::Marquee {
                start,
                end,
                additive,
            }) => self
                .editor
                .select_region(Rect::from_points(start, end), additive),
            _ => {}
        }
        cx.notify();
    }

    pub fn scroll(&mut self, event: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.dialog.is_some() || self.popup.is_some() {
            cx.stop_propagation();
            return;
        }
        let delta = event.delta.pixel_delta(px(20.0));
        if event.modifiers.platform || event.modifiers.control {
            let factor = (f32::from(delta.y) / 160.0).exp();
            let anchor = Point::new(
                f32::from(event.position.x) - f32::from(self.canvas_bounds.left()),
                f32::from(event.position.y) - f32::from(self.canvas_bounds.top()),
            );
            self.editor.viewport.zoom_at(factor, anchor);
        } else {
            self.editor.viewport.pan += Point::new(f32::from(delta.x), f32::from(delta.y));
        }
        cx.notify();
    }

    pub fn pinch(&mut self, event: &PinchEvent, _: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        if self.dialog.is_some() || self.popup.is_some() || self.prompting {
            return;
        }
        let factor = 1.0 + event.delta;
        if !factor.is_finite()
            || factor <= 0.0
            || matches!(
                event.phase,
                gpui::TouchPhase::Ended | gpui::TouchPhase::Cancelled
            )
        {
            return;
        }
        let anchor = Point::new(
            f32::from(event.position.x - self.canvas_bounds.left()),
            f32::from(event.position.y - self.canvas_bounds.top()),
        );
        self.editor.viewport.zoom_at(factor, anchor);
        cx.notify();
    }

    pub fn dismiss_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.dialog, Some(Dialog::Recovery)) {
            return;
        }
        self.dialog = None;
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .waveform_search
            .read(cx)
            .focus_handle(cx)
            .is_focused(window)
            && self.dialog.is_none()
        {
            if event.keystroke.key == "escape" {
                self.waveform_search
                    .update(cx, |field, cx| field.set_value(String::new(), cx));
                self.focus.focus(window, cx);
            }
            return;
        }
        if self
            .message_log
            .read(cx)
            .focus_handle(cx)
            .is_focused(window)
            && self.dialog.is_none()
        {
            return;
        }
        if self
            .component_search
            .read(cx)
            .focus_handle(cx)
            .is_focused(window)
            && self.dialog.is_none()
        {
            if event.keystroke.key == "escape" {
                self.component_search
                    .update(cx, |field, cx| field.set_value(String::new(), cx));
                self.focus.focus(window, cx);
            } else if event.keystroke.key == "enter" {
                self.focus.focus(window, cx);
            }
            cx.notify();
            return;
        }

        #[cfg(not(target_family = "wasm"))]
        if self.source_editor_focused(window, cx) && self.dialog.is_none() {
            return;
        }
        if self.dialog.is_some() {
            match event.keystroke.key.as_str() {
                "escape" => {
                    self.dismiss_dialog(window, cx);
                }
                "enter" if !matches!(self.dialog, Some(Dialog::Verilog { .. })) => {
                    self.apply_dialog(window, cx)
                }
                _ => {}
            }
            return;
        }
        if event.keystroke.modifiers.platform || event.keystroke.modifiers.control {
            return;
        }
        let command = match event.keystroke.key.as_str() {
            "escape" => {
                cx.stop_active_drag(window);
                self.editor.cancel();
                self.popup = None;
                self.drag = None;
                cx.notify();
                return;
            }
            "space" => Command::PlayPause,
            "tab" => Command::ClockStep,
            "f6" => Command::Step,
            "backspace" | "delete" => Command::Delete,
            "enter" => Command::Properties,
            "r" => Command::Rotate(!event.keystroke.modifiers.shift),
            "+" | "=" => Command::Zoom(1.25),
            "-" => Command::Zoom(0.8),
            "g" => Command::ToggleGrid,
            "v" => Command::Tool(Tool::Select),
            "w" => Command::Tool(Tool::Wire),
            "p" => Command::Tool(Tool::Pan),
            "d" => Command::Tool(Tool::Delete),
            "a" => Command::Tool(Tool::Place(GateKind::And)),
            "o" => Command::Tool(Tool::Place(GateKind::Or)),
            "x" => Command::Tool(Tool::Place(GateKind::Xor)),
            "n" => Command::Tool(Tool::Place(GateKind::Not)),
            "s" => Command::Tool(Tool::Place(GateKind::Switch)),
            "l" => Command::Tool(Tool::Place(GateKind::Led)),
            "c" => Command::Tool(Tool::Place(GateKind::Clock)),
            "f" => Command::Tool(Tool::Place(GateKind::Dff)),
            "m" => Command::Tool(Tool::Place(GateKind::Mux)),
            "t" => Command::Tool(Tool::Place(GateKind::Comment)),
            "f1" => Command::Help,
            _ => return,
        };
        self.command(command, window, cx);
        cx.stop_propagation();
    }

    pub fn scene(&self) -> crate::canvas::Scene {
        let values = self
            .simulation
            .as_ref()
            .map(|simulator| {
                self.editor
                    .module()
                    .nets
                    .iter()
                    .filter_map(|net| {
                        self.simulation_net(net.id)
                            .and_then(|id| simulator.value(id))
                            .cloned()
                            .map(|value| (net.id, value))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let inputs = self
            .simulation
            .as_ref()
            .map(|simulator| {
                self.editor
                    .module()
                    .gates
                    .iter()
                    .filter(|gate| gate.kind.is_source())
                    .filter_map(|gate| {
                        self.simulation_gate(gate.id)
                            .and_then(|id| simulator.gate_value(id))
                            .cloned()
                            .map(|value| (gate.id, value))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut draft = self
            .editor
            .draft
            .as_ref()
            .map(|draft| draft.points.clone())
            .unwrap_or_default();
        if !draft.is_empty() {
            route_to(&mut draft, self.editor.snapped(self.mouse));
        }
        let ghost = if let Tool::Place(kind) = &self.editor.tool {
            let position = self.editor.snapped(self.mouse);
            let mut gate = if let GateKind::Module(name) = kind {
                self.editor
                    .circuit()
                    .module_instance(name, GateId(0), position)
                    .unwrap_or_else(|_| rgate_core::Gate::new(GateId(0), kind.clone(), position))
            } else {
                rgate_core::Gate::new(GateId(0), kind.clone(), position)
            };
            gate.show_name = false;
            Some(gate)
        } else {
            None
        };
        let marquee = if let Some(Drag::Marquee { start, end, .. }) = self.drag {
            Some(Rect::from_points(start, end))
        } else {
            None
        };
        crate::canvas::Scene {
            theme: self.theme,
            palette: self.theme.palette(),
            module: self.editor.module().clone(),
            viewport: self.editor.viewport.clone(),
            selection: self.editor.selection.clone(),
            selected_net: self.editor.selected_net,
            selected_wires: self.editor.selected_wires.clone(),
            values,
            inputs,
            probes: self
                .editor
                .module()
                .nets
                .iter()
                .filter(|net| self.is_probed(net.id))
                .map(|net| net.id)
                .collect(),
            grid: self.editor.show_grid,
            hover_pin: match &self.hover {
                Some(Hit::Pin(pin)) => Some(pin.clone()),
                _ => None,
            },
            draft,
            ghost,
            marquee,
        }
    }
}

use gpui::Focusable;
