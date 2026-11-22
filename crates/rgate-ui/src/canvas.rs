use crate::{theme, view::mono_font};
use gpui::{
    App, Bounds, ContentMask, Hsla, PathBuilder, Pixels, Window, fill, font, point, px, quad, rgb,
    size,
};
use rgate_core::{Gate, GateId, GateKind, Module, NetId, Point, Rect, Signal};
use rgate_editor::Viewport;
use std::collections::{BTreeSet, HashMap, HashSet};

pub struct Scene {
    pub theme: theme::Theme,
    pub palette: theme::Palette,
    pub module: Module,
    pub viewport: Viewport,
    pub selection: BTreeSet<GateId>,
    pub selected_net: Option<NetId>,
    pub selected_wires: BTreeSet<rgate_core::WireId>,
    pub values: HashMap<NetId, Signal>,
    pub inputs: HashMap<GateId, Signal>,
    pub probes: BTreeSet<NetId>,
    pub grid: bool,
    pub hover_pin: Option<rgate_core::PinRef>,
    pub swap_pin: Option<rgate_core::PinRef>,
    pub draft: Vec<Point>,
    pub ghost: Option<Gate>,
    pub marquee: Option<Rect>,
}

pub fn screen(position: Point, bounds: Bounds<Pixels>, viewport: &Viewport) -> gpui::Point<Pixels> {
    let position = viewport.to_screen(position);
    bounds.origin + point(px(position.x), px(position.y))
}

pub fn paint(bounds: Bounds<Pixels>, scene: Scene, window: &mut Window, cx: &mut App) {
    let painter = CanvasPainter {
        scene: &scene,
        bounds,
    };
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        painter.paint(window, cx);
    });
}

struct CanvasPainter<'a> {
    scene: &'a Scene,
    bounds: Bounds<Pixels>,
}

impl<'a> CanvasPainter<'a> {
    fn screen(&self, position: Point) -> gpui::Point<Pixels> {
        screen(position, self.bounds, &self.scene.viewport)
    }

    fn zoom(&self) -> f32 {
        self.scene.viewport.zoom
    }

    fn rect(&self, rectangle: Rect, color: Hsla, dashed: bool, window: &mut Window) {
        rect(
            rectangle,
            self.bounds,
            &self.scene.viewport,
            color,
            dashed,
            window,
        );
    }

    fn paint(&self, window: &mut Window, cx: &mut App) {
        window.paint_quad(fill(self.bounds, rgb(self.scene.palette.panel)));
        if self.scene.grid {
            self.paint_grid(window);
        }
        self.paint_frames(window, cx);
        self.paint_wires(window, cx);
        self.paint_junctions(window);
        self.paint_gates(window, cx);
        self.paint_overlays(window, cx);
    }

    fn paint_grid(&self, window: &mut Window) {
        let min = self.scene.viewport.to_world(Point::ZERO);
        let max = self.scene.viewport.to_world(Point::new(
            f32::from(self.bounds.size.width),
            f32::from(self.bounds.size.height),
        ));
        let step = if self.zoom() < 0.7 { 20.0 } else { 10.0 };
        let mut x = (min.x / step).ceil() * step;
        while x <= max.x {
            let mut y = (min.y / step).ceil() * step;
            while y <= max.y {
                let p = self.screen(Point::new(x, y));
                window.paint_quad(fill(
                    Bounds::new(p, size(px(1.0), px(1.0))),
                    rgb(self.scene.palette.grid),
                ));
                y += step;
            }
            x += step;
        }
    }

    fn paint_frames(&self, window: &mut Window, cx: &mut App) {
        for gate in self
            .scene
            .module
            .gates
            .iter()
            .filter(|gate| gate.kind == GateKind::Frame)
        {
            self.rect(
                gate.bounds(),
                rgb(self.scene.palette.muted).into(),
                false,
                window,
            );
            text(
                if gate.text.is_empty() {
                    &gate.name
                } else {
                    &gate.text
                },
                self.screen(gate.bounds().min + Point::new(6.0, 4.0)),
                11.0 * self.zoom(),
                rgb(self.scene.palette.text).into(),
                window,
                cx,
            );
        }
    }

    fn paint_wires(&self, window: &mut Window, cx: &mut App) {
        let mut labels = HashSet::new();
        let mut occupied_labels = Vec::new();
        for wire in &self.scene.module.wires {
            let definition = self.scene.module.net(wire.net).unwrap();
            let color = if self.scene.selected_wires.contains(&wire.id)
                || (self.scene.selected_wires.is_empty()
                    && self.scene.selected_net == Some(wire.net))
            {
                rgb(self.scene.palette.selected_wire).into()
            } else if let Some(value) = self.scene.values.get(&wire.net) {
                self.scene.palette.signal_color(value)
            } else {
                rgb(if definition.width > 1 {
                    self.scene.palette.bus
                } else {
                    self.scene.palette.wire
                })
                .into()
            };
            let width = if definition.width > 1 { 2.2 } else { 1.0 } * self.zoom().max(0.85);
            stroke(
                wire.points.iter().map(|p| self.screen(*p)),
                width,
                color,
                false,
                window,
            );
            if definition.show_name && !labels.contains(&wire.net) {
                let label = if let Some(value) = self.scene.values.get(&wire.net) {
                    format!("{} = {}", definition.name, value)
                } else {
                    definition.name.clone()
                };
                if let Some(location) = crate::label_layout::wire_label(
                    &self.scene.module,
                    wire,
                    &label,
                    &occupied_labels,
                ) {
                    labels.insert(wire.net);
                    occupied_labels.push(
                        crate::label_layout::label_rect(location, &label, 10.0).expanded(2.0),
                    );
                    text(
                        &label,
                        self.screen(location),
                        10.0 * self.zoom(),
                        color,
                        window,
                        cx,
                    );
                }
            }
            if self.scene.probes.contains(&wire.net) && !wire.points.is_empty() {
                let position = self.screen(wire.points[wire.points.len() / 2]);
                circle(
                    position,
                    3.0,
                    rgb(0xffc800).into(),
                    rgb(self.scene.palette.text).into(),
                    window,
                );
            }
        }
    }

    fn paint_junctions(&self, window: &mut Window) {
        let endpoints = self
            .scene
            .module
            .wires
            .iter()
            .flat_map(|wire| {
                let start = wire.start.is_none().then_some((wire.net, wire.points[0]));
                let end = wire
                    .end
                    .is_none()
                    .then_some((wire.net, *wire.points.last().unwrap()));
                [start, end].into_iter().flatten()
            })
            .collect::<Vec<_>>();
        let mut painted = HashSet::new();
        for &(net, location) in &endpoints {
            let nearby = endpoints
                .iter()
                .filter(|(other, point)| *other == net && point.distance(location) <= 4.5)
                .map(|(_, point)| *point)
                .collect::<Vec<_>>();
            if nearby.len() < 2 {
                continue;
            }
            let center = nearby
                .iter()
                .copied()
                .fold(Point::ZERO, |sum, point| sum + point)
                / nearby.len() as f32;
            let key = (
                net,
                (center.x / 4.0).round() as i32,
                (center.y / 4.0).round() as i32,
            );
            if painted.insert(key) {
                let color = self
                    .scene
                    .values
                    .get(&net)
                    .map(|value| self.scene.palette.signal_color(value))
                    .unwrap_or_else(|| rgb(self.scene.palette.wire).into());
                circle(
                    self.screen(center),
                    1.8 * self.zoom(),
                    color,
                    color,
                    window,
                );
            }
        }
    }

    fn paint_gates(&self, window: &mut Window, cx: &mut App) {
        for gate in &self.scene.module.gates {
            if self.scene.selection.contains(&gate.id) {
                self.paint_selection_box(gate.bounds().expanded(5.0), window);
            }
            if gate.kind != GateKind::Frame {
                self.paint_gate(gate, window, cx);
            }
        }
    }

    fn paint_selection_box(&self, rectangle: Rect, window: &mut Window) {
        self.rect(rectangle, rgb(self.scene.palette.gate).into(), true, window);
        for corner in [
            rectangle.min,
            rectangle.max,
            Point::new(rectangle.min.x, rectangle.max.y),
            Point::new(rectangle.max.x, rectangle.min.y),
        ] {
            let position = self.screen(corner) - point(px(2.0), px(2.0));
            window.paint_quad(fill(
                Bounds::new(position, size(px(4.0), px(4.0))),
                rgb(self.scene.palette.gate),
            ));
        }
    }

    fn paint_overlays(&self, window: &mut Window, cx: &mut App) {
        if !self.scene.draft.is_empty() {
            stroke(
                self.scene.draft.iter().map(|p| self.screen(*p)),
                1.5,
                rgb(self.scene.palette.selected_wire).into(),
                true,
                window,
            );
        }
        if let Some(gate) = &self.scene.ghost {
            self.paint_gate(gate, window, cx);
            self.rect(
                gate.bounds().expanded(5.0),
                rgb(self.scene.palette.muted).into(),
                true,
                window,
            );
        }
        if let Some(marquee) = self.scene.marquee {
            let origin = self.screen(marquee.min);
            let end = self.screen(marquee.max);
            window.paint_quad(fill(
                Bounds::from_corners(origin, end),
                gpui::rgba((self.scene.palette.gate << 8) | 0x1f),
            ));
            self.rect(marquee, rgb(self.scene.palette.gate).into(), true, window);
        }
    }

    fn paint_gate(&self, gate: &Gate, window: &mut Window, cx: &mut App) {
        if gate.kind == GateKind::Comment {
            self.paint_comment(gate, window, cx);
            return;
        }
        self.paint_gate_body(gate, window, cx);
        if matches!(gate.kind, GateKind::Module(_)) && gate.config.custom_symbol.is_empty() {
            self.paint_module_leads(gate, window);
        }
        self.paint_gate_pins(gate, window, cx);
        self.paint_gate_annotations(gate, window, cx);
    }

    fn paint_comment(&self, gate: &Gate, window: &mut Window, cx: &mut App) {
        let zoom = self.zoom();
        for run in rgate_core::rich_comment(&gate.text) {
            let origin = self.screen(gate.position + run.position);
            if run.image.is_some() {
                text(
                    &format!("[{}]", run.text),
                    origin,
                    run.style.size * zoom,
                    rgb(self.scene.palette.muted).into(),
                    window,
                    cx,
                );
                continue;
            }
            let family = if run.style.monospace {
                mono_font()
            } else {
                ui_font()
            };
            let mut font = font(family);
            if run.style.bold {
                font.weight = gpui::FontWeight::BOLD;
            }
            if run.style.italic {
                font.style = gpui::FontStyle::Italic;
            }
            let color = rgb(run.style.color.unwrap_or(self.scene.palette.text)).into();
            let styled = gpui::TextRun {
                len: run.text.len(),
                font,
                color,
                background_color: None,
                underline: run.style.link.as_ref().map(|_| gpui::UnderlineStyle {
                    color: Some(color),
                    thickness: px(1.0),
                    wavy: false,
                }),
                strikethrough: None,
            };
            let line = window.text_system().shape_line(
                run.text.into(),
                px(run.style.size * zoom),
                &[styled],
                None,
            );
            if let Err(error) = line.paint(
                origin,
                px(run.size.y * zoom),
                gpui::TextAlign::Left,
                None,
                window,
                cx,
            ) {
                eprintln!("comment text: {error}");
            }
        }
    }

    fn paint_gate_body(&self, gate: &Gate, window: &mut Window, cx: &mut App) {
        let zoom = self.zoom();
        if !gate.config.custom_symbol.is_empty() {
            crate::symbol_editor::paint_shapes(
                &gate.config.custom_symbol,
                |p| self.screen(gate.position + p.rotated(gate.rotation)),
                zoom,
                self.scene.palette.module,
                self.scene.palette.panel,
                window,
                cx,
            );
        } else if gate.kind.port_direction().is_some() {
            crate::modern_gates::paint(gate, self.scene, self.bounds, window, cx);
        } else if gate.is_compact_bus_join() {
            self.paint_compact_bus_join(gate, window);
        } else if self.scene.theme != theme::Theme::Classic {
            crate::modern_gates::paint(gate, self.scene, self.bounds, window, cx);
        } else if crate::classic_gates::supports(&gate.kind) {
            crate::classic_gates::paint(gate, self.scene, self.bounds, window, cx);
        } else {
            let rectangle = if matches!(gate.kind, GateKind::Module(_)) {
                gate.module_body_bounds()
            } else {
                gate.bounds()
            };
            let origin = self.screen(rectangle.min);
            let extent = rectangle.size() * zoom;
            window.paint_quad(quad(
                Bounds::new(origin, size(px(extent.x), px(extent.y))),
                px(0.0),
                rgb(self.scene.palette.panel),
                px(zoom),
                rgb(self.scene.palette.module),
                Default::default(),
            ));
            text(
                gate.kind.name(),
                self.screen(crate::label_layout::block_title(gate)),
                9.0 * zoom,
                rgb(self.scene.palette.module).into(),
                window,
                cx,
            );
        }
    }

    fn paint_compact_bus_join(&self, gate: &Gate, window: &mut Window) {
        let zoom = self.zoom();
        let transform = |local: Point| self.screen(gate.position + local.rotated(gate.rotation));
        let height = gate
            .pins
            .iter()
            .map(|pin| pin.offset.y.abs())
            .fold(2.0, f32::max)
            + 1.0;
        let color: Hsla = rgb(self.scene.palette.gate).into();
        stroke(
            [
                transform(Point::new(-5.0, -height)),
                transform(Point::new(-2.0, -height)),
                transform(Point::new(-2.0, height)),
                transform(Point::new(-5.0, height)),
            ],
            zoom,
            color,
            false,
            window,
        );
        if let Some(pin) = gate.pin("Z") {
            stroke(
                [
                    transform(Point::new(-2.0, 0.0)),
                    self.screen(gate.pin_position(pin)),
                ],
                zoom,
                color,
                false,
                window,
            );
        }
        for pin in gate.pins.iter().filter(|pin| pin.name != "Z") {
            stroke(
                [
                    self.screen(gate.pin_position(pin)),
                    transform(Point::new(-2.0, pin.offset.y)),
                ],
                zoom,
                color,
                false,
                window,
            );
        }
    }

    fn paint_module_leads(&self, gate: &Gate, window: &mut Window) {
        let zoom = self.zoom();
        for pin in &gate.pins {
            let inward = Point::new(-pin.offset.x.signum() * 6.0, 0.0);
            stroke(
                [
                    self.screen(gate.pin_position(pin)),
                    self.screen(gate.position + (pin.offset + inward).rotated(gate.rotation)),
                ],
                module_lead_width(gate.pin_width(&pin.name), zoom),
                rgb(self.scene.palette.module).into(),
                false,
                window,
            );
        }
    }

    fn paint_gate_pins(&self, gate: &Gate, window: &mut Window, cx: &mut App) {
        let zoom = self.zoom();
        let show_pin_labels = (matches!(gate.kind, GateKind::Module(_))
            || (gate.kind.is_extended() && !gate.is_compact_bus_join()))
            && !(self.scene.theme != theme::Theme::Classic && gate.kind == GateKind::Jkff);
        let padding = if matches!(gate.kind, GateKind::Module(_))
            && gate.config.custom_symbol.is_empty()
        {
            10.0
        } else {
            4.0
        };
        for pin in &gate.pins {
            if show_pin_labels {
                let label = format!(
                    "{}{}",
                    pin.name,
                    pin.width
                        .filter(|width| *width > 1)
                        .map(|width| format!(" [{}:0]", width - 1))
                        .unwrap_or_default()
                );
                let offset = if pin.direction == rgate_core::Direction::Output {
                    let run = gpui::TextRun {
                        len: label.len(),
                        font: font(ui_font()),
                        color: rgb(self.scene.palette.module).into(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    let shaped = window.text_system().shape_line(
                        label.clone().into(),
                        px((8.0 * zoom).clamp(6.0, 64.0)),
                        &[run],
                        None,
                    );
                    Point::new(
                        pin.offset.x - f32::from(shaped.width) / zoom - padding,
                        pin.offset.y - 5.0,
                    )
                } else {
                    Point::new(pin.offset.x + padding, pin.offset.y - 5.0)
                };
                text(
                    &label,
                    self.screen(gate.position + offset.rotated(gate.rotation)),
                    8.0 * zoom,
                    rgb(self.scene.palette.module).into(),
                    window,
                    cx,
                );
            }
            let reference = rgate_core::PinRef::new(gate.id, &pin.name);
            let position = self.screen(gate.pin_position(pin));
            if self.scene.hover_pin.as_ref() == Some(&reference)
                || self.scene.swap_pin.as_ref() == Some(&reference)
            {
                circle(
                    position,
                    5.0,
                    rgb(0xffffa0).into(),
                    rgb(self.scene.palette.wire).into(),
                    window,
                );
            } else if pin.net.is_none() {
                circle(
                    position,
                    3.2,
                    rgb(self.scene.palette.panel).into(),
                    rgb(self.scene.palette.wire).into(),
                    window,
                );
            }
        }
    }

    fn paint_gate_annotations(&self, gate: &Gate, window: &mut Window, cx: &mut App) {
        let zoom = self.zoom();
        if gate.kind == GateKind::Peripheral {
            let incoming = gate
                .pin("I")
                .and_then(|pin| pin.net)
                .and_then(|net| self.scene.values.get(&net))
                .map(ToString::to_string)
                .unwrap_or_else(|| "Z".into());
            let outgoing = self.scene.inputs.get(&gate.id).unwrap_or(&gate.initial);
            text(
                &format!("in:{incoming} out:{outgoing}"),
                self.screen(gate.position + Point::new(-43.0, gate.bounds().size().y / 2.0 + 3.0)),
                8.0 * zoom,
                rgb(self.scene.palette.module).into(),
                window,
                cx,
            );
        }
        if gate.show_name {
            text(
                &gate.name,
                self.screen(crate::label_layout::instance_label(gate)),
                10.5 * zoom,
                rgb(self.scene.palette.gate).into(),
                window,
                cx,
            );
        }
        if gate.kind == GateKind::Led
            && gate.config.led_display == rgate_core::LedDisplay::Bit
            && gate.width > 1
            && let Some(value) = gate
                .pins
                .first()
                .and_then(|pin| pin.net)
                .and_then(|net| self.scene.values.get(&net))
        {
            text(
                &value.display_value(),
                self.screen(gate.position + Point::new(10.0, 0.0)),
                9.0 * zoom,
                rgb(self.scene.palette.gate).into(),
                window,
                cx,
            );
        }
    }
}

fn ui_font() -> &'static str {
    if cfg!(target_family = "wasm") {
        "IBM Plex Sans"
    } else {
        "Helvetica"
    }
}

pub fn text(
    content: &str,
    position: gpui::Point<Pixels>,
    font_size: f32,
    color: Hsla,
    window: &mut Window,
    cx: &mut App,
) {
    if content.is_empty() {
        return;
    }
    let run = gpui::TextRun {
        len: content.len(),
        font: font(ui_font()),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let line = window.text_system().shape_line(
        content.to_owned().into(),
        px(font_size.clamp(6.0, 64.0)),
        &[run],
        None,
    );
    if let Err(error) = line.paint(
        position,
        px(font_size * 1.3),
        gpui::TextAlign::Left,
        None,
        window,
        cx,
    ) {
        eprintln!("schematic text paint failed: {error}");
    }
}

pub fn stroke(
    points: impl IntoIterator<Item = gpui::Point<Pixels>>,
    width: f32,
    color: Hsla,
    dashed: bool,
    window: &mut Window,
) {
    let mut builder = PathBuilder::stroke(px(width));
    if dashed {
        builder = builder.dash_array(&[px(4.0), px(3.0)]);
    }
    for (index, point) in points.into_iter().enumerate() {
        if index == 0 {
            builder.move_to(point);
        } else {
            builder.line_to(point);
        }
    }
    match builder.build() {
        Ok(path) => window.paint_path(path, color),
        Err(error) => eprintln!("schematic path paint failed: {error}"),
    }
}

pub fn circle(
    position: gpui::Point<Pixels>,
    radius: f32,
    background: Hsla,
    border: Hsla,
    window: &mut Window,
) {
    window.paint_quad(quad(
        Bounds::new(
            position - point(px(radius), px(radius)),
            size(px(radius * 2.0), px(radius * 2.0)),
        ),
        px(radius),
        background,
        px(1.0),
        border,
        Default::default(),
    ));
}

fn rect(
    rectangle: Rect,
    bounds: Bounds<Pixels>,
    viewport: &Viewport,
    color: Hsla,
    dashed: bool,
    window: &mut Window,
) {
    stroke(
        [
            rectangle.min,
            Point::new(rectangle.max.x, rectangle.min.y),
            rectangle.max,
            Point::new(rectangle.min.x, rectangle.max.y),
            rectangle.min,
        ]
        .into_iter()
        .map(|point| screen(point, bounds, viewport)),
        1.0,
        color,
        dashed,
        window,
    );
}

fn module_lead_width(bits: u16, zoom: f32) -> f32 {
    (if bits > 1 { 2.2 } else { 1.0 }) * zoom.max(0.85)
}
