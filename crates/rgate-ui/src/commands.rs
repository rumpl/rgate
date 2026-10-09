use crate::theme::Theme;
use gpui::{App, KeyBinding, Menu, MenuItem, SystemMenuType, actions};
use rgate_core::{Direction, GateKind};
use rgate_editor::Tool;

actions!(
    rgate,
    [
        New,
        Open,
        Save,
        SaveAs,
        Export,
        Undo,
        Redo,
        Cut,
        Copy,
        Paste,
        Delete,
        SelectAll,
        Rotate,
        RotateBack,
        PlayPause,
        Stop,
        Step,
        ClockStep,
        ZoomIn,
        ZoomOut,
        Fit,
        ClassicTheme,
        ModernTheme,
        DarkTheme,
        ToggleGrid,
        ToggleSnap,
        ToggleScope,
        Properties,
        NewModule,
        Find,
        About,
        Help,
        Quit,
    ]
);

#[derive(Clone, Debug)]
pub enum Command {
    New,
    Open,
    Save,
    SaveAs,
    Export,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    Delete,
    SelectAll,
    Rotate(bool),
    PlayPause,
    Stop,
    Step,
    ClockStep,
    Zoom(f32),
    Fit,
    SetTheme(Theme),
    ToggleGrid,
    ToggleSnap,
    ToggleScope,
    Properties,
    LoadMemory,
    EditSymbol,
    NewModule,
    NewVerilog,
    EditVerilog,
    VerilogInterface,
    SetHdlInput,
    Find,
    Align(rgate_editor::Align),
    Distribute(rgate_editor::Distribute),
    AutoRoute,
    TidyLayout,
    About,
    Help,
    Quit,
    Tool(Tool),
    #[cfg_attr(not(test), allow(dead_code))]
    Example(&'static str),
    Port(Option<Direction>),
}

pub struct MenuEntry {
    pub label: &'static str,
    pub shortcut: &'static str,
    pub command: Option<Command>,
}

impl MenuEntry {
    fn item(label: &'static str, shortcut: &'static str, command: Command) -> Self {
        Self {
            label,
            shortcut,
            command: Some(command),
        }
    }
    fn separator() -> Self {
        Self {
            label: "",
            shortcut: "",
            command: None,
        }
    }
}

pub fn entries(menu: &str) -> Vec<MenuEntry> {
    use Command as C;
    use MenuEntry as E;
    match menu {
        "File" => vec![
            E::item("New circuit", "⌘N", C::New),
            E::item("Open…", "⌘O", C::Open),
            E::item("Save", "⌘S", C::Save),
            E::item("Save as…", "⇧⌘S", C::SaveAs),
            E::item("Export Verilog…", "", C::Export),
            E::separator(),
            E::item("Quit RGate", "⌘Q", C::Quit),
        ],
        "Edit" => vec![
            E::item("Undo", "⌘Z", C::Undo),
            E::item("Redo", "⇧⌘Z", C::Redo),
            E::separator(),
            E::item("Cut", "⌘X", C::Cut),
            E::item("Copy", "⌘C", C::Copy),
            E::item("Paste", "⌘V", C::Paste),
            E::item("Delete", "⌫", C::Delete),
            E::item("Select all", "⌘A", C::SelectAll),
            E::separator(),
            E::item("Properties…", "Enter", C::Properties),
            E::item("Find gate/net…", "⌘F", C::Find),
        ],
        "Arrange" => vec![
            E::item("Align left", "", C::Align(rgate_editor::Align::Left)),
            E::item("Align right", "", C::Align(rgate_editor::Align::Right)),
            E::item("Align top", "", C::Align(rgate_editor::Align::Top)),
            E::item("Align bottom", "", C::Align(rgate_editor::Align::Bottom)),
            E::item(
                "Align horizontal center",
                "",
                C::Align(rgate_editor::Align::HorizontalCenter),
            ),
            E::item(
                "Align vertical center",
                "",
                C::Align(rgate_editor::Align::VerticalCenter),
            ),
            E::separator(),
            E::item(
                "Distribute horizontally",
                "",
                C::Distribute(rgate_editor::Distribute::Horizontal),
            ),
            E::item(
                "Distribute vertically",
                "",
                C::Distribute(rgate_editor::Distribute::Vertical),
            ),
            E::separator(),
            E::item("Auto-route selected wires", "", C::AutoRoute),
            E::item("Tidy layered layout (Undo available)", "", C::TidyLayout),
        ],
        "Tool" => vec![
            E::item("Select / move", "V", C::Tool(Tool::Select)),
            E::item("Connect wires", "W", C::Tool(Tool::Wire)),
            E::item("Scroll canvas", "P", C::Tool(Tool::Pan)),
            E::item("Cut wire / delete", "D", C::Tool(Tool::Delete)),
            E::separator(),
            E::item("Zoom in", "+", C::Zoom(1.25)),
            E::item("Zoom out", "−", C::Zoom(0.8)),
            E::item("Fit circuit", "⌘0", C::Fit),
            E::separator(),
            E::item("Show grid", "G", C::ToggleGrid),
            E::item("Snap to grid", "", C::ToggleSnap),
            E::item("Waveform / messages", "", C::ToggleScope),
        ],
        "View" => vec![
            E::item("Theme: Classic (TkGate)", "", C::SetTheme(Theme::Classic)),
            E::item("Theme: Modern", "", C::SetTheme(Theme::Modern)),
            E::item("Theme: Dark", "", C::SetTheme(Theme::Dark)),
            E::separator(),
            E::item("Show grid", "G", C::ToggleGrid),
            E::item("Snap to grid", "", C::ToggleSnap),
            E::item("Fit circuit", "⌘0", C::Fit),
        ],
        "Simulate" => vec![
            E::item("Run / pause", "Space", C::PlayPause),
            E::item("Stop simulation", "", C::Stop),
            E::item("Step event", "F6", C::Step),
            E::item("Advance clock cycle", "Tab", C::ClockStep),
            E::separator(),
            E::item("Show waveforms", "", C::ToggleScope),
        ],
        "Module" => vec![
            E::item("New module…", "", C::NewModule),
            E::item("New Verilog module…", "", C::NewVerilog),
            E::item("Edit Verilog source…", "", C::EditVerilog),
            E::item("Verilog interface…", "", C::VerilogInterface),
            E::item("Set selected HDL input…", "", C::SetHdlInput),
            E::item("Edit module symbol…", "", C::EditSymbol),
            E::item("Module properties", "", C::Properties),
        ],
        "Gate" => vec![
            E::item("Properties…", "Enter", C::Properties),
            E::item("Rotate clockwise", "R", C::Rotate(true)),
            E::item("Rotate counterclockwise", "⇧R", C::Rotate(false)),
            E::item("Delete", "⌫", C::Delete),
        ],
        "Components" => [
            ("AND", "A", GateKind::And),
            ("NAND", "", GateKind::Nand),
            ("OR", "O", GateKind::Or),
            ("NOR", "", GateKind::Nor),
            ("XOR", "X", GateKind::Xor),
            ("XNOR", "", GateKind::Xnor),
            ("NOT", "N", GateKind::Not),
            ("Buffer", "", GateKind::Buffer),
            ("Switch", "S", GateKind::Switch),
            ("DIP switch", "", GateKind::Dip),
            ("LED", "L", GateKind::Led),
            ("Clock", "C", GateKind::Clock),
            ("D flip-flop", "F", GateKind::Dff),
            ("Register", "", GateKind::Register),
            ("Multiplexer", "M", GateKind::Mux),
            ("Adder", "", GateKind::Add),
            ("Ground", "", GateKind::Ground),
            ("Vdd", "", GateKind::Vdd),
            ("Tri-state buffer", "", GateKind::TriState),
            ("Bus splitter", "", GateKind::Splitter),
            ("Concatenation", "", GateKind::Concat),
            ("Bus tap", "", GateKind::Tap),
            ("RAM", "", GateKind::Ram),
            ("ROM", "", GateKind::Rom),
            ("JK flip-flop", "", GateKind::Jkff),
            ("Decoder", "", GateKind::Decoder),
            ("Demultiplexer", "", GateKind::Demux),
            ("Multiplier", "", GateKind::Multiply),
            ("Divider", "", GateKind::Divide),
            ("Shift left", "", GateKind::ShiftLeft),
            ("Shift right", "", GateKind::ShiftRight),
            ("Arithmetic shift right", "", GateKind::ArithmeticShiftRight),
            ("Rotate left", "", GateKind::RotateLeft),
            ("Rotate right", "", GateKind::RotateRight),
            ("NMOS", "", GateKind::Nmos),
            ("PMOS", "", GateKind::Pmos),
            ("TTY terminal", "", GateKind::Tty),
            ("VGA display", "", GateKind::Vga),
            ("GPIO peripheral", "", GateKind::Peripheral),
            ("Reduction AND", "", GateKind::ReduceAnd),
            ("Reduction NAND", "", GateKind::ReduceNand),
            ("Reduction OR", "", GateKind::ReduceOr),
            ("Reduction NOR", "", GateKind::ReduceNor),
            ("Reduction XOR", "", GateKind::ReduceXor),
            ("Reduction XNOR", "", GateKind::ReduceXnor),
            ("Frame", "", GateKind::Frame),
            ("Comment", "T", GateKind::Comment),
        ]
        .into_iter()
        .map(|(label, key, kind)| E::item(label, key, C::Tool(Tool::Place(kind))))
        .collect(),
        "Help" => vec![
            E::item("Getting started", "F1", C::Help),
            E::item("About RGate / licenses", "", C::About),
        ],
        "Canvas" => vec![
            E::item("Properties…", "", C::Properties),
            E::item("Copy", "⌘C", C::Copy),
            E::item("Paste", "⌘V", C::Paste),
            E::item("Delete", "⌫", C::Delete),
            E::separator(),
            E::item("Rotate clockwise", "R", C::Rotate(true)),
            E::item("Select / move", "V", C::Tool(Tool::Select)),
            E::item("Connect wires", "W", C::Tool(Tool::Wire)),
            E::item("Fit circuit", "⌘0", C::Fit),
        ],
        _ => Vec::new(),
    }
}

pub fn install(cx: &mut App) {
    cx.set_menus(vec![
        Menu {
            disabled: false,
            name: "RGate".into(),
            items: vec![
                MenuItem::action("About RGate", About),
                MenuItem::separator(),
                MenuItem::os_submenu("Services", SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action("Quit RGate", Quit),
            ],
        },
        Menu {
            disabled: false,
            name: "File".into(),
            items: vec![
                MenuItem::action("New", New),
                MenuItem::action("Open…", Open),
                MenuItem::separator(),
                MenuItem::action("Save", Save),
                MenuItem::action("Save As…", SaveAs),
                MenuItem::action("Export Verilog…", Export),
            ],
        },
        Menu {
            disabled: false,
            name: "Edit".into(),
            items: vec![
                MenuItem::action("Undo", Undo),
                MenuItem::action("Redo", Redo),
                MenuItem::separator(),
                MenuItem::action("Cut", Cut),
                MenuItem::action("Copy", Copy),
                MenuItem::action("Paste", Paste),
                MenuItem::action("Delete", Delete),
                MenuItem::action("Select All", SelectAll),
                MenuItem::action("Properties…", Properties),
            ],
        },
        Menu {
            disabled: false,
            name: "Simulate".into(),
            items: vec![
                MenuItem::action("Run / Pause", PlayPause),
                MenuItem::action("Stop", Stop),
                MenuItem::action("Step Event", Step),
                MenuItem::action("Advance Clock", ClockStep),
            ],
        },
        Menu {
            disabled: false,
            name: "View".into(),
            items: vec![
                MenuItem::action("Theme: Classic (TkGate)", ClassicTheme),
                MenuItem::action("Theme: Modern", ModernTheme),
                MenuItem::action("Theme: Dark", DarkTheme),
                MenuItem::separator(),
                MenuItem::action("Zoom In", ZoomIn),
                MenuItem::action("Zoom Out", ZoomOut),
                MenuItem::action("Fit Circuit", Fit),
                MenuItem::action("Show Grid", ToggleGrid),
                MenuItem::action("Snap to Grid", ToggleSnap),
                MenuItem::action("Waveforms / Messages", ToggleScope),
            ],
        },
        Menu {
            disabled: false,
            name: "Module".into(),
            items: vec![MenuItem::action("New Module…", NewModule)],
        },
        Menu {
            disabled: false,
            name: "Gate".into(),
            items: vec![
                MenuItem::action("Rotate Clockwise", Rotate),
                MenuItem::action("Rotate Counterclockwise", RotateBack),
            ],
        },
        Menu {
            disabled: false,
            name: "Help".into(),
            items: vec![
                MenuItem::action("Getting Started", Help),
                MenuItem::action("About RGate", About),
            ],
        },
    ]);
    let modifier = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    };
    cx.bind_keys([
        KeyBinding::new(&format!("{modifier}-f"), Find, Some("RGate")),
        KeyBinding::new(&format!("{modifier}-n"), New, Some("RGate")),
        KeyBinding::new(&format!("{modifier}-o"), Open, Some("RGate")),
        KeyBinding::new(&format!("{modifier}-s"), Save, Some("RGate")),
        KeyBinding::new(&format!("{modifier}-shift-s"), SaveAs, Some("RGate")),
        KeyBinding::new(&format!("{modifier}-z"), Undo, Some("RGate")),
        KeyBinding::new(&format!("{modifier}-shift-z"), Redo, Some("RGate")),
        KeyBinding::new(&format!("{modifier}-x"), Cut, Some("RGate")),
        KeyBinding::new(&format!("{modifier}-c"), Copy, Some("RGate")),
        KeyBinding::new(&format!("{modifier}-v"), Paste, Some("RGate")),
        KeyBinding::new(&format!("{modifier}-a"), SelectAll, Some("RGate")),
        KeyBinding::new(&format!("{modifier}-0"), Fit, Some("RGate")),
        KeyBinding::new(&format!("{modifier}-q"), Quit, Some("RGate")),
    ]);
}
