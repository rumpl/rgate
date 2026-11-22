use crate::Hit;
use rgate_core::{GateId, GateKind, Module, NetId, PinRef, Point, Rect, WireId, segment_distance};
use std::collections::BTreeSet;

/// Owns the currently selected gates, wires, and net with consistent toggle/clear invariants.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub gates: BTreeSet<GateId>,
    pub wires: BTreeSet<WireId>,
    pub net: Option<NetId>,
}

impl Selection {
    pub fn clear(&mut self) {
        self.gates.clear();
        self.wires.clear();
        self.net = None;
    }

    pub fn select_net(&mut self, net: NetId) {
        self.gates.clear();
        self.wires.clear();
        self.net = Some(net);
    }

    pub fn select_wire_single(&mut self, id: WireId, net: NetId) {
        self.gates.clear();
        self.wires.clear();
        self.wires.insert(id);
        self.net = Some(net);
    }

    pub fn select_gate(&mut self, gate: GateId, additive: bool) {
        if !additive {
            self.gates.clear();
            self.wires.clear();
        }
        if additive && self.gates.contains(&gate) {
            self.gates.remove(&gate);
        } else {
            self.gates.insert(gate);
        }
        self.net = None;
    }

    pub fn select_wire(&mut self, module: &Module, id: WireId, additive: bool) {
        if !additive {
            self.gates.clear();
            self.wires.clear();
        }
        if additive && self.wires.contains(&id) {
            self.wires.remove(&id);
        } else {
            self.wires.insert(id);
        }
        self.net = if self.wires.contains(&id) {
            module
                .wires
                .iter()
                .find(|wire| wire.id == id)
                .map(|wire| wire.net)
        } else {
            None
        };
    }

    pub fn select_all(&mut self, module: &Module) {
        self.gates = module.gates.iter().map(|gate| gate.id).collect();
        self.wires = module.wires.iter().map(|wire| wire.id).collect();
        self.net = None;
    }

    pub fn select_region(&mut self, module: &Module, bounds: Rect, additive: bool) {
        if !additive {
            self.gates.clear();
            self.wires.clear();
        }
        let ids = module
            .gates
            .iter()
            .filter(|gate| bounds.contains(gate.position))
            .map(|gate| gate.id);
        self.gates.extend(ids);
        let wires = module
            .wires
            .iter()
            .filter(|wire| {
                wire.points
                    .windows(2)
                    .any(|segment| segment_intersects_rect(segment[0], segment[1], bounds))
            })
            .map(|wire| wire.id);
        self.wires.extend(wires);
        self.net = None;
    }
}

pub fn hit_test(module: &Module, point: Point, tolerance: f32) -> Option<Hit> {
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
