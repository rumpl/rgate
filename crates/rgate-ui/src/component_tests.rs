use crate::{
    app::{GateApp, WorkspaceTab},
    commands::Command,
    components,
    theme::Theme,
};
use gpui::{Modifiers, MouseButton, TestAppContext, point, px, size};
use rgate_core::{Circuit, GateKind, Point};
use rgate_editor::Tool;

#[test]
fn sidebar_catalog_covers_every_component_once() {
    let catalog = components::catalog();
    let menu = crate::commands::entries("Components");
    assert_eq!(catalog.len(), menu.len());
    for entry in menu {
        let Some(Command::Tool(Tool::Place(kind))) = entry.command else {
            panic!("unexpected menu entry")
        };
        assert_eq!(
            catalog
                .iter()
                .filter(|component| component.kind == kind)
                .count(),
            1
        );
    }
}

#[gpui::test]
fn dragging_from_sidebar_places_one_snapped_component_and_is_undoable(cx: &mut TestAppContext) {
    cx.update(crate::commands::install);
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(Circuit::default(), None, window, cx));
    cx.simulate_resize(size(px(1240.), px(820.)));
    cx.run_until_parked();
    for theme in [Theme::Classic, Theme::Modern, Theme::Dark] {
        cx.update(|window, cx| {
            view.update(cx, |app, cx| {
                app.command(Command::SetTheme(theme), window, cx)
            })
        });
        cx.run_until_parked();
        let source = cx.debug_bounds("component-AND").unwrap().center();
        let (target, expected) = view.read_with(cx, |app, _| {
            let screen = Point::new(183., 117.);
            (
                app.canvas_bounds.origin + point(px(screen.x), px(screen.y)),
                app.editor.snapped(app.editor.viewport.to_world(screen)),
            )
        });
        cx.simulate_mouse_down(source, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(
            source + point(px(8.), px(0.)),
            MouseButton::Left,
            Modifiers::default(),
        );
        cx.run_until_parked();
        cx.update(|_, cx| assert!(cx.has_active_drag()));
        assert_eq!(
            view.read_with(cx, |app, _| app.editor.module().gates.len()),
            0
        );
        cx.simulate_mouse_move(target, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(target, MouseButton::Left, Modifiers::default());
        cx.run_until_parked();
        view.read_with(cx, |app, _| {
            assert_eq!(app.editor.module().gates.len(), 1);
            let gate = &app.editor.module().gates[0];
            assert_eq!(gate.kind, GateKind::And);
            assert_eq!(gate.position, expected);
            assert_eq!(app.editor.tool, Tool::Select);
            assert!(app.editor.selection.contains(&gate.id));
            app.editor.circuit().validate().unwrap();
        });
        cx.simulate_keystrokes("cmd-z");
        assert_eq!(
            view.read_with(cx, |app, _| app.editor.module().gates.len()),
            0
        );
        assert!(!view.read_with(cx, |app, _| app.editor.is_dirty()));
    }
}

#[gpui::test]
fn component_drag_can_be_cancelled_or_dropped_outside_without_editing(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(Circuit::default(), None, window, cx));
    cx.run_until_parked();
    let source = cx.debug_bounds("component-AND").unwrap().center();
    for cancel in [true, false] {
        cx.simulate_mouse_down(source, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(
            source + point(px(10.), px(0.)),
            MouseButton::Left,
            Modifiers::default(),
        );
        cx.run_until_parked();
        if cancel {
            cx.simulate_keystrokes("escape");
        }
        let destination = if cancel {
            view.read_with(cx, |app, _| app.canvas_bounds.center())
        } else {
            point(px(50.), px(15.))
        };
        cx.simulate_mouse_move(destination, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(destination, MouseButton::Left, Modifiers::default());
        cx.run_until_parked();
        assert_eq!(
            view.read_with(cx, |app, _| app.editor.module().gates.len()),
            0
        );
        assert!(!view.read_with(cx, |app, _| app.editor.is_dirty()));
    }
}

#[gpui::test]
fn sidebar_scroll_exposes_all_components_and_does_not_pan_canvas(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(Circuit::default(), None, window, cx));
    cx.run_until_parked();
    let original_pan = view.read_with(cx, |app, _| app.editor.viewport.pan);
    let bounds = cx.debug_bounds("component-list").unwrap();
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: bounds.center(),
        delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-2000.))),
        ..Default::default()
    });
    cx.run_until_parked();
    let comment = cx.debug_bounds("component-Comment").unwrap().center();
    assert!(bounds.contains(&comment));
    cx.simulate_click(comment, Modifiers::default());
    assert_eq!(
        view.read_with(cx, |app, _| app.editor.tool.clone()),
        Tool::Place(GateKind::Comment)
    );
    assert_eq!(
        view.read_with(cx, |app, _| app.editor.viewport.pan),
        original_pan
    );
}

#[gpui::test]
fn component_drop_is_blocked_by_dialogs_and_interface_tab(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(Circuit::default(), None, window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            let position = app.canvas_bounds.center();
            app.tab = WorkspaceTab::Interface;
            app.drop_component(GateKind::And, position, window, cx);
            app.tab = WorkspaceTab::Edit;
            app.command(Command::Help, window, cx);
            app.drop_component(GateKind::And, position, window, cx);
            assert!(app.editor.module().gates.is_empty());
            assert!(!app.editor.is_dirty());
        })
    });
}

#[gpui::test]
fn module_definitions_are_available_for_drag_and_drop(cx: &mut TestAppContext) {
    let mut circuit = Circuit::default();
    circuit.modules.push(rgate_core::Module::new("child"));
    let (view, cx) = cx.add_window_view(|window, cx| GateApp::new(circuit, None, window, cx));
    cx.run_until_parked();
    let bounds = cx.debug_bounds("component-list").unwrap();
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: bounds.center(),
        delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-5000.))),
        ..Default::default()
    });
    cx.run_until_parked();
    let source = cx.debug_bounds("component-child").unwrap().center();
    assert!(bounds.contains(&source));
    let target = view.read_with(cx, |app, _| app.canvas_bounds.center());
    cx.simulate_mouse_down(source, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        source + point(px(10.), px(0.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_move(target, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(target, MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
    view.read_with(cx, |app, _| {
        assert_eq!(app.editor.module().gates.len(), 1);
        assert_eq!(
            app.editor.module().gates[0].kind,
            GateKind::Module("child".into())
        );
        app.editor.circuit().validate().unwrap();
    });
}

#[gpui::test]
fn nested_adder_example_opens_and_simulates_from_file_menu(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(Circuit::default(), None, window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.command(Command::Example("adder8"), window, cx);
            assert_eq!(app.editor.circuit().modules.len(), 5);
            app.command(Command::Step, window, cx);
            app.simulation.as_mut().unwrap().advance(500).unwrap();
        })
    });
    cx.run_until_parked();
    view.read_with(cx, |app, _| {
        assert_eq!(
            app.simulation
                .as_ref()
                .unwrap()
                .value(rgate_core::NetId(4))
                .unwrap()
                .to_u64(),
            Some(0x5c)
        );
        assert!(
            app.messages
                .iter()
                .all(|message| !message.starts_with("Error:"))
        );
    });
}

#[gpui::test]
fn components_sidebar_divider_expands_and_shrinks_without_editing_circuit(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(Circuit::default(), None, window, cx));
    cx.simulate_resize(size(px(1240.0), px(820.0)));
    cx.run_until_parked();
    let original = cx.debug_bounds("component-sidebar").unwrap();
    assert_eq!(original.size.width, px(205.0));
    let divider = cx.debug_bounds("components-splitter").unwrap().center();
    cx.simulate_mouse_down(divider, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        divider - point(px(100.0), px(0.0)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_up(
        divider - point(px(100.0), px(0.0)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.run_until_parked();
    assert_eq!(
        cx.debug_bounds("component-sidebar").unwrap().size.width,
        px(305.0)
    );
    let divider = cx.debug_bounds("components-splitter").unwrap().center();
    cx.simulate_mouse_down(divider, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        divider + point(px(75.0), px(0.0)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_up(
        divider + point(px(75.0), px(0.0)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.run_until_parked();
    assert_eq!(
        cx.debug_bounds("component-sidebar").unwrap().size.width,
        px(230.0)
    );
    assert!(view.read_with(cx, |app, _| app.drag.is_none()));
    assert!(!view.read_with(cx, |app, _| app.editor.is_dirty()));
}

#[test]
fn search_matches_labels_gate_names_categories_and_multiple_words() {
    let catalog = components::catalog();
    assert!(
        catalog
            .iter()
            .any(|component| component.kind == GateKind::Ram
                && components::matches_search(component, "ram"))
    );
    assert!(
        catalog
            .iter()
            .any(|component| component.kind == GateKind::Ram
                && components::matches_search(component, "memory RAM"))
    );
    assert!(
        catalog
            .iter()
            .any(|component| component.kind == GateKind::ShiftLeft
                && components::matches_search(component, "lshift"))
    );
    assert!(
        !catalog
            .iter()
            .any(|component| components::matches_search(component, "not_a_component"))
    );
}

#[gpui::test]
fn component_search_updates_live_and_typing_does_not_trigger_canvas_tools(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(Circuit::default(), None, window, cx));
    cx.simulate_resize(size(px(1240.0), px(820.0)));
    cx.run_until_parked();
    let input = cx.debug_bounds("component-search").unwrap().center();
    cx.simulate_click(input, Modifiers::default());
    cx.run_until_parked();
    cx.simulate_input("RAM");
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |app, cx| app.component_search.read(cx).value.clone()),
        "RAM"
    );
    assert_eq!(
        view.read_with(cx, |app, _| app.editor.tool.clone()),
        Tool::Select
    );
    assert!(cx.debug_bounds("component-RAM").is_some());
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("nothing_matches");
    cx.run_until_parked();
    assert!(cx.debug_bounds("component-search-empty").is_some());
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(view.read_with(cx, |app, cx| app.component_search.read(cx).value.is_empty()));
    assert!(!view.read_with(cx, |app, _| app.editor.is_dirty()));
}
