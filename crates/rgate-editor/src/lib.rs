mod organization;
mod routing;
pub use organization::{Align, Distribute, SearchResult, SearchTarget, search};
use rgate_core::{
    Circuit, Gate, GateId, GateKind, Module, Net, NetId, PinRef, Point, Rect, Signal, Wire, WireId,
    route_to, segment_distance,
};
use std::collections::{BTreeSet, HashMap};
use thiserror::Error;

const HISTORY_LIMIT: usize = 128;

#[derive(Clone, Debug, PartialEq)]
pub enum Tool {
    Select,
    Wire,
    Pan,
    Delete,
    Place(GateKind),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Hit {
    Pin(PinRef),
    Gate(GateId),
    Wire {
        id: WireId,
        net: NetId,
        point: Point,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct WireDraft {
    pub net: Option<NetId>,
    pub start: Option<PinRef>,
    pub points: Vec<Point>,
}

#[derive(Clone, Debug)]
pub struct WireSegmentDrag {
    wire: WireId,
    points: Vec<Point>,
    segment: usize,
    horizontal: bool,
    anchor: Point,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Viewport {
    pub zoom: f32,
    pub pan: Point,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            zoom: 1.5,
            pan: Point::new(25.0, 20.0),
        }
    }
}

impl Viewport {
    pub fn to_screen(&self, point: Point) -> Point {
        point * self.zoom + self.pan
    }
    pub fn to_world(&self, point: Point) -> Point {
        (point - self.pan) / self.zoom
    }

    pub fn zoom_at(&mut self, factor: f32, anchor: Point) {
        let world = self.to_world(anchor);
        self.zoom = (self.zoom * factor).clamp(0.25, 6.0);
        self.pan = anchor - world * self.zoom;
    }

    pub fn fit(&mut self, bounds: Option<Rect>, available: Point) {
        if let Some(bounds) = bounds {
            let size = bounds.size();
            self.zoom = ((available.x - 70.0) / size.x.max(1.0))
                .min((available.y - 70.0) / size.y.max(1.0))
                .clamp(0.25, 3.0);
            self.pan = available / 2.0 - bounds.center() * self.zoom;
        } else {
            *self = Self::default();
        }
    }
}

#[derive(Debug, Error)]
pub enum EditError {
    #[error("invalid circuit: {0}")]
    Invalid(String),
    #[error("unknown module {0}")]
    UnknownModule(String),
    #[error("select a pin or wire to begin a connection")]
    NoConnection,
    #[error("wire endpoints must have the same bit width ({0} vs {1})")]
    WidthMismatch(u16, u16),
    #[error("unknown pin or wire endpoint")]
    UnknownEndpoint,
    #[error("no wire is being drawn")]
    NoDraft,
}

/// UI-independent document controller. Every committed edit is validated and undoable.
#[derive(Clone, Debug)]
pub struct Editor {
    circuit: Circuit,
    saved: Circuit,
    recovered: bool,
    active: String,
    undo: Vec<Circuit>,
    redo: Vec<Circuit>,
    gesture: Option<Circuit>,
    pub selection: BTreeSet<GateId>,
    pub selected_net: Option<NetId>,
    pub selected_wires: BTreeSet<WireId>,
    pub tool: Tool,
    pub viewport: Viewport,
    pub grid: f32,
    pub snap: bool,
    pub show_grid: bool,
    pub draft: Option<WireDraft>,
}

impl Editor {
    pub fn new(circuit: Circuit) -> Result<Self, EditError> {
        circuit
            .validate()
            .map_err(|error| EditError::Invalid(error.to_string()))?;
        Ok(Self {
            active: circuit.root.clone(),
            saved: circuit.clone(),
            recovered: false,
            circuit,
            undo: Vec::new(),
            redo: Vec::new(),
            gesture: None,
            selection: BTreeSet::new(),
            selected_net: None,
            selected_wires: BTreeSet::new(),
            tool: Tool::Select,
            viewport: Viewport::default(),
            grid: 5.0,
            snap: true,
            show_grid: false,
            draft: None,
        })
    }

    pub fn circuit(&self) -> &Circuit {
        &self.circuit
    }
    pub fn active_module(&self) -> &str {
        &self.active
    }
    pub fn module(&self) -> &Module {
        self.circuit
            .module(&self.active)
            .expect("validated active module")
    }
    pub fn is_dirty(&self) -> bool {
        self.recovered || self.circuit != self.saved
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn mark_recovered(&mut self) {
        self.recovered = true;
    }

    pub fn mark_saved(&mut self) {
        self.saved = self.circuit.clone();
        self.recovered = false;
    }

    pub fn switch_module(&mut self, name: &str) -> Result<(), EditError> {
        if self.circuit.module(name).is_none() {
            return Err(EditError::UnknownModule(name.into()));
        }
        self.finish_gesture();
        self.active = name.into();
        self.selection.clear();
        self.selected_wires.clear();
        self.selected_net = None;
        self.draft = None;
        Ok(())
    }

    pub fn create_module(&mut self, name: String) -> Result<(), EditError> {
        self.create_definition(Module::new(name))
    }

    pub fn create_definition(&mut self, module: Module) -> Result<(), EditError> {
        module
            .validate()
            .map_err(|e| EditError::Invalid(e.to_string()))?;
        let name = module.name.clone();
        if name.is_empty() || self.circuit.module(&name).is_some() {
            return Err(EditError::Invalid(
                "module name is empty or already exists".into(),
            ));
        }
        self.finish_gesture();
        let before = self.circuit.clone();
        self.circuit.modules.push(module);
        self.record(before);
        self.switch_module(&name)
    }

    pub fn set_tool(&mut self, tool: Tool) {
        self.finish_gesture();
        self.tool = tool;
        self.draft = None;
    }

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

    pub fn place(&mut self, kind: GateKind, point: Point) -> Result<GateId, EditError> {
        if self.module().verilog.is_some() {
            return Err(EditError::Invalid(
                "Edit Verilog source instead of placing gates inside a source module".into(),
            ));
        }
        let id = self.module().next_gate_id();
        let position = self.snapped(point);
        let instance = if let GateKind::Module(name) = &kind {
            Some(
                self.circuit
                    .module_instance(name, id, position)
                    .map_err(EditError::Invalid)?,
            )
        } else {
            None
        };
        self.edit_module(|module| {
            let mut gate = instance.unwrap_or_else(|| Gate::new(id, kind, position));
            gate.name = unique_name(
                module.gates.iter().map(|gate| gate.name.as_str()),
                &gate.name,
            );
            if gate.kind == GateKind::Comment {
                gate.text = "Comment — double-click to edit".into();
            }
            module.gates.push(gate);
        })?;
        self.select(id, false);
        Ok(id)
    }

    pub fn edit_module(&mut self, edit: impl FnOnce(&mut Module)) -> Result<(), EditError> {
        self.finish_gesture();
        let before = self.circuit.clone();
        edit(self.circuit.module_mut(&self.active).unwrap());
        if let Err(error) = self.sync_instances() {
            self.circuit = before;
            return Err(error);
        }
        if let Err(error) = self.circuit.validate() {
            self.circuit = before;
            return Err(EditError::Invalid(error.to_string()));
        }
        if before != self.circuit {
            self.record(before);
        }
        Ok(())
    }

    fn sync_instances(&mut self) -> Result<(), EditError> {
        let templates = self
            .circuit
            .modules
            .iter()
            .map(|module| {
                self.circuit
                    .module_instance(&module.name, GateId(0), Point::ZERO)
                    .map(|gate| (module.name.clone(), (gate.pins, gate.config.custom_symbol)))
                    .map_err(EditError::Invalid)
            })
            .collect::<Result<HashMap<_, _>, _>>()?;
        for module in &mut self.circuit.modules {
            for gate in &mut module.gates {
                let GateKind::Module(name) = &gate.kind else {
                    continue;
                };
                let template = templates
                    .get(name)
                    .ok_or_else(|| EditError::UnknownModule(name.clone()))?;
                let mut pins = template.0.clone();
                gate.config.custom_symbol = template.1.clone();
                for pin in &mut pins {
                    if let Some(old) = gate.pin(&pin.name) {
                        pin.net = old.net;
                        pin.offset = gate
                            .config
                            .custom_ports
                            .get(&pin.name)
                            .copied()
                            .unwrap_or(old.offset);
                    }
                }
                for wire in &mut module.wires {
                    for endpoint in [&mut wire.start, &mut wire.end] {
                        if endpoint.as_ref().is_some_and(|reference| {
                            reference.gate == gate.id
                                && !pins.iter().any(|pin| pin.name == reference.pin)
                        }) {
                            *endpoint = None;
                        }
                    }
                }
                gate.pins = pins;
            }
        }
        Ok(())
    }

    pub fn begin_wire_drag(&mut self, wire: WireId, point: Point) -> Option<WireSegmentDrag> {
        let definition = self
            .module()
            .wires
            .iter()
            .find(|definition| definition.id == wire)?;
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
        let drag = WireSegmentDrag {
            wire,
            points: definition.points.clone(),
            segment,
            horizontal: definition.points[segment].y == definition.points[segment + 1].y,
            anchor: point,
        };
        self.begin_gesture();
        Some(drag)
    }

    pub fn drag_wire_segment(&mut self, drag: &WireSegmentDrag, point: Point) {
        if !point.is_finite() || self.gesture.is_none() {
            return;
        }
        let original = drag.points[drag.segment];
        let raw = if drag.horizontal {
            original.y + point.y - drag.anchor.y
        } else {
            original.x + point.x - drag.anchor.x
        };
        let coordinate = if self.snap {
            (raw / self.grid).round() * self.grid
        } else {
            raw
        };
        let moved = |mut point: Point| {
            if drag.horizontal {
                point.y = coordinate;
            } else {
                point.x = coordinate;
            }
            point
        };
        let a = drag.points[drag.segment];
        let b = drag.points[drag.segment + 1];
        let delta = if drag.horizontal {
            coordinate - a.y
        } else {
            coordinate - a.x
        };
        let points = if delta.abs() < 0.001 {
            drag.points.clone()
        } else {
            // Keep endpoints fixed, including junctions and unattached wire ends.
            // Their new elbows extend the chosen segment without disconnecting pins.
            let mut points = drag.points[..drag.segment].to_vec();
            if drag.segment == 0 {
                points.push(a);
            }
            points.push(moved(a));
            points.push(moved(b));
            if drag.segment + 2 == drag.points.len() {
                points.push(b);
            }
            points.extend_from_slice(&drag.points[drag.segment + 2..]);
            points.dedup();
            points
        };
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

    pub fn resize_frame(&mut self, id: GateId, width: f32, height: f32) {
        if self.gesture.is_none() || !width.is_finite() || !height.is_finite() {
            return;
        }
        let width = if self.snap {
            (width / self.grid).round() * self.grid
        } else {
            width
        }
        .clamp(20.0, 10000.0);
        let height = if self.snap {
            (height / self.grid).round() * self.grid
        } else {
            height
        }
        .clamp(20.0, 10000.0);
        if let Some(gate) = self.circuit.module_mut(&self.active).unwrap().gate_mut(id)
            && gate.kind == GateKind::Frame
        {
            gate.config.frame_width = width;
            gate.config.frame_height = height;
        }
    }

    pub fn set_module_symbol(
        &mut self,
        name: &str,
        symbol: Vec<rgate_core::SymbolPrimitive>,
    ) -> Result<(), EditError> {
        let active = self.active.clone();
        self.switch_module(name)?;
        let result = self.edit_module(|module| module.symbol = symbol);
        self.switch_module(&active)?;
        result
    }

    pub fn begin_gesture(&mut self) {
        self.finish_gesture();
        self.gesture = Some(self.circuit.clone());
    }

    pub fn move_selection(&mut self, delta: Point) {
        if self.gesture.is_none() {
            self.begin_gesture();
        }
        if !delta.is_finite() {
            return;
        }
        let selected = self.selection.clone();
        let selected_wires = self.selected_wires.clone();
        let module = self.circuit.module_mut(&self.active).unwrap();
        let old = endpoint_positions(module, &selected);
        for gate in &mut module.gates {
            if selected.contains(&gate.id) {
                gate.position += delta;
            }
        }
        let original_wires = module
            .wires
            .iter()
            .map(|wire| (wire.id, wire.points.clone()))
            .collect::<HashMap<_, _>>();
        let internal = module
            .wires
            .iter()
            .filter(|wire| {
                let start = wire
                    .start
                    .as_ref()
                    .is_some_and(|pin| selected.contains(&pin.gate));
                let end = wire
                    .end
                    .as_ref()
                    .is_some_and(|pin| selected.contains(&pin.gate));
                (start && end)
                    || (selected_wires.contains(&wire.id)
                        && wire
                            .start
                            .as_ref()
                            .is_none_or(|pin| selected.contains(&pin.gate))
                        && wire
                            .end
                            .as_ref()
                            .is_none_or(|pin| selected.contains(&pin.gate)))
            })
            .map(|wire| wire.id)
            .collect::<BTreeSet<_>>();
        update_wire_endpoints(module, &old, &selected);
        for wire in &mut module.wires {
            if internal.contains(&wire.id) {
                wire.points = original_wires[&wire.id]
                    .iter()
                    .map(|point| *point + delta)
                    .collect();
            }
        }
    }

    pub fn finish_gesture(&mut self) {
        if let Some(before) = self.gesture.take()
            && before != self.circuit
        {
            self.record(before);
        }
    }

    pub fn cancel(&mut self) {
        if let Some(before) = self.gesture.take() {
            self.circuit = before;
        }
        self.draft = None;
        self.tool = Tool::Select;
    }

    pub fn rotate_selection(&mut self, clockwise: bool) -> Result<(), EditError> {
        let selected = self.selection.clone();
        self.edit_module(|module| {
            let old = endpoint_positions(module, &selected);
            for gate in &mut module.gates {
                if selected.contains(&gate.id) {
                    gate.rotation = (gate.rotation + if clockwise { 3 } else { 1 }) % 4;
                }
            }
            update_wire_endpoints(module, &old, &selected);
        })
    }

    pub fn delete_selection(&mut self) -> Result<(), EditError> {
        let selected = self.selection.clone();
        let wires = self.selected_wires.clone();
        let net = if wires.is_empty() {
            self.selected_net
        } else {
            None
        };
        self.edit_module(|module| {
            let affected_nets = module
                .gates
                .iter()
                .filter(|gate| selected.contains(&gate.id))
                .flat_map(|gate| gate.pins.iter())
                .filter_map(|pin| pin.net)
                .collect::<BTreeSet<_>>();
            module.gates.retain(|gate| !selected.contains(&gate.id));
            let removed = module
                .wires
                .iter()
                .filter(|wire| wires.contains(&wire.id) || Some(wire.net) == net)
                .cloned()
                .collect::<Vec<_>>();
            module.wires.retain(|wire| {
                Some(wire.net) != net
                    && !wires.contains(&wire.id)
                    && !(wire
                        .start
                        .as_ref()
                        .is_some_and(|pin| selected.contains(&pin.gate))
                        && wire
                            .end
                            .as_ref()
                            .is_some_and(|pin| selected.contains(&pin.gate)))
            });
            for reference in removed
                .iter()
                .flat_map(|wire| [&wire.start, &wire.end].into_iter().flatten())
            {
                let remains = module.wires.iter().any(|wire| {
                    wire.start.as_ref() == Some(reference) || wire.end.as_ref() == Some(reference)
                });
                if !remains
                    && let Some(pin) = module
                        .gate_mut(reference.gate)
                        .and_then(|gate| gate.pin_mut(&reference.pin))
                {
                    pin.net = None;
                }
            }
            for wire in &mut module.wires {
                if wire
                    .start
                    .as_ref()
                    .is_some_and(|pin| selected.contains(&pin.gate))
                {
                    wire.start = None;
                }
                if wire
                    .end
                    .as_ref()
                    .is_some_and(|pin| selected.contains(&pin.gate))
                {
                    wire.end = None;
                }
            }
            // Remove every branch/junction segment on a net once its last gate
            // endpoint is gone. This also handles deleting the gates in separate edits.
            let connected = module
                .gates
                .iter()
                .flat_map(|gate| gate.pins.iter())
                .filter_map(|pin| pin.net)
                .collect::<BTreeSet<_>>();
            module
                .wires
                .retain(|wire| !affected_nets.contains(&wire.net) || connected.contains(&wire.net));
            module.nets.retain(|net| {
                !affected_nets.contains(&net.id)
                    || connected.contains(&net.id)
                    || net.port.is_some()
            });
            if let Some(net) = net {
                for gate in &mut module.gates {
                    for pin in &mut gate.pins {
                        if pin.net == Some(net) {
                            pin.net = None;
                        }
                    }
                }
                module.nets.retain(|definition| definition.id != net);
            }
        })?;
        self.selection.clear();
        self.selected_wires.clear();
        self.selected_net = None;
        Ok(())
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

    pub fn toggle_initial(&mut self, id: GateId) -> Result<(), EditError> {
        self.edit_module(|module| {
            if let Some(gate) = module.gate_mut(id)
                && matches!(gate.kind, GateKind::Switch | GateKind::Dip)
            {
                let value = gate.initial.to_u64().unwrap_or(0).wrapping_add(1);
                gate.initial = Signal::from_u64(value, gate.width);
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
        let net = draft
            .net
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
        self.selected_net = Some(net);
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

    pub fn copy_selection(&self) -> Module {
        let mut copied = self.module().clone();
        copied
            .gates
            .retain(|gate| self.selection.contains(&gate.id));
        copied.wires.retain(|wire| {
            wire.start
                .as_ref()
                .is_some_and(|pin| self.selection.contains(&pin.gate))
                && wire
                    .end
                    .as_ref()
                    .is_some_and(|pin| self.selection.contains(&pin.gate))
        });
        let nets = copied
            .wires
            .iter()
            .map(|wire| wire.net)
            .collect::<BTreeSet<_>>();
        copied.nets.retain(|net| nets.contains(&net.id));
        for gate in &mut copied.gates {
            for pin in &mut gate.pins {
                if pin.net.is_some_and(|net| !nets.contains(&net)) {
                    pin.net = None;
                }
            }
        }
        copied
    }

    pub fn paste(&mut self, copied: &Module, offset: Point) -> Result<(), EditError> {
        copied
            .validate()
            .map_err(|error| EditError::Invalid(error.to_string()))?;
        let mut selected = BTreeSet::new();
        self.edit_module(|module| {
            let mut gates = HashMap::new();
            let mut nets = HashMap::new();
            for original in &copied.nets {
                let mut net = original.clone();
                net.id = module.next_net_id();
                net.name = unique_name(
                    module.nets.iter().map(|net| net.name.as_str()),
                    &original.name,
                );
                nets.insert(original.id, net.id);
                module.nets.push(net);
            }
            for original in &copied.gates {
                let mut gate = original.clone();
                gate.id = module.next_gate_id();
                gate.name = unique_name(
                    module.gates.iter().map(|gate| gate.name.as_str()),
                    &original.name,
                );
                gate.position += offset;
                for pin in &mut gate.pins {
                    pin.net = pin.net.and_then(|net| nets.get(&net).copied());
                }
                gates.insert(original.id, gate.id);
                selected.insert(gate.id);
                module.gates.push(gate);
            }
            for original in &copied.wires {
                let mut wire = original.clone();
                wire.id = module.next_wire_id();
                wire.net = nets[&wire.net];
                for point in &mut wire.points {
                    *point += offset;
                }
                for reference in [&mut wire.start, &mut wire.end].into_iter().flatten() {
                    reference.gate = gates[&reference.gate];
                }
                module.wires.push(wire);
            }
        })?;
        self.selection = selected;
        Ok(())
    }

    pub fn undo(&mut self) {
        self.finish_gesture();
        if let Some(previous) = self.undo.pop() {
            self.redo
                .push(std::mem::replace(&mut self.circuit, previous));
            self.clean_selection();
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.circuit, next));
            self.clean_selection();
        }
    }

    fn clean_selection(&mut self) {
        if self.circuit.module(&self.active).is_none() {
            self.active = self.circuit.root.clone();
        }
        self.selection.clear();
        self.selected_wires.clear();
        self.selected_net = None;
        self.draft = None;
    }

    fn record(&mut self, before: Circuit) {
        self.undo.push(before);
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
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

fn unique_name<'a>(existing: impl Iterator<Item = &'a str>, requested: &str) -> String {
    let existing = existing.collect::<BTreeSet<_>>();
    if !existing.contains(requested) {
        return requested.into();
    }
    let mut index = 2;
    loop {
        let name = format!("{requested}_{index}");
        if !existing.contains(name.as_str()) {
            return name;
        }
        index += 1;
    }
}

fn endpoint_positions(module: &Module, selected: &BTreeSet<GateId>) -> HashMap<PinRef, Point> {
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

fn update_wire_endpoints(
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

#[cfg(test)]
mod tests {
    use super::*;
    use rgate_core::demo;

    fn editor() -> Editor {
        Editor::new(demo::full_adder()).unwrap()
    }

    #[test]
    fn gesture_is_one_undo_step_and_dirty_tracks_saved_content() {
        let mut editor = editor();
        let initial = editor.circuit().clone();
        editor.select(GateId(1), false);
        editor.begin_gesture();
        for _ in 0..10 {
            editor.move_selection(Point::new(5.0, 0.0));
        }
        editor.finish_gesture();
        editor.circuit().validate().unwrap();
        assert!(editor.is_dirty());
        editor.undo();
        assert_eq!(editor.circuit(), &initial);
        assert!(!editor.is_dirty());
        assert!(!editor.can_undo());
        editor.redo();
        assert!(editor.is_dirty());
    }

    #[test]
    fn paste_remaps_ids_nets_names_and_endpoints() {
        let mut editor = editor();
        editor.select_all();
        let copied = editor.copy_selection();
        let count = copied.gates.len();
        editor.paste(&copied, Point::new(40.0, 30.0)).unwrap();
        assert_eq!(editor.module().gates.len(), count * 2);
        editor.circuit().validate().unwrap();
    }

    #[test]
    fn wiring_connects_pins_and_merges_nets() {
        let mut editor = Editor::new(Circuit::default()).unwrap();
        let a = editor
            .place(GateKind::Switch, Point::new(20.0, 30.0))
            .unwrap();
        let b = editor.place(GateKind::Not, Point::new(80.0, 30.0)).unwrap();
        editor.start_wire(Hit::Pin(PinRef::new(a, "Z"))).unwrap();
        let net = editor.finish_wire(Hit::Pin(PinRef::new(b, "I"))).unwrap();
        assert_eq!(
            editor.module().pin(&PinRef::new(a, "Z")).unwrap().net,
            Some(net)
        );
        assert_eq!(
            editor.module().pin(&PinRef::new(b, "I")).unwrap().net,
            Some(net)
        );
        editor.circuit().validate().unwrap();
        editor.undo();
        assert!(editor.module().wires.is_empty());
    }

    #[test]
    fn mismatched_bus_width_is_rejected_without_mutation() {
        let mut editor = Editor::new(Circuit::default()).unwrap();
        let a = editor.place(GateKind::Dip, Point::ZERO).unwrap();
        let b = editor.place(GateKind::Not, Point::new(80.0, 0.0)).unwrap();
        editor.start_wire(Hit::Pin(PinRef::new(a, "Z"))).unwrap();
        let before = editor.circuit().clone();
        assert!(matches!(
            editor.finish_wire(Hit::Pin(PinRef::new(b, "I"))),
            Err(EditError::WidthMismatch(8, 1))
        ));
        assert_eq!(editor.circuit(), &before);
    }

    #[test]
    fn invalid_property_edit_rolls_back() {
        let mut editor = editor();
        let before = editor.circuit().clone();
        assert!(
            editor
                .edit_module(|module| module.gates[0].width = 0)
                .is_err()
        );
        assert_eq!(editor.circuit(), &before);
    }

    #[test]
    fn zoom_keeps_cursor_anchored() {
        let mut viewport = Viewport::default();
        let anchor = Point::new(250.0, 180.0);
        let world = viewport.to_world(anchor);
        viewport.zoom_at(1.25, anchor);
        assert!(viewport.to_world(anchor).distance(world) < 0.001);
    }

    #[test]
    fn deletion_leaves_valid_dangling_wires() {
        let mut editor = editor();
        editor.select(GateId(1), false);
        editor.delete_selection().unwrap();
        editor.circuit().validate().unwrap();
        assert!(editor.module().gate(GateId(1)).is_none());
    }

    #[test]
    fn module_creation_is_undoable_and_switching_preserves_contents() {
        let mut editor = editor();
        let original = editor.circuit().clone();
        editor.create_module("child".into()).unwrap();
        assert_eq!(editor.active_module(), "child");
        assert!(editor.is_dirty());
        assert!(editor.create_module("child".into()).is_err());
        editor.undo();
        assert_eq!(editor.circuit(), &original);
        assert_eq!(editor.active_module(), "main");
        editor.redo();
        editor.switch_module("child").unwrap();
        editor.place(GateKind::And, Point::ZERO).unwrap();
        editor.switch_module("main").unwrap();
        assert_eq!(editor.module().gates.len(), 12);
        editor.switch_module("child").unwrap();
        assert_eq!(editor.module().gates.len(), 1);
    }
    #[test]
    fn placing_instances_and_interface_updates_are_transactional() {
        let mut editor = Editor::new(demo::hierarchical_inverters()).unwrap();
        let id = editor
            .place(
                GateKind::Module("inverter".into()),
                Point::new(500.0, 200.0),
            )
            .unwrap();
        assert_eq!(editor.module().gate(id).unwrap().pins.len(), 2);
        let before = editor.circuit().clone();
        assert!(
            editor
                .place(GateKind::Module("main".into()), Point::ZERO)
                .is_err()
        );
        assert_eq!(editor.circuit(), &before);
        editor.switch_module("inverter").unwrap();
        editor
            .edit_module(|module| {
                let mut bus = Net::new(NetId(3), "bus", 8);
                bus.port = Some(rgate_core::Direction::Input);
                module.nets.push(bus);
            })
            .unwrap();
        editor.switch_module("main").unwrap();
        assert_eq!(editor.module().gate(id).unwrap().pin_width("bus"), 8);
        editor.switch_module("inverter").unwrap();
        let before = editor.circuit().clone();
        assert!(
            editor
                .edit_module(|module| module.nets[0].width = 8)
                .is_err()
        );
        assert_eq!(editor.circuit(), &before);
    }
    #[test]
    fn wire_drag_is_snapped_endpoint_preserving_and_one_undo_step() {
        let mut editor = Editor::new(demo::bus_memory()).unwrap();
        let original = editor.circuit().clone();
        let wire = editor.module().wires[1].clone();
        let anchor = (wire.points[0] + wire.points[1]) / 2.0;
        let drag = editor.begin_wire_drag(wire.id, anchor).unwrap();
        for offset in [3.0, 8.0, 12.0] {
            editor.drag_wire_segment(&drag, anchor + Point::new(20.0, offset));
        }
        let changed = editor
            .module()
            .wires
            .iter()
            .find(|other| other.id == wire.id)
            .unwrap();
        assert_eq!(changed.points[0], wire.points[0]);
        assert_eq!(changed.points.last(), wire.points.last());
        assert_eq!(changed.start, wire.start);
        assert_eq!(changed.end, wire.end);
        assert_eq!(changed.net, wire.net);
        assert!(
            changed
                .points
                .windows(2)
                .all(|segment| segment[0].x == segment[1].x || segment[0].y == segment[1].y)
        );
        editor.circuit().validate().unwrap();
        editor.finish_gesture();
        editor.undo();
        assert_eq!(editor.circuit(), &original);
        assert!(!editor.can_undo());
        editor.redo();
        assert_ne!(editor.circuit(), &original);
    }

    #[test]
    fn dragging_vertical_segment_can_be_cancelled_without_editing_connectivity() {
        let mut editor = editor();
        let original = editor.circuit().clone();
        let wire = editor.module().wires[0].clone();
        let anchor = (wire.points[1] + wire.points[2]) / 2.0;
        let drag = editor.begin_wire_drag(wire.id, anchor).unwrap();
        editor.drag_wire_segment(&drag, anchor + Point::new(25.0, 0.0));
        let changed = &editor.module().wires[0];
        assert_eq!(changed.points[1].x, wire.points[1].x + 25.0);
        assert_eq!(changed.points[0], wire.points[0]);
        assert_eq!(changed.points.last(), wire.points.last());
        editor.cancel();
        assert_eq!(editor.circuit(), &original);
        assert!(!editor.can_undo());
    }
    #[test]
    fn dropping_narrow_wire_on_bus_creates_undoable_tap() {
        let mut editor = Editor::new(demo::bus_memory()).unwrap();
        let input = editor
            .place(GateKind::Led, Point::new(200.0, 200.0))
            .unwrap();
        let bus = editor.module().wires[0].clone();
        let point = bus.points[1];
        let original = editor.circuit().clone();
        editor
            .start_wire(Hit::Pin(PinRef::new(input, "I")))
            .unwrap();
        let net = editor
            .finish_wire(Hit::Wire {
                id: bus.id,
                net: bus.net,
                point,
            })
            .unwrap();
        let tap = editor
            .module()
            .gates
            .iter()
            .find(|gate| gate.kind == GateKind::Tap)
            .unwrap();
        assert_eq!(tap.config.tap_width, 1);
        assert_eq!(tap.pin("I").unwrap().net, Some(bus.net));
        assert_eq!(tap.pin("Z").unwrap().net, Some(net));
        editor.circuit().validate().unwrap();
        editor.undo();
        assert_eq!(editor.circuit(), &original);
    }
    #[test]
    fn custom_module_symbol_propagates_and_undo_restores_instances() {
        let mut editor = Editor::new(demo::hierarchical_inverters()).unwrap();
        let shape = rgate_core::SymbolPrimitive::Ellipse {
            start: Point::new(-40.0, -30.0),
            end: Point::new(40.0, 30.0),
            filled: false,
        };
        editor
            .set_module_symbol("wrapped_inverter", vec![shape.clone()])
            .unwrap();
        assert_eq!(
            editor
                .circuit()
                .module("main")
                .unwrap()
                .gate(GateId(2))
                .unwrap()
                .config
                .custom_symbol,
            vec![shape]
        );
        editor.undo();
        assert!(
            editor
                .circuit()
                .module("main")
                .unwrap()
                .gate(GateId(2))
                .unwrap()
                .config
                .custom_symbol
                .is_empty()
        );
    }
    #[test]
    fn removing_both_gates_cleans_wires_simultaneously_or_separately() {
        for together in [true, false] {
            let mut editor = Editor::new(Circuit::default()).unwrap();
            let a = editor.place(GateKind::Switch, Point::ZERO).unwrap();
            let b = editor.place(GateKind::Led, Point::new(100.0, 0.0)).unwrap();
            editor.start_wire(Hit::Pin(PinRef::new(a, "Z"))).unwrap();
            editor.finish_wire(Hit::Pin(PinRef::new(b, "I"))).unwrap();
            let before = editor.circuit().clone();
            editor.select(a, false);
            if together {
                editor.select(b, true);
            }
            editor.delete_selection().unwrap();
            if !together {
                assert_eq!(editor.module().wires.len(), 1);
                editor.select(b, false);
                editor.delete_selection().unwrap();
            }
            assert!(editor.module().wires.is_empty());
            assert!(editor.module().nets.is_empty());
            editor.circuit().validate().unwrap();
            editor.undo();
            if !together {
                editor.undo();
            }
            assert_eq!(editor.circuit(), &before);
        }
    }

    #[test]
    fn wire_click_shift_toggle_and_region_select_work_independently_of_gates() {
        let mut editor = editor();
        editor.select_wire(WireId(1), false);
        assert_eq!(editor.selected_wires, BTreeSet::from([WireId(1)]));
        editor.select_wire(WireId(2), true);
        assert_eq!(editor.selected_wires.len(), 2);
        editor.select_wire(WireId(1), true);
        assert_eq!(editor.selected_wires, BTreeSet::from([WireId(2)]));
        editor.select_region(
            Rect::from_points(Point::new(160.0, 123.0), Point::new(175.0, 131.0)),
            false,
        );
        assert_eq!(editor.selected_wires, BTreeSet::from([WireId(1)]));
        assert!(editor.selection.is_empty());
    }
}
