use rgate_core::{Gate, GateKind, Module, Point, Rect, Wire};

pub fn intersects(a: Rect, b: Rect) -> bool {
    a.min.x < b.max.x && a.max.x > b.min.x && a.min.y < b.max.y && a.max.y > b.min.y
}

pub fn label_rect(origin: Point, label: &str, font_size: f32) -> Rect {
    Rect::from_points(
        origin,
        origin
            + Point::new(
                label.chars().count() as f32 * font_size * 0.65,
                font_size * 1.35,
            ),
    )
}

pub fn block_title(gate: &Gate) -> Point {
    let bounds = gate.bounds();
    Point::new(
        bounds.center().x - gate.kind.name().chars().count() as f32 * 3.0,
        bounds.min.y - 14.0,
    )
}

pub fn instance_label(gate: &Gate) -> Point {
    let bounds = gate.bounds();
    let block = matches!(gate.kind, GateKind::Module(_) | GateKind::Unsupported(_))
        || (gate.kind.is_extended() && !gate.is_compact_bus_join());
    Point::new(
        gate.position.x - gate.name.chars().count() as f32 * 2.8,
        bounds.min.y - if block { 29.0 } else { 14.0 },
    )
}

pub fn value_label(gate: &Gate) -> Point {
    Point::new(gate.bounds().min.x + 4.0, gate.bounds().max.y + 3.0)
}

pub fn wire_label(module: &Module, wire: &Wire, label: &str, occupied: &[Rect]) -> Option<Point> {
    let mut segments = wire
        .points
        .windows(2)
        .filter(|segment| {
            (segment[0].y - segment[1].y).abs() < 0.5 && segment[0].distance(segment[1]) > 22.0
        })
        .collect::<Vec<_>>();
    segments.sort_by(|a, b| b[0].distance(b[1]).total_cmp(&a[0].distance(a[1])));
    let width = label.chars().count() as f32 * 6.5;
    for segment in segments {
        let left = segment[0].x.min(segment[1].x);
        let right = segment[0].x.max(segment[1].x);
        for x in [
            (left + right - width) / 2.0,
            left + 5.0,
            right - width - 5.0,
        ] {
            for offset in [-15.0, 4.0, -28.0, 17.0] {
                let origin = Point::new(x, segment[0].y + offset);
                let rect = label_rect(origin, label, 10.0).expanded(2.0);
                let blocked = module
                    .gates
                    .iter()
                    .filter(|gate| gate.kind != GateKind::Frame)
                    .any(|gate| {
                        intersects(rect, gate.bounds().expanded(3.0))
                            || (gate.show_name
                                && intersects(
                                    rect,
                                    label_rect(instance_label(gate), &gate.name, 10.5),
                                ))
                            || ((gate.kind.is_extended()
                                || matches!(gate.kind, GateKind::Module(_)))
                                && intersects(
                                    rect,
                                    label_rect(block_title(gate), gate.kind.name(), 9.0),
                                ))
                            || (gate.kind == GateKind::Dip
                                && intersects(
                                    rect,
                                    label_rect(value_label(gate), "0xFFFFFFFFFFFFFFFF", 8.0),
                                ))
                    })
                    || occupied.iter().any(|other| intersects(rect, *other));
                if !blocked {
                    return Some(origin);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use rgate_core::demo;

    #[test]
    fn bus_example_labels_avoid_bodies_titles_and_other_labels() {
        let circuit = demo::bus_memory();
        let module = &circuit.modules[0];
        let mut occupied = Vec::new();
        let mut count = 0;
        for wire in &module.wires {
            let name = &module.net(wire.net).unwrap().name;
            if let Some(origin) = wire_label(module, wire, name, &occupied) {
                let rect = label_rect(origin, name, 10.0);
                assert!(
                    module
                        .gates
                        .iter()
                        .all(|gate| !intersects(rect, gate.bounds()))
                );
                assert!(occupied.iter().all(|other| !intersects(rect, *other)));
                occupied.push(rect.expanded(2.0));
                count += 1;
            }
        }
        assert_eq!(count, 6);
        for gate in &module.gates {
            if gate.kind.is_extended() {
                assert!(!intersects(
                    label_rect(block_title(gate), gate.kind.name(), 9.0),
                    gate.bounds()
                ));
                assert!(!intersects(
                    label_rect(instance_label(gate), &gate.name, 10.5),
                    label_rect(block_title(gate), gate.kind.name(), 9.0)
                ));
            }
        }
    }
}
