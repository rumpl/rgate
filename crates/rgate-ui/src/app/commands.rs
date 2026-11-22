use super::{Dialog, GateApp, WorkspaceTab};
use crate::commands::Command;
#[cfg(not(target_family = "wasm"))]
use gpui::PromptLevel;
use gpui::{Bounds, ClipboardItem, Context, Window};
use gpui_component::input::InputEvent;
use rgate_core::{Circuit, Direction, Module, Point};

impl GateApp {
    pub fn command(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        self.context_menu = None;
        self.input_swap = None;
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
            Some(Dialog::Terminal { .. } | Dialog::Vga { .. } | Dialog::HdlSignals { .. })
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
        if matches!(command, Command::New | Command::Open | Command::Quit) && self.editor.is_dirty()
        {
            self.confirm_discard_and_execute(command, window, cx);
            return;
        }
        self.execute(command, window, cx);
    }

    fn confirm_discard_and_execute(
        &mut self,
        command: Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        #[cfg(target_family = "wasm")]
        {
            if crate::browser::confirm_discard() {
                self.execute(command, window, cx);
            }
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
        }
    }

    fn execute(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(not(target_family = "wasm"))]
        if self.compile_before_command(&command, window, cx) {
            return;
        }
        match command {
            Command::New => self.replace_circuit(Circuit::default(), None),
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
                if let Err(error) = self.editor.delete_selection() {
                    self.error(error);
                } else {
                    self.changed();
                }
            }
            Command::Copy => self.copy_selection(false, cx),
            Command::Cut => self.copy_selection(true, cx),
            Command::Paste => self.paste_clipboard(cx),
            Command::Rotate(clockwise) => {
                if let Err(error) = self.editor.rotate_selection(clockwise) {
                    self.error(error);
                } else {
                    self.changed();
                }
            }
            Command::SwapInputs => self.swap_selected_inputs(),
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
                #[cfg(not(target_family = "wasm"))]
                self.cancel_compilation();
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
                window.refresh();
            }
            Command::SetAppearance(appearance) => self.set_appearance(appearance, window, cx),
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
                    text: crate::input::text_field("Label", window, cx),
                }));
            }
            Command::Find => {
                let query =
                    crate::input::search_field("Gate/net name, type, or module…", window, cx);
                cx.subscribe(&query, |_, _, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        cx.notify();
                    }
                })
                .detach();
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
            Command::TidyLayout => match self.editor.tidy_layout() {
                Ok(()) => {
                    self.changed();
                    self.need_fit = true;
                    self.log(
                        "Tidied component placement and wire routes; Undo restores both.".into(),
                    );
                }
                Err(error) => self.error(error),
            },
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
            Command::HdlSignals => {
                if self.ensure_simulator() {
                    let query =
                        crate::input::search_field("Filter hierarchical signals…", window, cx);
                    cx.subscribe(&query, |_, _, event: &InputEvent, cx| {
                        if matches!(event, InputEvent::Change) {
                            cx.notify();
                        }
                    })
                    .detach();
                    self.dialog = Some(Dialog::HdlSignals { query });
                }
            }
            Command::SetHdlInput => {
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
                        value: crate::input::text_field("0", window, cx),
                        error: None,
                    });
                } else {
                    self.log("Select an input net in a live Verilog source module.".into());
                }
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
                let field = crate::input::text_field(format!("module{index}"), window, cx);
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

    fn copy_selection(&mut self, cut: bool, cx: &mut Context<Self>) {
        let copied = self.editor.copy_selection();
        if copied.gates.is_empty() {
            return;
        }
        match serde_json::to_string(&copied) {
            Ok(text) => cx.write_to_clipboard(ClipboardItem::new_string(format!(
                "RGATE-CLIPBOARD\n{text}"
            ))),
            Err(error) => self.error(error),
        }
        if cut {
            if let Err(error) = self.editor.delete_selection() {
                self.error(error);
            } else {
                self.changed();
            }
        }
    }

    fn paste_clipboard(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        let Some(json) = text.strip_prefix("RGATE-CLIPBOARD\n") else {
            self.log("Clipboard does not contain RGate components.".into());
            return;
        };
        match serde_json::from_str::<Module>(json) {
            Ok(module) => match self.editor.paste(&module, Point::new(20.0, 20.0)) {
                Ok(()) => self.changed(),
                Err(error) => self.error(error),
            },
            Err(error) => self.error(error),
        }
    }

    fn swap_selected_inputs(&mut self) {
        if self.simulation.is_some() {
            self.log("Stop simulation before swapping inputs.".into());
            return;
        }
        if self.editor.selection.len() != 1 {
            return;
        }
        let id = *self.editor.selection.iter().next().unwrap();
        let inputs = self
            .editor
            .module()
            .gate(id)
            .unwrap()
            .pins
            .iter()
            .filter(|pin| pin.direction == Direction::Input)
            .map(|pin| pin.name.clone())
            .collect::<Vec<_>>();
        if inputs.len() == 2 {
            match self.editor.swap_inputs(id, &inputs[0], &inputs[1]) {
                Ok(()) => {
                    self.changed();
                    self.log("Input connections swapped.".into());
                }
                Err(error) => self.error(error),
            }
        } else if inputs.len() > 2 {
            self.editor.cancel();
            self.drag = None;
            self.input_swap = Some((id, None));
            self.status = "Swap inputs: click the first input pin; Escape cancels.".into();
        }
    }
}
