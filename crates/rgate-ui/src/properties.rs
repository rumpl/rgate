use rgate_core::GateKind;

pub struct Fields {
    pub width: bool,
    pub initial: bool,
    pub delay: bool,
    pub clock: bool,
    pub count: bool,
}

pub fn fields(kind: &GateKind) -> Fields {
    let annotation = matches!(kind, GateKind::Frame | GateKind::Comment);
    let block = matches!(kind, GateKind::Module(_));
    Fields {
        width: !annotation
            && !block
            && !matches!(
                kind,
                GateKind::Clock | GateKind::Ground | GateKind::Vdd | GateKind::Tty
            ),
        initial: matches!(
            kind,
            GateKind::Switch | GateKind::Dip | GateKind::Peripheral
        ),
        delay: !annotation
            && !block
            && !matches!(
                kind,
                GateKind::Switch
                    | GateKind::Dip
                    | GateKind::Clock
                    | GateKind::Ground
                    | GateKind::Vdd
                    | GateKind::Led
            ),
        clock: *kind == GateKind::Clock,
        count: kind.is_logic()
            || matches!(kind, GateKind::Mux | GateKind::Decoder | GateKind::Demux),
    }
}
