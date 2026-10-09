//! Orthogonal visibility-grid A* routing with bend, crossing and congestion costs.
//! The heuristic reduces crossings; it does not claim a global crossing minimum.
use rgate_core::{GateKind, Module, Point, Rect, Wire, WireId};
use std::{
    cmp::{Ordering, Reverse},
    collections::{BTreeSet, BinaryHeap, HashMap},
};

#[derive(Clone, Copy, Debug)]
struct Cost(f32);
impl PartialEq for Cost {
    fn eq(&self, rhs: &Self) -> bool {
        self.0 == rhs.0
    }
}
impl Eq for Cost {}
impl PartialOrd for Cost {
    fn partial_cmp(&self, rhs: &Self) -> Option<Ordering> {
        Some(self.cmp(rhs))
    }
}
impl Ord for Cost {
    fn cmp(&self, rhs: &Self) -> Ordering {
        self.0.total_cmp(&rhs.0)
    }
}

pub fn crosses_body(a: Point, b: Point, rect: Rect) -> bool {
    if a.x == b.x {
        rect.min.x < a.x
            && a.x < rect.max.x
            && a.y.min(b.y).max(rect.min.y) < a.y.max(b.y).min(rect.max.y)
    } else {
        rect.min.y < a.y
            && a.y < rect.max.y
            && a.x.min(b.x).max(rect.min.x) < a.x.max(b.x).min(rect.max.x)
    }
}
fn crossing(a: Point, b: Point, c: Point, d: Point) -> bool {
    if a.x == b.x && c.y == d.y {
        a.x > c.x.min(d.x) && a.x < c.x.max(d.x) && c.y > a.y.min(b.y) && c.y < a.y.max(b.y)
    } else if a.y == b.y && c.x == d.x {
        crossing(c, d, a, b)
    } else {
        false
    }
}
fn overlap(a: Point, b: Point, c: Point, d: Point) -> f32 {
    if a.x == b.x && c.x == d.x && a.x == c.x {
        (a.y.max(b.y).min(c.y.max(d.y)) - a.y.min(b.y).max(c.y.min(d.y))).max(0.0)
    } else if a.y == b.y && c.y == d.y && a.y == c.y {
        (a.x.max(b.x).min(c.x.max(d.x)) - a.x.min(b.x).max(c.x.min(d.x))).max(0.0)
    } else {
        0.0
    }
}
fn escape(module: &Module, reference: &Option<rgate_core::PinRef>, point: Point) -> Point {
    let Some(gate) = reference.as_ref().and_then(|pin| module.gate(pin.gate)) else {
        return point;
    };
    let rect = gate.bounds();
    let mut choices = [
        (
            (point.x - rect.min.x).abs(),
            Point::new(rect.min.x - 12.0, point.y),
        ),
        (
            (point.x - rect.max.x).abs(),
            Point::new(rect.max.x + 12.0, point.y),
        ),
        (
            (point.y - rect.min.y).abs(),
            Point::new(point.x, rect.min.y - 12.0),
        ),
        (
            (point.y - rect.max.y).abs(),
            Point::new(point.x, rect.max.y + 12.0),
        ),
    ];
    choices.sort_by(|a, b| a.0.total_cmp(&b.0));
    choices[0].1
}
fn coordinates(values: &mut Vec<f32>) {
    values.sort_by(f32::total_cmp);
    values.dedup();
}
fn index(values: &[f32], value: f32) -> usize {
    values
        .binary_search_by(|candidate| candidate.total_cmp(&value))
        .unwrap()
}

pub fn route_selected(module: &mut Module, selected: &BTreeSet<WireId>) -> Result<(), String> {
    if selected.is_empty() {
        return Err("Select wires to route (or Select all).".into());
    }
    let obstacles = module
        .gates
        .iter()
        .filter(|gate| !matches!(gate.kind, GateKind::Frame | GateKind::Comment))
        .map(|gate| (gate.id, gate.bounds().expanded(4.0)))
        .collect::<Vec<_>>();
    let targets = module
        .wires
        .iter()
        .filter(|wire| selected.contains(&wire.id))
        .cloned()
        .collect::<Vec<_>>();
    if targets.len() > 1000 || obstacles.len() > 500 {
        return Err("Routing is limited to 1000 wires / 500 obstacles per operation; route smaller selections.".into());
    }
    let mut accepted = module
        .wires
        .iter()
        .filter(|wire| !selected.contains(&wire.id))
        .cloned()
        .collect::<Vec<_>>();
    for wire in targets {
        let points = route(module, &wire, &obstacles, &accepted)?;
        let mut routed = wire.clone();
        routed.points = points.clone();
        accepted.push(routed);
        module
            .wires
            .iter_mut()
            .find(|item| item.id == wire.id)
            .unwrap()
            .points = points;
    }
    Ok(())
}
fn route(
    module: &Module,
    wire: &Wire,
    obstacles: &[(rgate_core::GateId, Rect)],
    existing: &[Wire],
) -> Result<Vec<Point>, String> {
    let a = *wire.points.first().ok_or("wire has no endpoints")?;
    let b = *wire.points.last().unwrap();
    let start = escape(module, &wire.start, a);
    let end = escape(module, &wire.end, b);
    // Pin-to-escape leads must not tunnel through another component.
    for (id, rect) in obstacles {
        if wire.start.as_ref().is_none_or(|pin| pin.gate != *id) && crosses_body(a, start, *rect) {
            return Err(format!(
                "Wire {} starts inside another component; move the gates first.",
                wire.id.0
            ));
        }
        if wire.end.as_ref().is_none_or(|pin| pin.gate != *id) && crosses_body(b, end, *rect) {
            return Err(format!(
                "Wire {} ends inside another component; move the gates first.",
                wire.id.0
            ));
        }
    }
    let mut xs = vec![start.x, end.x];
    let mut ys = vec![start.y, end.y];
    for (_, rect) in obstacles {
        xs.extend([rect.min.x - 8.0, rect.max.x + 8.0]);
        ys.extend([rect.min.y - 8.0, rect.max.y + 8.0]);
    }
    coordinates(&mut xs);
    coordinates(&mut ys);
    if xs.len() * ys.len() > 400_000 {
        return Err("Routing grid too large; reduce the selection or module size.".into());
    }
    let initial = (index(&xs, start.x), index(&ys, start.y), 0u8);
    let goal = (index(&xs, end.x), index(&ys, end.y));
    let mut queue = BinaryHeap::from([Reverse((Cost(0.0), initial))]);
    let mut costs = HashMap::from([(initial, 0.0f32)]);
    let mut previous = HashMap::new();
    let mut blocked = HashMap::new();
    let mut final_node = None;
    let mut expanded = 0;
    while let Some(Reverse((_, node))) = queue.pop() {
        expanded += 1;
        if expanded > 300_000 {
            return Err(format!("Wire {} route search exceeded budget", wire.id.0));
        }
        let (ix, iy, axis) = node;
        if (ix, iy) == goal {
            final_node = Some(node);
            break;
        }
        let cost = costs[&node];
        let p = Point::new(xs[ix], ys[iy]);
        for (dx, dy, direction) in [(1isize, 0isize, 1u8), (-1, 0, 1), (0, 1, 2), (0, -1, 2)] {
            let nx = ix as isize + dx;
            let ny = iy as isize + dy;
            if nx < 0 || ny < 0 || nx >= xs.len() as isize || ny >= ys.len() as isize {
                continue;
            }
            let next = (nx as usize, ny as usize, direction);
            let q = Point::new(xs[next.0], ys[next.1]);
            let edge = if (ix, iy) < (next.0, next.1) {
                (ix, iy, next.0, next.1)
            } else {
                (next.0, next.1, ix, iy)
            };
            let is_blocked = *blocked
                .entry(edge)
                .or_insert_with(|| obstacles.iter().any(|(_, rect)| crosses_body(p, q, *rect)));
            if is_blocked {
                continue;
            }
            let mut penalty = 0.0;
            for other in existing.iter().filter(|other| other.net != wire.net) {
                for segment in other.points.windows(2) {
                    if crossing(p, q, segment[0], segment[1]) {
                        penalty += 90.0;
                    }
                    penalty += overlap(p, q, segment[0], segment[1]) * 1.5;
                }
            }
            let distance = (p.x - q.x).abs() + (p.y - q.y).abs();
            let next_cost = cost
                + distance
                + penalty
                + if axis != 0 && axis != direction {
                    20.0
                } else {
                    0.0
                };
            if next_cost < *costs.get(&next).unwrap_or(&f32::INFINITY) {
                costs.insert(next, next_cost);
                previous.insert(next, node);
                let heuristic = (q.x - end.x).abs() + (q.y - end.y).abs();
                queue.push(Reverse((Cost(next_cost + heuristic), next)));
            }
        }
    }
    let mut node = final_node.ok_or_else(|| {
        format!(
            "No obstacle-free route for wire {}. Move components or route a smaller selection.",
            wire.id.0
        )
    })?;
    let mut points = vec![b, end];
    while node != initial {
        points.push(Point::new(xs[node.0], ys[node.1]));
        node = previous[&node];
    }
    points.push(start);
    points.push(a);
    points.reverse();
    let mut simplified: Vec<Point> = Vec::new();
    for p in points {
        if simplified.last() == Some(&p) {
            continue;
        }
        while simplified.len() >= 2 {
            let n = simplified.len();
            let prev = simplified[n - 2];
            let last = simplified[n - 1];
            if (prev.x == last.x && last.x == p.x) || (prev.y == last.y && last.y == p.y) {
                simplified.pop();
            } else {
                break;
            }
        }
        simplified.push(p);
    }
    if simplified.len() < 2 {
        simplified.push(b);
    }
    Ok(simplified)
}

pub fn crossing_count(module: &Module) -> usize {
    let mut count = 0;
    for (index, a) in module.wires.iter().enumerate() {
        for b in &module.wires[index + 1..] {
            if a.net == b.net {
                continue;
            }
            for x in a.points.windows(2) {
                for y in b.points.windows(2) {
                    if crossing(x[0], x[1], y[0], y[1]) {
                        count += 1;
                    }
                }
            }
        }
    }
    count
}
