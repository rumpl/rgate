use crate::{GateState, Simulator};
use rgate_core::{Gate, GateKind, Logic, Signal};

impl Simulator {
    pub(super) fn evaluate_component(&mut self, index: usize) -> Vec<(String, Signal)> {
        let gate = &self.flattened.gates[index];
        let w = gate.width;
        let unknown = || Signal::filled(w, Logic::Unknown);
        let input = |name: &str| self.input(gate, name);
        let scalar = |name, default| self.scalar_input(gate, name, default);
        match gate.kind {
            GateKind::Vga => {
                self.evaluate_vga(index);
                Vec::new()
            }
            GateKind::Multiply | GateKind::Divide => {
                let a = input("A");
                let b = input("B");
                if gate.kind == GateKind::Multiply {
                    vec![("P".into(), a.multiply(&b, w))]
                } else {
                    let (q, r) = a.divide(&b, w);
                    vec![("Q".into(), q), ("R".into(), r)]
                }
            }
            GateKind::ShiftLeft
            | GateKind::ShiftRight
            | GateKind::ArithmeticShiftRight
            | GateKind::RotateLeft
            | GateKind::RotateRight => {
                let data = input("I");
                let value = if let Some(amount) = input("S").to_u64() {
                    let bits = (0..usize::from(w))
                        .map(|bit| {
                            let amount = amount.min(usize::MAX as u64) as usize;
                            match gate.kind {
                                GateKind::ShiftLeft => bit
                                    .checked_sub(amount)
                                    .map(|bit| data.bit(bit))
                                    .unwrap_or(Logic::Low),
                                GateKind::ShiftRight => bit
                                    .checked_add(amount)
                                    .filter(|bit| *bit < usize::from(w))
                                    .map(|bit| data.bit(bit))
                                    .unwrap_or(Logic::Low),
                                GateKind::ArithmeticShiftRight => bit
                                    .checked_add(amount)
                                    .filter(|bit| *bit < usize::from(w))
                                    .map(|bit| data.bit(bit))
                                    .unwrap_or_else(|| data.bit(usize::from(w) - 1)),
                                GateKind::RotateLeft => data.bit(
                                    (bit + usize::from(w) - amount % usize::from(w))
                                        % usize::from(w),
                                ),
                                _ => data.bit((bit + amount % usize::from(w)) % usize::from(w)),
                            }
                        })
                        .collect();
                    Signal::from_bits(bits)
                } else {
                    unknown()
                };
                vec![("Z".into(), value)]
            }
            GateKind::Concat => {
                let bits = gate
                    .config
                    .partitions
                    .iter()
                    .enumerate()
                    .flat_map(|(index, width)| {
                        input(&format!("I{index}")).resized(*width).bits().to_vec()
                    })
                    .collect();
                vec![("Z".into(), Signal::from_bits(bits))]
            }
            GateKind::Splitter => {
                let mut offset = 0;
                gate.config
                    .partitions
                    .iter()
                    .enumerate()
                    .map(|(index, width)| {
                        let result = (format!("Z{index}"), input("I").slice(offset, *width));
                        offset += width;
                        result
                    })
                    .collect()
            }
            GateKind::Tap => vec![(
                "Z".into(),
                input("I").slice(gate.config.tap_offset, gate.config.tap_width),
            )],
            GateKind::Decoder | GateKind::Demux => {
                let select = input(if gate.kind == GateKind::Decoder {
                    "I"
                } else {
                    "S"
                })
                .to_u64();
                let enable = scalar("E", Logic::High);
                (0..gate.input_count)
                    .map(|index| {
                        let value = if enable == Logic::Low {
                            Signal::filled(gate.pin_width(&format!("Z{index}")), Logic::Low)
                        } else if enable != Logic::High || select.is_none() {
                            Signal::filled(gate.pin_width(&format!("Z{index}")), Logic::Unknown)
                        } else if select == Some(u64::from(index)) {
                            if gate.kind == GateKind::Decoder {
                                Signal::scalar(Logic::High)
                            } else {
                                input("F").resized(w)
                            }
                        } else {
                            Signal::filled(gate.pin_width(&format!("Z{index}")), Logic::Low)
                        };
                        (format!("Z{index}"), value)
                    })
                    .collect()
            }
            GateKind::Nmos | GateKind::Pmos => {
                let source = input("S");
                let control = input("G");
                let active = if gate.kind == GateKind::Nmos {
                    Logic::High
                } else {
                    Logic::Low
                };
                let bits = (0..usize::from(w))
                    .map(|bit| match control.bit(bit) {
                        value if value == active => source.bit(bit),
                        Logic::Low | Logic::High => Logic::HighZ,
                        _ if source.bit(bit) == Logic::HighZ => Logic::HighZ,
                        _ => Logic::Unknown,
                    })
                    .collect();
                vec![("Z".into(), Signal::from_bits(bits))]
            }
            GateKind::Jkff => {
                let clock = scalar("CK", Logic::Low);
                let clear = scalar("CLR", Logic::High);
                let preset = scalar("PRE", Logic::High);
                let stored = if clear == Logic::Low && preset == Logic::Low {
                    unknown()
                } else if clear == Logic::Low {
                    Signal::from_u64(0, w)
                } else if preset == Logic::Low {
                    Signal::filled(w, Logic::High)
                } else if clear != Logic::High || preset != Logic::High {
                    unknown()
                } else if clock == Logic::High && self.states[index].previous_clock == Logic::Low {
                    let j = input("J");
                    let k = input("K");
                    let q = &self.states[index].stored;
                    Signal::from_bits(
                        (0..usize::from(w))
                            .map(|bit| {
                                j.bit(bit)
                                    .and(!q.bit(bit))
                                    .or((!k.bit(bit)).and(q.bit(bit)))
                            })
                            .collect(),
                    )
                } else {
                    self.states[index].stored.clone()
                };
                self.states[index].stored = stored.clone();
                self.states[index].previous_clock = clock;
                vec![
                    ("Q".into(), stored.clone()),
                    ("_Q".into(), stored.map(|bit| !bit)),
                ]
            }
            GateKind::Ram | GateKind::Rom => self.evaluate_memory(index),
            GateKind::Tty => self.evaluate_terminal(index),
            _ => Vec::new(),
        }
    }

    fn evaluate_memory(&mut self, index: usize) -> Vec<(String, Signal)> {
        let gate = &self.flattened.gates[index];
        let address = self
            .input(gate, "A")
            .to_u64()
            .filter(|address| *address < (1u64 << gate.config.address_bits));
        let selected = if gate.kind == GateKind::Rom {
            Logic::Low
        } else {
            self.scalar_input(gate, "CS", Logic::High)
        };
        let oe = self.scalar_input(gate, "OE", Logic::High);
        let we = self.scalar_input(gate, "WE", Logic::High);
        if gate.kind == GateKind::Ram && selected == Logic::Low && we == Logic::Low {
            if let Some(address) = address {
                let data = self.external_input(index, gate, "D");
                self.states[index].memory.insert(address as usize, data);
            } else {
                self.states[index].memory.clear();
            }
        }
        let value = if selected == Logic::High || oe == Logic::High {
            Signal::filled(gate.width, Logic::HighZ)
        } else if selected != Logic::Low || oe != Logic::Low {
            Signal::filled(gate.width, Logic::Unknown)
        } else {
            address
                .and_then(|address| self.states[index].memory.get(&(address as usize)).cloned())
                .unwrap_or_else(|| Signal::filled(gate.width, Logic::Unknown))
        };
        vec![("D".into(), value)]
    }

    fn external_input(&self, index: usize, gate: &Gate, name: &str) -> Signal {
        let Some(net) = gate.pin(name).and_then(|pin| pin.net) else {
            return Signal::filled(gate.width, Logic::HighZ);
        };
        self.drivers
            .iter()
            .filter(|((driver, pin), _)| {
                *driver != index
                    && self.flattened.gates[*driver]
                        .pin(pin)
                        .and_then(|pin| pin.net)
                        == Some(net)
            })
            .fold(
                Signal::filled(gate.width, Logic::HighZ),
                |value, (_, drive)| value.zip(drive, Logic::resolve),
            )
    }

    fn evaluate_terminal(&mut self, index: usize) -> Vec<(String, Signal)> {
        let gate = &self.flattened.gates[index];
        if gate.config.tty_tkgate {
            let dsr = self.scalar_input(gate, "DSR", Logic::Low);
            let cts = self.scalar_input(gate, "CTS", Logic::High);
            let data = self.input(gate, "RD").to_u64();
            let capture = dsr == Logic::High && self.states[index].previous_write == Logic::Low;
            let consume = cts == Logic::High && self.states[index].previous_read == Logic::Low;
            self.states[index].previous_write = dsr;
            self.states[index].previous_read = cts;
            if capture {
                self.push(
                    self.time.saturating_add(10),
                    crate::EventKind::TerminalCapture {
                        gate: index,
                        byte: data.map(|value| value as u8).unwrap_or(b'?'),
                    },
                );
            }
            if consume {
                self.push(
                    self.time.saturating_add(10),
                    crate::EventKind::TerminalConsume { gate: index },
                );
            }
            let state = &self.states[index];
            return vec![
                (
                    "TD".into(),
                    Signal::from_u64(u64::from(state.received.front().copied().unwrap_or(0)), 8),
                ),
                (
                    "RTS".into(),
                    Signal::scalar(Logic::from_bool(
                        cts == Logic::Low && !state.received.is_empty(),
                    )),
                ),
                ("DTR".into(), Signal::scalar(dsr.buffered())),
            ];
        }
        let write = self.scalar_input(gate, "WR", Logic::Low);
        let read = self.scalar_input(gate, "RD", Logic::Low);
        let tx = self.input(gate, "TX").to_u64();
        let state = &mut self.states[index];
        if write == Logic::High && state.previous_write == Logic::Low {
            if state.terminal.len() >= 65536 {
                state.terminal.drain(..32768);
            }
            state
                .terminal
                .push(tx.map(|byte| byte as u8).unwrap_or(b'?'));
        }
        if read == Logic::High && state.previous_read == Logic::Low {
            state.received.pop_front();
        }
        state.previous_write = write;
        state.previous_read = read;
        vec![
            (
                "RX".into(),
                Signal::from_u64(u64::from(state.received.front().copied().unwrap_or(0)), 8),
            ),
            (
                "READY".into(),
                Signal::scalar(Logic::from_bool(!state.received.is_empty())),
            ),
        ]
    }
}

impl GateState {
    pub(super) fn extended(gate: &Gate) -> Self {
        Self {
            source: gate.initial.resized(gate.width),
            stored: Signal::filled(gate.width, Logic::Unknown),
            previous_clock: Logic::Low,
            memory: gate
                .config
                .memory
                .iter()
                .cloned()
                .enumerate()
                .chain(
                    gate.config
                        .sparse_memory
                        .iter()
                        .map(|(address, value)| (*address as usize, value.clone())),
                )
                .collect(),
            terminal: Vec::new(),
            received: std::collections::VecDeque::new(),
            previous_write: Logic::Low,
            previous_read: Logic::Low,
            vga: (gate.kind == GateKind::Vga)
                .then(|| crate::vga::VgaState::new(gate.config.vga_width, gate.config.vga_height)),
        }
    }
}
