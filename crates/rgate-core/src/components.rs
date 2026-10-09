use crate::{Direction, Gate, GateKind, Pin, Point, Signal};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ComponentConfig {
    /// I0/Z0 is the least-significant partition.
    pub partitions: Vec<u16>,
    pub led_display: crate::LedDisplay,
    pub tap_offset: u16,
    pub tap_width: u16,
    pub address_bits: u8,
    pub vga_width: u16,
    pub vga_height: u16,
    pub tty_tkgate: bool,
    pub reduction: bool,
    pub enable_low: bool,
    pub invert_output: bool,
    pub clock_phase: u64,
    pub clock_duty: u8,
    pub operand_a_width: Option<u16>,
    pub operand_b_width: Option<u16>,
    pub frame_width: f32,
    pub frame_height: f32,
    pub module_width: f32,
    pub custom_symbol: Vec<crate::SymbolPrimitive>,
    pub custom_ports: std::collections::BTreeMap<String, Point>,
    /// Sparse words at high addresses, avoiding multi-gigabyte dense memory images.
    pub sparse_memory: std::collections::BTreeMap<u32, Signal>,
    /// Sparse initial contents; unspecified memory words start unknown.
    pub memory: Vec<Signal>,
}

impl Default for ComponentConfig {
    fn default() -> Self {
        Self {
            partitions: Vec::new(),
            led_display: crate::LedDisplay::default(),
            tap_offset: 0,
            tap_width: 1,
            address_bits: 8,
            vga_width: 32,
            vga_height: 24,
            tty_tkgate: false,
            reduction: false,
            enable_low: false,
            invert_output: false,
            clock_phase: 0,
            clock_duty: 50,
            operand_a_width: None,
            operand_b_width: None,
            frame_width: 240.0,
            frame_height: 160.0,
            module_width: 100.0,
            custom_symbol: Vec::new(),
            custom_ports: std::collections::BTreeMap::new(),
            sparse_memory: std::collections::BTreeMap::new(),
            memory: Vec::new(),
        }
    }
}

impl ComponentConfig {
    pub fn for_kind(kind: &GateKind) -> Self {
        if matches!(kind, GateKind::Concat | GateKind::Splitter) {
            Self {
                partitions: vec![4, 4],
                ..Self::default()
            }
        } else {
            Self {
                partitions: Vec::new(),
                ..Self::default()
            }
        }
    }
}

impl GateKind {
    pub fn is_reduction(&self) -> bool {
        matches!(
            self,
            Self::ReduceAnd
                | Self::ReduceNand
                | Self::ReduceOr
                | Self::ReduceNor
                | Self::ReduceXor
                | Self::ReduceXnor
        )
    }

    pub fn default_bus_width(&self) -> u16 {
        match self {
            Self::ReduceAnd
            | Self::ReduceNand
            | Self::ReduceOr
            | Self::ReduceNor
            | Self::ReduceXor
            | Self::ReduceXnor => 8,
            Self::Register
            | Self::Dip
            | Self::Add
            | Self::Multiply
            | Self::Divide
            | Self::ShiftLeft
            | Self::ShiftRight
            | Self::ArithmeticShiftRight
            | Self::RotateLeft
            | Self::RotateRight
            | Self::Concat
            | Self::Splitter
            | Self::Tap
            | Self::Ram
            | Self::Rom
            | Self::Tty
            | Self::Peripheral => 8,
            _ => 1,
        }
    }

    pub fn is_extended(&self) -> bool {
        matches!(
            self,
            Self::Vga
                | Self::Jkff
                | Self::Decoder
                | Self::Demux
                | Self::Multiply
                | Self::Divide
                | Self::ShiftLeft
                | Self::ShiftRight
                | Self::ArithmeticShiftRight
                | Self::RotateLeft
                | Self::RotateRight
                | Self::Concat
                | Self::Splitter
                | Self::Tap
                | Self::Ram
                | Self::Rom
                | Self::Nmos
                | Self::Pmos
                | Self::Tty
                | Self::Peripheral
        )
    }
}

impl Gate {
    pub fn clock_high_time(&self) -> u64 {
        ((u128::from(self.period) * u128::from(self.config.clock_duty) / 100) as u64)
            .clamp(1, self.period.saturating_sub(1).max(1))
    }
    pub fn clock_low_time(&self) -> u64 {
        self.period.saturating_sub(self.clock_high_time()).max(1)
    }

    pub fn component_pins(&self) -> Vec<Pin> {
        use Direction::{InOut, Input, Output};
        let mut specs: Vec<(String, Direction, u16)> = Vec::new();
        let mut add = |name: &str, direction, width| specs.push((name.into(), direction, width));
        let w = self.width;
        let select_width = (u8::BITS - (self.input_count.max(2) - 1).leading_zeros()) as u16;
        match self.kind {
            GateKind::Jkff => {
                for name in ["J", "K"] {
                    add(name, Input, w);
                }
                for name in ["CK", "PRE", "CLR"] {
                    add(name, Input, 1);
                }
                for name in ["Q", "_Q"] {
                    add(name, Output, w);
                }
            }
            GateKind::Decoder | GateKind::Demux => {
                if self.kind == GateKind::Demux {
                    add("F", Input, w);
                }
                add(
                    if self.kind == GateKind::Decoder {
                        "I"
                    } else {
                        "S"
                    },
                    Input,
                    select_width,
                );
                add("E", Input, 1);
                for index in 0..self.input_count {
                    add(
                        &format!("Z{index}"),
                        Output,
                        if self.kind == GateKind::Decoder { 1 } else { w },
                    );
                }
            }
            GateKind::Multiply | GateKind::Divide => {
                add("A", Input, self.config.operand_a_width.unwrap_or(w));
                add("B", Input, self.config.operand_b_width.unwrap_or(w));
                if self.kind == GateKind::Multiply {
                    add("P", Output, w);
                } else {
                    add("Q", Output, w);
                    add("R", Output, w);
                }
            }
            GateKind::ShiftLeft
            | GateKind::ShiftRight
            | GateKind::ArithmeticShiftRight
            | GateKind::RotateLeft
            | GateKind::RotateRight => {
                add("I", Input, w);
                add(
                    "S",
                    Input,
                    (u16::BITS - (w.max(2) - 1).leading_zeros()) as u16,
                );
                add("Z", Output, w);
            }
            GateKind::Concat => {
                for (index, width) in self.config.partitions.iter().enumerate() {
                    add(&format!("I{index}"), Input, *width);
                }
                add("Z", Output, w);
            }
            GateKind::Splitter => {
                add("I", Input, w);
                for (index, width) in self.config.partitions.iter().enumerate() {
                    add(&format!("Z{index}"), Output, *width);
                }
            }
            GateKind::Tap => {
                add("I", Input, w);
                add("Z", Output, self.config.tap_width);
            }
            GateKind::Ram => {
                add("A", Input, u16::from(self.config.address_bits));
                add("D", InOut, w);
                for name in ["WE", "OE", "CS"] {
                    add(name, Input, 1);
                }
            }
            GateKind::Rom => {
                add("A", Input, u16::from(self.config.address_bits));
                add("D", Output, w);
                add("OE", Input, 1);
            }
            GateKind::Nmos | GateKind::Pmos => {
                add("S", Input, w);
                add("G", Input, w);
                add("Z", Output, w);
            }
            GateKind::Vga => {
                for name in ["PCLK", "HSYNC", "VSYNC", "DE"] {
                    add(name, Input, 1);
                }
                for name in ["R", "G", "B"] {
                    add(name, Input, 4);
                }
            }
            GateKind::Tty if self.config.tty_tkgate => {
                add("RD", Input, 8);
                add("DSR", Input, 1);
                add("CTS", Input, 1);
                add("TD", Output, 8);
                add("RTS", Output, 1);
                add("DTR", Output, 1);
            }
            GateKind::Tty => {
                add("TX", Input, 8);
                add("WR", Input, 1);
                add("RX", Output, 8);
                add("RD", Input, 1);
                add("READY", Output, 1);
            }
            GateKind::Peripheral => {
                add("I", Input, w);
                add("Z", Output, w);
            }
            _ => return Vec::new(),
        }
        let left = specs
            .iter()
            .filter(|(_, direction, _)| *direction == Input)
            .count();
        let right = specs.len() - left;
        let mut l = 0;
        let mut r = 0;
        specs
            .into_iter()
            .map(|(name, direction, width)| {
                let (index, count, x) = if direction == Input {
                    let index = l;
                    l += 1;
                    (index, left, -45.0)
                } else {
                    let index = r;
                    r += 1;
                    (index, right, 45.0)
                };
                let mut pin = Pin::new(
                    name,
                    direction,
                    Point::new(x, (index as f32 - (count as f32 - 1.0) / 2.0) * 16.0),
                );
                pin.width = Some(width);
                pin
            })
            .collect()
    }

    pub fn validate_component(&self) -> Result<(), String> {
        if self.config.custom_symbol.len() > 1024
            || self
                .config
                .custom_symbol
                .iter()
                .any(|shape| !shape.is_valid())
        {
            return Err(format!("{} invalid custom symbol", self.name));
        }
        let invalid = |message: &str| Err(format!("{}: {message}", self.name));
        if matches!(self.kind, GateKind::Concat | GateKind::Splitter)
            && (self.config.partitions.is_empty()
                || self.config.partitions.len() > 64
                || self
                    .config
                    .partitions
                    .iter()
                    .any(|width| !(1..=4096).contains(width))
                || self
                    .config
                    .partitions
                    .iter()
                    .map(|width| u32::from(*width))
                    .sum::<u32>()
                    != u32::from(self.width))
        {
            return invalid("partition widths must be 1–4096 bits and sum to the bus width");
        }
        if self.kind == GateKind::Tap
            && (self.config.tap_width == 0
                || u32::from(self.config.tap_offset) + u32::from(self.config.tap_width)
                    > u32::from(self.width))
        {
            return invalid("tap slice is outside the input bus");
        }
        if matches!(self.kind, GateKind::Ram | GateKind::Rom)
            && (!(1..=32).contains(&self.config.address_bits)
                || self.config.memory.len() as u64 > (1u64 << self.config.address_bits)
                || self.config.sparse_memory.iter().any(|(address, word)| {
                    u64::from(*address) >= (1u64 << self.config.address_bits)
                        || !word.is_valid()
                        || word.width() != self.width
                })
                || self
                    .config
                    .memory
                    .iter()
                    .any(|word| !word.is_valid() || word.width() != self.width))
        {
            return invalid("memory requires 1–32 address bits and words matching the data width");
        }
        if self.kind == GateKind::Clock
            && (!(1..=99).contains(&self.config.clock_duty) || self.period < 2)
        {
            return invalid("clock requires a period >=2 ns and duty from 1 to 99 percent");
        }
        for width in [self.config.operand_a_width, self.config.operand_b_width]
            .into_iter()
            .flatten()
        {
            if !(1..=4096).contains(&width) {
                return invalid("operand widths must be 1–64");
            }
        }
        if self
            .config
            .custom_ports
            .values()
            .any(|point| !point.is_finite() || point.x.abs() > 10000.0 || point.y.abs() > 10000.0)
        {
            return invalid("port coordinates must be finite and within 10000");
        }
        if !self.config.module_width.is_finite()
            || self.config.module_width < 40.0
            || self.config.module_width > 4096.0
        {
            return invalid("module width must be 40–4096");
        }
        if self.kind == GateKind::Frame
            && (!self.config.frame_width.is_finite()
                || !self.config.frame_height.is_finite()
                || !(20.0..=10000.0).contains(&self.config.frame_width)
                || !(20.0..=10000.0).contains(&self.config.frame_height))
        {
            return invalid("frame dimensions must be 20–10000");
        }
        if matches!(self.kind, GateKind::Decoder | GateKind::Demux)
            && !(2..=64).contains(&self.input_count)
        {
            return invalid("decoder/demux requires 2–64 outputs");
        }
        if self.kind == GateKind::Vga
            && (self.config.vga_width == 0
                || self.config.vga_height == 0
                || self.config.vga_width > 1024
                || self.config.vga_height > 768)
        {
            return invalid("VGA framebuffer must be 1–1024 by 1–768");
        }
        if self.kind == GateKind::Tty && self.width != 8 {
            return invalid("TTY uses 8-bit bytes");
        }
        Ok(())
    }
}

/// Parse whitespace/comma-separated hex words, optionally with @hex-address directives.
pub fn parse_memory(text: &str, width: u16, address_bits: u8) -> Result<Vec<Signal>, String> {
    if !(1..=16).contains(&address_bits) || !(1..=4096).contains(&width) {
        return Err("invalid memory dimensions".into());
    }
    let mut memory = Vec::new();
    let mut address = 0;
    for line in text.lines() {
        let line = line
            .split("//")
            .next()
            .unwrap_or("")
            .split('#')
            .next()
            .unwrap_or("");
        for token in line
            .split(|ch: char| ch.is_whitespace() || ch == ',')
            .filter(|token| !token.is_empty())
        {
            if let Some(value) = token.strip_prefix('@') {
                address = usize::from_str_radix(value, 16)
                    .map_err(|_| format!("invalid address {token}"))?;
                continue;
            }
            if address >= (1usize << address_bits) {
                return Err("memory image exceeds address range".into());
            }
            if token.eq_ignore_ascii_case("x") || token.eq_ignore_ascii_case("z") {
                if memory.len() <= address {
                    memory
                        .resize_with(address + 1, || Signal::filled(width, crate::Logic::Unknown));
                }
                memory[address] = Signal::filled(
                    width,
                    if token.eq_ignore_ascii_case("x") {
                        crate::Logic::Unknown
                    } else {
                        crate::Logic::HighZ
                    },
                );
                address += 1;
                continue;
            }
            let word = Signal::parse(token.trim_start_matches("0x"), width, 16)?;
            if memory.len() <= address {
                memory.resize_with(address + 1, || Signal::filled(width, crate::Logic::Unknown));
            }
            memory[address] = word;
            address += 1;
        }
    }
    Ok(memory)
}

/// Sparse image parser supporting the full 32-bit memory address range.
pub fn parse_sparse_memory(
    text: &str,
    width: u16,
    address_bits: u8,
) -> Result<std::collections::BTreeMap<u32, Signal>, String> {
    if !(1..=32).contains(&address_bits) || !(1..=4096).contains(&width) {
        return Err("invalid memory dimensions".into());
    }
    let mut words = std::collections::BTreeMap::new();
    let mut address = 0u64;
    for line in text.lines() {
        let line = line
            .split("//")
            .next()
            .unwrap_or("")
            .split('#')
            .next()
            .unwrap_or("");
        for token in line
            .split(|ch: char| ch.is_whitespace() || ch == ',')
            .filter(|token| !token.is_empty())
        {
            if let Some(value) = token.strip_prefix('@') {
                address = u64::from_str_radix(value, 16)
                    .map_err(|_| format!("invalid address {token}"))?;
                continue;
            }
            if address >= (1u64 << address_bits) {
                return Err("memory image exceeds address range".into());
            }
            let word = if token.eq_ignore_ascii_case("x") || token.eq_ignore_ascii_case("z") {
                Signal::filled(
                    width,
                    if token.eq_ignore_ascii_case("x") {
                        crate::Logic::Unknown
                    } else {
                        crate::Logic::HighZ
                    },
                )
            } else {
                Signal::parse(token.trim_start_matches("0x"), width, 16)?
            };
            words.insert(address as u32, word);
            address += 1;
            if words.len() > 1_000_000 {
                return Err("memory image exceeds one million initialized words".into());
            }
        }
    }
    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GateId, Logic};
    #[test]
    fn hex_memory_images_have_explicit_bounds_and_sparse_unknowns() {
        let memory = parse_memory("41 42 // letters\n@10 FF x z", 8, 8).unwrap();
        assert_eq!(memory[0].to_u64(), Some(65));
        assert_eq!(memory[16].to_u64(), Some(255));
        assert_eq!(memory[15].bit(0), Logic::Unknown);
        assert_eq!(memory[18].bit(0), Logic::HighZ);
        assert!(parse_memory("100", 8, 8).is_err());
        assert!(parse_memory("@100 1", 8, 8).is_err());
    }
    #[test]
    fn invalid_partitions_slices_and_memory_configurations_fail() {
        let mut gate = Gate::new(GateId(1), GateKind::Concat, Point::ZERO);
        gate.config.partitions = vec![3, 3];
        assert!(gate.validate_component().is_err());
        gate = Gate::new(GateId(1), GateKind::Tap, Point::ZERO);
        gate.config.tap_offset = 8;
        assert!(gate.validate_component().is_err());
        gate = Gate::new(GateId(1), GateKind::Ram, Point::ZERO);
        gate.config.address_bits = 33;
        assert!(gate.validate_component().is_err());
    }
}
