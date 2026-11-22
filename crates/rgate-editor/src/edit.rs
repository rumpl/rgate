use crate::wiring::{endpoint_positions, update_wire_endpoints};
use crate::{EditError, Editor};
use rgate_core::{Direction, Gate, GateId, GateKind, Module, Net, PinRef, Point, Signal};
use std::collections::{BTreeSet, HashMap};

impl Editor {
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
            if let Some(direction) = gate.kind.port_direction() {
                let base = match direction {
                    Direction::Input => "A",
                    Direction::Output => "Y",
                    Direction::InOut => "DATA",
                };
                let name = unique_name(module.nets.iter().map(|n| n.name.as_str()), base);
                gate.name = unique_name(module.gates.iter().map(|g| g.name.as_str()), &name);
                let mut net = Net::new(module.next_net_id(), &gate.name, gate.width);
                net.port = Some(direction);
                gate.pin_mut("P").unwrap().net = Some(net.id);
                module.nets.push(net);
            }
            if gate.kind == GateKind::Comment {
                gate.text = "Comment — double-click to edit".into();
            }
            module.gates.push(gate);
        })?;
        self.select(id, false);
        Ok(id)
    }

    pub fn resize_frame(&mut self, id: GateId, width: f32, height: f32) {
        if !self.history.in_gesture() || !width.is_finite() || !height.is_finite() {
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

    pub fn move_selection(&mut self, delta: Point) {
        if !self.history.in_gesture() {
            self.begin_gesture();
        }
        if !delta.is_finite() {
            return;
        }
        let selected = self.selection.gates.clone();
        let selected_wires = self.selection.wires.clone();
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

    pub fn rotate_selection(&mut self, clockwise: bool) -> Result<(), EditError> {
        let selected = self.selection.gates.clone();
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

    pub fn swap_inputs(&mut self, id: GateId, first: &str, second: &str) -> Result<(), EditError> {
        let gate = self.module().gate(id).ok_or(EditError::UnknownEndpoint)?;
        let a = gate.pin(first).ok_or(EditError::UnknownEndpoint)?;
        let b = gate.pin(second).ok_or(EditError::UnknownEndpoint)?;
        if first == second || a.direction != Direction::Input || b.direction != Direction::Input {
            return Err(EditError::Invalid(
                "Choose two different input pins on the same component.".into(),
            ));
        }
        if gate.pin_width(first) != gate.pin_width(second) {
            return Err(EditError::WidthMismatch(
                gate.pin_width(first),
                gate.pin_width(second),
            ));
        }
        let (a_net, b_net) = (a.net, b.net);
        let selected = BTreeSet::from([id]);
        self.edit_module(|module| {
            let mut old = endpoint_positions(module, &selected);
            let a_ref = PinRef::new(id, first);
            let b_ref = PinRef::new(id, second);
            let (a_position, b_position) = (old[&a_ref], old[&b_ref]);
            old.insert(a_ref, b_position);
            old.insert(b_ref, a_position);
            let gate = module.gate_mut(id).unwrap();
            gate.pin_mut(first).unwrap().net = b_net;
            gate.pin_mut(second).unwrap().net = a_net;
            for wire in &mut module.wires {
                for reference in [&mut wire.start, &mut wire.end].into_iter().flatten() {
                    if reference.gate == id {
                        if reference.pin == first {
                            reference.pin = second.into();
                        } else if reference.pin == second {
                            reference.pin = first.into();
                        }
                    }
                }
            }
            update_wire_endpoints(module, &old, &selected);
        })
    }

    pub fn delete_selection(&mut self) -> Result<(), EditError> {
        let selected = self.selection.gates.clone();
        let wires = self.selection.wires.clone();
        let net = if wires.is_empty() {
            self.selection.net
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
        Ok(())
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

    pub fn copy_selection(&self) -> Module {
        let mut copied = self.module().clone();
        copied
            .gates
            .retain(|gate| self.selection.gates.contains(&gate.id));
        copied.wires.retain(|wire| {
            wire.start
                .as_ref()
                .is_some_and(|pin| self.selection.gates.contains(&pin.gate))
                && wire
                    .end
                    .as_ref()
                    .is_some_and(|pin| self.selection.gates.contains(&pin.gate))
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
        self.selection.clear();
        self.selection.gates = selected;
        Ok(())
    }
}

pub(crate) fn unique_name<'a>(existing: impl Iterator<Item = &'a str>, requested: &str) -> String {
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
