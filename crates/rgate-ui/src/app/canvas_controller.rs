use super::sim_session::SimSession;
use crate::theme::{Palette, Theme};
use gpui::{Bounds, Pixels};
use rgate_core::{GateId, GateKind, Point, Rect, route_to};
use rgate_editor::{Editor, Hit, Tool, Viewport};

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

/// Owns schematic canvas pointer state, active drag gesture, hover hit, bounds, and auto-fit.
pub struct CanvasController {
    pub hover: Option<Hit>,
    pub input_swap: Option<(GateId, Option<String>)>,
    pub mouse: Point,
    pub drag: Option<Drag>,
    pub bounds: Bounds<Pixels>,
    pub need_fit: bool,
    /// The viewport produced by the last automatic fit, while the user has not moved it.
    pub auto_fit: Option<Viewport>,
}

impl Default for CanvasController {
    fn default() -> Self {
        Self {
            hover: None,
            input_swap: None,
            mouse: Point::ZERO,
            drag: None,
            bounds: Bounds::default(),
            need_fit: true,
            auto_fit: None,
        }
    }
}

impl CanvasController {
    pub fn world_point(&self, editor: &Editor, position: gpui::Point<Pixels>) -> Point {
        editor.viewport.to_world(Point::new(
            f32::from(position.x) - f32::from(self.bounds.left()),
            f32::from(position.y) - f32::from(self.bounds.top()),
        ))
    }

    pub fn center_screen(&self) -> Point {
        Point::new(
            f32::from(self.bounds.size.width) / 2.0,
            f32::from(self.bounds.size.height) / 2.0,
        )
    }

    pub fn center_on(&mut self, editor: &mut Editor, world: Point) {
        editor.viewport.pan = self.center_screen() - world * editor.viewport.zoom;
        self.need_fit = false;
    }

    pub fn clear_interaction(&mut self) {
        self.hover = None;
        self.drag = None;
        self.input_swap = None;
    }

    pub fn update_bounds_and_fit(&mut self, editor: &mut Editor, bounds: Bounds<Pixels>) {
        let previous_size = self.bounds.size;
        self.bounds = bounds;
        // The browser canvas grows over several frames after startup; keep
        // refitting until the user moves the view themselves.
        if cfg!(target_family = "wasm")
            && previous_size != bounds.size
            && self.auto_fit.as_ref() == Some(&editor.viewport)
        {
            self.need_fit = true;
        }
        if self.need_fit {
            let content_bounds = editor.module().bounds();
            editor.viewport.fit(
                content_bounds,
                Point::new(
                    f32::from(bounds.size.width),
                    f32::from(bounds.size.height),
                ),
            );
            self.need_fit = false;
            self.auto_fit = Some(editor.viewport.clone());
        }
    }

    pub fn handle_move(
        &mut self,
        editor: &mut Editor,
        position: gpui::Point<Pixels>,
        dragging: bool,
    ) {
        let world = self.world_point(editor, position);
        self.mouse = world;
        let screen = Point::new(f32::from(position.x), f32::from(position.y));
        match self.drag.clone() {
            Some(Drag::FrameResize {
                gate,
                anchor,
                width,
                height,
            }) if dragging => editor.resize_frame(
                gate,
                width + (world.x - anchor.x) * 2.0,
                height + (world.y - anchor.y) * 2.0,
            ),
            Some(Drag::Wire(drag)) if dragging => editor.drag_wire_segment(&drag, world),
            Some(Drag::Gates { last }) if dragging => {
                let current = editor.snapped(world);
                editor.move_selection(current - last);
                self.drag = Some(Drag::Gates { last: current });
            }
            Some(Drag::Pan { last }) => {
                editor.viewport.pan += screen - last;
                self.drag = Some(Drag::Pan { last: screen });
            }
            Some(Drag::Marquee {
                start, additive, ..
            }) if dragging => {
                self.drag = Some(Drag::Marquee {
                    start,
                    end: world,
                    additive,
                })
            }
            _ => {}
        }
        self.hover = editor.hit_test(world, 4.5 / editor.viewport.zoom);
    }

    /// Finish the active drag gesture. Returns `true` if a model-mutating gesture completed.
    pub fn finish_drag(&mut self, editor: &mut Editor) -> bool {
        match self.drag.take() {
            Some(Drag::Gates { .. } | Drag::Wire(_) | Drag::FrameResize { .. }) => {
                editor.finish_gesture();
                true
            }
            Some(Drag::Marquee {
                start,
                end,
                additive,
            }) => {
                editor.select_region(Rect::from_points(start, end), additive);
                false
            }
            _ => false,
        }
    }

    pub fn build_scene(
        &self,
        editor: &Editor,
        sim: &SimSession,
        theme: Theme,
        palette: Palette,
    ) -> crate::canvas::Scene {
        let values = sim
            .live
            .as_ref()
            .map(|simulator| {
                editor
                    .module()
                    .nets
                    .iter()
                    .filter_map(|net| {
                        sim.scoped_net(net.id)
                            .and_then(|id| simulator.value(id))
                            .cloned()
                            .map(|value| (net.id, value))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let inputs = sim
            .live
            .as_ref()
            .map(|simulator| {
                editor
                    .module()
                    .gates
                    .iter()
                    .filter(|gate| gate.kind.is_source())
                    .filter_map(|gate| {
                        sim.scoped_gate(gate.id)
                            .and_then(|id| simulator.gate_value(id))
                            .cloned()
                            .map(|value| (gate.id, value))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut draft = editor
            .draft
            .as_ref()
            .map(|draft| draft.points.clone())
            .unwrap_or_default();
        if !draft.is_empty() {
            route_to(&mut draft, editor.snapped(self.mouse));
        }
        let ghost = if let Tool::Place(kind) = &editor.tool {
            let position = editor.snapped(self.mouse);
            let mut gate = if let GateKind::Module(name) = kind {
                editor
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
            theme,
            palette,
            module: editor.module().clone(),
            viewport: editor.viewport.clone(),
            selection: editor.selection.gates.clone(),
            selected_net: editor.selection.net,
            selected_wires: editor.selection.wires.clone(),
            values,
            inputs,
            probes: editor
                .module()
                .nets
                .iter()
                .filter(|net| sim.is_probed(net.id))
                .map(|net| net.id)
                .collect(),
            grid: editor.show_grid,
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
