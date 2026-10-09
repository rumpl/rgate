use gpui::{Div, Global, Hsla, div, prelude::*, px, rgb};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Theme {
    #[default]
    Classic,
    Modern,
    Dark,
}

impl Global for Theme {}

#[derive(Clone, Copy)]
pub struct Palette {
    pub chrome: u32,
    pub light: u32,
    pub shadow: u32,
    pub panel: u32,
    pub text: u32,
    pub muted: u32,
    pub accent: u32,
    pub hover: u32,
    pub selection: u32,
    pub gate: u32,
    pub wire: u32,
    pub bus: u32,
    pub low: u32,
    pub high: u32,
    pub unknown: u32,
    pub float: u32,
    pub grid: u32,
    pub selected_wire: u32,
    pub module: u32,
    pub error: u32,
    pub radius: f32,
    pub font: &'static str,
}

impl Theme {
    pub fn palette(self) -> Palette {
        match self {
            Self::Classic => Palette {
                chrome: 0xd9d9d9,
                light: 0xf5f5f5,
                shadow: 0x858585,
                panel: 0xffffff,
                text: 0x202020,
                muted: 0x666666,
                accent: 0xaaaacc,
                hover: 0xeeeeee,
                selection: 0xccccff,
                gate: 0x0000ff,
                wire: 0x008b00,
                bus: 0xff0000,
                low: 0xdd00dd,
                high: 0x008b00,
                unknown: 0xe02020,
                float: 0x0000ff,
                grid: 0xcccccc,
                selected_wire: 0xf08000,
                module: 0x8b008b,
                error: 0xa00000,
                radius: 0.0,
                font: "Helvetica",
            },
            Self::Modern => Palette {
                chrome: 0xf1f5f9,
                light: 0xf8fafc,
                shadow: 0xcbd5e1,
                panel: 0xffffff,
                text: 0x1e293b,
                muted: 0x64748b,
                accent: 0xdbeafe,
                hover: 0xe2e8f0,
                selection: 0xdbeafe,
                gate: 0x2563eb,
                wire: 0x0f766e,
                bus: 0x9333ea,
                low: 0x7c3aed,
                high: 0x059669,
                unknown: 0xdc2626,
                float: 0x0284c7,
                grid: 0xe2e8f0,
                selected_wire: 0xea580c,
                module: 0x7c3aed,
                error: 0xb91c1c,
                radius: 6.0,
                font: ".SystemUIFont",
            },
            Self::Dark => Palette {
                chrome: 0x1e293b,
                light: 0x182436,
                shadow: 0x334155,
                panel: 0x0f172a,
                text: 0xe2e8f0,
                muted: 0xa8b8ce,
                accent: 0x1e3a5f,
                hover: 0x2a3b52,
                selection: 0x234567,
                gate: 0x60a5fa,
                wire: 0x2dd4bf,
                bus: 0xc084fc,
                low: 0xa78bfa,
                high: 0x34d399,
                unknown: 0xfb7185,
                float: 0x38bdf8,
                grid: 0x293548,
                selected_wire: 0xfbbf24,
                module: 0xc4b5fd,
                error: 0xfda4af,
                radius: 6.0,
                font: ".SystemUIFont",
            },
        }
    }
}

impl Palette {
    pub fn sunken(self) -> Div {
        div()
            .bg(rgb(self.panel))
            .border_1()
            .border_color(rgb(self.shadow))
            .rounded(px(self.radius))
    }

    pub fn signal_color(self, signal: &rgate_core::Signal) -> Hsla {
        use rgate_core::Logic;
        rgb(if signal.bits().contains(&Logic::Unknown) {
            self.unknown
        } else if signal.bits().contains(&Logic::HighZ) {
            self.float
        } else if signal.to_u64() == Some(0) {
            self.low
        } else {
            self.high
        })
        .into()
    }
}
