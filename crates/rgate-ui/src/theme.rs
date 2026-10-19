use gpui::{Hsla, rgb};

/// Drawing style of the schematic canvas. The surrounding chrome always uses
/// GPUI Kit, so this only affects gates, wires and the drawing surface.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Theme {
    #[default]
    Classic,
    // Older preference files stored the dark palette as its own theme.
    #[serde(alias = "Dark")]
    Modern,
}

/// Light or dark application appearance, shared by GPUI Kit and the canvas.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Appearance {
    #[default]
    Light,
    Dark,
}

impl Appearance {
    pub fn is_dark(self) -> bool {
        self == Self::Dark
    }
}

#[derive(Clone, Copy)]
pub struct Palette {
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
}

impl Theme {
    pub fn palette(self, appearance: Appearance) -> Palette {
        match (self, appearance) {
            (Self::Classic, Appearance::Light) => Palette {
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
            },
            (Self::Classic, Appearance::Dark) => Palette {
                light: 0x1c1c1c,
                shadow: 0x5a5a5a,
                panel: 0x121212,
                text: 0xe6e6e6,
                muted: 0x9a9a9a,
                accent: 0x3a3a5c,
                hover: 0x262626,
                selection: 0x2e2e66,
                gate: 0x7a8cff,
                wire: 0x3ccf3c,
                bus: 0xff5a5a,
                low: 0xf070f0,
                high: 0x3ccf3c,
                unknown: 0xff6060,
                float: 0x7a8cff,
                grid: 0x2c2c2c,
                selected_wire: 0xffa040,
                module: 0xd47ad4,
            },
            (Self::Modern, Appearance::Light) => Palette {
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
            },
            (Self::Modern, Appearance::Dark) => Palette {
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
            },
        }
    }
}

impl Palette {
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
