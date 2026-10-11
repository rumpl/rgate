use crate::{EditError, Editor, endpoint_positions, update_wire_endpoints};
use rgate_core::{Circuit, Direction, GateId, GateKind, NetId, Point};
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Clone, Copy, Debug)]
pub enum Align {
    Left,
    Right,
    Top,
    Bottom,
    HorizontalCenter,
    VerticalCenter,
}
#[derive(Clone, Copy, Debug)]
pub enum Distribute {
    Horizontal,
    Vertical,
}
#[derive(Clone, Debug, PartialEq)]
pub enum SearchTarget {
    Gate(GateId),
    Net(NetId),
}
#[derive(Clone, Debug)]
pub struct SearchResult {
    pub module: String,
    pub name: String,
    pub description: String,
    pub target: SearchTarget,
}

pub fn search(circuit: &Circuit, query: &str) -> Vec<SearchResult> {
    let terms = query
        .split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>();
    if terms.is_empty() {
        return Vec::new();
    }
    let mut results = Vec::new();
    for module in &circuit.modules {
        for gate in &module.gates {
            let text = format!("{} {} {}", module.name, gate.name, gate.kind.name()).to_lowercase();
            if terms.iter().all(|term| text.contains(term)) {
                results.push(SearchResult {
                    module: module.name.clone(),
                    name: gate.name.clone(),
                    description: gate.kind.name().into(),
                    target: SearchTarget::Gate(gate.id),
                });
            }
        }
        for net in &module.nets {
            let text = format!("{} {} net", module.name, net.name).to_lowercase();
            if terms.iter().all(|term| text.contains(term)) {
                results.push(SearchResult {
                    module: module.name.clone(),
                    name: net.name.clone(),
                    description: format!("{}-bit net", net.width),
                    target: SearchTarget::Net(net.id),
                });
            }
        }
    }
    results.truncate(200);
    results
}
impl Editor {
    pub fn align_selection(&mut self, alignment: Align) -> Result<(), EditError> {
        if self.selection.len() < 2 {
            return Err(EditError::Invalid(
                "Select at least two components to align.".into(),
            ));
        }
        let gates = self
            .module()
            .gates
            .iter()
            .filter(|gate| self.selection.contains(&gate.id))
            .collect::<Vec<_>>();
        let bounds = gates
            .iter()
            .map(|gate| gate.bounds())
            .reduce(|a, b| a.union(b))
            .unwrap();
        let targets = gates
            .iter()
            .map(|gate| {
                let body = gate.bounds();
                let delta = match alignment {
                    Align::Left => Point::new(bounds.min.x - body.min.x, 0.0),
                    Align::Right => Point::new(bounds.max.x - body.max.x, 0.0),
                    Align::Top => Point::new(0.0, bounds.min.y - body.min.y),
                    Align::Bottom => Point::new(0.0, bounds.max.y - body.max.y),
                    Align::HorizontalCenter => Point::new(0.0, bounds.center().y - body.center().y),
                    Align::VerticalCenter => Point::new(bounds.center().x - body.center().x, 0.0),
                };
                (gate.id, self.snapped(gate.position + delta))
            })
            .collect();
        self.apply_positions(targets)
    }
    pub fn distribute_selection(&mut self, axis: Distribute) -> Result<(), EditError> {
        if self.selection.len() < 3 {
            return Err(EditError::Invalid(
                "Select at least three components to distribute.".into(),
            ));
        }
        let coordinate = |p: Point| match axis {
            Distribute::Horizontal => p.x,
            Distribute::Vertical => p.y,
        };
        let mut gates = self
            .module()
            .gates
            .iter()
            .filter(|gate| self.selection.contains(&gate.id))
            .collect::<Vec<_>>();
        gates.sort_by(|a, b| {
            coordinate(a.position)
                .total_cmp(&coordinate(b.position))
                .then(a.id.cmp(&b.id))
        });
        let start = coordinate(gates[0].position);
        let end = coordinate(gates.last().unwrap().position);
        let step = (end - start) / (gates.len() - 1) as f32;
        let targets = gates
            .iter()
            .enumerate()
            .map(|(index, gate)| {
                let mut p = gate.position;
                match axis {
                    Distribute::Horizontal => p.x = start + index as f32 * step,
                    Distribute::Vertical => p.y = start + index as f32 * step,
                };
                (gate.id, self.snapped(p))
            })
            .collect();
        self.apply_positions(targets)
    }
    fn apply_positions(&mut self, positions: HashMap<GateId, Point>) -> Result<(), EditError> {
        let selected = positions.keys().copied().collect();
        self.edit_module(|module| {
            let old = endpoint_positions(module, &selected);
            for gate in &mut module.gates {
                if let Some(position) = positions.get(&gate.id) {
                    gate.position = *position;
                }
            }
            update_wire_endpoints(module, &old, &selected);
        })
    }
    pub fn autoroute_selection(&mut self) -> Result<(usize, usize), EditError> {
        let mut selected = self.selected_wires.clone();
        if selected.is_empty() {
            selected = self
                .module()
                .wires
                .iter()
                .filter(|wire| {
                    wire.start
                        .as_ref()
                        .is_some_and(|pin| self.selection.contains(&pin.gate))
                        || wire
                            .end
                            .as_ref()
                            .is_some_and(|pin| self.selection.contains(&pin.gate))
                })
                .map(|wire| wire.id)
                .collect();
        }
        let before = crate::routing::crossing_count(self.module());
        let mut routed = self.module().clone();
        crate::routing::route_selected(&mut routed, &selected).map_err(EditError::Invalid)?;
        let after = crate::routing::crossing_count(&routed);
        self.edit_module(|module| *module = routed)?;
        Ok((before, after))
    }
    /// Layered directed placement; cycle-breaking DFS plus barycenter sweeps.
    pub fn tidy_layout(&mut self) -> Result<(), EditError> {
        let ids = self
            .module()
            .gates
            .iter()
            .filter(|gate| {
                !matches!(gate.kind, GateKind::Frame | GateKind::Comment)
                    && (self.selection.is_empty() || self.selection.contains(&gate.id))
            })
            .map(|gate| gate.id)
            .collect::<BTreeSet<_>>();
        if ids.len() > 500 {
            return Err(EditError::Invalid(
                "Tidy layout supports up to 500 selected components; arrange smaller sections."
                    .into(),
            ));
        }
        if ids.len() < 2 {
            return Err(EditError::Invalid(
                "Select at least two components, or clear selection to arrange the module.".into(),
            ));
        }
        let mut edges: BTreeMap<GateId, BTreeSet<GateId>> =
            ids.iter().map(|id| (*id, BTreeSet::new())).collect();
        for net in &self.module().nets {
            let drivers = self
                .module()
                .gates
                .iter()
                .filter(|gate| {
                    ids.contains(&gate.id)
                        && gate.pins.iter().any(|pin| {
                            pin.net == Some(net.id)
                                && (gate.kind == GateKind::InputPort
                                    || (gate.kind.port_direction().is_none()
                                        && pin.direction == Direction::Output))
                        })
                })
                .map(|gate| gate.id)
                .collect::<Vec<_>>();
            let sinks = self
                .module()
                .gates
                .iter()
                .filter(|gate| {
                    ids.contains(&gate.id)
                        && gate.pins.iter().any(|pin| {
                            pin.net == Some(net.id)
                                && (gate.kind == GateKind::OutputPort
                                    || (gate.kind.port_direction().is_none()
                                        && pin.direction == Direction::Input))
                        })
                })
                .map(|gate| gate.id)
                .collect::<Vec<_>>();
            for driver in drivers {
                for sink in &sinks {
                    if driver != *sink {
                        edges.get_mut(&driver).unwrap().insert(*sink);
                    }
                }
            }
        }
        fn rank(
            id: GateId,
            edges: &BTreeMap<GateId, BTreeSet<GateId>>,
            visiting: &mut BTreeSet<GateId>,
            ranks: &mut HashMap<GateId, usize>,
        ) -> usize {
            if let Some(rank) = ranks.get(&id) {
                return *rank;
            }
            if !visiting.insert(id) {
                return 0;
            }
            let predecessors = edges
                .iter()
                .filter(|(_, targets)| targets.contains(&id))
                .map(|(id, _)| *id)
                .collect::<Vec<_>>();
            let mut value = 0;
            for predecessor in predecessors {
                if !visiting.contains(&predecessor) {
                    value = value.max(rank(predecessor, edges, visiting, ranks) + 1);
                }
            }
            visiting.remove(&id);
            ranks.insert(id, value);
            value
        }
        let mut ranks = HashMap::new();
        for id in &ids {
            rank(*id, &edges, &mut BTreeSet::new(), &mut ranks);
        }
        let mut layers: BTreeMap<usize, Vec<GateId>> = BTreeMap::new();
        for id in &ids {
            layers.entry(ranks[id]).or_default().push(*id);
        }
        for layer in layers.values_mut() {
            layer.sort_by(|a, b| {
                self.module()
                    .gate(*a)
                    .unwrap()
                    .position
                    .y
                    .total_cmp(&self.module().gate(*b).unwrap().position.y)
            });
        }
        for sweep in 0..8 {
            let order = layers
                .values()
                .flat_map(|layer| {
                    layer
                        .iter()
                        .enumerate()
                        .map(|(index, id)| (*id, index as f32))
                })
                .collect::<HashMap<_, _>>();
            for layer in layers.values_mut() {
                layer.sort_by(|a, b| {
                    let score = |id: &GateId| {
                        let adjacent = if sweep % 2 == 0 {
                            edges
                                .iter()
                                .filter(|(_, targets)| targets.contains(id))
                                .map(|(id, _)| *id)
                                .collect::<Vec<_>>()
                        } else {
                            edges[id].iter().copied().collect()
                        };
                        if adjacent.is_empty() {
                            order[id]
                        } else {
                            let connected = self
                                .module()
                                .wires
                                .iter()
                                .filter_map(|wire| {
                                    let (start, end) = (wire.start.as_ref()?, wire.end.as_ref()?);
                                    let neighbor = if start.gate == *id {
                                        end
                                    } else if end.gate == *id {
                                        start
                                    } else {
                                        return None;
                                    };
                                    let gate = self.module().gate(neighbor.gate)?;
                                    let pin = gate.pin(&neighbor.pin)?;
                                    let height = gate.bounds().size().y.max(1.0);
                                    order
                                        .get(&neighbor.gate)
                                        .map(|rank| *rank + pin.offset.y / height)
                                })
                                .collect::<Vec<_>>();
                            if connected.is_empty() {
                                adjacent.iter().map(|id| order[id]).sum::<f32>()
                                    / adjacent.len() as f32
                            } else {
                                connected.iter().sum::<f32>() / connected.len() as f32
                            }
                        }
                    };
                    score(a).total_cmp(&score(b)).then(a.cmp(b))
                });
            }
        }
        let origin = self
            .module()
            .gates
            .iter()
            .filter(|gate| ids.contains(&gate.id))
            .map(|gate| gate.bounds())
            .reduce(|a, b| a.union(b))
            .unwrap()
            .min;
        const ROW_GAP: f32 = 42.0;
        let height = |layer: &Vec<GateId>| {
            layer
                .iter()
                .map(|id| self.module().gate(*id).unwrap().bounds().size().y)
                .sum::<f32>()
                + ROW_GAP * layer.len().saturating_sub(1) as f32
        };
        let tallest = layers.values().map(&height).fold(0.0, f32::max);
        let mut x = origin.x;
        let mut positions = HashMap::new();
        for layer in layers.values() {
            let width = layer
                .iter()
                .map(|id| self.module().gate(*id).unwrap().bounds().size().x)
                .fold(0.0, f32::max);
            let mut y = origin.y + (tallest - height(layer)) / 2.0;
            for id in layer {
                let gate = self.module().gate(*id).unwrap();
                let size = gate.bounds().size();
                let center = Point::new(x + width / 2.0, y + size.y / 2.0);
                positions.insert(
                    *id,
                    self.snapped(gate.position + center - gate.bounds().center()),
                );
                y += size.y + ROW_GAP;
            }
            x += width + 90.0;
        }
        let mut arranged = self.module().clone();
        let old = endpoint_positions(&arranged, &ids);
        for gate in &mut arranged.gates {
            if let Some(position) = positions.get(&gate.id) {
                gate.position = *position;
            }
        }
        update_wire_endpoints(&mut arranged, &old, &ids);
        let wires = arranged
            .wires
            .iter()
            .filter(|wire| {
                wire.start.as_ref().is_some_and(|p| ids.contains(&p.gate))
                    || wire.end.as_ref().is_some_and(|p| ids.contains(&p.gate))
            })
            .map(|w| w.id)
            .collect::<BTreeSet<_>>();
        if !wires.is_empty() {
            crate::routing::route_selected(&mut arranged, &wires).map_err(EditError::Invalid)?;
        }
        self.edit_module(|module| *module = arranged)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rgate_core::{Gate, Net, PinRef, Wire, WireId, demo};
    #[test]
    fn search_alignment_distribution_and_undo() {
        let circuit = demo::full_adder();
        let results = search(&circuit, "main xor");
        assert!(results.iter().any(|row| row.name == "xor_sum"));
        assert!(
            search(&circuit, "Cout")
                .iter()
                .any(|row| matches!(row.target, SearchTarget::Net(_)))
        );
        let mut editor = Editor::new(circuit).unwrap();
        let before = editor.circuit().clone();
        for id in [GateId(1), GateId(2), GateId(3)] {
            editor.select(id, true);
        }
        editor.align_selection(Align::Left).unwrap();
        editor.distribute_selection(Distribute::Vertical).unwrap();
        let ys = editor
            .selection
            .iter()
            .map(|id| editor.module().gate(*id).unwrap().position.y)
            .collect::<Vec<_>>();
        assert_eq!(ys[1] - ys[0], ys[2] - ys[1]);
        editor.circuit().validate().unwrap();
        editor.undo();
        editor.undo();
        assert_eq!(editor.circuit(), &before);
    }
    #[test]
    fn obstacle_routing_preserves_connections_and_is_undoable() {
        let mut circuit = Circuit::default();
        let module = &mut circuit.modules[0];
        module.nets.push(Net::new(NetId(1), "wire", 1));
        let mut a = Gate::new(GateId(1), GateKind::Switch, Point::new(0.0, 0.0));
        a.pin_mut("Z").unwrap().net = Some(NetId(1));
        let mut b = Gate::new(GateId(2), GateKind::Led, Point::new(220.0, 0.0));
        b.rotation = 3;
        b.pin_mut("I").unwrap().net = Some(NetId(1));
        let blocker = Gate::new(GateId(3), GateKind::Ram, Point::new(110.0, 0.0));
        module.gates = vec![a, b, blocker];
        let start = PinRef::new(GateId(1), "Z");
        let end = PinRef::new(GateId(2), "I");
        module.wires.push(Wire {
            id: WireId(1),
            net: NetId(1),
            points: vec![
                module.pin_position(&start).unwrap(),
                module.pin_position(&end).unwrap(),
            ],
            start: Some(start),
            end: Some(end),
        });
        let before = circuit.clone();
        let mut editor = Editor::new(circuit).unwrap();
        editor.select_wire(WireId(1), false);
        editor.autoroute_selection().unwrap();
        let wire = &editor.module().wires[0];
        let bounds = editor.module().gate(GateId(3)).unwrap().bounds();
        assert!(
            wire.points
                .windows(2)
                .all(|pair| !crate::routing::crosses_body(pair[0], pair[1], bounds))
        );
        assert_eq!(wire.start, before.modules[0].wires[0].start);
        assert_eq!(wire.end, before.modules[0].wires[0].end);
        editor.undo();
        assert_eq!(editor.circuit(), &before);
    }
    #[test]
    fn group_internal_wire_moves_rigidly_and_tidy_layout_keeps_logic() {
        let mut editor = Editor::new(demo::bus_memory()).unwrap();
        let before = editor.circuit().clone();
        editor.select_all();
        editor.begin_gesture();
        editor.move_selection(Point::new(25.0, 40.0));
        editor.finish_gesture();
        for (old, new) in before.modules[0].wires.iter().zip(&editor.module().wires) {
            for (a, b) in old.points.iter().zip(&new.points) {
                assert_eq!(*b, *a + Point::new(25.0, 40.0));
            }
        }
        editor.undo();
        editor.tidy_layout().unwrap();
        editor.circuit().validate().unwrap();
        for (old, new) in before.modules[0].gates.iter().zip(&editor.module().gates) {
            assert_eq!(old.kind, new.kind);
            assert_eq!(
                old.pins.iter().map(|pin| pin.net).collect::<Vec<_>>(),
                new.pins.iter().map(|pin| pin.net).collect::<Vec<_>>()
            );
        }
        editor.undo();
        assert_eq!(editor.circuit(), &before);
    }
}

#[cfg(test)]
mod tidy_regression_tests {
    use super::*;
    #[test]
    fn led_chaser_layout_centers_layers_and_removes_wire_detours() {
        let document: serde_json::Value =
            serde_json::from_str(include_str!("../../../examples/verilog-led-chaser.rgate"))
                .unwrap();
        let circuit: Circuit = serde_json::from_value(document["circuit"].clone()).unwrap();
        let mut editor = Editor::new(circuit).unwrap();
        let before = editor.circuit().clone();
        editor.tidy_layout().unwrap();
        let module = editor.module();
        let chaser = module.gates.iter().find(|g| g.name == "chaser").unwrap();
        let inputs = module
            .gates
            .iter()
            .filter(|g| matches!(g.kind, GateKind::Switch | GateKind::Dip | GateKind::Clock))
            .map(|g| g.bounds())
            .reduce(|a, b| a.union(b))
            .unwrap();
        assert!((chaser.bounds().center().y - inputs.center().y).abs() < 10.0);
        assert_eq!(
            crate::routing::crossing_count(module),
            0,
            "tidy must separate the chaser input channels"
        );
        for wire in &module.wires {
            let start = module.pin_position(wire.start.as_ref().unwrap()).unwrap();
            let end = module.pin_position(wire.end.as_ref().unwrap()).unwrap();
            assert_eq!(wire.points[0], start);
            assert_eq!(*wire.points.last().unwrap(), end);
            let length = wire
                .points
                .windows(2)
                .map(|p| (p[1].x - p[0].x).abs() + (p[1].y - p[0].y).abs())
                .sum::<f32>();
            let direct = (start.x - end.x).abs() + (start.y - end.y).abs();
            assert!(
                length <= direct + 120.0,
                "{}: length {length}, direct {direct}",
                wire.id.0
            );
        }
        editor.undo();
        assert_eq!(editor.circuit(), &before);
    }
}

#[cfg(test)]
mod port_layout_tests {
    use super::*;
    use crate::Hit;
    use rgate_core::PinRef;
    #[test]
    fn input_ports_logic_and_output_are_separate_layers() {
        let mut editor = Editor::new(Circuit::default()).unwrap();
        let and = editor
            .place(GateKind::And, Point::new(100.0, 100.0))
            .unwrap();
        let a = editor
            .place(GateKind::InputPort, Point::new(10.0, 50.0))
            .unwrap();
        let b = editor
            .place(GateKind::InputPort, Point::new(10.0, 150.0))
            .unwrap();
        let y = editor
            .place(GateKind::OutputPort, Point::new(220.0, 100.0))
            .unwrap();
        for (marker, pin) in [(a, "I0"), (b, "I1"), (y, "Z")] {
            editor.start_wire(Hit::Pin(PinRef::new(and, pin))).unwrap();
            editor
                .finish_wire(Hit::Pin(PinRef::new(marker, "P")))
                .unwrap();
        }
        editor.selection.clear();
        editor.tidy_layout().unwrap();
        let module = editor.module();
        assert!(module.gate(a).unwrap().bounds().max.x < module.gate(and).unwrap().bounds().min.x);
        assert!(module.gate(b).unwrap().bounds().max.x < module.gate(and).unwrap().bounds().min.x);
        assert!(module.gate(y).unwrap().bounds().min.x > module.gate(and).unwrap().bounds().max.x);
        assert_eq!(crate::routing::crossing_count(module), 0);
    }
}
