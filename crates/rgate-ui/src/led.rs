use crate::canvas::{Scene, screen, stroke};
use gpui::{Bounds, Pixels, Window, point, px, quad, rgb, size};
use rgate_core::{Gate, LedDisplay, Logic, Point, Signal};

pub fn paint(gate: &Gate, scene: &Scene, bounds: Bounds<Pixels>, window: &mut Window) {
    let mode = gate.config.led_display;
    let zoom = scene.viewport.zoom;
    let extent = mode.size(gate.width);
    let value = gate
        .pin("I")
        .and_then(|pin| pin.net)
        .and_then(|net| scene.values.get(&net))
        .cloned()
        .unwrap_or_else(|| Signal::filled(gate.width, Logic::Low))
        .resized(gate.width);
    let body_offset = Point::new(0.0, 7.0 - extent.y / 2.0);
    let transform = |local: Point| {
        screen(
            gate.position + (local + body_offset).rotated(gate.rotation),
            bounds,
            &scene.viewport,
        )
    };
    let half = extent / 2.0;
    let modern = scene.theme != crate::theme::Theme::Classic;
    let blue = rgb(scene.palette.gate).into();
    let corners = [
        Point::new(-half.x, -half.y),
        Point::new(half.x, -half.y),
        Point::new(half.x, half.y),
        Point::new(-half.x, half.y),
        Point::new(-half.x, -half.y),
    ];
    if modern {
        crate::vector::Shape::rounded_rect(-half.x, -half.y, extent.x, extent.y, 4.0).paint(
            transform,
            1.4 * zoom,
            blue,
            Some(rgb(scene.palette.light).into()),
            window,
        );
    } else {
        stroke(
            corners.into_iter().map(transform),
            zoom,
            blue,
            false,
            window,
        );
    }
    // Keep the existing electrical pin position and visibly connect it to the resized body.
    if let Some(pin) = gate.pin("I") {
        stroke(
            [
                transform(Point::new(0.0, half.y)),
                screen(gate.pin_position(pin), bounds, &scene.viewport),
            ],
            zoom,
            blue,
            false,
            window,
        );
    }
    if mode == LedDisplay::Bar {
        for index in 0..gate.width {
            let local = Point::new(-half.x + 3.0 + f32::from(index) * 6.0, 0.0);
            let center = transform(local);
            let (w, h) = if gate.rotation.is_multiple_of(2) {
                (3.0, 10.0)
            } else {
                (10.0, 3.0)
            };
            window.paint_quad(quad(
                Bounds::new(
                    center - point(px(w * zoom / 2.0), px(h * zoom / 2.0)),
                    size(px(w * zoom), px(h * zoom)),
                ),
                px(0.0),
                rgb(display_color(
                    value.bit(usize::from(gate.width - 1 - index)),
                    scene,
                )),
                px(0.0),
                rgb(0x000000),
                Default::default(),
            ));
        }
        return;
    }
    let segments = mode.segments(&value);
    let paths = [
        (Point::new(-6.0, -12.0), Point::new(6.0, -12.0)),
        (Point::new(-7.0, -10.0), Point::new(-7.0, -2.0)),
        (Point::new(7.0, -10.0), Point::new(7.0, -2.0)),
        (Point::new(-6.0, 0.0), Point::new(6.0, 0.0)),
        (Point::new(-7.0, 2.0), Point::new(-7.0, 10.0)),
        (Point::new(7.0, 2.0), Point::new(7.0, 10.0)),
        (Point::new(-6.0, 12.0), Point::new(6.0, 12.0)),
    ];
    for (index, digit) in segments.iter().enumerate() {
        let center = Point::new(-half.x + 14.0 + index as f32 * 24.0, 0.0);
        let border = [
            Point::new(-10.0, -15.0),
            Point::new(10.0, -15.0),
            Point::new(10.0, 15.0),
            Point::new(-10.0, 15.0),
            Point::new(-10.0, -15.0),
        ];
        stroke(
            border.into_iter().map(|p| transform(center + p)),
            zoom,
            blue,
            false,
            window,
        );
        for (segment, (a, b)) in paths.iter().enumerate() {
            stroke(
                [transform(center + *a), transform(center + *b)],
                3.0 * zoom,
                rgb(display_color(digit[segment], scene)).into(),
                false,
                window,
            );
        }
    }
}

fn display_color(value: Logic, scene: &Scene) -> u32 {
    if scene.theme != crate::theme::Theme::Classic {
        return match value {
            Logic::High => scene.palette.high,
            Logic::Low => scene.palette.hover,
            Logic::Unknown => scene.palette.unknown,
            Logic::HighZ => scene.palette.float,
        };
    }
    match value {
        Logic::High => 0xff0000,
        Logic::Low => 0xffcccc,
        Logic::Unknown => 0x888888,
        Logic::HighZ => 0xe0b000,
    }
}
