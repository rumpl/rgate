use crate::{SimError, Simulator};
use rgate_core::{Direction, Gate, GateKind, Logic, Signal};

impl Simulator {
    pub(crate) fn input(&self, gate: &Gate, name: &str) -> Signal {
        gate.pin(name)
            .and_then(|pin| pin.net)
            .and_then(|net| self.nets.value(net))
            .cloned()
            .unwrap_or_else(|| Signal::filled(gate.pin_width(name), Logic::HighZ))
    }

    pub(crate) fn scalar_input(&self, gate: &Gate, name: &str, default: Logic) -> Logic {
        if gate.pin(name).is_none_or(|pin| pin.net.is_none()) {
            default
        } else {
            self.input(gate, name).bit(0)
        }
    }

    pub(crate) fn evaluate(&mut self, index: usize) -> Result<(), SimError> {
        let gate = &self.flattened.gates[index];
        let mut outputs: Vec<(String, Signal)> = Vec::new();
        let output = match &gate.kind {
            kind if kind.is_logic() => {
                let operation = match kind {
                    GateKind::And | GateKind::Nand | GateKind::ReduceAnd | GateKind::ReduceNand => {
                        Logic::and
                    }
                    GateKind::Or | GateKind::Nor | GateKind::ReduceOr | GateKind::ReduceNor => {
                        Logic::or
                    }
                    _ => Logic::xor,
                };
                let inputs = gate
                    .pins
                    .iter()
                    .filter(|pin| pin.direction == Direction::Input)
                    .map(|pin| self.input(gate, &pin.name).broadcast_or_resize(gate.width));
                let mut value = inputs
                    .reduce(|a, b| a.zip(&b, operation))
                    .unwrap_or_else(|| Signal::filled(gate.width, Logic::Unknown));
                if gate.config.reduction || kind.is_reduction() {
                    value = Signal::scalar(
                        value
                            .bits()
                            .iter()
                            .copied()
                            .reduce(operation)
                            .unwrap_or(Logic::Unknown),
                    );
                }
                Some(
                    if matches!(
                        kind,
                        GateKind::Nand
                            | GateKind::Nor
                            | GateKind::Xnor
                            | GateKind::ReduceNand
                            | GateKind::ReduceNor
                            | GateKind::ReduceXnor
                    ) {
                        value.map(|bit| !bit)
                    } else {
                        value
                    },
                )
            }
            GateKind::Buffer => Some(self.input(gate, "I").map(Logic::buffered)),
            GateKind::Not => Some(self.input(gate, "I").map(|bit| !bit)),
            GateKind::TriState => {
                let enable = self.input(gate, "E").bit(0);
                let active = if gate.config.enable_low {
                    Logic::Low
                } else {
                    Logic::High
                };
                let data = self.input(gate, "I").map(|bit| {
                    if gate.config.invert_output {
                        !bit
                    } else {
                        bit.buffered()
                    }
                });
                Some(if enable == active {
                    data
                } else if matches!(enable, Logic::Low | Logic::High) {
                    Signal::filled(gate.width, Logic::HighZ)
                } else {
                    data.map(|bit| {
                        if bit == Logic::HighZ {
                            Logic::HighZ
                        } else {
                            Logic::Unknown
                        }
                    })
                })
            }
            GateKind::Switch | GateKind::Dip | GateKind::Clock | GateKind::Peripheral => {
                Some(self.states[index].source.clone())
            }
            GateKind::Ground => Some(Signal::from_u64(0, gate.width)),
            GateKind::Vdd => Some(Signal::filled(gate.width, Logic::High)),
            GateKind::Mux => Some(match self.input(gate, "S").to_u64() {
                Some(selection) if selection < u64::from(gate.input_count) => {
                    self.input(gate, &format!("I{selection}"))
                }
                _ => Signal::filled(gate.width, Logic::Unknown),
            }),
            GateKind::Add => {
                let (sum, carry) = self.input(gate, "A").add_with_carry(
                    &self.input(gate, "B"),
                    self.input(gate, "CI").bit(0),
                    gate.width,
                );
                let carry = Signal::scalar(carry);
                outputs.push(("S".into(), sum));
                outputs.push(("CO".into(), carry));
                None
            }
            GateKind::Dff | GateKind::Register => {
                let clock = self.scalar_input(gate, "CK", Logic::Low);
                let clear = self.scalar_input(gate, "CLR", Logic::High);
                let enable = self.scalar_input(gate, "EN", Logic::Low);
                if clear == Logic::Low {
                    self.states[index].stored = Signal::from_u64(0, gate.width);
                } else if clock == Logic::High && self.states[index].previous_clock == Logic::Low {
                    self.states[index].stored = match (clear, enable) {
                        (Logic::High, Logic::Low) => self.input(gate, "D").resized(gate.width),
                        (Logic::High, Logic::High) => self.states[index].stored.clone(),
                        _ => Signal::filled(gate.width, Logic::Unknown),
                    };
                }
                self.states[index].previous_clock = clock;
                outputs.push(("Q".into(), self.states[index].stored.clone()));
                if gate.kind == GateKind::Dff {
                    outputs.push(("_Q".into(), self.states[index].stored.map(|bit| !bit)));
                }
                None
            }
            GateKind::Module(_) | GateKind::Unsupported(_) => {
                outputs.extend(
                    gate.pins
                        .iter()
                        .filter(|pin| pin.direction == Direction::Output)
                        .map(|pin| {
                            (
                                pin.name.clone(),
                                Signal::filled(gate.pin_width(&pin.name), Logic::Unknown),
                            )
                        }),
                );
                None
            }
            kind if kind.is_extended() => None,
            _ => None,
        };
        if let Some(value) = output {
            outputs.push(("Z".into(), value.resized(gate.pin_width("Z"))));
        }
        let delay = if gate.kind.is_source() {
            0
        } else if gate.kind == GateKind::Tty && gate.config.tty_tkgate {
            10
        } else {
            gate.delay
        };
        if gate.kind.is_extended() && gate.kind != GateKind::Peripheral {
            outputs = self.evaluate_component(index);
        }
        for (pin, value) in outputs {
            self.events
                .schedule_drive(self.time, delay, index, pin, value)?;
        }
        Ok(())
    }
}
