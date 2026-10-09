use crate::{
    canvas::{Scene, screen, text},
    modern_gates::{PinLabel, Symbol, paint_pin_labels},
    theme::Palette,
    vector::Shape,
};
use gpui::{AnyElement, App, Bounds, Pixels, Window, canvas, point, prelude::*, px, rgb};
use rgate_core::{Gate, GateId, GateKind, LedDisplay, Logic, Point};

pub fn supports(kind: &GateKind) -> bool {
    kind.is_logic()
        || matches!(
            kind,
            GateKind::Buffer
                | GateKind::Not
                | GateKind::TriState
                | GateKind::Clock
                | GateKind::Led
                | GateKind::Dff
                | GateKind::Register
                | GateKind::Mux
                | GateKind::Add
                | GateKind::Ground
                | GateKind::Vdd
                | GateKind::Dip
                | GateKind::Switch
        )
}

/// Reconstruct the upstream silhouettes in circuit coordinates, not enlarged bitmap pixels.
pub fn symbol(gate: &Gate) -> Symbol {
    let mut symbol = crate::modern_gates::symbol(gate);
    let line = Shape::polyline;
    match gate.kind {
        GateKind::And | GateKind::Nand => {
            let h = (f32::from(gate.input_count) * 2.5 + 1.).max(7.);
            let mut body = Shape::default();
            body.move_to(-10., -h)
                .line_to(2., -h)
                .curve_to(9., 0., 9., -h)
                .curve_to(2., h, 9., h)
                .line_to(-10., h)
                .close();
            symbol.bodies[0] = body;
            symbol.details.clear();
            for pin in gate.pins.iter().filter(|pin| pin.name != "Z") {
                symbol
                    .details
                    .push(line(&[(pin.offset.x, pin.offset.y), (-10., pin.offset.y)]));
            }
            if gate.kind == GateKind::Nand {
                let mut body = Shape::default();
                body.move_to(-10., -h)
                    .line_to(0., -h)
                    .curve_to(7., 0., 7., -h)
                    .curve_to(0., h, 7., h)
                    .line_to(-10., h)
                    .close();
                symbol.bodies[0] = body;
            } else {
                symbol.details.push(line(&[(9., 0.), (10., 0.)]));
            }
        }
        GateKind::Clock => {
            let mut body = line(&[(-12., -12.), (12., 0.), (-12., 12.)]);
            body.close();
            symbol.bodies = vec![body];
            symbol.details = vec![
                line(&[
                    (-10., 4.),
                    (-8., 4.),
                    (-8., -4.),
                    (-4., -4.),
                    (-4., 4.),
                    (-2., 4.),
                ]),
                line(&[(12., 0.), (13., 0.)]),
            ];
            symbol.label = None;
        }
        GateKind::Led => {
            let mut body = Shape::default();
            body.move_to(-6., 6.)
                .line_to(-6., 0.)
                .curve_to(0., -6., -6., -6.)
                .curve_to(6., 0., 6., -6.)
                .line_to(6., 6.)
                .close();
            symbol.bodies = vec![body];
            symbol.details = vec![line(&[(0., 6.), (0., 7.)])];
            symbol.label = None;
        }
        GateKind::Dff => {
            symbol.bodies = vec![Shape::rounded_rect(-15., -15., 30., 30., 0.)];
            symbol.details = vec![line(&[(-4., 15.), (0., 11.), (4., 15.)])];
            for pin in &gate.pins {
                symbol.details.push(line(&[
                    (pin.offset.x, pin.offset.y),
                    (pin.offset.x.clamp(-15., 15.), pin.offset.y.clamp(-15., 15.)),
                ]));
            }
            for label in &mut symbol.pin_labels {
                label.position = match label.pin.as_str() {
                    "D" => Point::new(-11., 0.),
                    "Q" => Point::new(11., -5.),
                    "_Q" => Point::new(11., 5.),
                    "CLR" => Point::new(-5., -10.),
                    "EN" => Point::new(5., -10.),
                    _ => label.position,
                };
            }
        }
        GateKind::Register => {
            symbol.bodies = vec![Shape::rounded_rect(-37., -10., 75., 20., 0.)];
            symbol.details = vec![line(&[(-37., -6.), (-31., 0.), (-37., 6.)])];
            symbol.pin_labels = vec![
                PinLabel {
                    pin: "CLR".into(),
                    text: "CLR".into(),
                    position: Point::new(28., -5.),
                    inverted: true,
                },
                PinLabel {
                    pin: "EN".into(),
                    text: "EN".into(),
                    position: Point::new(30., 5.),
                    inverted: true,
                },
            ];
            for pin in &gate.pins {
                symbol.details.push(line(&[
                    (pin.offset.x, pin.offset.y),
                    (pin.offset.x.clamp(-37., 38.), pin.offset.y.clamp(-10., 10.)),
                ]));
            }
            symbol.label = None;
        }
        GateKind::Mux | GateKind::Add => {
            let points = if gate.kind == GateKind::Add {
                vec![
                    (-29., -15.),
                    (-5., -15.),
                    (0., -10.),
                    (5., -15.),
                    (29., -15.),
                    (16., 13.),
                    (-16., 13.),
                ]
            } else {
                vec![(-29., -15.), (29., -15.), (16., 13.), (-16., 13.)]
            };
            let mut body = line(&points);
            body.close();
            symbol.bodies = vec![body];
            symbol.details.clear();
            symbol.pin_labels.clear();
            for pin in &gate.pins {
                let y = pin.offset.y.clamp(-15., 13.);
                let edge = 29. - (y + 15.) * 13. / 28.;
                symbol.details.push(line(&[
                    (pin.offset.x, pin.offset.y),
                    (pin.offset.x.clamp(-edge, edge), y),
                ]));
            }
            symbol.label = None;
            if gate.kind == GateKind::Add {
                symbol.details.push(line(&[(-3., 0.), (3., 0.)]));
                symbol.details.push(line(&[(0., -3.), (0., 3.)]));
            }
        }
        GateKind::Ground => {
            symbol.bodies.clear();
            symbol.details = vec![line(&[(0., -6.), (0., 0.)])];
            for (y, width) in [(0., 4.), (2., 3.), (4., 2.), (6., 1.)] {
                symbol.details.push(line(&[(-width, y), (width, y)]));
            }
        }
        GateKind::Vdd => {
            symbol.bodies.clear();
            symbol.details = vec![line(&[(-11., 0.), (-3., 0.)])];
            symbol.label = None;
            symbol.pin_labels = vec![PinLabel {
                pin: "Z".into(),
                text: "Vdd".into(),
                position: Point::new(6., 0.),
                inverted: false,
            }];
        }
        GateKind::Dip => {
            symbol.bodies = vec![Shape::rounded_rect(-37., -10., 74., 20., 0.)];
            symbol.details.clear();
            symbol.pin_labels.clear();
            symbol.label = None;
            for index in 0..8 {
                let x = -18. + index as f32 * 7.;
                symbol
                    .details
                    .push(Shape::rounded_rect(x, -6., 3., 12., 0.));
                symbol.details.push(line(&[(x, -2.), (x + 3., -2.)]));
                symbol.details.push(line(&[(x, 2.), (x + 3., 2.)]));
            }
        }
        GateKind::Switch => {
            symbol.bodies = vec![
                Shape::rounded_rect(-12., -7., 24., 14., 0.),
                Shape::rounded_rect(-10., -5., 7., 10., 0.),
            ];
            symbol.details = vec![
                line(&[(-8.5, -3.), (-4.5, -3.)]),
                line(&[(-8.5, 3.), (-4.5, 3.)]),
            ];
            symbol.label = None;
            symbol.pin_labels = vec![
                PinLabel {
                    pin: "Z".into(),
                    text: "on".into(),
                    position: Point::new(4., -3.5),
                    inverted: false,
                },
                PinLabel {
                    pin: "Z".into(),
                    text: "off".into(),
                    position: Point::new(4., 3.5),
                    inverted: false,
                },
            ];
        }
        _ => {}
    }
    symbol
}

pub fn led_color(logic: Logic) -> u32 {
    match logic {
        Logic::High => 0xff0000,
        Logic::Low => 0xe6e6fa,
        Logic::Unknown => 0x888888,
        Logic::HighZ => 0xffff00,
    }
}

fn paint_symbol(
    symbol: &Symbol,
    transform: impl Fn(Point) -> gpui::Point<Pixels>,
    zoom: f32,
    palette: Palette,
    body_color: u32,
    window: &mut Window,
) {
    for body in &symbol.bodies {
        body.paint(
            &transform,
            zoom,
            rgb(palette.gate).into(),
            Some(rgb(body_color).into()),
            window,
        );
    }
    for detail in &symbol.details {
        detail.paint(&transform, zoom, rgb(palette.gate).into(), None, window);
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
    let symbol = symbol(gate);
    let rotation = if gate.kind == GateKind::Dip {
        0
    } else {
        gate.rotation
    };
    let transform = |p: Point| screen(gate.position + p.rotated(rotation), bounds, &scene.viewport);
    let color = if gate.kind == GateKind::Led {
        let logic = gate
            .pin("I")
            .and_then(|pin| pin.net)
            .and_then(|net| scene.values.get(&net))
            .map_or(Logic::Low, |value| value.bit(0));
        led_color(logic)
    } else {
        scene.palette.panel
    };
    paint_symbol(
        &symbol,
        transform,
        scene.viewport.zoom,
        scene.palette,
        color,
        window,
    );
    let mut label_palette = scene.palette;
    label_palette.text = scene.palette.gate;
    paint_pin_labels(
        &symbol,
        transform,
        scene.viewport.zoom,
        label_palette,
        window,
        cx,
    );
    if gate.kind == GateKind::Switch {
        let high = scene.inputs.get(&gate.id).unwrap_or(&gate.initial).bit(0) == Logic::High;
        let y = if high { -2.5 } else { 2.5 };
        Shape::rounded_rect(-8.5, y - 1.5, 4., 3., 0.).paint(
            transform,
            scene.viewport.zoom,
            rgb(scene.palette.gate).into(),
            Some(rgb(scene.palette.gate).into()),
            window,
        );
        if let Some(pin) = gate.pin("Z") {
            crate::canvas::stroke(
                [
                    transform(Point::new(12., 0.)),
                    screen(gate.pin_position(pin), bounds, &scene.viewport),
                ],
                scene.viewport.zoom,
                rgb(scene.palette.gate).into(),
                false,
                window,
            );
        }
    }
    if matches!(gate.kind, GateKind::Dip | GateKind::Register) {
        let value = if gate.kind == GateKind::Dip {
            scene
                .inputs
                .get(&gate.id)
                .unwrap_or(&gate.initial)
                .display_value()
        } else {
            format!("{}-bit", gate.width)
        };
        text(
            &value,
            screen(
                crate::label_layout::value_label(gate),
                bounds,
                &scene.viewport,
            ),
            8. * scene.viewport.zoom,
            rgb(scene.palette.gate).into(),
            window,
            cx,
        );
    }
    if gate.kind == GateKind::Dip {
        text(
            "ON",
            transform(Point::new(-34., -8.)),
            5. * scene.viewport.zoom,
            rgb(scene.palette.gate).into(),
            window,
            cx,
        );
        text(
            "OFF",
            transform(Point::new(-34., 1.)),
            5. * scene.viewport.zoom,
            rgb(scene.palette.gate).into(),
            window,
            cx,
        );
        if let Some(pin) = gate.pin("Z") {
            let local = gate.pin_position(pin) - gate.position;
            crate::canvas::stroke(
                [
                    transform(Point::new(
                        local.x.clamp(-37., 37.),
                        local.y.clamp(-10., 10.),
                    )),
                    screen(gate.pin_position(pin), bounds, &scene.viewport),
                ],
                scene.viewport.zoom,
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
            let transform = |p: Point| center + point(px(p.x * scale), px(p.y * scale));
            paint_symbol(
                &symbol,
                transform,
                scale.max(0.6),
                palette,
                if gate.kind == GateKind::Led {
                    led_color(Logic::Low)
                } else {
                    palette.panel
                },
                window,
            );
            if gate.kind == GateKind::Dff {
                text(
                    "D",
                    center - point(px(2.), px(4.)),
                    6.,
                    rgb(palette.gate).into(),
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
    fn classic_designs_are_vectors_with_original_geometry() {
        for entry in crate::commands::entries("Components") {
            let Some(crate::commands::Command::Tool(rgate_editor::Tool::Place(kind))) =
                entry.command
            else {
                continue;
            };
            if !supports(&kind) {
                continue;
            }
            let mut gate = Gate::new(GateId(1), kind, Point::new(80., 100.));
            for rotation in 0..4 {
                gate.rotation = rotation;
                let before = serde_json::to_string(&gate).unwrap();
                let symbol = symbol(&gate);
                assert!(!symbol.bodies.is_empty() || !symbol.details.is_empty());
                assert_eq!(serde_json::to_string(&gate).unwrap(), before);
            }
        }
        let dff = symbol(&Gate::new(GateId(1), GateKind::Dff, Point::ZERO));
        assert_eq!(
            dff.bodies,
            vec![Shape::rounded_rect(-15., -15., 30., 30., 0.)]
        );
        assert_eq!(dff.pin_labels.len(), 6);
        let clock = symbol(&Gate::new(GateId(1), GateKind::Clock, Point::ZERO));
        let mut triangle = Shape::polyline(&[(-12., -12.), (12., 0.), (-12., 12.)]);
        triangle.close();
        assert_eq!(clock.bodies, vec![triangle]);
    }

    #[test]
    fn led_is_one_filled_dome_with_tkgate_state_colors() {
        let gate = Gate::new(GateId(1), GateKind::Led, Point::ZERO);
        let symbol = symbol(&gate);
        assert_eq!(symbol.bodies.len(), 1);
        assert_ne!(symbol.bodies[0], Shape::circle(0., 0., 5.));
        assert_eq!(symbol.details.len(), 1);
        assert_eq!(led_color(Logic::High), 0xff0000);
        assert_eq!(led_color(Logic::Low), 0xe6e6fa);
        assert_eq!(led_color(Logic::Unknown), 0x888888);
        assert_eq!(led_color(Logic::HighZ), 0xffff00);
    }
}
