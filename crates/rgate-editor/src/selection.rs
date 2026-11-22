use crate::{Editor, Hit};
use rgate_core::{GateId, GateKind, PinRef, Point, Rect, WireId, segment_distance};

impl Editor {
    pub fn snapped(&self, point: Point) -> Point {
        if self.snap {
            point.snapped(self.grid)
        } else {
            point
        }
    }

    pub fn select(&mut self, gate: GateId, additive: bool) {
        if !additive {
            self.selection.clear();
            self.selected_wires.clear();
        }
        if additive && self.selection.contains(&gate) {
            self.selection.remove(&gate);
        } else {
            self.selection.insert(gate);
        }
        self.selected_net = None;
    }

    pub fn select_wire(&mut self, id: WireId, additive: bool) {
        if !additive {
            self.selection.clear();
            self.selected_wires.clear();
        }
        if additive && self.selected_wires.contains(&id) {
            self.selected_wires.remove(&id);
        } else {
            self.selected_wires.insert(id);
        }
        self.selected_net = if self.selected_wires.contains(&id) {
            self.module()
                .wires
                .iter()
                .find(|wire| wire.id == id)
                .map(|wire| wire.net)
        } else {
            None
        };
    }

    pub fn select_all(&mut self) {
        self.selection = self.module().gates.iter().map(|gate| gate.id).collect();
        self.selected_wires = self.module().wires.iter().map(|wire| wire.id).collect();
        self.selected_net = None;
    }

    pub fn select_region(&mut self, bounds: Rect, additive: bool) {
        if !additive {
            self.selection.clear();
            self.selected_wires.clear();
        }
        let ids = self
            .module()
            .gates
            .iter()
            .filter(|gate| bounds.contains(gate.position))
            .map(|gate| gate.id)
            .collect::<Vec<_>>();
        self.selection.extend(ids);
        let wires = self
            .module()
            .wires
            .iter()
            .filter(|wire| {
                wire.points
                    .windows(2)
                    .any(|segment| segment_intersects_rect(segment[0], segment[1], bounds))
            })
            .map(|wire| wire.id)
            .collect::<Vec<_>>();
        self.selected_wires.extend(wires);
        self.selected_net = None;
    }

    pub fn hit_test(&self, point: Point, tolerance: f32) -> Option<Hit> {
        let module = self.module();
        for gate in module.gates.iter().rev() {
            for pin in &gate.pins {
                if gate.pin_position(pin).distance(point) <= tolerance {
                    return Some(Hit::Pin(PinRef::new(gate.id, &pin.name)));
                }
            }
        }
        if let Some(gate) = module.gates.iter().rev().find(|gate| {
            let bounds = gate.bounds();
            bounds.expanded(2.0).contains(point)
                && (gate.kind != GateKind::Frame
                    || !Rect::from_points(
                        bounds.min + Point::new(5.0, 5.0),
                        bounds.max - Point::new(5.0, 5.0),
                    )
                    .contains(point))
        }) {
            return Some(Hit::Gate(gate.id));
        }
        module.wires.iter().rev().find_map(|wire| {
            wire.points.windows(2).find_map(|segment| {
                let (distance, nearest) = segment_distance(point, segment[0], segment[1]);
                (distance <= tolerance).then_some(Hit::Wire {
                    id: wire.id,
                    net: wire.net,
                    point: nearest,
                })
            })
        })
    }

    pub(crate) fn clean_selection(&mut self) {
        if self.circuit.module(&self.active).is_none() {
            self.active = self.circuit.root.clone();
        }
        self.selection.clear();
        self.selected_wires.clear();
        self.selected_net = None;
        self.draft = None;
    }
}

fn segment_intersects_rect(a: Point, b: Point, rect: Rect) -> bool {
    let delta = b - a;
    let mut low = 0.0f32;
    let mut high = 1.0f32;
    for (p, q) in [
        (-delta.x, a.x - rect.min.x),
        (delta.x, rect.max.x - a.x),
        (-delta.y, a.y - rect.min.y),
        (delta.y, rect.max.y - a.y),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return false;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                low = low.max(t);
            } else {
                high = high.min(t);
            }
            if low > high {
                return false;
            }
        }
    }
    true
}
