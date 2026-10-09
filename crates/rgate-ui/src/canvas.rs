use crate::theme;
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
    pub draft: Vec<Point>,
    pub ghost: Option<Gate>,
    pub marquee: Option<Rect>,
}

pub fn screen(position: Point, bounds: Bounds<Pixels>, viewport: &Viewport) -> gpui::Point<Pixels> {
    let position = viewport.to_screen(position);
    bounds.origin + point(px(position.x), px(position.y))
}

pub fn paint(bounds: Bounds<Pixels>, scene: Scene, window: &mut Window, cx: &mut App) {
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        window.paint_quad(fill(bounds, rgb(scene.palette.panel)));
        if scene.grid {
            let min = scene.viewport.to_world(Point::ZERO);
            let max = scene.viewport.to_world(Point::new(
                f32::from(bounds.size.width),
                f32::from(bounds.size.height),
            ));
            let step = if scene.viewport.zoom < 0.7 {
                20.0
            } else {
                10.0
            };
            let mut x = (min.x / step).ceil() * step;
            while x <= max.x {
                let mut y = (min.y / step).ceil() * step;
                while y <= max.y {
                    let p = screen(Point::new(x, y), bounds, &scene.viewport);
                    window.paint_quad(fill(
                        Bounds::new(p, size(px(1.0), px(1.0))),
                        rgb(scene.palette.grid),
                    ));
                    y += step;
                }
                x += step;
            }
        }
        for gate in scene
            .module
            .gates
            .iter()
            .filter(|gate| gate.kind == GateKind::Frame)
        {
            rect(
                gate.bounds(),
                bounds,
                &scene.viewport,
                rgb(scene.palette.muted).into(),
                false,
                window,
            );
            text(
                if gate.text.is_empty() {
                    &gate.name
                } else {
                    &gate.text
                },
                screen(
                    gate.bounds().min + Point::new(6.0, 4.0),
                    bounds,
                    &scene.viewport,
                ),
                11.0 * scene.viewport.zoom,
                rgb(scene.palette.text).into(),
                window,
                cx,
            );
        }
        let mut labels = HashSet::new();
        let mut occupied_labels = Vec::new();
        for wire in &scene.module.wires {
            let definition = scene.module.net(wire.net).unwrap();
            let color = if scene.selected_wires.contains(&wire.id)
                || (scene.selected_wires.is_empty() && scene.selected_net == Some(wire.net))
            {
                rgb(scene.palette.selected_wire).into()
            } else if let Some(value) = scene.values.get(&wire.net) {
                scene.palette.signal_color(value)
            } else {
                rgb(if definition.width > 1 {
                    scene.palette.bus
                } else {
                    scene.palette.wire
                })
                .into()
            };
            let width =
                if definition.width > 1 { 2.2 } else { 1.0 } * scene.viewport.zoom.max(0.85);
            stroke(
                wire.points
                    .iter()
                    .map(|p| screen(*p, bounds, &scene.viewport)),
                width,
                color,
                false,
                window,
            );
            if definition.show_name && !labels.contains(&wire.net) {
                let label = if let Some(value) = scene.values.get(&wire.net) {
                    format!("{} = {}", definition.name, value)
                } else {
                    definition.name.clone()
                };
                if let Some(location) =
                    crate::label_layout::wire_label(&scene.module, wire, &label, &occupied_labels)
                {
                    labels.insert(wire.net);
                    occupied_labels.push(
                        crate::label_layout::label_rect(location, &label, 10.0).expanded(2.0),
                    );
                    text(
                        &label,
                        screen(location, bounds, &scene.viewport),
                        10.0 * scene.viewport.zoom,
                        color,
                        window,
                        cx,
                    );
                }
            }
            if scene.probes.contains(&wire.net) && !wire.points.is_empty() {
                let position = screen(wire.points[wire.points.len() / 2], bounds, &scene.viewport);
                circle(
                    position,
                    3.0,
                    rgb(0xffc800).into(),
                    rgb(scene.palette.text).into(),
                    window,
                );
            }
        }
        paint_junctions(&scene, bounds, window);
        for gate in &scene.module.gates {
            if scene.selection.contains(&gate.id) {
                let rectangle = gate.bounds().expanded(5.0);
                rect(
                    rectangle,
                    bounds,
                    &scene.viewport,
                    rgb(scene.palette.gate).into(),
                    true,
                    window,
                );
                for corner in [
                    rectangle.min,
                    rectangle.max,
                    Point::new(rectangle.min.x, rectangle.max.y),
                    Point::new(rectangle.max.x, rectangle.min.y),
                ] {
                    let position =
                        screen(corner, bounds, &scene.viewport) - point(px(2.0), px(2.0));
                    window.paint_quad(fill(
                        Bounds::new(position, size(px(4.0), px(4.0))),
                        rgb(scene.palette.gate),
                    ));
                }
            }
            if gate.kind != GateKind::Frame {
                paint_gate(gate, &scene, bounds, window, cx);
            }
        }
        if !scene.draft.is_empty() {
            stroke(
                scene
                    .draft
                    .iter()
                    .map(|p| screen(*p, bounds, &scene.viewport)),
                1.5,
                rgb(scene.palette.selected_wire).into(),
                true,
                window,
            );
        }
        if let Some(gate) = &scene.ghost {
            paint_gate(gate, &scene, bounds, window, cx);
            rect(
                gate.bounds().expanded(5.0),
                bounds,
                &scene.viewport,
                rgb(scene.palette.muted).into(),
                true,
                window,
            );
        }
        if let Some(marquee) = scene.marquee {
            let origin = screen(marquee.min, bounds, &scene.viewport);
            let end = screen(marquee.max, bounds, &scene.viewport);
            window.paint_quad(fill(
                Bounds::from_corners(origin, end),
                gpui::rgba((scene.palette.gate << 8) | 0x1f),
            ));
            rect(
                marquee,
                bounds,
                &scene.viewport,
                rgb(scene.palette.gate).into(),
                true,
                window,
            );
        }
    });
}

fn paint_gate(
    gate: &Gate,
    scene: &Scene,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let zoom = scene.viewport.zoom;
    if gate.kind == GateKind::Comment {
        for run in rgate_core::rich_comment(&gate.text) {
            let origin = screen(gate.position + run.position, bounds, &scene.viewport);
            if run.image.is_some() {
                text(
                    &format!("[{}]", run.text),
                    origin,
                    run.style.size * zoom,
                    rgb(scene.palette.muted).into(),
                    window,
                    cx,
                );
            } else {
                let family = if run.style.monospace {
                    if cfg!(target_family = "wasm") {
                        "Lilex"
                    } else {
                        "Menlo"
                    }
                } else if cfg!(target_family = "wasm") {
                    "IBM Plex Sans"
                } else {
                    "Helvetica"
                };
                let mut font = gpui::font(family);
                if run.style.bold {
                    font.weight = gpui::FontWeight::BOLD;
                }
                if run.style.italic {
                    font.style = gpui::FontStyle::Italic;
                }
                let color = rgb(run.style.color.unwrap_or(scene.palette.text)).into();
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
        return;
    }
    if !gate.config.custom_symbol.is_empty() {
        crate::symbol_editor::paint_shapes(
            &gate.config.custom_symbol,
            |p| {
                screen(
                    gate.position + p.rotated(gate.rotation),
                    bounds,
                    &scene.viewport,
                )
            },
            zoom,
            scene.palette.module,
            scene.palette.panel,
            window,
            cx,
        );
    } else if gate.is_compact_bus_join() {
        let transform = |local: Point| {
            screen(
                gate.position + local.rotated(gate.rotation),
                bounds,
                &scene.viewport,
            )
        };
        let height = gate
            .pins
            .iter()
            .map(|pin| pin.offset.y.abs())
            .fold(2.0, f32::max)
            + 1.0;
        stroke(
            [
                transform(Point::new(-5.0, -height)),
                transform(Point::new(-2.0, -height)),
                transform(Point::new(-2.0, height)),
                transform(Point::new(-5.0, height)),
            ],
            zoom,
            rgb(scene.palette.gate).into(),
            false,
            window,
        );
        if let Some(pin) = gate.pin("Z") {
            stroke(
                [
                    transform(Point::new(-2.0, 0.0)),
                    screen(gate.pin_position(pin), bounds, &scene.viewport),
                ],
                zoom,
                rgb(scene.palette.gate).into(),
                false,
                window,
            );
        }
        for pin in gate.pins.iter().filter(|pin| pin.name != "Z") {
            stroke(
                [
                    screen(gate.pin_position(pin), bounds, &scene.viewport),
                    transform(Point::new(-2.0, pin.offset.y)),
                ],
                zoom,
                rgb(scene.palette.gate).into(),
                false,
                window,
            );
        }
    } else if scene.theme != theme::Theme::Classic {
        crate::modern_gates::paint(gate, scene, bounds, window, cx);
    } else if crate::classic_gates::supports(&gate.kind) {
        crate::classic_gates::paint(gate, scene, bounds, window, cx);
    } else {
        let rectangle = gate.bounds();
        let origin = screen(rectangle.min, bounds, &scene.viewport);
        let extent = rectangle.size() * zoom;
        window.paint_quad(quad(
            Bounds::new(origin, size(px(extent.x), px(extent.y))),
            px(0.0),
            rgb(scene.palette.panel),
            px(zoom),
            rgb(scene.palette.module),
            Default::default(),
        ));
        text(
            gate.kind.name(),
            screen(
                crate::label_layout::block_title(gate),
                bounds,
                &scene.viewport,
            ),
            9.0 * zoom,
            rgb(scene.palette.module).into(),
            window,
            cx,
        );
    }
    for pin in &gate.pins {
        if (matches!(gate.kind, GateKind::Module(_))
            || (gate.kind.is_extended() && !gate.is_compact_bus_join()))
            && !(scene.theme != theme::Theme::Classic && gate.kind == GateKind::Jkff)
        {
            let label = format!(
                "{}{}",
                pin.name,
                pin.width
                    .filter(|width| *width > 1)
                    .map(|width| format!(" [{}:0]", width - 1))
                    .unwrap_or_default()
            );
            let offset = if pin.direction == rgate_core::Direction::Output {
                Point::new(
                    pin.offset.x - label.len() as f32 * 5.0 - 4.0,
                    pin.offset.y - 5.0,
                )
            } else {
                Point::new(pin.offset.x + 4.0, pin.offset.y - 5.0)
            };
            text(
                &label,
                screen(
                    gate.position + offset.rotated(gate.rotation),
                    bounds,
                    &scene.viewport,
                ),
                8.0 * zoom,
                rgb(scene.palette.module).into(),
                window,
                cx,
            );
        }
        let reference = rgate_core::PinRef::new(gate.id, &pin.name);
        let position = screen(gate.pin_position(pin), bounds, &scene.viewport);
        if scene.hover_pin.as_ref() == Some(&reference) {
            circle(
                position,
                4.0,
                rgb(0xffffa0).into(),
                rgb(scene.palette.wire).into(),
                window,
            );
        } else if pin.net.is_none() {
            circle(
                position,
                1.8,
                rgb(scene.palette.panel).into(),
                rgb(scene.palette.wire).into(),
                window,
            );
        }
    }
    if gate.kind == GateKind::Peripheral {
        let incoming = gate
            .pin("I")
            .and_then(|pin| pin.net)
            .and_then(|net| scene.values.get(&net))
            .map(ToString::to_string)
            .unwrap_or_else(|| "Z".into());
        let outgoing = scene.inputs.get(&gate.id).unwrap_or(&gate.initial);
        text(
            &format!("in:{incoming} out:{outgoing}"),
            screen(
                gate.position + Point::new(-43.0, gate.bounds().size().y / 2.0 + 3.0),
                bounds,
                &scene.viewport,
            ),
            8.0 * zoom,
            rgb(scene.palette.module).into(),
            window,
            cx,
        );
    }
    if gate.show_name {
        let offset = crate::label_layout::instance_label(gate);
        text(
            &gate.name,
            screen(offset, bounds, &scene.viewport),
            10.5 * zoom,
            rgb(scene.palette.gate).into(),
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
            .and_then(|net| scene.values.get(&net))
    {
        text(
            &value.display_value(),
            screen(
                gate.position + Point::new(10.0, 0.0),
                bounds,
                &scene.viewport,
            ),
            9.0 * zoom,
            rgb(scene.palette.gate).into(),
            window,
            cx,
        );
    }
}

fn paint_junctions(scene: &Scene, bounds: Bounds<Pixels>, window: &mut Window) {
    let endpoints = scene
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
            let color = scene
                .values
                .get(&net)
                .map(|value| scene.palette.signal_color(value))
                .unwrap_or_else(|| rgb(scene.palette.wire).into());
            circle(
                screen(center, bounds, &scene.viewport),
                1.8 * scene.viewport.zoom,
                color,
                color,
                window,
            );
        }
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
        font: font(if cfg!(target_family = "wasm") {
            "IBM Plex Sans"
        } else {
            "Helvetica"
        }),
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
