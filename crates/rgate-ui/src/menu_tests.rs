use crate::{app::GateApp, commands::Command, theme::Theme};
use gpui::{Modifiers, MouseButton, TestAppContext, point, px, size};
use rgate_core::{Point, demo};

fn move_pointer(cx: &mut gpui::VisualTestContext, position: gpui::Point<gpui::Pixels>) {
    cx.simulate_event(gpui::MouseMoveEvent {
        position,
        ..Default::default()
    });
    cx.run_until_parked();
}

#[gpui::test]
fn dropdowns_anchor_to_labels_and_switch_on_hover_in_both_themes(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(demo::full_adder(), None, window, cx));
    cx.simulate_resize(size(px(1240.), px(820.)));
    cx.run_until_parked();
    assert!(cx.debug_bounds("menu-Make").is_none());
    for theme in [Theme::Classic, Theme::Modern, Theme::Dark] {
        cx.update(|window, cx| {
            view.update(cx, |app, cx| {
                app.command(Command::SetTheme(theme), window, cx)
            });
        });
        cx.run_until_parked();
        let tool = cx.debug_bounds("menu-Tool").unwrap();
        move_pointer(cx, tool.center());
        assert!(view.read_with(cx, |app, _| app.popup.is_none()));
        cx.simulate_click(tool.center(), Modifiers::default());
        cx.run_until_parked();
        for name in [
            "Tool", "Simulate", "Module", "Gate", "Arrange", "Help", "File",
        ] {
            let label = cx
                .debug_bounds(Box::leak(format!("menu-{name}").into_boxed_str()))
                .unwrap();
            move_pointer(cx, label.center());
            assert!(view.read_with(cx, |app, _| {
                app.popup.as_ref().is_some_and(|popup| popup.menu == name)
            }));
            let dropdown = cx.debug_bounds("popup-items").unwrap();
            assert!((f32::from(dropdown.left() - label.left())).abs() <= 1.0);
            assert!((f32::from(dropdown.top() - label.bottom())).abs() <= 1.0);
            move_pointer(cx, dropdown.center());
            assert!(view.read_with(cx, |app, _| {
                app.popup.as_ref().is_some_and(|popup| popup.menu == name)
            }));
        }
        let file = cx.debug_bounds("menu-File").unwrap().center();
        cx.simulate_click(file, Modifiers::default());
        cx.run_until_parked();
        assert!(view.read_with(cx, |app, _| app.popup.is_none()));
        cx.simulate_click(tool.center(), Modifiers::default());
        cx.run_until_parked();
        let edit = cx.debug_bounds("menu-Edit").unwrap().center();
        cx.simulate_click(edit, Modifiers::default());
        cx.run_until_parked();
        assert!(view.read_with(cx, |app, _| {
            app.popup.as_ref().is_some_and(|popup| popup.menu == "Edit")
        }));
        cx.simulate_keystrokes("escape");
        assert!(view.read_with(cx, |app, _| app.popup.is_none()));
    }
}

#[gpui::test]
fn menus_and_dialogs_do_not_pan_or_zoom_underlying_canvas(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(demo::full_adder(), None, window, cx));
    cx.simulate_resize(size(px(1240.), px(820.)));
    cx.run_until_parked();
    let viewport = view.read_with(cx, |app, _| {
        (app.editor.viewport.pan, app.editor.viewport.zoom)
    });
    let menu = cx.debug_bounds("menu-File").unwrap().center();
    cx.simulate_click(menu, Modifiers::default());
    cx.run_until_parked();
    let dropdown = cx.debug_bounds("popup-items").unwrap();
    let first_before = cx.debug_bounds("popup-item-0").unwrap().top();
    for modifiers in [
        Modifiers::default(),
        Modifiers {
            control: true,
            ..Default::default()
        },
    ] {
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: dropdown.center(),
            delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-100.))),
            modifiers,
            ..Default::default()
        });
        cx.run_until_parked();
        assert_eq!(
            view.read_with(cx, |app, _| (
                app.editor.viewport.pan,
                app.editor.viewport.zoom
            )),
            viewport
        );
    }
    assert!(cx.debug_bounds("popup-item-0").unwrap().top() <= first_before);
    cx.simulate_click(point(px(1000.), px(600.)), Modifiers::default());
    cx.run_until_parked();
    assert!(view.read_with(cx, |app, _| app.popup.is_none()));
    cx.update(|window, cx| {
        view.update(cx, |app, cx| app.command(Command::Help, window, cx));
    });
    cx.run_until_parked();
    let dialog = cx.debug_bounds("dialog-content").unwrap().center();
    for position in [dialog, point(px(1100.), px(500.))] {
        for control in [false, true] {
            cx.simulate_event(gpui::ScrollWheelEvent {
                position,
                delta: gpui::ScrollDelta::Pixels(point(px(15.), px(-80.))),
                modifiers: Modifiers {
                    control,
                    ..Default::default()
                },
                ..Default::default()
            });
            cx.run_until_parked();
        }
    }
    assert_eq!(
        view.read_with(cx, |app, _| (
            app.editor.viewport.pan,
            app.editor.viewport.zoom
        )),
        viewport
    );
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    let canvas = view.read_with(cx, |app, _| app.canvas_bounds.center());
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: canvas,
        delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-40.))),
        ..Default::default()
    });
    cx.run_until_parked();
    assert_ne!(
        view.read_with(cx, |app, _| app.editor.viewport.pan),
        viewport.0
    );

    cx.simulate_mouse_down(canvas, MouseButton::Right, Modifiers::default());
    cx.simulate_mouse_up(canvas, MouseButton::Right, Modifiers::default());
    cx.run_until_parked();
    assert!(view.read_with(cx, |app, _| {
        app.popup
            .as_ref()
            .is_some_and(|popup| popup.menu == "Canvas")
    }));
    let tool = cx.debug_bounds("menu-Tool").unwrap().center();
    move_pointer(cx, tool);
    assert!(view.read_with(cx, |app, _| {
        app.popup
            .as_ref()
            .is_some_and(|popup| popup.menu == "Canvas")
    }));
    assert_ne!(
        view.read_with(cx, |app, _| app.editor.viewport.pan),
        Point::ZERO
    );
}

#[gpui::test]
fn trackpad_pinch_zoom_is_pointer_anchored_and_respects_overlays(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(demo::full_adder(), None, window, cx));
    cx.simulate_resize(size(px(1240.), px(820.)));
    cx.run_until_parked();
    let (position, anchor, world, original_zoom) = view.read_with(cx, |app, _| {
        let anchor = Point::new(250., 180.);
        (
            app.canvas_bounds.origin + point(px(anchor.x), px(anchor.y)),
            anchor,
            app.editor.viewport.to_world(anchor),
            app.editor.viewport.zoom,
        )
    });
    // Use upstream native pinch events, without synthesized modifier-scroll.
    for (magnification, expected_zoom) in [(0.25_f32, original_zoom * 1.25), (-0.2, original_zoom)]
    {
        cx.simulate_event(gpui::PinchEvent {
            position,
            delta: magnification,
            ..Default::default()
        });
        cx.run_until_parked();
        view.read_with(cx, |app, _| {
            assert!((app.editor.viewport.zoom - expected_zoom).abs() < 0.0001);
            assert!(app.editor.viewport.to_world(anchor).distance(world) < 0.0001);
            assert!(!app.editor.is_dirty());
        });
    }
    for (delta, phase) in [
        (f32::NAN, gpui::TouchPhase::Moved),
        (-1.0, gpui::TouchPhase::Moved),
        (0.5, gpui::TouchPhase::Cancelled),
        (0.5, gpui::TouchPhase::Ended),
    ] {
        cx.simulate_event(gpui::PinchEvent {
            position,
            delta,
            phase,
            ..Default::default()
        });
        cx.run_until_parked();
        assert!(
            (view.read_with(cx, |app, _| app.editor.viewport.zoom) - original_zoom).abs() < 0.0001
        );
    }
    let menu = cx.debug_bounds("menu-File").unwrap().center();
    cx.simulate_click(menu, Modifiers::default());
    cx.run_until_parked();
    for position in [position, cx.debug_bounds("popup-items").unwrap().center()] {
        cx.simulate_event(gpui::PinchEvent {
            position,
            delta: 0.5,
            ..Default::default()
        });
        cx.run_until_parked();
        assert!(
            (view.read_with(cx, |app, _| app.editor.viewport.zoom) - original_zoom).abs() < 0.0001
        );
    }
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    for command in [Command::Help, Command::About] {
        cx.update(|window, cx| view.update(cx, |app, cx| app.command(command, window, cx)));
        cx.run_until_parked();
        cx.simulate_event(gpui::PinchEvent {
            position,
            delta: 0.25,
            ..Default::default()
        });
        cx.run_until_parked();
        view.read_with(cx, |app, _| {
            assert!((app.editor.viewport.zoom - original_zoom).abs() < 0.0001);
        });
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
    }
    move_pointer(cx, position);
    for (delta, expected_zoom) in [(100.0, 6.0), (-0.999, 0.25)] {
        cx.simulate_event(gpui::PinchEvent {
            position,
            delta,
            ..Default::default()
        });
        cx.run_until_parked();
        view.read_with(cx, |app, _| {
            assert_eq!(app.editor.viewport.zoom, expected_zoom);
            assert!(app.editor.viewport.to_world(anchor).distance(world) < 0.001);
            assert!(!app.editor.is_dirty());
        });
    }
}

#[test]
fn file_menu_contains_document_actions_not_examples() {
    let entries = crate::commands::entries("File");
    assert!(
        !entries
            .iter()
            .any(|entry| matches!(entry.command, Some(Command::Example(_))))
    );
    assert!(
        entries
            .iter()
            .any(|entry| matches!(entry.command, Some(Command::Open)))
    );
}
