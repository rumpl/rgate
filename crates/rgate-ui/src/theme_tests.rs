use crate::{
    app::{GateApp, WorkspaceTab},
    commands::{self, Command},
    theme::Theme,
};
use gpui::TestAppContext;
use rgate_core::{Logic, NetId, Signal, demo};

#[test]
fn palettes_preserve_classic_and_distinguish_signal_states() {
    assert_eq!(Theme::default(), Theme::Classic);
    assert_eq!(Theme::Classic.palette().chrome, 0xd9d9d9);
    assert_eq!(Theme::Classic.palette().gate, 0x0000ff);
    assert_eq!(Theme::Classic.palette().radius, 0.0);
    assert!(Theme::Modern.palette().radius > 0.0);
    for theme in [Theme::Classic, Theme::Modern, Theme::Dark] {
        let palette = theme.palette();
        let colors = [Logic::Low, Logic::High, Logic::Unknown, Logic::HighZ]
            .map(|logic| palette.signal_color(&Signal::filled(1, logic)));
        for (index, color) in colors.iter().enumerate() {
            assert_ne!(*color, gpui::rgb(palette.panel).into());
            for other in &colors[index + 1..] {
                assert_ne!(color, other);
            }
        }
        let mixed = Signal::from_bits(vec![Logic::HighZ, Logic::Unknown]);
        assert_eq!(
            palette.signal_color(&mixed),
            gpui::rgb(palette.unknown).into()
        );
    }
}

#[test]
fn view_menu_exposes_all_themes() {
    let themes = commands::entries("View")
        .into_iter()
        .filter_map(|entry| match entry.command {
            Some(Command::SetTheme(theme)) => Some(theme),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(themes, vec![Theme::Classic, Theme::Modern, Theme::Dark]);
}

#[gpui::test]
fn switching_themes_renders_all_surfaces_without_changing_document(cx: &mut TestAppContext) {
    cx.update(commands::install);
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(demo::full_adder(), None, window, cx));
    cx.run_until_parked();
    let original = view.read_with(cx, |app, _| {
        serde_json::to_string(app.editor.circuit()).unwrap()
    });
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.toggle_probe(NetId(7), cx);
            app.command(Command::SetTheme(Theme::Modern), window, cx);
        });
    });
    cx.run_until_parked();
    view.read_with(cx, |app, cx| {
        assert_eq!(app.theme, Theme::Modern);
        assert_eq!(*cx.global::<Theme>(), Theme::Modern);
        assert_eq!(app.scene().palette.wire, Theme::Modern.palette().wire);
        assert_eq!(
            serde_json::to_string(app.editor.circuit()).unwrap(),
            original
        );
        assert!(!app.editor.is_dirty());
        assert!(app.simulation.is_some());
        assert!(app.probes.contains(&NetId(7)));
        assert!(app.bottom_scope);
    });
    cx.update(|_, cx| {
        view.update(cx, |app, cx| {
            app.tab = WorkspaceTab::Interface;
            cx.notify();
        });
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.editor.select(rgate_core::GateId(1), false);
            app.command(Command::Properties, window, cx);
        });
    });
    cx.run_until_parked();
    view.read_with(cx, |app, _| assert!(app.dialog.is_some()));
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.dialog = None;
            app.command(Command::SetTheme(Theme::Classic), window, cx);
        });
    });
    cx.run_until_parked();
    view.read_with(cx, |app, cx| {
        assert_eq!(app.theme, Theme::Classic);
        assert_eq!(*cx.global::<Theme>(), Theme::Classic);
        assert!(!app.editor.is_dirty());
        assert!(app.simulation.is_some());
    });
}

#[gpui::test]
fn vector_artwork_renders_all_gates_rotations_and_led_modes(cx: &mut TestAppContext) {
    use rgate_core::{Circuit, Gate, GateId, GateKind, LedDisplay, Point};

    let mut circuit = Circuit::default();
    let module = circuit.module_mut("main").unwrap();
    let mut id = 1;
    for entry in commands::entries("Components") {
        let Some(Command::Tool(rgate_editor::Tool::Place(kind))) = entry.command else {
            continue;
        };
        for rotation in 0..4 {
            let mut gate = Gate::new(
                GateId(id),
                kind.clone(),
                Point::new(((id - 1) % 12) as f32 * 120., ((id - 1) / 12) as f32 * 110.),
            );
            gate.rotation = rotation;
            module.gates.push(gate);
            id += 1;
        }
    }
    for mode in [
        LedDisplay::Bar,
        LedDisplay::Hex,
        LedDisplay::Decimal,
        LedDisplay::SevenSegment,
    ] {
        for rotation in 0..4 {
            let mut gate = Gate::new(
                GateId(id),
                GateKind::Led,
                Point::new(id as f32 * 100., 1500.),
            );
            gate.config.led_display = mode;
            gate.width = 8;
            gate.initial = Signal::from_u64(0, 8);
            gate.rotation = rotation;
            module.gates.push(gate);
            id += 1;
        }
    }
    circuit.validate().unwrap();
    let original = serde_json::to_string(&circuit).unwrap();
    let (view, cx) = cx.add_window_view(|window, cx| GateApp::new(circuit, None, window, cx));
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.command(Command::SetTheme(Theme::Modern), window, cx)
        });
    });
    cx.run_until_parked();
    for theme in [Theme::Classic, Theme::Modern, Theme::Dark] {
        cx.update(|window, cx| {
            view.update(cx, |app, cx| {
                app.command(Command::SetTheme(theme), window, cx)
            })
        });
        for zoom in [0.5, 1., 2., 6.] {
            cx.update(|_, cx| {
                view.update(cx, |app, cx| {
                    app.editor.viewport.zoom = zoom;
                    app.need_fit = false;
                    cx.notify();
                });
            });
            cx.run_until_parked();
        }
    }
    view.read_with(cx, |app, _| {
        assert_eq!(app.scene().theme, Theme::Dark);
        assert_eq!(
            serde_json::to_string(app.editor.circuit()).unwrap(),
            original
        );
        assert!(!app.editor.is_dirty());
    });
}

#[test]
fn dark_palette_keeps_text_and_signals_readable() {
    fn luminance(color: u32) -> f64 {
        let channel = |shift| {
            let value = f64::from((color >> shift) & 255_u32) / 255.;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
    }
    let palette = Theme::Dark.palette();
    for background in [
        palette.panel,
        palette.chrome,
        palette.light,
        palette.accent,
        palette.hover,
        palette.selection,
    ] {
        for foreground in [palette.text, palette.muted] {
            assert!((luminance(foreground) + 0.05) / (luminance(background) + 0.05) >= 4.5);
        }
    }
    for color in [
        palette.gate,
        palette.wire,
        palette.bus,
        palette.low,
        palette.high,
        palette.unknown,
        palette.float,
        palette.module,
    ] {
        assert!((luminance(color) + 0.05) / (luminance(palette.panel) + 0.05) >= 4.5);
    }
}

#[gpui::test]
fn dark_theme_action_renders_waveforms_dialogs_and_preserves_simulation(cx: &mut TestAppContext) {
    cx.update(commands::install);
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(demo::full_adder(), None, window, cx));
    cx.run_until_parked();
    let document = view.read_with(cx, |app, _| {
        serde_json::to_string(app.editor.circuit()).unwrap()
    });
    cx.update(|_, cx| view.update(cx, |app, cx| app.toggle_probe(NetId(7), cx)));
    cx.dispatch_action(commands::DarkTheme);
    cx.run_until_parked();
    view.read_with(cx, |app, cx| {
        assert_eq!(app.theme, Theme::Dark);
        assert_eq!(*cx.global::<Theme>(), Theme::Dark);
        assert_eq!(app.scene().palette.panel, Theme::Dark.palette().panel);
        assert!(app.bottom_scope);
        assert!(app.simulation.is_some());
        assert!(!app.editor.is_dirty());
        assert_eq!(
            serde_json::to_string(app.editor.circuit()).unwrap(),
            document
        );
    });
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.editor.select(rgate_core::GateId(1), false);
            app.command(Command::Properties, window, cx);
        })
    });
    cx.run_until_parked();
    assert!(view.read_with(cx, |app, _| app.dialog.is_some()));
    cx.simulate_keystrokes("escape");
    cx.dispatch_action(commands::ClassicTheme);
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |app, _| app.theme), Theme::Classic);
}

#[gpui::test]
fn modules_header_controls_share_height_and_vertical_alignment(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(demo::full_adder(), None, window, cx));
    cx.run_until_parked();
    for theme in [Theme::Classic, Theme::Modern, Theme::Dark] {
        for flat in [false, true] {
            cx.update(|window, cx| {
                view.update(cx, |app, cx| {
                    app.module_list = flat;
                    app.command(Command::SetTheme(theme), window, cx);
                })
            });
            cx.run_until_parked();
            let tree = cx.debug_bounds("modules-tree").unwrap();
            let list = cx.debug_bounds("modules-flat").unwrap();
            let title = cx.debug_bounds("modules-heading").unwrap();
            assert_eq!(tree.top(), title.top());
            assert_eq!(list.top(), title.top());
            assert_eq!(tree.bottom(), title.bottom());
            assert_eq!(list.bottom(), title.bottom());
            assert_eq!(tree.size.width, list.size.width);
        }
    }
}
