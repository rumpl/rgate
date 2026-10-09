use crate::{Circuit, Direction, Gate, GateId, GateKind, Module, NetId, Pin, Point};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug)]
pub struct HierarchyRow {
    pub path: String,
    pub module: String,
    pub label: String,
    pub depth: usize,
    pub has_children: bool,
}

#[derive(Clone, Debug)]
pub struct InstanceScope {
    pub module: String,
    pub nets: HashMap<NetId, NetId>,
    pub gates: HashMap<GateId, GateId>,
}

#[derive(Clone, Debug)]
pub struct FlattenedCircuit {
    pub module: Module,
    pub scopes: HashMap<String, InstanceScope>,
}

impl Circuit {
    pub fn validate_hierarchy(&self) -> Result<(), String> {
        fn visit(
            circuit: &Circuit,
            name: &str,
            stack: &mut Vec<String>,
            done: &mut HashSet<String>,
        ) -> Result<(), String> {
            if stack.iter().any(|ancestor| ancestor == name) {
                return Err(format!(
                    "recursive instances: {} → {name}",
                    stack.join(" → ")
                ));
            }
            if done.contains(name) {
                return Ok(());
            }
            if stack.len() >= 64 {
                return Err("module nesting exceeds 64 levels".into());
            }
            let module = circuit
                .module(name)
                .ok_or_else(|| format!("missing module {name}"))?;
            stack.push(name.into());
            for gate in &module.gates {
                if let GateKind::Module(child) = &gate.kind {
                    let definition = circuit.module(child).ok_or_else(|| {
                        format!(
                            "{}.{} references missing module {child}",
                            module.name, gate.name
                        )
                    })?;
                    for pin in &gate.pins {
                        let port = definition
                            .nets
                            .iter()
                            .find(|net| net.name == pin.name && net.port.is_some())
                            .ok_or_else(|| {
                                format!(
                                    "{}.{}: missing port {} in {child}",
                                    module.name, gate.name, pin.name
                                )
                            })?;
                        if pin.direction != port.port.unwrap() {
                            return Err(format!(
                                "{}.{}: direction of port {} changed",
                                module.name, gate.name, pin.name
                            ));
                        }
                        if pin.width.is_some_and(|width| width != port.width) {
                            return Err(format!(
                                "{}.{}: width of port {} changed",
                                module.name, gate.name, pin.name
                            ));
                        }
                        if let Some(net) = pin.net.and_then(|id| module.net(id))
                            && net.width != port.width
                        {
                            return Err(format!(
                                "{}.{}: port {} requires {} bits, net {} has {}",
                                module.name, gate.name, pin.name, port.width, net.name, net.width
                            ));
                        }
                    }
                    visit(circuit, child, stack, done)?;
                }
            }
            stack.pop();
            done.insert(name.into());
            Ok(())
        }
        let mut done = HashSet::new();
        for module in &self.modules {
            visit(self, &module.name, &mut Vec::new(), &mut done)?;
        }
        Ok(())
    }

    pub fn module_instance(&self, name: &str, id: GateId, position: Point) -> Result<Gate, String> {
        let definition = self
            .module(name)
            .ok_or_else(|| format!("unknown module {name}"))?;
        let mut gate = Gate::new(id, GateKind::Module(name.into()), position);
        gate.config.custom_symbol = definition.symbol.clone();
        let mut left = 0;
        let mut right = 0;
        for net in definition.nets.iter().filter(|net| net.port.is_some()) {
            let direction = net.port.unwrap();
            let index = if direction == Direction::Output {
                let index = right;
                right += 1;
                index
            } else {
                let index = left;
                left += 1;
                index
            };
            let mut pin = Pin::new(
                &net.name,
                direction,
                Point::new(
                    if direction == Direction::Output {
                        50.0
                    } else {
                        -50.0
                    },
                    index as f32 * 20.0,
                ),
            );
            pin.width = Some(net.width);
            gate.pins.push(pin);
        }
        for pin in &mut gate.pins {
            let count = if pin.direction == Direction::Output {
                right
            } else {
                left
            };
            pin.offset.y -= (count as f32 - 1.0) * 10.0;
        }
        Ok(gate)
    }

    pub fn hierarchy_rows(&self, collapsed: &HashSet<String>) -> Vec<HierarchyRow> {
        fn walk(
            circuit: &Circuit,
            module: &str,
            path: String,
            label: String,
            depth: usize,
            collapsed: &HashSet<String>,
            rows: &mut Vec<HierarchyRow>,
        ) {
            if rows.len() >= 4096 || depth >= 64 {
                return;
            }
            let Some(definition) = circuit.module(module) else {
                return;
            };
            let children = definition
                .gates
                .iter()
                .filter_map(|gate| match &gate.kind {
                    GateKind::Module(child) => Some((gate, child)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            rows.push(HierarchyRow {
                path: path.clone(),
                module: module.into(),
                label,
                depth,
                has_children: !children.is_empty(),
            });
            if collapsed.contains(&path) {
                return;
            }
            for (gate, child) in children {
                walk(
                    circuit,
                    child,
                    format!("{path}/{}", gate.name),
                    format!("{} : {child}", gate.name),
                    depth + 1,
                    collapsed,
                    rows,
                );
            }
        }
        let mut rows = Vec::new();
        walk(
            self,
            &self.root,
            self.root.clone(),
            self.root.clone(),
            0,
            collapsed,
            &mut rows,
        );
        let referenced = self
            .modules
            .iter()
            .flat_map(|module| module.gates.iter())
            .filter_map(|gate| match &gate.kind {
                GateKind::Module(name) => Some(name.as_str()),
                _ => None,
            })
            .collect::<HashSet<_>>();
        for module in &self.modules {
            if module.name != self.root && !referenced.contains(module.name.as_str()) {
                walk(
                    self,
                    &module.name,
                    format!("unused/{}", module.name),
                    format!("{} (unused)", module.name),
                    0,
                    collapsed,
                    &mut rows,
                );
            }
        }
        rows
    }

    /// Flatten instances into primitive gates. Top-level gate/net IDs remain unchanged,
    /// so editor selection, switch controls, probes and wire colors keep working.
    pub fn flatten_module(&self, name: &str) -> Result<Module, String> {
        Ok(self.flatten_with_scopes(name)?.module)
    }

    pub fn flatten_with_scopes(&self, name: &str) -> Result<FlattenedCircuit, String> {
        self.validate().map_err(|error| error.to_string())?;
        let mut flat = self
            .module(name)
            .ok_or_else(|| format!("unknown module {name}"))?
            .clone();
        let mut scopes = HashMap::new();
        scopes.insert(
            name.to_owned(),
            InstanceScope {
                module: name.into(),
                nets: flat.nets.iter().map(|net| (net.id, net.id)).collect(),
                gates: flat
                    .gates
                    .iter()
                    .filter(|gate| !matches!(gate.kind, GateKind::Module(_)))
                    .map(|gate| (gate.id, gate.id))
                    .collect(),
            },
        );
        let mut ids = (flat.next_net_id().0, flat.next_gate_id().0);
        let top = flat.gates.clone();
        flat.gates
            .retain(|gate| !matches!(gate.kind, GateKind::Module(_)));
        fn expand(
            circuit: &Circuit,
            instance: &Gate,
            parent_map: &HashMap<NetId, NetId>,
            path: &str,
            scopes: &mut HashMap<String, InstanceScope>,
            flat: &mut Module,
            ids: &mut (u64, u64),
        ) -> Result<(), String> {
            let GateKind::Module(name) = &instance.kind else {
                return Ok(());
            };
            let definition = circuit
                .module(name)
                .ok_or_else(|| format!("missing module {name}"))?;
            let mut map = HashMap::new();
            for net in &definition.nets {
                let external = net
                    .port
                    .and_then(|_| instance.pin(&net.name))
                    .and_then(|pin| pin.net)
                    .and_then(|net| parent_map.get(&net).copied());
                let id = if let Some(id) = external {
                    id
                } else {
                    let id = NetId(ids.0);
                    ids.0 += 1;
                    let mut net = net.clone();
                    net.id = id;
                    net.name = format!("{path}/{}", net.name);
                    net.port = None;
                    flat.nets.push(net);
                    id
                };
                map.insert(net.id, id);
            }
            scopes.insert(
                path.into(),
                InstanceScope {
                    module: name.clone(),
                    nets: map.clone(),
                    gates: HashMap::new(),
                },
            );
            for gate in &definition.gates {
                if flat.gates.len() + flat.nets.len() > 100_000 {
                    return Err("expanded hierarchy exceeds 100000 elements".into());
                }
                let child_path = format!("{path}/{}", gate.name);
                if matches!(gate.kind, GateKind::Module(_)) {
                    expand(circuit, gate, &map, &child_path, scopes, flat, ids)?;
                } else {
                    let original_id = gate.id;
                    let mut gate = gate.clone();
                    gate.id = GateId(ids.1);
                    ids.1 += 1;
                    scopes
                        .get_mut(path)
                        .unwrap()
                        .gates
                        .insert(original_id, gate.id);
                    gate.name = child_path
                        .split_once('/')
                        .map_or(child_path.clone(), |(_, path)| path.to_owned());
                    for pin in &mut gate.pins {
                        pin.net = pin.net.and_then(|net| map.get(&net).copied());
                    }
                    flat.gates.push(gate);
                }
            }
            Ok(())
        }
        let map = flat.nets.iter().map(|net| (net.id, net.id)).collect();
        for gate in top
            .iter()
            .filter(|gate| matches!(gate.kind, GateKind::Module(_)))
        {
            expand(
                self,
                gate,
                &map,
                &format!("{name}/{}", gate.name),
                &mut scopes,
                &mut flat,
                &mut ids,
            )?;
        }
        // Geometry stays in the source document, not the flattened simulation graph.
        flat.wires.clear();
        flat.validate().map_err(|error| error.to_string())?;
        Ok(FlattenedCircuit {
            module: flat,
            scopes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo;

    #[test]
    fn hierarchy_is_path_specific_and_expandable() {
        let circuit = demo::hierarchical_inverters();
        circuit.validate().unwrap();
        let rows = circuit.hierarchy_rows(&HashSet::new());
        assert_eq!(rows.len(), 5);
        assert!(
            rows.iter()
                .any(|row| row.path == "main/u0/inner" && row.depth == 2)
        );
        let collapsed = HashSet::from(["main/u0".into()]);
        let rows = circuit.hierarchy_rows(&collapsed);
        assert_eq!(rows.len(), 4);
        assert!(!rows.iter().any(|row| row.path == "main/u0/inner"));
        assert!(rows.iter().any(|row| row.path == "main/u1/inner"));
    }

    #[test]
    fn recursion_and_missing_definitions_are_rejected() {
        let mut circuit = demo::hierarchical_inverters();
        let recursive = circuit
            .module_instance("main", GateId(9), Point::ZERO)
            .unwrap();
        circuit
            .module_mut("inverter")
            .unwrap()
            .gates
            .push(recursive);
        assert!(
            circuit
                .validate()
                .unwrap_err()
                .to_string()
                .contains("recursive")
        );
        circuit.module_mut("inverter").unwrap().gates.pop();
        circuit.module_mut("main").unwrap().gates[1].kind = GateKind::Module("missing".into());
        assert!(
            circuit
                .validate()
                .unwrap_err()
                .to_string()
                .contains("missing")
        );
    }

    #[test]
    fn mixed_width_ports_are_retained_and_flattening_keeps_root_ids() {
        let mut circuit = demo::hierarchical_inverters();
        let module = circuit.module_mut("inverter").unwrap();
        let mut bus = crate::Net::new(NetId(3), "bus", 8);
        bus.port = Some(Direction::Input);
        module.nets.push(bus);
        let instance = circuit
            .module_instance("inverter", GateId(10), Point::ZERO)
            .unwrap();
        assert_eq!(instance.pin_width("A"), 1);
        assert_eq!(instance.pin_width("bus"), 8);
        let flattened = circuit.flatten_module("main").unwrap();
        assert_eq!(flattened.net(NetId(4)).unwrap().name, "output1");
        assert!(flattened.gate(GateId(1)).is_some());
        assert!(
            flattened
                .gates
                .iter()
                .any(|gate| gate.name == "u0/inner/inv")
        );
        assert!(
            flattened
                .gates
                .iter()
                .all(|gate| !matches!(gate.kind, GateKind::Module(_)))
        );
    }
}
