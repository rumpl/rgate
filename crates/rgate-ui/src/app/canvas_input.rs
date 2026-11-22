use super::{Dialog, Drag, GateApp, WorkspaceTab};
use crate::commands::Command;
use gpui::{
    Context, Focusable, KeyDownEvent, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PinchEvent,
    Pixels, ScrollEvent, Window, px,
};
use gpui_component::WindowExt as _;
use rgate_core::{Direction, GateId, GateKind, Point, Rect, route_to};
use rgate_editor::{Hit, Tool};

impl GateApp {
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
            || self.context_menu.is_some()
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
        self.context_menu = None;
        let point = self.canvas_point(event.position);
        let hit = self.editor.hit_test(point, 4.5 / self.editor.viewport.zoom);

        if self.handle_input_swap_click(event, &hit, cx) {
            return;
        }
        if self.try_open_comment_link(event, point, cx) {
            return;
        }
        if event.button == gpui::MouseButton::Right {
            self.handle_right_click(event, &hit, window, cx);
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
            self.handle_simulation_click(event, hit, window, cx);
            return;
        }
        if self.try_start_frame_resize(point, cx) {
            return;
        }
        self.handle_edit_click(event, point, hit, window, cx);
    }

    fn handle_input_swap_click(
        &mut self,
        event: &MouseDownEvent,
        hit: &Option<Hit>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((id, first)) = self.input_swap.clone() else {
            return false;
        };
        if event.button == gpui::MouseButton::Left {
            if let Some(Hit::Pin(reference)) = hit
                && reference.gate == id
                && self
                    .editor
                    .module()
                    .pin(reference)
                    .is_some_and(|pin| pin.direction == Direction::Input)
            {
                if let Some(first) = first {
                    match self.editor.swap_inputs(id, &first, &reference.pin) {
                        Ok(()) => {
                            self.changed();
                            self.log("Input connections swapped.".into());
                        }
                        Err(error) => self.error(error),
                    }
                } else {
                    self.input_swap = Some((id, Some(reference.pin.clone())));
                    let net = self
                        .editor
                        .module()
                        .pin(reference)
                        .and_then(|pin| pin.net)
                        .and_then(|net| self.editor.module().net(net))
                        .map_or("unconnected", |net| net.name.as_str());
                    self.status = format!(
                        "Swap inputs: {} ({net}) selected; click the second input pin. Escape cancels.",
                        reference.pin
                    );
                }
            }
            cx.notify();
            return true;
        }
        if event.button == gpui::MouseButton::Right {
            self.input_swap = None;
        }
        false
    }

    fn try_open_comment_link(
        &mut self,
        event: &MouseDownEvent,
        point: Point,
        cx: &mut Context<Self>,
    ) -> bool {
        if !(event.modifiers.platform || event.modifiers.control || event.click_count >= 2) {
            return false;
        }
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
        let Some(target) = target else {
            return false;
        };
        if target.starts_with("https://") || target.starts_with("http://") {
            cx.open_url(&target);
            return true;
        }
        if let Some(name) = target.strip_prefix("module:") {
            self.switch_module(name.into(), cx);
            return true;
        }
        if target.to_ascii_lowercase().ends_with(".v")
            || target.to_ascii_lowercase().ends_with(".rgate")
        {
            if self.editor.is_dirty() {
                self.log("Save your current edits before following a circuit link.".into());
                return true;
            }
            #[cfg(not(target_family = "wasm"))]
            if let Some(parent) = self.file.as_ref().and_then(|path| path.parent()) {
                match rgate_format::load(&parent.join(&target)) {
                    Ok(result) => {
                        self.replace_circuit(result.circuit, Some(parent.join(&target)));
                    }
                    Err(error) => self.error(error),
                }
                return true;
            }
            self.log(format!("Open tutorial link via File → Open: {target}"));
            return true;
        }
        false
    }

    fn handle_right_click(
        &mut self,
        event: &MouseDownEvent,
        hit: &Option<Hit>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match hit {
            Some(Hit::Gate(id) | Hit::Pin(rgate_core::PinRef { gate: id, .. })) => {
                self.editor.select(*id, false)
            }
            Some(Hit::Wire { net, id, .. }) => {
                self.editor.selection.clear();
                self.editor.selected_wires.clear();
                self.editor.selected_wires.insert(*id);
                self.editor.selected_net = Some(*net);
            }
            _ => {
                self.editor.selection.clear();
                self.editor.selected_wires.clear();
                self.editor.selected_net = None;
            }
        }
        self.open_canvas_menu(event.position, window, cx);
        cx.notify();
    }

    fn handle_simulation_click(
        &mut self,
        event: &MouseDownEvent,
        hit: Option<Hit>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
                    let address = crate::input::text_field("0", window, cx);
                    self.dialog = Some(Dialog::Memory {
                        gate: id,
                        address,
                        value: crate::input::text_field("00", window, cx),
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
                    let input = crate::input::text_field("", window, cx);
                    self.dialog = Some(Dialog::Terminal {
                        gate: id,
                        input,
                        error: None,
                    });
                    cx.notify();
                    return;
                }
                if self.editor.module().gate(id).is_some_and(|gate| {
                    gate.kind == GateKind::Dip
                        || (gate.kind == GateKind::InputPort && gate.width > 1)
                }) && self
                    .simulation_gate(id)
                    .is_some_and(|runtime| self.simulation.as_ref().unwrap().is_input(runtime))
                {
                    let runtime = self.simulation_gate(id).unwrap();
                    let value = self
                        .simulation
                        .as_ref()
                        .unwrap()
                        .gate_value(runtime)
                        .map(|value| value.display_value().trim_start_matches("0x").to_owned())
                        .unwrap_or_else(|| "0".into());
                    let field = crate::input::text_field(value, window, cx);
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
                        GateKind::Switch
                            | GateKind::Dip
                            | GateKind::Peripheral
                            | GateKind::InputPort
                    )
                }) {
                    let Some(runtime_id) = self.simulation_gate(id) else {
                        self.log("Input is not in this live instance.".into());
                        return;
                    };
                    if !self.simulation.as_ref().unwrap().is_input(runtime_id) {
                        self.editor.select(id, event.modifiers.shift);
                        cx.notify();
                        return;
                    }
                    if let Err(error) = self.simulation.as_mut().unwrap().toggle_input(runtime_id) {
                        self.error(error);
                    }
                    if let Err(error) = self.simulation.as_mut().unwrap().advance(30) {
                        self.error(error);
                    }
                } else {
                    self.editor.select(id, event.modifiers.shift);
                }
            }
            Some(Hit::Pin(reference)) => self.editor.select(reference.gate, event.modifiers.shift),
            Some(Hit::Wire { net, .. }) => self.editor.selected_net = Some(net),
            _ => {}
        }
        cx.notify();
    }

    fn try_start_frame_resize(&mut self, point: Point, cx: &mut Context<Self>) -> bool {
        if self.editor.tool != Tool::Select || self.simulation.is_some() {
            return false;
        }
        let Some(gate) = self
            .editor
            .module()
            .gates
            .iter()
            .find(|gate| {
                gate.kind == GateKind::Frame
                    && gate.bounds().max.distance(point) < 10.0 / self.editor.viewport.zoom
            })
            .cloned()
        else {
            return false;
        };
        self.editor.select(gate.id, false);
        self.editor.begin_gesture();
        self.drag = Some(Drag::FrameResize {
            gate: gate.id,
            anchor: point,
            width: gate.config.frame_width,
            height: gate.config.frame_height,
        });
        cx.notify();
        true
    }

    fn handle_edit_click(
        &mut self,
        event: &MouseDownEvent,
        point: Point,
        hit: Option<Hit>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

    pub fn scroll(&mut self, event: &ScrollEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.dialog.is_some() || self.context_menu.is_some() {
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
        if self.dialog.is_some() || self.context_menu.is_some() || self.prompting {
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

    pub fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.dialog.is_some() {
            return;
        }
        if window.has_focused_input(cx) {
            if event.keystroke.key == "escape" {
                for search in [self.waveform_search.clone(), self.component_search.clone()] {
                    if search.read(cx).focus_handle(cx).is_focused(window) {
                        search.update(cx, |field, cx| field.set_value("", window, cx));
                        self.focus.focus(window, cx);
                    }
                }
            } else if event.keystroke.key == "enter"
                && self
                    .component_search
                    .read(cx)
                    .focus_handle(cx)
                    .is_focused(window)
            {
                self.focus.focus(window, cx);
            }
            cx.notify();
            return;
        }
        if event.keystroke.modifiers.platform || event.keystroke.modifiers.control {
            return;
        }
        let command = match event.keystroke.key.as_str() {
            "escape" => {
                cx.stop_active_drag(window);
                self.editor.cancel();
                self.input_swap = None;
                self.status = "Ready".into();
                self.context_menu = None;
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
            palette: self.palette(),
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
            swap_pin: self.input_swap.as_ref().and_then(|(gate, pin)| {
                pin.as_ref().map(|pin| rgate_core::PinRef::new(*gate, pin))
            }),
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
