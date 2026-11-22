use crate::edit::unique_name;
use crate::{EditError, Editor, Hit, WireDraft};
use rgate_core::{
    Gate, GateId, GateKind, Module, Net, NetId, PinRef, Point, Signal, Wire, WireId, route_to,
    segment_distance,
};
use std::collections::{BTreeSet, HashMap};

#[derive(Clone, Debug)]
pub struct WireSegmentDrag {
    pub wire: WireId,
    points: Vec<Point>,
    segment: usize,
    horizontal: bool,
    anchor: Point,
}

impl WireSegmentDrag {
    pub fn from_module(module: &Module, wire: WireId, point: Point) -> Option<Self> {
        let definition = module.wires.iter().find(|definition| definition.id == wire)?;
        let (segment, _) = definition
            .points
            .windows(2)
            .enumerate()
            .filter(|(_, segment)| {
                segment[0] != segment[1]
                    && (segment[0].x == segment[1].x || segment[0].y == segment[1].y)
            })
            .map(|(index, segment)| (index, segment_distance(point, segment[0], segment[1]).0))
            .min_by(|a, b| a.1.total_cmp(&b.1))?;
        Some(Self {
            wire,
            points: definition.points.clone(),
            segment,
            horizontal: definition.points[segment].y == definition.points[segment + 1].y,
            anchor: point,
        })
    }

    pub fn moved_points(&self, point: Point, grid: f32, snap: bool) -> Vec<Point> {
        let original = self.points[self.segment];
        let raw = if self.horizontal {
            original.y + point.y - self.anchor.y
        } else {
            original.x + point.x - self.anchor.x
        };
        let coordinate = if snap {
            (raw / grid).round() * grid
        } else {
            raw
        };
        let moved = |mut pt: Point| {
            if self.horizontal {
                pt.y = coordinate;
            } else {
                pt.x = coordinate;
            }
            pt
        };
        let a = self.points[self.segment];
        let b = self.points[self.segment + 1];
        let delta = if self.horizontal {
            coordinate - a.y
        } else {
            coordinate - a.x
        };
        if delta.abs() < 0.001 {
            return self.points.clone();
        }
        // Keep endpoints fixed, including junctions and unattached wire ends.
        // Their new elbows extend the chosen segment without disconnecting pins.
        let mut points = self.points[..self.segment].to_vec();
        if self.segment == 0 {
            points.push(a);
        }
        points.push(moved(a));
        points.push(moved(b));
        if self.segment + 2 == self.points.len() {
            points.push(b);
        }
        points.extend_from_slice(&self.points[self.segment + 2..]);
        points.dedup();
        points
    }
}

impl Editor {
    pub fn begin_wire_drag(&mut self, wire: WireId, point: Point) -> Option<WireSegmentDrag> {
        let drag = WireSegmentDrag::from_module(self.module(), wire, point)?;
        self.begin_gesture();
        Some(drag)
    }

    pub fn drag_wire_segment(&mut self, drag: &WireSegmentDrag, point: Point) {
        if !point.is_finite() || !self.history.in_gesture() {
            return;
        }
        let points = drag.moved_points(point, self.grid, self.snap);
        if let Some(wire) = self
            .circuit
            .module_mut(&self.active)
            .unwrap()
            .wires
            .iter_mut()
            .find(|wire| wire.id == drag.wire)
        {
            wire.points = points;
        }
    }

    pub fn delete_wire(&mut self, id: WireId) -> Result<(), EditError> {
        self.edit_module(|module| {
            if let Some(index) = module.wires.iter().position(|wire| wire.id == id) {
                let wire = module.wires.remove(index);
                for reference in [wire.start, wire.end].into_iter().flatten() {
                    let remains = module.wires.iter().any(|wire| {
                        wire.start.as_ref() == Some(&reference)
                            || wire.end.as_ref() == Some(&reference)
                    });
                    if !remains
                        && let Some(pin) = module
                            .gate_mut(reference.gate)
                            .and_then(|gate| gate.pin_mut(&reference.pin))
                    {
                        pin.net = None;
                    }
                }
            }
        })
    }

    pub fn start_wire(&mut self, hit: Hit) -> Result<(), EditError> {
        let (start, net, point) = match hit {
            Hit::Pin(reference) => {
                let pin = self
                    .module()
                    .pin(&reference)
                    .ok_or(EditError::UnknownEndpoint)?;
                let position = self.module().pin_position(&reference).unwrap();
                (Some(reference), pin.net, position)
            }
            Hit::Wire { net, point, .. } => (None, Some(net), point),
            Hit::Gate(_) => return Err(EditError::NoConnection),
        };
        self.draft = Some(WireDraft {
            net,
            start,
            points: vec![point],
        });
        Ok(())
    }

    pub fn wire_corner(&mut self, point: Point) -> Result<(), EditError> {
        let point = self.snapped(point);
        route_to(
            &mut self.draft.as_mut().ok_or(EditError::NoDraft)?.points,
            point,
        );
        Ok(())
    }

    pub fn finish_wire(&mut self, hit: Hit) -> Result<NetId, EditError> {
        let draft = self.draft.clone().ok_or(EditError::NoDraft)?;
        let (target, target_net, target_point) = match hit {
            Hit::Pin(reference) => {
                let pin = self
                    .module()
                    .pin(&reference)
                    .ok_or(EditError::UnknownEndpoint)?;
                (
                    Some(reference.clone()),
                    pin.net,
                    self.module().pin_position(&reference).unwrap(),
                )
            }
            Hit::Wire { net, point, .. } => (None, Some(net), point),
            _ => return Err(EditError::NoConnection),
        };
        let width_of = |net: Option<NetId>, pin: &Option<PinRef>| {
            net.and_then(|net| self.module().net(net))
                .map(|net| net.width)
                .or_else(|| {
                    pin.as_ref().and_then(|pin| {
                        self.module()
                            .gate(pin.gate)
                            .map(|gate| gate.pin_width(&pin.pin))
                    })
                })
                .unwrap_or(1)
        };
        let width = width_of(draft.net, &draft.start);
        let target_width = width_of(target_net, &target);
        if width != target_width {
            if width < target_width
                && target.is_none()
                && let Some(bus) = target_net
            {
                return self.finish_tap(draft, bus, target_point, width, target_width);
            }
            let scalar_allowed = |reference: &Option<PinRef>, small: u16, large: u16| {
                reference
                    .as_ref()
                    .and_then(|reference| self.module().gate(reference.gate))
                    .is_some_and(|gate| {
                        gate.kind.is_logic()
                            && !gate.kind.is_reduction()
                            && !gate.config.reduction
                            && small == 1
                            && large == gate.width
                    })
            };
            if !scalar_allowed(&target, target_width, width)
                && !scalar_allowed(&draft.start, width, target_width)
            {
                return Err(EditError::WidthMismatch(width, target_width));
            }
        }
        if draft.start.is_some() && draft.start == target {
            return Err(EditError::NoConnection);
        }
        if let (Some(a), Some(b)) = (draft.net, target_net)
            && a != b
            && self.module().net(a).unwrap().port.is_some()
            && self.module().net(b).unwrap().port.is_some()
        {
            return Err(EditError::Invalid(
                "Two different module ports must remain separate nets".into(),
            ));
        }
        let port_net = [draft.net, target_net]
            .into_iter()
            .flatten()
            .find(|id| self.module().net(*id).is_some_and(|n| n.port.is_some()));
        let net = port_net
            .or(draft.net)
            .or(target_net)
            .unwrap_or_else(|| self.module().next_net_id());
        let mut points = draft.points;
        route_to(&mut points, target_point);
        if points.len() < 2 {
            return Err(EditError::NoConnection);
        }
        let new_name = unique_name(
            self.module().nets.iter().map(|net| net.name.as_str()),
            &format!("w{}", net.0),
        );
        self.edit_module(|module| {
            if module.net(net).is_none() {
                module.nets.push(Net::new(net, new_name, width));
            }
            for merge in [draft.net, target_net]
                .into_iter()
                .flatten()
                .filter(|other| *other != net)
            {
                for gate in &mut module.gates {
                    for pin in &mut gate.pins {
                        if pin.net == Some(merge) {
                            pin.net = Some(net);
                        }
                    }
                }
                for wire in &mut module.wires {
                    if wire.net == merge {
                        wire.net = net;
                    }
                }
                module.nets.retain(|definition| definition.id != merge);
            }
            for reference in [&draft.start, &target].into_iter().flatten() {
                module
                    .gate_mut(reference.gate)
                    .unwrap()
                    .pin_mut(&reference.pin)
                    .unwrap()
                    .net = Some(net);
            }
            module.wires.push(Wire {
                id: module.next_wire_id(),
                net,
                points,
                start: draft.start,
                end: target,
            });
        })?;
        self.draft = None;
        self.selection.net = Some(net);
        Ok(net)
    }

    fn finish_tap(
        &mut self,
        draft: WireDraft,
        bus: NetId,
        position: Point,
        width: u16,
        bus_width: u16,
    ) -> Result<NetId, EditError> {
        let net = draft.net.unwrap_or_else(|| self.module().next_net_id());
        let id = self.module().next_gate_id();
        let mut gate = Gate::new(id, GateKind::Tap, position);
        gate.name = unique_name(
            self.module().gates.iter().map(|gate| gate.name.as_str()),
            &format!("tap{}", id.0),
        );
        gate.width = bus_width;
        gate.config.tap_width = width;
        gate.initial = Signal::from_u64(0, bus_width);
        gate.reset_pins();
        gate.position = position - gate.pin("I").unwrap().offset;
        gate.pin_mut("I").unwrap().net = Some(bus);
        gate.pin_mut("Z").unwrap().net = Some(net);
        let output = gate.pin_position(gate.pin("Z").unwrap());
        let mut points = draft.points;
        route_to(&mut points, output);
        let name = unique_name(
            self.module().nets.iter().map(|net| net.name.as_str()),
            &format!("w{}", net.0),
        );
        self.edit_module(|module| {
            if module.net(net).is_none() {
                module.nets.push(Net::new(net, name, width));
            }
            if let Some(reference) = &draft.start {
                module
                    .gate_mut(reference.gate)
                    .unwrap()
                    .pin_mut(&reference.pin)
                    .unwrap()
                    .net = Some(net);
            }
            module.gates.push(gate);
            module.wires.push(Wire {
                id: module.next_wire_id(),
                net,
                points,
                start: draft.start,
                end: Some(PinRef::new(id, "Z")),
            });
        })?;
        self.draft = None;
        self.select(id, false);
        Ok(net)
    }
}

pub(crate) fn endpoint_positions(
    module: &Module,
    selected: &BTreeSet<GateId>,
) -> HashMap<PinRef, Point> {
    module
        .gates
        .iter()
        .filter(|gate| selected.contains(&gate.id))
        .flat_map(|gate| {
            gate.pins
                .iter()
                .map(|pin| (PinRef::new(gate.id, &pin.name), gate.pin_position(pin)))
        })
        .collect()
}

pub(crate) fn update_wire_endpoints(
    module: &mut Module,
    old: &HashMap<PinRef, Point>,
    selected: &BTreeSet<GateId>,
) {
    let new = endpoint_positions(module, selected);
    for wire in &mut module.wires {
        let start_delta = wire
            .start
            .as_ref()
            .and_then(|reference| new.get(reference).zip(old.get(reference)))
            .map(|(new, old)| *new - *old);
        let end_delta = wire
            .end
            .as_ref()
            .and_then(|reference| new.get(reference).zip(old.get(reference)))
            .map(|(new, old)| *new - *old);
        if let (Some(a), Some(b)) = (start_delta, end_delta)
            && a == b
        {
            for point in &mut wire.points {
                *point += a;
            }
            continue;
        }
        if let Some(delta) = start_delta {
            move_endpoint(&mut wire.points, true, delta);
        }
        if let Some(delta) = end_delta {
            move_endpoint(&mut wire.points, false, delta);
        }
    }
}

fn move_endpoint(points: &mut Vec<Point>, start: bool, delta: Point) {
    if !start {
        points.reverse();
    }
    let old = points[0];
    let new = old + delta;
    points[0] = new;
    if points.len() > 2 {
        if (old.x - points[1].x).abs() < 0.1 {
            points[1].x = new.x;
        } else {
            points[1].y = new.y;
        }
    } else if points.len() == 2 && new.x != points[1].x && new.y != points[1].y {
        points.insert(1, Point::new(points[1].x, new.y));
    }
    if !start {
        points.reverse();
    }
}
