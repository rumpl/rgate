use crate::{Logic, Point, Rect, Signal};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use thiserror::Error;

macro_rules! identifier {
    ($name:ident) => {
        #[derive(
            Clone,
            Copy,
            Debug,
            Default,
            Eq,
            PartialEq,
            Ord,
            PartialOrd,
            Hash,
            Serialize,
            Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub u64);
    };
}

identifier!(GateId);
identifier!(NetId);
identifier!(WireId);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Input,
    Output,
    InOut,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateKind {
    And,
    Nand,
    Or,
    Nor,
    Xor,
    Xnor,
    Buffer,
    Not,
    TriState,
    ReduceAnd,
    ReduceNand,
    ReduceOr,
    ReduceNor,
    ReduceXor,
    ReduceXnor,
    Frame,
    Switch,
    Dip,
    Led,
    Clock,
    Ground,
    Vdd,
    Dff,
    Register,
    Mux,
    Add,
    Jkff,
    Decoder,
    Demux,
    Multiply,
    Divide,
    ShiftLeft,
    ShiftRight,
    ArithmeticShiftRight,
    RotateLeft,
    RotateRight,
    Concat,
    Splitter,
    Tap,
    Ram,
    Rom,
    Nmos,
    Pmos,
    Tty,
    Peripheral,
    Vga,
    Comment,
    Module(String),
    Unsupported(String),
}

impl GateKind {
    pub fn name(&self) -> &str {
        match self {
            Self::And => "AND",
            Self::Nand => "NAND",
            Self::Or => "OR",
            Self::Nor => "NOR",
            Self::Xor => "XOR",
            Self::Xnor => "XNOR",
            Self::Buffer => "BUF",
            Self::Not => "NOT",
            Self::TriState => "TRI-STATE",
            Self::ReduceAnd => "REDUCE AND",
            Self::ReduceNand => "REDUCE NAND",
            Self::ReduceOr => "REDUCE OR",
            Self::ReduceNor => "REDUCE NOR",
            Self::ReduceXor => "REDUCE XOR",
            Self::ReduceXnor => "REDUCE XNOR",
            Self::Frame => "FRAME",
            Self::Switch => "SWITCH",
            Self::Dip => "DIP",
            Self::Led => "LED",
            Self::Clock => "CLOCK",
            Self::Ground => "GROUND",
            Self::Vdd => "VDD",
            Self::Dff => "D FLIP-FLOP",
            Self::Register => "REGISTER",
            Self::Mux => "MUX",
            Self::Add => "ADD",
            Self::Jkff => "JK FLIP-FLOP",
            Self::Decoder => "DECODER",
            Self::Demux => "DEMUX",
            Self::Multiply => "MUL",
            Self::Divide => "DIV",
            Self::ShiftLeft => "LSHIFT",
            Self::ShiftRight => "RSHIFT",
            Self::ArithmeticShiftRight => "ARSHIFT",
            Self::RotateLeft => "ROL",
            Self::RotateRight => "ROR",
            Self::Concat => "CONCAT",
            Self::Splitter => "SPLITTER",
            Self::Tap => "TAP",
            Self::Ram => "RAM",
            Self::Rom => "ROM",
            Self::Nmos => "NMOS",
            Self::Pmos => "PMOS",
            Self::Tty => "TTY",
            Self::Peripheral => "GPIO",
            Self::Vga => "VGA DISPLAY",
            Self::Comment => "COMMENT",
            Self::Module(name) | Self::Unsupported(name) => name,
        }
    }

    pub fn is_source(&self) -> bool {
        matches!(
            self,
            Self::Switch | Self::Dip | Self::Clock | Self::Ground | Self::Vdd | Self::Peripheral
        )
    }

    pub fn is_logic(&self) -> bool {
        matches!(
            self,
            Self::And
                | Self::Nand
                | Self::Or
                | Self::Nor
                | Self::Xor
                | Self::Xnor
                | Self::ReduceAnd
                | Self::ReduceNand
                | Self::ReduceOr
                | Self::ReduceNor
                | Self::ReduceXor
                | Self::ReduceXnor
        )
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pin {
    pub name: String,
    pub direction: Direction,
    pub offset: Point,
    pub net: Option<NetId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u16>,
}

impl Pin {
    pub fn new(name: impl Into<String>, direction: Direction, offset: Point) -> Self {
        Self {
            name: name.into(),
            direction,
            offset,
            net: None,
            width: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Gate {
    pub id: GateId,
    pub name: String,
    pub kind: GateKind,
    pub position: Point,
    pub rotation: u8,
    pub width: u16,
    pub input_count: u8,
    pub delay: u64,
    pub period: u64,
    pub initial: Signal,
    pub pins: Vec<Pin>,
    pub text: String,
    pub show_name: bool,
    #[serde(default)]
    pub config: crate::ComponentConfig,
}

impl Gate {
    pub fn new(id: GateId, kind: GateKind, position: Point) -> Self {
        let width = if kind.default_bus_width() > 1 {
            kind.default_bus_width()
        } else {
            1
        };
        let mut gate = Self {
            id,
            name: format!("g{}", id.0),
            kind: kind.clone(),
            position,
            rotation: 0,
            width,
            input_count: 2,
            delay: 4,
            period: 100,
            initial: Signal::from_u64(0, width),
            pins: Vec::new(),
            text: String::new(),
            show_name: true,
            config: crate::ComponentConfig::for_kind(&kind),
        };
        gate.reset_pins();
        gate
    }

    pub fn reset_pins(&mut self) {
        use Direction::{Input, Output};
        let pin = |name: &str, direction, x, y| Pin::new(name, direction, Point::new(x, y));
        self.pins = match &self.kind {
            kind if kind.is_logic() => {
                let count = if self.config.reduction || self.kind.is_reduction() {
                    1
                } else {
                    self.input_count.max(1)
                };
                let mut pins = (0..count)
                    .map(|index| {
                        let y = (f32::from(index) - (f32::from(count) - 1.0) / 2.0) * 5.0;
                        pin(&format!("I{index}"), Input, -11.0, y)
                    })
                    .collect::<Vec<_>>();
                let mut output = pin("Z", Output, 10.0, 0.0);
                if self.config.reduction || self.kind.is_reduction() {
                    output.width = Some(1);
                }
                pins.push(output);
                pins
            }
            GateKind::Buffer | GateKind::Not => {
                vec![pin("I", Input, -6.0, 0.0), pin("Z", Output, 10.0, 0.0)]
            }
            GateKind::TriState => vec![
                pin("I", Input, -6.0, 0.0),
                pin("E", Input, 2.0, -5.0),
                pin("Z", Output, 10.0, 0.0),
            ],
            GateKind::Switch => vec![pin("Z", Output, 17.0, 0.0)],
            GateKind::Dip => vec![pin("Z", Output, 0.0, 10.0)],
            GateKind::Clock => vec![pin("Z", Output, 13.0, 0.0)],
            GateKind::Ground => vec![pin("Z", Output, 0.0, -6.0)],
            GateKind::Vdd => vec![pin("Z", Output, -11.0, 0.0)],
            GateKind::Led => vec![pin("I", Input, 0.0, 7.0)],
            GateKind::Dff => vec![
                pin("Q", Output, 16.0, -5.0),
                pin("_Q", Output, 16.0, 5.0),
                pin("D", Input, -16.0, 0.0),
                pin("EN", Input, 5.0, -16.0),
                pin("CLR", Input, -5.0, -16.0),
                pin("CK", Input, 0.0, 16.0),
            ],
            GateKind::Register => vec![
                pin("Q", Output, 0.0, 11.0),
                pin("D", Input, 0.0, -10.0),
                pin("EN", Input, 39.0, 5.0),
                pin("CLR", Input, 39.0, -5.0),
                pin("CK", Input, -37.0, 0.0),
            ],
            GateKind::Mux => {
                let count = self.input_count.max(2);
                let mut pins = (0..count)
                    .map(|index| {
                        let x = -29.0 + 58.0 * (f32::from(index) + 1.0) / (f32::from(count) + 1.0);
                        pin(&format!("I{index}"), Input, x, -16.0)
                    })
                    .collect::<Vec<_>>();
                pins.push(pin("S", Input, -23.0, 0.0));
                pins.push(pin("Z", Output, 0.0, 13.0));
                pins
            }
            GateKind::Add => vec![
                pin("A", Input, -16.0, -16.0),
                pin("B", Input, 16.0, -16.0),
                pin("S", Output, 0.0, 13.0),
                pin("CI", Input, 24.0, -2.0),
                pin("CO", Output, -24.0, -2.0),
            ],
            _ => self.component_pins(),
        };
    }

    pub fn pin(&self, name: &str) -> Option<&Pin> {
        self.pins.iter().find(|pin| pin.name == name)
    }

    pub fn pin_mut(&mut self, name: &str) -> Option<&mut Pin> {
        self.pins.iter_mut().find(|pin| pin.name == name)
    }

    pub fn pin_position(&self, pin: &Pin) -> Point {
        let offsets = match self.kind {
            GateKind::Switch if pin.offset == Point::new(17.0, 0.0) => Some([
                Point::new(17.0, 0.0),
                Point::new(0.0, -14.0),
                Point::new(-18.0, 0.0),
                Point::new(0.0, 13.0),
            ]),
            GateKind::Dip if pin.offset == Point::new(0.0, 10.0) => Some([
                Point::new(0.0, 10.0),
                Point::new(38.0, 0.0),
                Point::new(0.0, -11.0),
                Point::new(-38.0, 0.0),
            ]),
            _ => None,
        };
        self.position
            + offsets
                .map(|offsets| offsets[usize::from(self.rotation % 4)])
                .unwrap_or_else(|| pin.offset.rotated(self.rotation))
    }

    pub fn pin_width(&self, name: &str) -> u16 {
        if let Some(width) = self.pin(name).and_then(|pin| pin.width) {
            return width;
        }
        if matches!(name, "EN" | "CLR" | "CK" | "E" | "CI" | "CO") {
            1
        } else if self.kind == GateKind::Mux && name == "S" {
            (u8::BITS - (self.input_count.max(2) - 1).leading_zeros()) as u16
        } else {
            self.width
        }
    }

    pub fn is_compact_bus_join(&self) -> bool {
        self.kind == GateKind::Concat && self.pins.iter().all(|pin| pin.offset.x.abs() <= 6.0)
    }

    pub fn bounds(&self) -> Rect {
        if self.kind == GateKind::Comment {
            let bounds = crate::rich_comment(&self.text)
                .iter()
                .map(|run| run.bounds())
                .reduce(Rect::union)
                .unwrap_or(Rect::around(Point::ZERO, 1.0, 15.0));
            return Rect::from_points(self.position + bounds.min, self.position + bounds.max);
        }
        if !self.config.custom_symbol.is_empty() {
            let local = self
                .config
                .custom_symbol
                .iter()
                .map(|shape| shape.bounds())
                .reduce(Rect::union)
                .unwrap();
            let corners = [
                local.min,
                local.max,
                Point::new(local.min.x, local.max.y),
                Point::new(local.max.x, local.min.y),
            ];
            let mut points = corners
                .into_iter()
                .map(|point| self.position + point.rotated(self.rotation))
                .collect::<Vec<_>>();
            points.extend(self.pins.iter().map(|pin| self.pin_position(pin)));
            return points
                .into_iter()
                .map(|point| Rect::around(point, 1.0, 1.0))
                .reduce(Rect::union)
                .unwrap();
        }
        if self.is_compact_bus_join() {
            let height = self
                .pins
                .iter()
                .map(|pin| pin.offset.y.abs() * 2.0 + 4.0)
                .fold(8.0, f32::max);
            let center = self.position + Point::new(-2.0, 0.0).rotated(self.rotation);
            return if self.rotation.is_multiple_of(2) {
                Rect::around(center, 6.0, height)
            } else {
                Rect::around(center, height, 6.0)
            };
        }
        if self.kind == GateKind::Led && self.config.led_display != crate::LedDisplay::Bit {
            let size = self.config.led_display.size(self.width);
            let center = self.position + Point::new(0.0, 7.0 - size.y / 2.0).rotated(self.rotation);
            return if self.rotation.is_multiple_of(2) {
                Rect::around(center, size.x, size.y)
            } else {
                Rect::around(center, size.y, size.x)
            };
        }
        let (width, height) = match &self.kind {
            GateKind::Switch => (34.0, 26.0),
            GateKind::Dip => (76.0, 22.0),
            GateKind::Led => {
                let size = self.config.led_display.size(self.width);
                (size.x, size.y)
            }
            GateKind::Dff => (32.0, 32.0),
            GateKind::Register => (76.0, 22.0),
            GateKind::Mux => (60.0, 30.0),
            GateKind::Add => (60.0, 30.0),
            GateKind::Frame => (self.config.frame_width, self.config.frame_height),
            GateKind::Clock => (28.0, 26.0),
            GateKind::Ground => (12.0, 12.0),
            GateKind::Vdd => (36.0, 10.0),
            GateKind::Buffer | GateKind::Not | GateKind::TriState => (20.0, 16.0),
            GateKind::Comment => (
                self.text
                    .lines()
                    .map(|line| line.chars().count())
                    .max()
                    .unwrap_or(1) as f32
                    * 6.4,
                self.text.lines().count().max(1) as f32 * 15.0,
            ),
            GateKind::Module(_) => (
                self.config.module_width,
                self.pins
                    .iter()
                    .map(|pin| pin.offset.y.abs() * 2.0 + 24.0)
                    .fold(44.0, f32::max),
            ),
            GateKind::Unsupported(_) => (70.0, 44.0),
            kind if kind.is_extended() => (
                90.0,
                self.pins
                    .iter()
                    .map(|pin| pin.offset.y.abs() * 2.0 + 20.0)
                    .fold(44.0, f32::max),
            ),
            _ => (26.0, 16.0_f32.max(f32::from(self.input_count) * 5.0)),
        };
        if self.kind == GateKind::Comment {
            return Rect::from_points(self.position, self.position + Point::new(width, height));
        }
        if self.rotation.is_multiple_of(2) || matches!(self.kind, GateKind::Switch | GateKind::Dip)
        {
            Rect::around(self.position, width, height)
        } else {
            Rect::around(self.position, height, width)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Net {
    pub id: NetId,
    pub name: String,
    pub width: u16,
    pub show_name: bool,
    pub port: Option<Direction>,
}

impl Net {
    pub fn new(id: NetId, name: impl Into<String>, width: u16) -> Self {
        Self {
            id,
            name: name.into(),
            width,
            show_name: true,
            port: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct PinRef {
    pub gate: GateId,
    pub pin: String,
}

impl PinRef {
    pub fn new(gate: GateId, pin: impl Into<String>) -> Self {
        Self {
            gate,
            pin: pin.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Wire {
    pub id: WireId,
    pub net: NetId,
    pub points: Vec<Point>,
    pub start: Option<PinRef>,
    pub end: Option<PinRef>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Module {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verilog: Option<String>,
    pub gates: Vec<Gate>,
    pub nets: Vec<Net>,
    pub wires: Vec<Wire>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub symbol: Vec<crate::SymbolPrimitive>,
}

impl Module {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            verilog: None,
            gates: Vec::new(),
            nets: Vec::new(),
            wires: Vec::new(),
            symbol: Vec::new(),
        }
    }

    pub fn gate(&self, id: GateId) -> Option<&Gate> {
        self.gates.iter().find(|gate| gate.id == id)
    }

    pub fn gate_mut(&mut self, id: GateId) -> Option<&mut Gate> {
        self.gates.iter_mut().find(|gate| gate.id == id)
    }

    pub fn net(&self, id: NetId) -> Option<&Net> {
        self.nets.iter().find(|net| net.id == id)
    }

    pub fn pin(&self, reference: &PinRef) -> Option<&Pin> {
        self.gate(reference.gate)?.pin(&reference.pin)
    }

    pub fn pin_position(&self, reference: &PinRef) -> Option<Point> {
        let gate = self.gate(reference.gate)?;
        Some(gate.pin_position(gate.pin(&reference.pin)?))
    }

    pub fn bounds(&self) -> Option<Rect> {
        let gates = self.gates.iter().map(|gate| gate.bounds().expanded(14.0));
        let points = self
            .wires
            .iter()
            .flat_map(|wire| wire.points.iter())
            .map(|point| Rect::around(*point, 1.0, 1.0));
        gates.chain(points).reduce(Rect::union)
    }

    pub fn next_gate_id(&self) -> GateId {
        GateId(self.gates.iter().map(|gate| gate.id.0).max().unwrap_or(0) + 1)
    }

    pub fn next_net_id(&self) -> NetId {
        NetId(self.nets.iter().map(|net| net.id.0).max().unwrap_or(0) + 1)
    }

    pub fn next_wire_id(&self) -> WireId {
        WireId(self.wires.iter().map(|wire| wire.id.0).max().unwrap_or(0) + 1)
    }

    pub fn validate(&self) -> Result<(), ValidationError> {
        if self
            .verilog
            .as_ref()
            .is_some_and(|source| source.len() > 1_000_000)
            || (self.verilog.is_some() && (!self.gates.is_empty() || !self.wires.is_empty()))
        {
            return Err(ValidationError::Component(
                "Verilog modules cannot contain schematic gates/wires or exceed 1 MB of source"
                    .into(),
            ));
        }
        if self.symbol.len() > 1024 || self.symbol.iter().any(|shape| !shape.is_valid()) {
            return Err(ValidationError::Component(format!(
                "{} custom symbol has invalid shapes or exceeds 1024 shapes",
                self.name
            )));
        }

        let mut gate_ids = HashSet::new();
        let mut gate_names = HashSet::new();
        let mut net_ids = HashSet::new();
        let mut net_names = HashSet::new();
        let mut wire_ids = HashSet::new();
        for net in &self.nets {
            if !net_ids.insert(net.id) || !net_names.insert(&net.name) || net.name.is_empty() {
                return Err(ValidationError::Duplicate(format!("net {}", net.name)));
            }
            if !(1..=4096).contains(&net.width) {
                return Err(ValidationError::Width(net.name.clone()));
            }
        }
        for gate in &self.gates {
            if !gate_ids.insert(gate.id) || !gate_names.insert(&gate.name) || gate.name.is_empty() {
                return Err(ValidationError::Duplicate(format!("gate {}", gate.name)));
            }
            if !(1..=4096).contains(&gate.width)
                || !gate.initial.is_valid()
                || !(1..=64).contains(&gate.input_count)
            {
                return Err(ValidationError::Width(gate.name.clone()));
            }
            gate.validate_component()
                .map_err(ValidationError::Component)?;
            if !gate.position.is_finite() || gate.pins.iter().any(|pin| !pin.offset.is_finite()) {
                return Err(ValidationError::Geometry(gate.name.clone()));
            }
            if gate.kind == GateKind::Clock && gate.period < 2 {
                return Err(ValidationError::Clock(gate.name.clone()));
            }
            let mut pin_names = HashSet::new();
            for pin in &gate.pins {
                if pin.width.is_some_and(|width| !(1..=4096).contains(&width)) {
                    return Err(ValidationError::Width(format!(
                        "{}.{}",
                        gate.name, pin.name
                    )));
                }
                if !pin_names.insert(&pin.name) {
                    return Err(ValidationError::Duplicate(format!(
                        "pin {}.{}",
                        gate.name, pin.name
                    )));
                }
                if let Some(net) = pin.net
                    && !net_ids.contains(&net)
                {
                    return Err(ValidationError::MissingNet(net));
                }
            }
        }
        for wire in &self.wires {
            if !wire_ids.insert(wire.id) {
                return Err(ValidationError::Duplicate(format!("wire {}", wire.id.0)));
            }
            if !net_ids.contains(&wire.net) {
                return Err(ValidationError::MissingNet(wire.net));
            }
            if wire.points.len() < 2 || wire.points.iter().any(|point| !point.is_finite()) {
                return Err(ValidationError::Geometry(format!("wire {}", wire.id.0)));
            }
            for reference in [&wire.start, &wire.end].into_iter().flatten() {
                if self
                    .pin(reference)
                    .is_none_or(|pin| pin.net != Some(wire.net))
                {
                    return Err(ValidationError::Endpoint(wire.id));
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Circuit {
    pub title: String,
    pub root: String,
    pub modules: Vec<Module>,
}

impl Default for Circuit {
    fn default() -> Self {
        Self {
            title: "untitled".into(),
            root: "main".into(),
            modules: vec![Module::new("main")],
        }
    }
}

impl Circuit {
    pub fn module(&self, name: &str) -> Option<&Module> {
        self.modules.iter().find(|module| module.name == name)
    }

    pub fn module_mut(&mut self, name: &str) -> Option<&mut Module> {
        self.modules.iter_mut().find(|module| module.name == name)
    }

    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.module(&self.root).is_none() {
            return Err(ValidationError::MissingRoot(self.root.clone()));
        }
        let mut names = HashSet::new();
        for module in &self.modules {
            if !names.insert(&module.name) || module.name.is_empty() {
                return Err(ValidationError::Duplicate(format!(
                    "module {}",
                    module.name
                )));
            }
            module.validate()?;
        }
        self.validate_hierarchy()
            .map_err(ValidationError::Hierarchy)?;
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("invalid component configuration: {0}")]
    Component(String),
    #[error("invalid module hierarchy: {0}")]
    Hierarchy(String),
    #[error("duplicate or empty identifier: {0}")]
    Duplicate(String),
    #[error("invalid bit width in {0}; supported widths are 1–64")]
    Width(String),
    #[error("invalid coordinates or empty geometry in {0}")]
    Geometry(String),
    #[error("missing net {}", .0.0)]
    MissingNet(NetId),
    #[error("invalid pin endpoint on wire {}", .0.0)]
    Endpoint(WireId),
    #[error("clock {0} must have a period of at least 2 ns")]
    Clock(String),
    #[error("root module {0} does not exist")]
    MissingRoot(String),
}

pub fn unconnected_signal(width: u16) -> Signal {
    Signal::filled(width, Logic::HighZ)
}
