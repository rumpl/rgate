use crate::{
    canvas::{Scene, screen, text},
    theme::Palette,
    vector::Shape,
};
use gpui::{AnyElement, App, Bounds, Pixels, Window, canvas, point, prelude::*, px, rgb};
use rgate_core::{Gate, GateId, GateKind, LedDisplay, Logic, Point};

pub struct PinLabel {
    pub pin: String,
    pub text: String,
    pub position: Point,
    pub inverted: bool,
}

pub struct Symbol {
    pub bodies: Vec<Shape>,
    pub details: Vec<Shape>,
    pub label: Option<String>,
    pub pin_labels: Vec<PinLabel>,
}

pub fn symbol(gate: &Gate) -> Symbol {
    let mut bodies = Vec::new();
    let mut details = Vec::new();
    let mut label = None;
    let mut pin_labels = Vec::new();
    let line = Shape::polyline;
    match gate.kind {
        GateKind::And | GateKind::Nand | GateKind::ReduceAnd | GateKind::ReduceNand => {
            let h = (f32::from(gate.input_count) * 2.5 + 1.0).max(7.0);
            let mut body = Shape::default();
            body.move_to(-8., -h)
                .line_to(0., -h)
                .curve_to(7., 0., 7., -h)
                .curve_to(0., h, 7., h)
                .line_to(-8., h)
                .close();
            bodies.push(body);
            for pin in gate.pins.iter().filter(|pin| pin.name != "Z") {
                details.push(line(&[(pin.offset.x, pin.offset.y), (-8., pin.offset.y)]));
            }
        }
        GateKind::Or
        | GateKind::Nor
        | GateKind::Xor
        | GateKind::Xnor
        | GateKind::ReduceOr
        | GateKind::ReduceNor
        | GateKind::ReduceXor
        | GateKind::ReduceXnor => {
            let h = (f32::from(gate.input_count) * 2.5 + 1.0).max(7.0);
            let mut body = Shape::default();
            body.move_to(-8., -h)
                .curve_to(7., 0., 3., -h)
                .curve_to(-8., h, 3., h)
                .curve_to(-8., -h, -2., 0.)
                .close();
            bodies.push(body);
            if matches!(gate.kind, GateKind::Xor | GateKind::Xnor) {
                let mut extra = Shape::default();
                extra.move_to(-10., -h).curve_to(-10., h, -4., 0.);
                details.push(extra);
            }
            for pin in gate.pins.iter().filter(|pin| pin.name != "Z") {
                let x = -5.0 - 3.0 * (pin.offset.y / h).powi(2);
                details.push(line(&[(pin.offset.x, pin.offset.y), (x, pin.offset.y)]));
            }
        }
        GateKind::Buffer | GateKind::Not | GateKind::TriState => {
            let mut body = line(&[(-5., -6.), (6., 0.), (-5., 6.)]);
            body.close();
            bodies.push(body);
            details.push(line(&[(-6., 0.), (-5., 0.)]));
            if gate.kind == GateKind::TriState {
                details.push(line(&[(2., -5.), (2., -2.)]));
            }
        }
        GateKind::Ground => {
            details.push(line(&[(0., -6.), (0., 0.)]));
            for (y, w) in [(0., 5.), (3., 3.5), (6., 1.5)] {
                details.push(line(&[(-w, y), (w, y)]));
            }
        }
        GateKind::Vdd => {
            details.push(line(&[(-11., 0.), (0., 0.), (0., -4.)]));
            details.push(line(&[(-3., -1.), (0., -4.), (3., -1.)]));
            label = Some("1".into());
        }
        GateKind::Led => {
            bodies.push(Shape::circle(0., 0., 5.));
            details.push(line(&[(0., 5.), (0., 7.)]));
        }
        GateKind::Switch => {
            bodies.push(Shape::rounded_rect(-14., -8., 28., 16., 8.));
        }
        GateKind::Dff => {
            bodies.push(Shape::rounded_rect(-12., -12., 24., 24., 2.));
            for pin in &gate.pins {
                let (caption, position, inverted) = match pin.name.as_str() {
                    "D" => ("D", Point::new(-8., 0.), false),
                    "Q" => ("Q", Point::new(8., -5.), false),
                    "_Q" => ("Q", Point::new(8., 5.), true),
                    "CLR" => ("C", Point::new(-4., -7.), true),
                    "EN" => ("E", Point::new(4., -7.), true),
                    "CK" => {
                        details.push(line(&[(-3., 12.), (0., 8.5), (3., 12.)]));
                        ("", Point::new(0., 7.), false)
                    }
                    _ => continue,
                };
                let edge = Point::new(pin.offset.x.clamp(-12., 12.), pin.offset.y.clamp(-12., 12.));
                terminal(&mut bodies, &mut details, pin.offset, edge, inverted);
                pin_labels.push(PinLabel {
                    pin: pin.name.clone(),
                    text: caption.into(),
                    position,
                    inverted,
                });
            }
        }
        GateKind::Comment => {}
        _ => {
            let mut unrotated = gate.clone();
            unrotated.rotation = 0;
            let half = unrotated.bounds().size() / 2.0;
            let w = (half.x - 4.).max(6.);
            let h = (half.y - 4.).max(5.);
            if matches!(gate.kind, GateKind::Mux | GateKind::Add) {
                let mut body = line(&[(-w, -h), (w, -h), (w * 0.6, h), (-w * 0.6, h)]);
                body.close();
                bodies.push(body);
            } else {
                bodies.push(Shape::rounded_rect(-w, -h, w * 2., h * 2., 3.));
            }
            for pin in &gate.pins {
                let y = pin.offset.y.clamp(-h, h);
                let edge = if matches!(gate.kind, GateKind::Mux | GateKind::Add) {
                    w * (0.8 - 0.2 * y / h)
                } else {
                    w
                };
                let end = Point::new(pin.offset.x.clamp(-edge, edge), y);
                let sequential = matches!(gate.kind, GateKind::Register | GateKind::Jkff);
                let inverted =
                    sequential && matches!(pin.name.as_str(), "EN" | "CLR" | "PRE" | "_Q");
                terminal(&mut bodies, &mut details, pin.offset, end, inverted);
                if matches!(
                    gate.kind,
                    GateKind::Register | GateKind::Mux | GateKind::Add | GateKind::Jkff
                ) {
                    let inward = if end.y.abs() >= h - 0.1 {
                        Point::new(0., -end.y.signum())
                    } else {
                        Point::new(-end.x.signum(), 0.)
                    };
                    let position = if end.x.abs() >= w - 0.1 {
                        Point::new(end.x - end.x.signum() * 6., end.y)
                    } else if end.y.abs() >= h - 0.1 {
                        Point::new(end.x, end.y - end.y.signum() * 4.)
                    } else {
                        end + inward * 5.
                    };
                    if sequential && pin.name == "CK" {
                        let tangent = Point::new(-inward.y, inward.x) * 2.5;
                        details.push(line(&[
                            (end.x + tangent.x, end.y + tangent.y),
                            (end.x + inward.x * 3.5, end.y + inward.y * 3.5),
                            (end.x - tangent.x, end.y - tangent.y),
                        ]));
                    }
                    let position = if gate.kind == GateKind::Register
                        && matches!(pin.name.as_str(), "D" | "Q")
                    {
                        Point::new(8., position.y)
                    } else {
                        position
                    };
                    let caption = match pin.name.as_str() {
                        "CK" => "".into(),
                        "CLR" => "C".into(),
                        "EN" => "E".into(),
                        "PRE" => "P".into(),
                        "_Q" => "Q".into(),
                        "CI" => "Ci".into(),
                        "CO" => "Co".into(),
                        name if name.starts_with('I') && gate.kind == GateKind::Mux => {
                            name[1..].into()
                        }
                        name => name.into(),
                    };
                    pin_labels.push(PinLabel {
                        pin: pin.name.clone(),
                        text: caption,
                        position,
                        inverted,
                    });
                }
            }
            label = Some(match gate.kind {
                GateKind::Add => "+".into(),
                GateKind::Clock => {
                    details.push(line(&[
                        (-8., 3.),
                        (-4., 3.),
                        (-4., -3.),
                        (2., -3.),
                        (2., 3.),
                        (7., 3.),
                    ]));
                    String::new()
                }
                GateKind::Dip => gate.initial.display_value(),
                GateKind::Register => "REG".into(),
                GateKind::Jkff => String::new(),
                GateKind::Module(ref name) | GateKind::Unsupported(ref name) => name.clone(),
                _ => gate.kind.name().into(),
            });
        }
    }
    if gate.kind.is_logic()
        || matches!(
            gate.kind,
            GateKind::Buffer | GateKind::Not | GateKind::TriState
        )
    {
        let inverted = matches!(
            gate.kind,
            GateKind::Nand | GateKind::Nor | GateKind::Xnor | GateKind::Not
        );
        if inverted {
            details.push(line(&[
                (if gate.kind == GateKind::Not { 6. } else { 7. }, 0.),
                (7., 0.),
            ]));
            bodies.push(Shape::circle(8.5, 0., 1.5));
        } else {
            details.push(line(&[(6., 0.), (10., 0.)]));
        }
    }
    Symbol {
        bodies,
        details,
        label,
        pin_labels,
    }
}

fn terminal(
    bodies: &mut Vec<Shape>,
    details: &mut Vec<Shape>,
    pin: Point,
    edge: Point,
    inverted: bool,
) {
    let direction = (pin - edge) / pin.distance(edge).max(1.);
    let start = if inverted {
        let center = edge + direction * 1.4;
        bodies.push(Shape::circle(center.x, center.y, 1.4));
        edge + direction * 2.8
    } else {
        edge
    };
    details.push(Shape::polyline(&[(pin.x, pin.y), (start.x, start.y)]));
}

pub(super) fn paint_pin_labels(
    symbol: &Symbol,
    transform: impl Fn(Point) -> gpui::Point<Pixels>,
    zoom: f32,
    palette: Palette,
    window: &mut Window,
    cx: &mut App,
) {
    let font_size = px(5.2 * zoom);
    for label in &symbol.pin_labels {
        debug_assert!(!label.pin.is_empty());
        if label.text.is_empty() || font_size < px(5.0) {
            continue;
        }
        let font = gpui::font(if cfg!(target_family = "wasm") {
            "IBM Plex Sans"
        } else {
            "Helvetica"
        });
        let font_id = window.text_system().resolve_font(&font);
        let cap_height = window.text_system().cap_height(font_id, font_size);
        let run = gpui::TextRun {
            len: label.text.len(),
            font,
            color: rgb(palette.text).into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let line =
            window
                .text_system()
                .shape_line(label.text.clone().into(), font_size, &[run], None);
        let center = transform(label.position);
        let line_height = line.ascent + line.descent;
        // Center the visible capital, not the line box (which includes leading/descent).
        let origin = center - point(line.width() / 2., line.ascent - cap_height / 2.);
        if let Err(error) = line.paint(origin, line_height, gpui::TextAlign::Left, None, window, cx)
        {
            eprintln!("pin label paint failed: {error}");
        }
        if label.inverted {
            let y = center.y - cap_height / 2. - px(1.0 * zoom);
            crate::canvas::stroke(
                [point(origin.x, y), point(origin.x + line.width(), y)],
                0.45 * zoom,
                rgb(palette.text).into(),
                false,
                window,
            );
        }
    }
}

fn paint_symbol(
    symbol: &Symbol,
    transform: impl Fn(Point) -> gpui::Point<Pixels>,
    zoom: f32,
    palette: Palette,
    window: &mut Window,
) {
    for body in &symbol.bodies {
        body.paint(
            &transform,
            1.05 * zoom,
            rgb(palette.gate).into(),
            Some(rgb(palette.light).into()),
            window,
        );
    }
    for detail in &symbol.details {
        detail.paint(
            &transform,
            1.05 * zoom,
            rgb(palette.gate).into(),
            None,
            window,
        );
    }
}

pub fn paint(
    gate: &Gate,
    scene: &Scene,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    if gate.kind == GateKind::Led && gate.config.led_display != LedDisplay::Bit {
        crate::led::paint(gate, scene, bounds, window);
        return;
    }
    let mut symbol = symbol(gate);
    if gate.kind == GateKind::Dip {
        symbol.label = Some(
            scene
                .inputs
                .get(&gate.id)
                .unwrap_or(&gate.initial)
                .display_value(),
        );
    }
    // DIP bodies stay horizontal in the existing model; only their output moves.
    let body_rotation = if gate.kind == GateKind::Dip {
        0
    } else {
        gate.rotation
    };
    let transform = |p: Point| {
        screen(
            gate.position + p.rotated(body_rotation),
            bounds,
            &scene.viewport,
        )
    };
    let zoom = scene.viewport.zoom;
    paint_symbol(&symbol, transform, zoom, scene.palette, window);
    paint_pin_labels(&symbol, transform, zoom, scene.palette, window, cx);
    if let Some(label) = &symbol.label {
        // Keep block labels upright as the component rotates.
        let font_size = if gate.kind.is_extended() || matches!(gate.kind, GateKind::Module(_)) {
            9.0
        } else {
            8.0
        };
        let position = if gate.kind.is_extended()
            || matches!(gate.kind, GateKind::Module(_) | GateKind::Unsupported(_))
        {
            screen(
                crate::label_layout::block_title(gate),
                bounds,
                &scene.viewport,
            )
        } else {
            transform(Point::ZERO)
                - point(
                    px(label.chars().count() as f32 * font_size * 0.26 * zoom),
                    px(font_size * 0.6 * zoom),
                )
        };
        text(
            label,
            position,
            font_size * zoom,
            rgb(scene.palette.text).into(),
            window,
            cx,
        );
    }
    if gate.kind == GateKind::Register {
        text(
            &format!("{}-bit", gate.width),
            screen(
                crate::label_layout::value_label(gate),
                bounds,
                &scene.viewport,
            ),
            8. * zoom,
            rgb(scene.palette.muted).into(),
            window,
            cx,
        );
    }
    if gate.kind == GateKind::Switch {
        let high = scene.inputs.get(&gate.id).unwrap_or(&gate.initial).bit(0) == Logic::High;
        let color = if high {
            scene.palette.high
        } else {
            scene.palette.muted
        };
        Shape::rounded_rect(-14., -8., 28., 16., 8.).paint(
            transform,
            1.05 * zoom,
            rgb(color).into(),
            Some(
                rgb(if high {
                    scene.palette.accent
                } else {
                    scene.palette.hover
                })
                .into(),
            ),
            window,
        );
        Shape::circle(if high { 6. } else { -6. }, 0., 5.).paint(
            transform,
            zoom,
            rgb(color).into(),
            Some(rgb(scene.palette.panel).into()),
            window,
        );
        if let Some(pin) = gate.pin("Z") {
            // TkGate switches use asymmetric, rotation-specific electrical endpoints.
            let endpoint = (gate.pin_position(pin) - gate.position).rotated(4 - gate.rotation % 4);
            Shape::polyline(&[(14., 0.), (endpoint.x, endpoint.y)]).paint(
                transform,
                1.05 * zoom,
                rgb(color).into(),
                None,
                window,
            );
        }
    }
    if gate.kind == GateKind::Led {
        let logic = gate
            .pins
            .first()
            .and_then(|pin| pin.net)
            .and_then(|net| scene.values.get(&net))
            .map_or(Logic::Low, |value| value.bit(0));
        let color = crate::classic_gates::led_color(logic);
        Shape::circle(0., 0., 5.).paint(
            transform,
            zoom,
            rgb(scene.palette.gate).into(),
            Some(rgb(color).into()),
            window,
        );
    }
    if gate.kind == GateKind::Dip {
        // Imported DIP symbols also have rotation-specific endpoints.
        if let Some(pin) = gate.pin("Z") {
            crate::canvas::stroke(
                [
                    transform({
                        let local = gate.pin_position(pin) - gate.position;
                        Point::new(local.x.clamp(-34., 34.), local.y.clamp(-7., 7.))
                    }),
                    screen(gate.pin_position(pin), bounds, &scene.viewport),
                ],
                zoom,
                rgb(scene.palette.gate).into(),
                false,
                window,
            );
        }
    }
}

pub fn preview(kind: GateKind, palette: Palette) -> AnyElement {
    let gate = Gate::new(GateId(0), kind, Point::ZERO);
    let symbol = symbol(&gate);
    let extent = gate.bounds().size();
    canvas(
        |_, _, _| {},
        move |bounds, _, window, cx| {
            let scale = (18. / extent.x).min(14. / extent.y);
            let center = bounds.origin + point(bounds.size.width / 2., bounds.size.height / 2.);
            paint_symbol(
                &symbol,
                |p| center + point(px(p.x * scale), px(p.y * scale)),
                scale.max(0.7),
                palette,
                window,
            );
            if gate.kind == GateKind::Switch {
                Shape::circle(-6., 0., 5.).paint(
                    |p| center + point(px(p.x * scale), px(p.y * scale)),
                    1.,
                    rgb(palette.muted).into(),
                    Some(rgb(palette.panel).into()),
                    window,
                );
            }
            let preview_label = if gate.kind == GateKind::Dff {
                Some("D")
            } else {
                symbol.label.as_deref()
            };
            if let Some(label) = preview_label {
                let label = match gate.kind {
                    GateKind::Register => "R",
                    GateKind::Mux => "M",
                    GateKind::Dip => "0",
                    _ => label,
                };
                text(
                    label,
                    center - point(px(label.len() as f32 * 2.), px(4.)),
                    7.,
                    rgb(palette.text).into(),
                    window,
                    cx,
                );
            }
        },
    )
    .w(px(20.))
    .h(px(16.))
    .flex_none()
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flip_flop_has_identifiable_pins_clock_edge_and_active_low_controls() {
        let gate = Gate::new(GateId(1), GateKind::Dff, Point::ZERO);
        let symbol = symbol(&gate);
        assert!(
            symbol.label.is_none(),
            "a central D must not replace the pin semantics"
        );
        assert_eq!(symbol.pin_labels.len(), gate.pins.len());
        for name in ["D", "Q", "_Q", "CLR", "EN", "CK"] {
            let label = symbol
                .pin_labels
                .iter()
                .find(|label| label.pin == name)
                .unwrap();
            assert_eq!(label.inverted, matches!(name, "_Q" | "CLR" | "EN"));
            if name != "CK" {
                assert!(!label.text.is_empty());
            }
        }
        assert_eq!(symbol.bodies.len(), 4, "body plus three inversion bubbles");
        assert_eq!(
            symbol.details.len(),
            7,
            "six leads plus the clock-edge marker"
        );
    }

    struct TestLabelView {
        _focus: gpui::FocusHandle,
    }

    impl Render for TestLabelView {
        fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
            gpui::div()
        }
    }

    #[gpui::test]
    fn flip_flop_pin_typography_clears_border_and_tracks_terminal_centers(
        cx: &mut gpui::TestAppContext,
    ) {
        let symbol = symbol(&Gate::new(GateId(1), GateKind::Dff, Point::ZERO));
        let (_view, cx) = cx.add_window_view(|_, cx| TestLabelView {
            _focus: cx.focus_handle(),
        });
        cx.update(|window, _| {
            let font = gpui::font("Helvetica");
            let font_id = window.text_system().resolve_font(&font);
            for zoom in [1., 2., 6.] {
                let size = px(5.2 * zoom);
                let cap = f32::from(window.text_system().cap_height(font_id, size)) / zoom;
                let mut occupied = Vec::new();
                for label in symbol
                    .pin_labels
                    .iter()
                    .filter(|label| !label.text.is_empty())
                {
                    let run = gpui::TextRun {
                        len: label.text.len(),
                        font: font.clone(),
                        color: rgb(0x202020).into(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    let line = window.text_system().shape_line(
                        label.text.clone().into(),
                        size,
                        &[run],
                        None,
                    );
                    let half_width = f32::from(line.width()) / zoom / 2.;
                    assert!(label.position.x - half_width > -11.);
                    assert!(label.position.x + half_width < 11.);
                    let top = label.position.y - cap / 2. - if label.inverted { 1. } else { 0. };
                    assert!(top > -11., "overbar touches body border");
                    assert!(label.position.y + cap / 2. < 11.);
                    let rect = rgate_core::Rect::from_points(
                        Point::new(label.position.x - half_width, top),
                        Point::new(label.position.x + half_width, label.position.y + cap / 2.),
                    );
                    assert!(
                        occupied
                            .iter()
                            .all(|other| !crate::label_layout::intersects(rect, *other)),
                        "pin glyphs overlap"
                    );
                    occupied.push(rect);
                }
            }
        });
        for (pin, y) in [("D", 0.), ("Q", -5.), ("_Q", 5.)] {
            assert_eq!(
                symbol
                    .pin_labels
                    .iter()
                    .find(|label| label.pin == pin)
                    .unwrap()
                    .position
                    .y,
                y
            );
        }
        for (pin, x) in [("CLR", -4.), ("EN", 4.)] {
            assert_eq!(
                symbol
                    .pin_labels
                    .iter()
                    .find(|label| label.pin == pin)
                    .unwrap()
                    .position
                    .x,
                x
            );
        }
    }

    #[test]
    fn functional_blocks_label_all_terminals_and_keep_pin_geometry() {
        for kind in [
            GateKind::Register,
            GateKind::Mux,
            GateKind::Add,
            GateKind::Jkff,
        ] {
            let mut gate = Gate::new(GateId(1), kind, Point::new(100., 200.));
            for rotation in 0..4 {
                gate.rotation = rotation;
                let endpoints = gate
                    .pins
                    .iter()
                    .map(|pin| gate.pin_position(pin))
                    .collect::<Vec<_>>();
                let symbol = symbol(&gate);
                assert_eq!(symbol.pin_labels.len(), gate.pins.len());
                for pin in &gate.pins {
                    assert!(symbol.pin_labels.iter().any(|label| label.pin == pin.name));
                }
                assert_eq!(
                    endpoints,
                    gate.pins
                        .iter()
                        .map(|pin| gate.pin_position(pin))
                        .collect::<Vec<_>>()
                );
            }
        }
    }

    #[test]
    fn symbols_cover_builtin_gates_without_mutating_geometry() {
        for entry in crate::commands::entries("Components") {
            if let Some(crate::commands::Command::Tool(rgate_editor::Tool::Place(kind))) =
                entry.command
            {
                let mut gate = Gate::new(GateId(1), kind, Point::new(50., 80.));
                for rotation in 0..4 {
                    gate.rotation = rotation;
                    let before = serde_json::to_string(&gate).unwrap();
                    let symbol = symbol(&gate);
                    if gate.kind != GateKind::Comment {
                        assert!(!symbol.bodies.is_empty() || !symbol.details.is_empty());
                    }
                    assert_eq!(serde_json::to_string(&gate).unwrap(), before);
                }
            }
        }
    }
}
