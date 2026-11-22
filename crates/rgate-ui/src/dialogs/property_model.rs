use anyhow::{Context as _, Result};
use gpui::{Context, Entity, Window};
use gpui_component::input::InputState;
use rgate_core::{ComponentConfig, Gate, GateId, GateKind, Module, Net, NetId, Point, Signal};
use rgate_editor::Editor;

pub struct NetDialog {
    pub net: NetId,
    pub name: Entity<InputState>,
    pub width: Entity<InputState>,
    pub show_name: bool,
    pub error: Option<String>,
}

impl NetDialog {
    pub fn from_net<T: 'static>(net: &Net, window: &mut Window, cx: &mut Context<T>) -> Self {
        Self {
            net: net.id,
            name: crate::input::text_field(&net.name, window, cx),
            width: crate::input::text_field(net.width.to_string(), window, cx),
            show_name: net.show_name,
            error: None,
        }
    }

    pub fn apply_to_editor<T: 'static>(&self, editor: &mut Editor, cx: &Context<T>) -> Result<()> {
        let id = self.net;
        let name = self.name.read(cx).value().trim().to_owned();
        if name.is_empty() {
            anyhow::bail!("Net name must not be empty.");
        }
        let width: u16 = self
            .width
            .read(cx)
            .value()
            .parse()
            .context("Width must be 1–4096.")?;
        if !(1..=4096).contains(&width) {
            anyhow::bail!("Width must be 1–4096.");
        }
        if editor.module().gates.iter().any(|gate| {
            gate.pins
                .iter()
                .any(|pin| pin.net == Some(id) && gate.pin_width(&pin.name) != width)
        }) {
            anyhow::bail!(
                "Width conflicts with connected pins. Disconnect or resize the components first."
            );
        }
        let show = self.show_name;
        editor.edit_module(|module| {
            if let Some(net) = module.nets.iter_mut().find(|net| net.id == id) {
                net.name = name;
                net.width = width;
                net.show_name = show;
            }
        })?;
        Ok(())
    }
}

pub struct PropertyDialog {
    pub gate: GateId,
    pub name: Entity<InputState>,
    pub value: Entity<InputState>,
    pub delay: Entity<InputState>,
    pub width: Entity<InputState>,
    pub period: Entity<InputState>,
    pub comment: Entity<InputState>,
    pub show_name: bool,
    pub inputs: Entity<InputState>,
    pub partitions: Entity<InputState>,
    pub tap_offset: Entity<InputState>,
    pub tap_width: Entity<InputState>,
    pub address_bits: Entity<InputState>,
    pub memory: Entity<InputState>,
    pub led_display: rgate_core::LedDisplay,
    pub tty_tkgate: bool,
    pub reduction: bool,
    pub enable_low: bool,
    pub invert_output: bool,
    pub phase: Entity<InputState>,
    pub duty: Entity<InputState>,
    pub operand_a: Entity<InputState>,
    pub operand_b: Entity<InputState>,
    pub frame_width: Entity<InputState>,
    pub frame_height: Entity<InputState>,
    pub module_width: Entity<InputState>,
    pub custom_ports: Entity<InputState>,
    pub error: Option<String>,
}

impl PropertyDialog {
    pub fn from_gate<T: 'static>(gate: &Gate, window: &mut Window, cx: &mut Context<T>) -> Self {
        let name = crate::input::text_field(&gate.name, window, cx);
        Self {
            gate: gate.id,
            name,
            value: crate::input::text_field(gate.initial.display_value(), window, cx),
            delay: crate::input::text_field(gate.delay.to_string(), window, cx),
            width: crate::input::text_field(gate.width.to_string(), window, cx),
            period: crate::input::text_field(gate.period.to_string(), window, cx),
            comment: crate::input::text_field(gate.text.replace('\n', "\\n"), window, cx),
            show_name: gate.show_name,
            led_display: gate.config.led_display,
            tty_tkgate: gate.config.tty_tkgate,
            reduction: gate.config.reduction || gate.kind.is_reduction(),
            enable_low: gate.config.enable_low,
            invert_output: gate.config.invert_output,
            phase: crate::input::text_field(gate.config.clock_phase.to_string(), window, cx),
            duty: crate::input::text_field(gate.config.clock_duty.to_string(), window, cx),
            operand_a: crate::input::text_field(
                gate.config
                    .operand_a_width
                    .unwrap_or(gate.width)
                    .to_string(),
                window,
                cx,
            ),
            operand_b: crate::input::text_field(
                gate.config
                    .operand_b_width
                    .unwrap_or(gate.width)
                    .to_string(),
                window,
                cx,
            ),
            frame_width: crate::input::text_field(gate.config.frame_width.to_string(), window, cx),
            module_width: crate::input::text_field(
                gate.config.module_width.to_string(),
                window,
                cx,
            ),
            custom_ports: crate::input::text_field(
                serde_json::to_string(&gate.config.custom_ports).unwrap_or_else(|_| "{}".into()),
                window,
                cx,
            ),
            frame_height: crate::input::text_field(
                gate.config.frame_height.to_string(),
                window,
                cx,
            ),
            inputs: crate::input::text_field(gate.input_count.to_string(), window, cx),
            partitions: crate::input::text_field(
                gate.config
                    .partitions
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(","),
                window,
                cx,
            ),
            tap_offset: crate::input::text_field(gate.config.tap_offset.to_string(), window, cx),
            tap_width: crate::input::text_field(gate.config.tap_width.to_string(), window, cx),
            address_bits: crate::input::text_field(
                gate.config.address_bits.to_string(),
                window,
                cx,
            ),
            memory: crate::input::text_field(format_gate_memory(gate), window, cx),
            error: None,
        }
    }

    pub fn select_led_display<T: 'static>(
        &mut self,
        mode: rgate_core::LedDisplay,
        connected: bool,
        window: &mut Window,
        cx: &mut Context<T>,
    ) {
        self.led_display = mode;
        if !connected {
            self.width.update(cx, |field, cx| {
                field.set_value(mode.recommended_width().to_string(), window, cx)
            });
        }
        self.error = None;
    }

    pub fn apply_memory_image<T: 'static>(
        &mut self,
        image: &str,
        window: &mut Window,
        cx: &mut Context<T>,
    ) -> Result<()> {
        let width = self.width.read(cx).value().parse()?;
        let address_bits = self.address_bits.read(cx).value().parse()?;
        let words =
            rgate_core::parse_sparse_memory(image, width, address_bits).map_err(anyhow::Error::msg)?;
        let contents = words
            .iter()
            .map(|(address, word)| {
                format!(
                    "@{address:X} {}",
                    word.to_biguint()
                        .map(|value| value.to_str_radix(16))
                        .unwrap_or_else(|| "x".into())
                )
            })
            .collect::<Vec<_>>()
            .join(" ");
        self.memory
            .update(cx, |field, cx| field.set_value(contents, window, cx));
        Ok(())
    }

    pub fn apply_to_editor<T: 'static>(
        &self,
        editor: &mut Editor,
        cx: &Context<T>,
    ) -> Result<()> {
        let id = self.gate;
        let name = self.name.read(cx).value().trim().to_owned();
        if name.is_empty() {
            anyhow::bail!("Name must not be empty.");
        }
        let gate = editor
            .module()
            .gate(id)
            .context("Selected gate no longer exists.")?;

        if matches!(gate.kind, GateKind::Frame | GateKind::Comment) {
            return self.apply_annotation(editor, id, name, gate.kind == GateKind::Frame, cx);
        }

        let fields = crate::properties::fields(&gate.kind);
        let width = self.parse_width(gate, fields.width, cx)?;
        let port_only = gate.kind.port_direction().is_some()
            && gate.pin("P").and_then(|p| p.net).is_some_and(|net_id| {
                editor
                    .module()
                    .gates
                    .iter()
                    .filter(|g| g.id != gate.id)
                    .all(|g| g.pins.iter().all(|p| p.net != Some(net_id)))
            });
        if gate.width != width && gate.pins.iter().any(|pin| pin.net.is_some()) && !port_only {
            anyhow::bail!(
                "Disconnect this gate before changing its bit width; existing nets retain their width."
            );
        }

        let value = if fields.initial {
            let value = self.value.read(cx).value().to_string();
            if let Some(hex) = value.strip_prefix("0x") {
                Signal::parse(hex, width, 16)
            } else {
                Signal::parse(&value, width, 10)
            }
            .map_err(anyhow::Error::msg)?
        } else {
            gate.initial.resized(width)
        };
        let delay = if fields.delay {
            self.delay
                .read(cx)
                .value()
                .parse()
                .context("Delay must be a nonnegative number.")?
        } else {
            gate.delay
        };
        let period = if fields.clock {
            self.period
                .read(cx)
                .value()
                .parse()
                .context("Period must be a number.")?
        } else {
            gate.period
        };
        let inputs = if fields.count {
            self.inputs
                .read(cx)
                .value()
                .parse()
                .context("Input/output count must be 1–64.")?
        } else {
            gate.input_count
        };

        let config = self.parse_config(gate, width, fields.clock, cx)?;
        if gate.pins.iter().any(|pin| pin.net.is_some())
            && (config.partitions != gate.config.partitions
                || config.tap_offset != gate.config.tap_offset
                || config.tap_width != gate.config.tap_width
                || config.address_bits != gate.config.address_bits
                || inputs != gate.input_count
                || config.reduction != gate.config.reduction
                || config.tty_tkgate != gate.config.tty_tkgate
                || config.operand_a_width != gate.config.operand_a_width
                || config.operand_b_width != gate.config.operand_b_width)
        {
            anyhow::bail!("Disconnect this component before changing its pin layout or widths.");
        }

        let comment = gate.text.clone();
        let old_gate = gate.clone();
        let show_name = self.show_name;
        editor.edit_module(|module| {
            if let Some(gate) = module.gate_mut(id) {
                gate.name = name;
                gate.width = width;
                gate.initial = value;
                gate.delay = delay;
                gate.period = period;
                gate.text = comment;
                gate.show_name = show_name;
                gate.input_count = inputs;
                gate.config = config;
                if matches!(gate.kind, GateKind::Module(_)) {
                    for pin in &mut gate.pins {
                        if let Some(offset) = gate.config.custom_ports.get(&pin.name) {
                            pin.offset = *offset;
                        } else if old_gate.config.module_width != gate.config.module_width {
                            pin.offset.x = pin.offset.x.signum() * gate.config.module_width / 2.0;
                        }
                    }
                }
                if gate.kind.is_extended()
                    || inputs != old_gate.input_count
                    || width != old_gate.width
                    || gate.config.reduction != old_gate.config.reduction
                {
                    let old = gate.pins.clone();
                    gate.reset_pins();
                    for pin in &mut gate.pins {
                        if let Some(previous) = old.iter().find(|old| old.name == pin.name) {
                            pin.net = previous.net;
                            pin.offset = previous.offset;
                        }
                    }
                }
            }
            reattach_gate_wires(module, id);
        })?;
        Ok(())
    }

    fn apply_annotation<T: 'static>(
        &self,
        editor: &mut Editor,
        id: GateId,
        name: String,
        is_frame: bool,
        cx: &Context<T>,
    ) -> Result<()> {
        let dimensions = if is_frame {
            let width: f32 = self
                .frame_width
                .read(cx)
                .value()
                .parse()
                .context("Frame width must be numeric.")?;
            let height: f32 = self
                .frame_height
                .read(cx)
                .value()
                .parse()
                .context("Frame height must be numeric.")?;
            if !width.is_finite()
                || !height.is_finite()
                || !(20.0..=10000.0).contains(&width)
                || !(20.0..=10000.0).contains(&height)
            {
                anyhow::bail!("Frame dimensions must be between 20 and 10000.");
            }
            Some((width, height))
        } else {
            None
        };
        let title = self.comment.read(cx).value().replace("\\n", "\n");
        let show_name = self.show_name;
        editor.edit_module(|module| {
            let gate = module.gate_mut(id).expect("selected annotation exists");
            gate.name = name;
            gate.text = title;
            gate.show_name = show_name;
            if let Some((width, height)) = dimensions {
                gate.config.frame_width = width;
                gate.config.frame_height = height;
            }
        })?;
        Ok(())
    }

    fn parse_width<T: 'static>(
        &self,
        gate: &Gate,
        has_width: bool,
        cx: &Context<T>,
    ) -> Result<u16> {
        if !has_width {
            return Ok(gate.width);
        }
        let width: u16 = self
            .width
            .read(cx)
            .value()
            .parse()
            .context("Width must be a number from 1 to 4096.")?;
        if !(1..=4096).contains(&width) {
            anyhow::bail!("Width must be from 1 to 4096.");
        }
        Ok(width)
    }

    fn parse_config<T: 'static>(
        &self,
        gate: &Gate,
        width: u16,
        has_clock: bool,
        cx: &Context<T>,
    ) -> Result<ComponentConfig> {
        let mut config = gate.config.clone();
        if gate.kind == GateKind::Led {
            config.led_display = self.led_display;
        }
        if gate.kind == GateKind::Tty {
            config.tty_tkgate = self.tty_tkgate;
        }
        if gate.kind.is_logic() {
            config.reduction = self.reduction;
        }
        if gate.kind == GateKind::TriState {
            config.enable_low = self.enable_low;
            config.invert_output = self.invert_output;
        }
        if has_clock {
            config.clock_phase = self
                .phase
                .read(cx)
                .value()
                .parse()
                .context("Phase must be nonnegative nanoseconds.")?;
            config.clock_duty = self
                .duty
                .read(cx)
                .value()
                .parse()
                .context("Duty must be 1–99 percent.")?;
        }
        if matches!(gate.kind, GateKind::Module(_)) {
            config.module_width = self
                .module_width
                .read(cx)
                .value()
                .parse()
                .context("Module width must be numeric")?;
            config.custom_ports = serde_json::from_str(&self.custom_ports.read(cx).value())
                .context("Port positions must be JSON: {\"A\":{\"x\":-50,\"y\":0}}")?;
            for name in config.custom_ports.keys() {
                if gate.pin(name).is_none() {
                    anyhow::bail!("Unknown port {name}");
                }
            }
        }
        if matches!(gate.kind, GateKind::Multiply | GateKind::Divide) {
            config.operand_a_width = Some(self.operand_a.read(cx).value().parse()?);
            config.operand_b_width = Some(self.operand_b.read(cx).value().parse()?);
        }
        if matches!(gate.kind, GateKind::Concat | GateKind::Splitter) {
            config.partitions = self
                .partitions
                .read(cx)
                .value()
                .split(',')
                .filter(|part| !part.trim().is_empty())
                .map(|part| part.trim().parse::<u16>())
                .collect::<Result<Vec<_>, _>>()
                .context("Partitions must be comma-separated widths.")?;
        }
        if gate.kind == GateKind::Tap {
            config.tap_offset = self
                .tap_offset
                .read(cx)
                .value()
                .parse()
                .context("Tap offset must be a number.")?;
            config.tap_width = self
                .tap_width
                .read(cx)
                .value()
                .parse()
                .context("Tap width must be a number.")?;
        }
        if matches!(gate.kind, GateKind::Ram | GateKind::Rom) {
            config.address_bits = self
                .address_bits
                .read(cx)
                .value()
                .parse()
                .context("Address bits must be 1–32.")?;
            config.sparse_memory = rgate_core::parse_sparse_memory(
                &self.memory.read(cx).value(),
                width,
                config.address_bits,
            )
            .map_err(anyhow::Error::msg)?;
            config.memory.clear();
        }
        Ok(config)
    }
}

fn format_gate_memory(gate: &Gate) -> String {
    gate.config
        .memory
        .iter()
        .enumerate()
        .map(|(address, word)| (address as u64, word))
        .chain(
            gate.config
                .sparse_memory
                .iter()
                .map(|(address, word)| (u64::from(*address), word)),
        )
        .map(|(address, word)| {
            format!(
                "@{address:X} {}",
                word.to_biguint()
                    .map(|value| value.to_str_radix(16))
                    .unwrap_or_else(|| "x".into())
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn reattach_gate_wires(module: &mut Module, id: GateId) {
    let gate = module.gate(id).unwrap().clone();
    for wire in &mut module.wires {
        for (start, reference) in [(true, &wire.start), (false, &wire.end)] {
            if let Some(reference) = reference
                && reference.gate == id
                && let Some(pin) = gate.pin(&reference.pin)
            {
                let target = gate.pin_position(pin);
                let index = if start { 0 } else { wire.points.len() - 1 };
                let old = wire.points[index];
                if old != target {
                    if start {
                        wire.points[0] = target;
                        wire.points.insert(1, Point::new(old.x, target.y));
                        wire.points.insert(2, old);
                    } else {
                        wire.points.pop();
                        wire.points.push(Point::new(target.x, old.y));
                        wire.points.push(target);
                    }
                }
            }
        }
    }
}
