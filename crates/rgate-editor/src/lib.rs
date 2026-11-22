mod edit;
mod organization;
mod routing;
mod selection;
mod wiring;

#[cfg(test)]
mod tests;

pub use organization::{Align, Distribute, SearchResult, SearchTarget, search};
use rgate_core::{Circuit, GateId, GateKind, Module, NetId, PinRef, Point, Rect, WireId};
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

    pub fn edit_module(&mut self, edit: impl FnOnce(&mut Module)) -> Result<(), EditError> {
        self.finish_gesture();
        let before = self.circuit.clone();
        edit(self.circuit.module_mut(&self.active).unwrap());
        {
            let module = self.circuit.module_mut(&self.active).unwrap();
            for gate in &mut module.gates {
                if let Some(direction) = gate.kind.port_direction()
                    && let Some(id) = gate.pin("P").and_then(|p| p.net)
                    && let Some(net) = module.nets.iter_mut().find(|n| n.id == id)
                {
                    let old_gate = before.module(&self.active).and_then(|m| m.gate(gate.id));
                    let old_net = before.module(&self.active).and_then(|m| m.net(id));
                    if old_gate.is_some_and(|old| old.name == gate.name)
                        && old_net.is_some_and(|old| old.name != net.name)
                    {
                        gate.name = net.name.clone();
                    } else {
                        net.name = gate.name.clone();
                    }
                    net.width = gate.width;
                    net.port = Some(direction);
                }
            }
        }
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

    pub fn begin_gesture(&mut self) {
        self.finish_gesture();
        self.gesture = Some(self.circuit.clone());
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

    fn record(&mut self, before: Circuit) {
        self.undo.push(before);
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }
}
