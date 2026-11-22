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
                wire: 0x008500,
                bus: 0xd12b2b,
                low: 0xb000b0,
                high: 0x008500,
                unknown: 0xe02020,
                float: 0x0000ff,
                grid: 0xcccccc,
                selected_wire: 0xa85200,
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
                high: 0x007d56,
                unknown: 0xdc2626,
                float: 0x0073ac,
                grid: 0xe2e8f0,
                selected_wire: 0xb84408,
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

/// Application chrome is independent of the schematic drawing style.
/// Small radii and restrained surfaces keep this an editor, not a set of cards.
pub fn apply_chrome(appearance: Appearance, cx: &mut gpui::App) {
    use gpui::px;
    gpui_component::Theme::update(cx, |theme| {
        let dark = appearance.is_dark();
        let color = |light, dark_color| rgb(if dark { dark_color } else { light }).into();
        theme.radius = px(4.0);
        theme.radius_lg = px(6.0);
        theme.font_size = px(16.0);
        theme.focus_ring = true;
        if cfg!(target_family = "wasm") {
            theme.font_family = "IBM Plex Sans".into();
            theme.mono_font_family = "Lilex".into();
        }
        let c = &mut theme.colors;
        c.background = color(0xf4f6f8, 0x161c25);
        c.foreground = color(0x202c3b, 0xe4eaf2);
        c.muted_foreground = color(0x526174, 0xa8b7ca);
        c.muted = color(0xe8edf3, 0x263140);
        c.sidebar = color(0xf4f6f8, 0x161c25);
        c.sidebar_foreground = c.foreground;
        c.border = color(0xc7d0dc, 0x3c4a5e);
        c.sidebar_border = c.border;
        c.title_bar = c.background;
        c.title_bar_border = c.border;
        c.tab_bar = c.background;
        c.tab = c.background;
        c.tab_foreground = c.muted_foreground;
        c.tab_active = color(0xffffff, 0x202b3b);
        c.tab_active_foreground = c.foreground;
        c.primary = color(0x245caa, 0x8ab9ff);
        c.primary_foreground = color(0xffffff, 0x101b2b);
        c.primary_hover = color(0x194b90, 0xa3c8ff);
        c.primary_active = color(0x16417c, 0xb6d3ff);
        c.button_primary = c.primary;
        c.button_primary_foreground = c.primary_foreground;
        c.button_primary_hover = c.primary_hover;
        c.button_primary_active = c.primary_active;
        c.popover = color(0xffffff, 0x1b232e);
        c.popover_foreground = c.foreground;
        c.ring = c.primary;
        c.caret = c.primary;
        c.accent = color(0xdde9f9, 0x283f60);
        c.accent_foreground = color(0x174780, 0xc3dcff);
        c.secondary = c.muted;
        c.secondary_foreground = c.foreground;
        c.secondary_hover = c.accent;
        c.secondary_active = c.accent;
        c.list_hover = color(0xe6ecf3, 0x233044);
        c.list_active = c.accent;
        c.list_active_border = c.primary;
        c.list_head = c.sidebar;
        c.input = color(0x7c8999, 0x7486a0);
        c.selection = c.accent;
        c.success = color(0x18734b, 0x69d5a3);
    });
}
