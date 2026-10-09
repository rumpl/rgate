use crate::{
    app::{Dialog, GateApp},
    commands::{self, Command},
};
use gpui::{AppContext, Focusable, Modifiers, MouseButton, TestAppContext, point, px, size};
use rgate_core::{GateId, GateKind, NetId, Point, demo};
use rgate_editor::Tool;

#[gpui::test]
fn renders_and_keyboard_placement_is_undoable(cx: &mut TestAppContext) {
    cx.update(commands::install);
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(demo::full_adder(), None, window, cx));
    cx.simulate_resize(size(px(1240.0), px(800.0)));
    cx.run_until_parked();
    let original = view.read_with(cx, |app, _| {
        assert!(app.canvas_bounds.size.width > px(600.0));
        assert!(app.canvas_bounds.size.height > px(300.0));
        app.editor.module().gates.len()
    });
    cx.simulate_keystrokes("a");
    assert_eq!(
        view.read_with(cx, |app, _| app.editor.tool.clone()),
        Tool::Place(GateKind::And)
    );
    let position = view.read_with(cx, |app, _| {
        app.canvas_bounds.origin + point(px(120.0), px(120.0))
    });
    cx.simulate_click(position, Modifiers::default());
    assert_eq!(
        view.read_with(cx, |app, _| app.editor.module().gates.len()),
        original + 1
    );
    cx.simulate_keystrokes("escape cmd-z");
    assert_eq!(
        view.read_with(cx, |app, _| app.editor.module().gates.len()),
        original
    );
    assert!(!view.read_with(cx, |app, _| app.editor.is_dirty()));
}

#[gpui::test]
fn simulator_switch_click_and_scope_probe_work(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(demo::full_adder(), None, window, cx));
    cx.run_until_parked();
    cx.simulate_keystrokes("space");
    assert!(view.read_with(cx, |app, _| app.running));
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.command(Command::PlayPause, window, cx);
            app.toggle_probe(NetId(7), cx);
            app.simulation.as_mut().unwrap().advance(40).unwrap();
        })
    });
    let (position, previous) = view.read_with(cx, |app, _| {
        let gate = app.editor.module().gate(GateId(1)).unwrap();
        let p = app.editor.viewport.to_screen(gate.position);
        (
            app.canvas_bounds.origin + point(px(p.x), px(p.y)),
            app.simulation
                .as_ref()
                .unwrap()
                .gate_value(gate.id)
                .unwrap()
                .to_u64(),
        )
    });
    cx.simulate_click(position, Modifiers::default());
    view.read_with(cx, |app, _| {
        assert_ne!(
            app.simulation
                .as_ref()
                .unwrap()
                .gate_value(GateId(1))
                .unwrap()
                .to_u64(),
            previous
        );
        assert!(
            app.simulation
                .as_ref()
                .unwrap()
                .traces()
                .contains_key(&NetId(7))
        );
        assert!(!app.running);
        assert!(app.bottom_scope);
    });
}

#[gpui::test]
fn property_text_input_updates_gate_not_canvas_shortcuts(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(demo::full_adder(), None, window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.editor.select(GateId(1), false);
            app.command(Command::Properties, window, cx);
        })
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("input_alpha");
    view.read_with(cx, |app, cx| {
        let Some(Dialog::Properties(dialog)) = &app.dialog else {
            panic!("properties did not open");
        };
        assert_eq!(dialog.name.read(cx).value, "input_alpha");
        assert_eq!(app.editor.module().gates.len(), 12);
    });
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |app, _| {
        assert!(app.dialog.is_none());
        assert_eq!(
            app.editor.module().gate(GateId(1)).unwrap().name,
            "input_alpha"
        );
        assert!(app.editor.is_dirty());
    });
}

#[gpui::test]
fn dragging_keeps_wire_endpoints_and_commits_once(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(demo::full_adder(), None, window, cx));
    cx.run_until_parked();
    let (position, original) = view.read_with(cx, |app, _| {
        let gate = app.editor.module().gate(GateId(1)).unwrap();
        let p = app.editor.viewport.to_screen(gate.position);
        (
            app.canvas_bounds.origin + point(px(p.x), px(p.y)),
            gate.position,
        )
    });
    cx.simulate_mouse_down(position, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        position + point(px(25.0), px(15.0)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_up(
        position + point(px(25.0), px(15.0)),
        MouseButton::Left,
        Modifiers::default(),
    );
    view.read_with(cx, |app, _| {
        assert_ne!(
            app.editor.module().gate(GateId(1)).unwrap().position,
            original
        );
        app.editor.circuit().validate().unwrap();
    });
    cx.update(|window, cx| view.update(cx, |app, cx| app.command(Command::Undo, window, cx)));
    assert_eq!(
        view.read_with(cx, |app, _| app
            .editor
            .module()
            .gate(GateId(1))
            .unwrap()
            .position),
        original
    );
}

#[gpui::test]
fn menus_and_adjustable_counter_render_without_missing_art(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(demo::full_adder(), None, window, cx));
    cx.run_until_parked();
    cx.simulate_click(point(px(27.0), px(13.0)), Modifiers::default());
    assert!(view.read_with(cx, |app, _| app.popup.is_some()));
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.command(Command::Example("counter"), window, cx)
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.command(Command::ClockStep, window, cx);
            app.toggle_probe(
                app.editor
                    .module()
                    .nets
                    .iter()
                    .find(|net| net.name == "count")
                    .unwrap()
                    .id,
                cx,
            );
        })
    });
    cx.run_until_parked();
    view.read_with(cx, |app, _| {
        assert!(app.simulation.is_some());
        assert!(app.simulation.as_ref().unwrap().time() >= 100);
        assert!(
            app.messages
                .iter()
                .all(|message| !message.starts_with("Error:"))
        );
    });
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.editor.viewport.pan = Point::ZERO;
            app.command(Command::About, window, cx);
        })
    });
    cx.run_until_parked();
}

#[gpui::test]
fn probing_does_not_disable_editing_and_mutation_invalidates_preview(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(demo::full_adder(), None, window, cx));
    cx.run_until_parked();
    cx.update(|_, cx| view.update(cx, |app, cx| app.toggle_probe(NetId(7), cx)));
    let position = view.read_with(cx, |app, _| {
        let p = app
            .editor
            .viewport
            .to_screen(app.editor.module().gate(GateId(1)).unwrap().position);
        app.canvas_bounds.origin + point(px(p.x), px(p.y))
    });
    cx.simulate_mouse_down(position, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        position + point(px(30.0), px(20.0)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_up(
        position + point(px(30.0), px(20.0)),
        MouseButton::Left,
        Modifiers::default(),
    );
    view.read_with(cx, |app, _| {
        assert!(app.editor.is_dirty());
        assert!(app.simulation.is_none());
        assert_eq!(app.tab, crate::app::WorkspaceTab::Edit);
        app.editor.circuit().validate().unwrap();
    });
}

#[gpui::test]
fn cancelling_discard_prompt_keeps_document_dirty(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(demo::full_adder(), None, window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.editor
                .place(GateKind::Not, Point::new(300.0, 300.0))
                .unwrap();
            app.command(Command::Open, window, cx);
        })
    });
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert!(view.read_with(cx, |app, _| app.editor.is_dirty()));
}

#[gpui::test]
fn dialog_blocks_native_edit_commands_and_unicode_input_works(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(demo::full_adder(), None, window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.editor.select(GateId(1), false);
            app.command(Command::Properties, window, cx);
            app.command(Command::Delete, window, cx);
        })
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("entrée⚡");
    cx.simulate_keystrokes("backspace");
    view.read_with(cx, |app, cx| {
        let Some(Dialog::Properties(dialog)) = &app.dialog else {
            panic!("missing dialog");
        };
        assert_eq!(dialog.name.read(cx).value, "entrée");
        assert!(app.editor.module().gate(GateId(1)).is_some());
    });
    cx.simulate_keystrokes("escape");
    assert!(!view.read_with(cx, |app, _| app.editor.is_dirty()));
}

#[gpui::test]
fn splitter_can_be_dragged_across_canvas_without_double_processing(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(demo::full_adder(), None, window, cx));
    cx.run_until_parked();
    let bounds = cx.debug_bounds("sidebar-splitter").unwrap();
    let position = bounds.center();
    cx.simulate_mouse_down(position, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        position + point(px(35.0), px(0.0)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_up(
        position + point(px(35.0), px(0.0)),
        MouseButton::Left,
        Modifiers::default(),
    );
    let changed = view.read_with(cx, |app, _| app.sidebar_width);
    assert!((changed - 245.0).abs() < 8.0, "sidebar width: {changed}");
    assert!(view.read_with(cx, |app, _| app.drag.is_none()));
}

#[gpui::test]
fn mouse_wiring_connects_pins_then_undo_disconnects(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        GateApp::new(rgate_core::Circuit::default(), None, window, cx)
    });
    cx.run_until_parked();
    let (source, target) = cx.update(|_, cx| {
        view.update(cx, |app, _| {
            let a = app
                .editor
                .place(GateKind::Switch, Point::new(60.0, 60.0))
                .unwrap();
            let b = app
                .editor
                .place(GateKind::Not, Point::new(150.0, 60.0))
                .unwrap();
            app.editor.set_tool(Tool::Wire);
            app.need_fit = true;
            (
                rgate_core::PinRef::new(a, "Z"),
                rgate_core::PinRef::new(b, "I"),
            )
        })
    });
    cx.update(|_, cx| view.update(cx, |_, cx| cx.notify()));
    cx.run_until_parked();
    let screen_pin = |pin: &rgate_core::PinRef, cx: &gpui::VisualTestContext| {
        view.read_with(cx, |app, _| {
            let p = app
                .editor
                .viewport
                .to_screen(app.editor.module().pin_position(pin).unwrap());
            app.canvas_bounds.origin + point(px(p.x), px(p.y))
        })
    };
    cx.simulate_click(screen_pin(&source, cx), Modifiers::default());
    assert!(view.read_with(cx, |app, _| app.editor.draft.is_some()));
    cx.simulate_click(screen_pin(&target, cx), Modifiers::default());
    view.read_with(cx, |app, _| {
        assert!(app.editor.draft.is_none());
        assert_eq!(app.editor.module().wires.len(), 1);
        assert_eq!(
            app.editor.module().pin(&source).unwrap().net,
            app.editor.module().pin(&target).unwrap().net
        );
        app.editor.circuit().validate().unwrap();
    });
    cx.update(|window, cx| view.update(cx, |app, cx| app.command(Command::Undo, window, cx)));
    assert!(view.read_with(cx, |app, _| app.editor.module().wires.is_empty()));
}

#[gpui::test]
fn create_module_dialog_switches_view_and_preserves_other_modules(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(demo::full_adder(), None, window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.toggle_probe(NetId(7), cx);
            app.editor.viewport.pan = Point::new(900.0, 800.0);
            app.command(Command::NewModule, window, cx);
        })
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    view.read_with(cx, |app, _| {
        assert_eq!(app.editor.active_module(), "module2");
        assert_eq!(app.editor.circuit().module("main").unwrap().gates.len(), 12);
        assert!(app.editor.module().gates.is_empty());
        assert!(app.probes.is_empty());
        assert!(app.simulation.is_none());
        assert_eq!(app.editor.viewport, rgate_editor::Viewport::default());
    });
    assert!(cx.debug_bounds("module-module2").is_some());
    assert!(cx.debug_bounds("nets-empty").is_some());
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.editor
                .place(GateKind::And, Point::new(80.0, 80.0))
                .unwrap();
            let circuit = rgate_format::native::decode(
                &rgate_format::native::encode(app.editor.circuit()).unwrap(),
            )
            .unwrap();
            assert_eq!(circuit.module("module2").unwrap().gates.len(), 1);
            app.command(Command::NewModule, window, cx);
            let Some(Dialog::NewModule { name, .. }) = &app.dialog else {
                panic!("missing new module dialog");
            };
            assert_eq!(name.read(cx).value, "module3");
        })
    });
    cx.simulate_keystrokes("escape");
    cx.update(|_, cx| view.update(cx, |app, cx| app.switch_module("main".into(), cx)));
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |app, _| app.editor.module().nets.len()),
        8
    );
    assert_eq!(
        view.read_with(cx, |app, _| app.editor.module().gates.len()),
        12
    );
    cx.update(|_, cx| view.update(cx, |app, cx| app.switch_module("module2".into(), cx)));
    assert_eq!(
        view.read_with(cx, |app, _| app.editor.module().gates.len()),
        1
    );
}

#[gpui::test]
fn duplicate_module_name_shows_error_without_mutating_document(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(demo::full_adder(), None, window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| view.update(cx, |app, cx| app.command(Command::NewModule, window, cx)));
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("main");
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |app, _| {
        let Some(Dialog::NewModule { error, .. }) = &app.dialog else {
            panic!("dialog closed on invalid name");
        };
        assert!(error.as_ref().unwrap().contains("already exists"));
        assert_eq!(app.editor.circuit().modules.len(), 1);
        assert!(!app.editor.is_dirty());
    });
}

#[gpui::test]
fn hierarchy_sidebar_places_instances_expands_and_navigates(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        GateApp::new(demo::hierarchical_inverters(), None, window, cx)
    });
    cx.simulate_resize(size(px(1240.0), px(820.0)));
    cx.run_until_parked();
    let collapsed = cx.debug_bounds("expand-main/u0").unwrap().center();
    cx.simulate_click(collapsed, Modifiers::default());
    view.read_with(cx, |app, _| {
        let rows = app.editor.circuit().hierarchy_rows(&app.collapsed_modules);
        assert!(!rows.iter().any(|row| row.path == "main/u0/inner"));
        assert!(rows.iter().any(|row| row.path == "main/u1/inner"));
    });
    let plus = cx.debug_bounds("instantiate-main/u1").unwrap().center();
    cx.simulate_click(plus, Modifiers::default());
    assert_eq!(
        view.read_with(cx, |app, _| app.editor.tool.clone()),
        Tool::Place(GateKind::Module("wrapped_inverter".into()))
    );
    assert_eq!(
        view.read_with(cx, |app, _| app.editor.active_module().to_owned()),
        "main"
    );
    let position = view.read_with(cx, |app, _| {
        app.canvas_bounds.origin + point(px(500.0), px(350.0))
    });
    cx.simulate_click(position, Modifiers::default());
    let id = view.read_with(cx, |app, _| {
        let id = *app.editor.selection.first().unwrap();
        assert_eq!(app.editor.module().gate(id).unwrap().pins.len(), 2);
        assert!(
            app.editor
                .circuit()
                .hierarchy_rows(&app.collapsed_modules)
                .iter()
                .any(|row| row.module == "wrapped_inverter"
                    && row.path != "main/u0"
                    && row.path != "main/u1")
        );
        id
    });
    cx.simulate_keystrokes("escape");
    let position = view.read_with(cx, |app, _| {
        let p = app
            .editor
            .viewport
            .to_screen(app.editor.module().gate(id).unwrap().position);
        app.canvas_bounds.origin + point(px(p.x), px(p.y))
    });
    cx.simulate_event(gpui::MouseDownEvent {
        position,
        button: MouseButton::Left,
        click_count: 2,
        ..Default::default()
    });
    assert_eq!(
        view.read_with(cx, |app, _| app.editor.active_module().to_owned()),
        "wrapped_inverter"
    );
    assert!(view.read_with(cx, |app, _| app.dialog.is_none()));
}

#[gpui::test]
fn hierarchical_simulation_colors_parent_nets_and_probes(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        GateApp::new(demo::hierarchical_inverters(), None, window, cx)
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.command(Command::ClockStep, window, cx);
            app.toggle_probe(NetId(2), cx);
            assert_eq!(app.scene().values[&NetId(2)].to_u64(), Some(1));
            assert_eq!(app.scene().values[&NetId(4)].to_u64(), Some(0));
            assert!(app.simulation.as_ref().unwrap().warnings().is_empty());
        })
    });
    cx.run_until_parked();
}

#[gpui::test]
fn net_properties_rename_labels_and_reject_connected_width_conflicts(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(demo::full_adder(), None, window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.editor.selected_net = Some(NetId(1));
            app.command(Command::Properties, window, cx);
            let Some(Dialog::Net(dialog)) = &mut app.dialog else {
                panic!("missing net dialog")
            };
            dialog
                .name
                .update(cx, |field, cx| field.set_value("input_bus".into(), cx));
            dialog.show_name = false;
            app.apply_dialog(window, cx);
            assert_eq!(app.editor.module().net(NetId(1)).unwrap().name, "input_bus");
            assert!(!app.editor.module().net(NetId(1)).unwrap().show_name);
            app.command(Command::Properties, window, cx);
            let Some(Dialog::Net(dialog)) = &mut app.dialog else {
                panic!("missing net dialog")
            };
            dialog
                .width
                .update(cx, |field, cx| field.set_value("8".into(), cx));
            app.apply_dialog(window, cx);
            assert!(matches!(&app.dialog,Some(Dialog::Net(dialog)) if dialog.error.is_some()));
            assert_eq!(app.editor.module().net(NetId(1)).unwrap().width, 1);
        })
    });
}

#[gpui::test]
fn extended_components_render_and_properties_configure_bus_memory(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        GateApp::new(rgate_core::Circuit::default(), None, window, cx)
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            let id = app
                .editor
                .place(GateKind::Splitter, Point::new(150.0, 100.0))
                .unwrap();
            app.command(Command::Properties, window, cx);
            let Some(Dialog::Properties(dialog)) = &mut app.dialog else {
                panic!("missing properties")
            };
            dialog
                .partitions
                .update(cx, |field, cx| field.set_value("1,3,4".into(), cx));
            app.apply_dialog(window, cx);
            assert!(app.dialog.is_none());
            assert_eq!(app.editor.module().gate(id).unwrap().pin_width("Z1"), 3);
            let memory = app
                .editor
                .place(GateKind::Rom, Point::new(350.0, 100.0))
                .unwrap();
            app.command(Command::Properties, window, cx);
            let Some(Dialog::Properties(dialog)) = &mut app.dialog else {
                panic!("missing properties")
            };
            dialog
                .memory
                .update(cx, |field, cx| field.set_value("@1 AB CD".into(), cx));
            app.apply_dialog(window, cx);
            assert_eq!(
                app.editor
                    .module()
                    .gate(memory)
                    .unwrap()
                    .config
                    .sparse_memory[&1]
                    .to_u64(),
                Some(0xab)
            );
            app.command(Command::ClockStep, window, cx);
        })
    });
    cx.run_until_parked();
    assert!(view.read_with(cx, |app, _| app.simulation.is_some()));
}

#[gpui::test]
fn runtime_terminal_and_memory_dialogs_keep_simulation_alive(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        GateApp::new(rgate_core::Circuit::default(), None, window, cx)
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            let tty = app
                .editor
                .place(GateKind::Tty, Point::new(100.0, 100.0))
                .unwrap();
            let ram = app
                .editor
                .place(GateKind::Ram, Point::new(300.0, 100.0))
                .unwrap();
            app.command(Command::ClockStep, window, cx);
            let input = cx.new(|cx| crate::input::TextField::new("hello", cx));
            app.dialog = Some(Dialog::Terminal {
                gate: tty,
                input,
                error: None,
            });
            app.apply_dialog(window, cx);
            assert!(app.dialog.is_none());
            assert!(app.simulation.is_some());
            let address = cx.new(|cx| crate::input::TextField::new("3", cx));
            let value = cx.new(|cx| crate::input::TextField::new("AB", cx));
            app.dialog = Some(Dialog::Memory {
                gate: ram,
                address,
                value,
                error: None,
            });
            app.apply_dialog(window, cx);
            assert_eq!(
                app.simulation.as_ref().unwrap().memory_words(ram).unwrap()[&3].to_u64(),
                Some(0xab)
            );
            assert!(app.dialog.is_none());
        })
    });
    cx.run_until_parked();
}

#[gpui::test]
fn led_properties_select_mode_render_and_roundtrip(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        GateApp::new(rgate_core::Circuit::default(), None, window, cx)
    });
    cx.run_until_parked();
    let id = cx.update(|_, cx| {
        view.update(cx, |app, _| {
            let id = app
                .editor
                .place(GateKind::Led, Point::new(100.0, 100.0))
                .unwrap();
            app.editor
                .edit_module(|module| {
                    let gate = module.gate_mut(id).unwrap();
                    gate.width = 8;
                    gate.initial = rgate_core::Signal::from_u64(0, 8);
                })
                .unwrap();
            id
        })
    });
    cx.update(|window, cx| view.update(cx, |app, cx| app.command(Command::Properties, window, cx)));
    cx.run_until_parked();
    let hex_mode = cx.debug_bounds("led-mode-Hex").unwrap().center();
    cx.simulate_click(hex_mode, Modifiers::default());
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    view.read_with(cx, |app, _| {
        assert_eq!(
            app.editor.module().gate(id).unwrap().config.led_display,
            rgate_core::LedDisplay::Hex
        );
        let encoded = rgate_format::native::encode(app.editor.circuit()).unwrap();
        assert_eq!(
            rgate_format::native::decode(&encoded).unwrap(),
            *app.editor.circuit()
        );
    });
    for mode in rgate_core::LedDisplay::ALL {
        for rotation in 0..4 {
            cx.update(|_, cx| {
                view.update(cx, |app, cx| {
                    app.editor
                        .edit_module(|module| {
                            let gate = module.gate_mut(id).unwrap();
                            gate.config.led_display = mode;
                            gate.rotation = rotation;
                        })
                        .unwrap();
                    cx.notify();
                })
            });
            cx.run_until_parked();
        }
    }
}

#[gpui::test]
fn led_mode_selection_sets_recommended_width_but_preserves_wired_buses(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        GateApp::new(rgate_core::Circuit::default(), None, window, cx)
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.editor
                .place(GateKind::Led, Point::new(100.0, 100.0))
                .unwrap();
            app.command(Command::Properties, window, cx);
            for mode in rgate_core::LedDisplay::ALL {
                app.select_led_display(mode, cx);
                let Some(Dialog::Properties(dialog)) = &app.dialog else {
                    panic!("missing properties")
                };
                assert_eq!(
                    dialog.width.read(cx).value,
                    mode.recommended_width().to_string()
                );
            }
            app.select_led_display(rgate_core::LedDisplay::SevenSegment, cx);
            app.apply_dialog(window, cx);
            assert_eq!(app.editor.module().gates[0].width, 7);
        })
    });
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(demo::bus_memory(), None, window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.editor.select(GateId(5), false);
            app.command(Command::Properties, window, cx);
            app.select_led_display(rgate_core::LedDisplay::SevenSegment, cx);
            let Some(Dialog::Properties(dialog)) = &app.dialog else {
                panic!("missing properties")
            };
            assert_eq!(dialog.width.read(cx).value, "8");
            app.apply_dialog(window, cx);
            assert!(app.dialog.is_none());
            assert_eq!(app.editor.module().gate(GateId(5)).unwrap().width, 8);
        })
    });
}

#[gpui::test]
fn original_shift_register_renders_and_drives_bar_display(cx: &mut TestAppContext) {
    let imported = rgate_format::parse(include_str!(
        "../../../examples/serial-shift-register.rgate"
    ))
    .unwrap();
    assert!(imported.warnings.is_empty());
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(imported.circuit, None, window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.command(Command::ClockStep, window, cx);
            let bus = app
                .editor
                .module()
                .nets
                .iter()
                .find(|net| net.name == "history")
                .unwrap()
                .id;
            let switch = app
                .editor
                .module()
                .gates
                .iter()
                .find(|gate| gate.name == "RESET_N")
                .unwrap()
                .id;
            app.simulation
                .as_mut()
                .unwrap()
                .set_input(switch, rgate_core::Signal::from_u64(0, 1))
                .unwrap();
            app.simulation.as_mut().unwrap().advance(40).unwrap();
            assert_eq!(
                app.simulation
                    .as_ref()
                    .unwrap()
                    .value(bus)
                    .unwrap()
                    .to_u64(),
                Some(0)
            );
            assert!(app.simulation.as_ref().unwrap().warnings().is_empty());
            for mode in [crate::theme::Theme::Classic, crate::theme::Theme::Modern] {
                app.theme = mode;
                cx.notify();
            }
        })
    });
    cx.run_until_parked();
}

#[gpui::test]
fn lc3_example_is_component_hierarchy_and_runs_in_the_ui(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        GateApp::new(rgate_core::Circuit::default(), None, window, cx)
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.command(Command::Example("lc3"), window, cx)
        })
    });
    cx.run_until_parked();
    for module in ["LC3", "Control", "RegisterFile", "main"] {
        cx.update(|_, cx| view.update(cx, |app, cx| app.switch_module(module.into(), cx)));
        cx.run_until_parked();
    }
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            assert_eq!(app.editor.circuit().modules.len(), 4);
            app.command(Command::ClockStep, window, cx);
            let reset = app
                .editor
                .module()
                .gates
                .iter()
                .find(|gate| gate.name == "RESET_N")
                .unwrap()
                .id;
            app.simulation
                .as_mut()
                .unwrap()
                .set_input(reset, rgate_core::Signal::from_u64(1, 1))
                .unwrap();
            app.simulation.as_mut().unwrap().advance(200_000).unwrap();
            let module = app.editor.module();
            let halted = module
                .nets
                .iter()
                .find(|net| net.name == "HALTED")
                .unwrap()
                .id;
            assert_eq!(
                app.simulation
                    .as_ref()
                    .unwrap()
                    .value(halted)
                    .unwrap()
                    .to_u64(),
                Some(1)
            );
            app.probes.insert(halted);
            cx.notify();
        })
    });
    cx.run_until_parked();
}

#[gpui::test]
fn buses_rom_label_layout_remains_clear_in_both_themes(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(demo::bus_memory(), None, window, cx));
    cx.run_until_parked();
    for theme in [crate::theme::Theme::Classic, crate::theme::Theme::Modern] {
        cx.update(|window, cx| {
            view.update(cx, |app, cx| {
                app.command(Command::SetTheme(theme), window, cx);
                app.command(Command::ClockStep, window, cx);
                let scene = app.scene();
                let mut occupied = Vec::new();
                for wire in &scene.module.wires {
                    let net = scene.module.net(wire.net).unwrap();
                    let label = format!("{} = {}", net.name, scene.values[&net.id]);
                    if let Some(origin) =
                        crate::label_layout::wire_label(&scene.module, wire, &label, &occupied)
                    {
                        let rect = crate::label_layout::label_rect(origin, &label, 10.0);
                        assert!(
                            scene
                                .module
                                .gates
                                .iter()
                                .all(|gate| !crate::label_layout::intersects(rect, gate.bounds()))
                        );
                        assert!(
                            occupied
                                .iter()
                                .all(|other| !crate::label_layout::intersects(rect, *other))
                        );
                        occupied.push(rect.expanded(2.0));
                    }
                }
                assert!(
                    occupied.len() >= 5,
                    "too many hidden net labels: {}",
                    occupied.len()
                );
                cx.notify();
            })
        });
        cx.run_until_parked();
    }
}

#[gpui::test]
fn select_mode_mouse_drag_moves_wire_without_detaching_pins(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(demo::full_adder(), None, window, cx));
    cx.run_until_parked();
    let (position, original) = view.read_with(cx, |app, _| {
        let wire = &app.editor.module().wires[0];
        let p = app
            .editor
            .viewport
            .to_screen((wire.points[1] + wire.points[2]) / 2.0);
        (
            app.canvas_bounds.origin + point(px(p.x), px(p.y)),
            app.editor.circuit().clone(),
        )
    });
    cx.simulate_mouse_down(position, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        position + point(px(25.0), px(0.0)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_up(
        position + point(px(25.0), px(0.0)),
        MouseButton::Left,
        Modifiers::default(),
    );
    view.read_with(cx, |app, _| {
        assert_ne!(app.editor.circuit(), &original);
        assert_eq!(
            app.editor.module().wires[0].start,
            original.modules[0].wires[0].start
        );
        assert_eq!(
            app.editor.module().wires[0].end,
            original.modules[0].wires[0].end
        );
        app.editor.circuit().validate().unwrap();
    });
    cx.update(|window, cx| view.update(cx, |app, cx| app.command(Command::Undo, window, cx)));
    assert_eq!(
        view.read_with(cx, |app, _| app.editor.circuit().clone()),
        original
    );
}

#[gpui::test]
fn live_instances_keep_time_state_probes_and_parent_navigation(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        GateApp::new(demo::hierarchical_inverters(), None, window, cx)
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.command(Command::ClockStep, window, cx);
            let time = app.simulation.as_ref().unwrap().time();
            app.running = true;
            app.navigate_scope("main/u0/inner".into(), "inverter".into(), cx);
            assert!(app.running);
            assert_eq!(app.simulation.as_ref().unwrap().time(), time);
            assert_eq!(app.displayed_value(NetId(2)).unwrap().to_u64(), Some(1));
            app.toggle_probe(NetId(2), cx);
            app.navigate_scope("main/u1/inner".into(), "inverter".into(), cx);
            assert_eq!(app.displayed_value(NetId(2)).unwrap().to_u64(), Some(0));
            assert!(!app.is_probed(NetId(2)));
            app.toggle_probe(NetId(2), cx);
            let sim = app.simulation.as_ref().unwrap();
            assert_eq!(sim.traces().len(), 2);
            assert!(
                sim.traces()
                    .values()
                    .any(|trace| trace.name == "main/u0/inner/Y")
            );
            assert!(
                sim.traces()
                    .values()
                    .any(|trace| trace.name == "main/u1/inner/Y")
            );
            app.navigate_scope("main".into(), "main".into(), cx);
            app.simulation
                .as_mut()
                .unwrap()
                .toggle_input(GateId(1))
                .unwrap();
            app.simulation.as_mut().unwrap().advance(20).unwrap();
            app.navigate_scope("main/u0/inner".into(), "inverter".into(), cx);
            assert_eq!(app.displayed_value(NetId(2)).unwrap().to_u64(), Some(0));
            assert!(app.is_probed(NetId(2)));
            assert!(app.simulation.as_ref().unwrap().time() > time);
            app.running = false;
        })
    });
    cx.run_until_parked();
    let up = cx.debug_bounds("debug-up").unwrap().center();
    cx.simulate_click(up, Modifiers::default());
    assert_eq!(
        view.read_with(cx, |app, _| app.debug_path.clone()),
        Some("main/u0".into())
    );
    assert!(view.read_with(cx, |app, _| app.simulation.is_some()));
}

#[gpui::test]
fn lc3_child_debug_advances_the_root_clock_not_a_new_simulator(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        GateApp::new(rgate_core::Circuit::default(), None, window, cx)
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.command(Command::Example("lc3"), window, cx);
            app.command(Command::ClockStep, window, cx);
            let reset = app
                .editor
                .module()
                .gates
                .iter()
                .find(|gate| gate.name == "RESET_N")
                .unwrap()
                .id;
            app.simulation
                .as_mut()
                .unwrap()
                .set_input(reset, rgate_core::Signal::from_u64(1, 1))
                .unwrap();
            app.simulation.as_mut().unwrap().advance(3000).unwrap();
            let time = app.simulation.as_ref().unwrap().time();
            app.navigate_scope("main/cpu/controller".into(), "Control".into(), cx);
            let state = app
                .editor
                .module()
                .nets
                .iter()
                .find(|net| net.name == "STATE")
                .unwrap()
                .id;
            app.toggle_probe(state, cx);
            app.command(Command::ClockStep, window, cx);
            assert_eq!(app.simulation.as_ref().unwrap().time(), time + 500);
            assert!(app.displayed_value(state).unwrap().to_u64().is_some());
            app.navigate_scope("main/cpu/register_file".into(), "RegisterFile".into(), cx);
            app.simulation.as_mut().unwrap().advance(200_000).unwrap();
            let r0 = app
                .editor
                .module()
                .nets
                .iter()
                .find(|net| net.name == "R0")
                .unwrap()
                .id;
            assert!(app.displayed_value(r0).unwrap().to_u64().is_some());
            app.navigate_scope("main".into(), "main".into(), cx);
            let halted = app
                .editor
                .module()
                .nets
                .iter()
                .find(|net| net.name == "HALTED")
                .unwrap()
                .id;
            assert_eq!(app.displayed_value(halted).unwrap().to_u64(), Some(1));
            assert_eq!(app.simulation.as_ref().unwrap().traces().len(), 1);
        })
    });
    cx.run_until_parked();
}

#[gpui::test]
fn live_child_switch_memory_and_terminal_use_instance_specific_runtime_ids(
    cx: &mut TestAppContext,
) {
    let mut circuit = rgate_core::Circuit::default();
    let mut child = rgate_core::Module::new("devices");
    for (index, kind) in [GateKind::Switch, GateKind::Ram, GateKind::Tty]
        .into_iter()
        .enumerate()
    {
        child.gates.push(rgate_core::Gate::new(
            GateId(index as u64 + 1),
            kind,
            Point::new(100.0 + index as f32 * 150.0, 100.0),
        ));
    }
    circuit.modules.push(child);
    for index in 0..2 {
        let mut instance = circuit
            .module_instance(
                "devices",
                GateId(index + 1),
                Point::new(100.0 + index as f32 * 200.0, 100.0),
            )
            .unwrap();
        instance.name = format!("u{index}");
        circuit.modules[0].gates.push(instance);
    }
    let (view, cx) = cx.add_window_view(|window, cx| GateApp::new(circuit, None, window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.command(Command::ClockStep, window, cx);
            app.navigate_scope("main/u0".into(), "devices".into(), cx);
            let address = cx.new(|cx| crate::input::TextField::new("2", cx));
            let value = cx.new(|cx| crate::input::TextField::new("AB", cx));
            app.dialog = Some(Dialog::Memory {
                gate: GateId(2),
                address,
                value,
                error: None,
            });
            app.apply_dialog(window, cx);
            let runtime = app.simulation_gate(GateId(2)).unwrap();
            assert_eq!(
                app.simulation
                    .as_ref()
                    .unwrap()
                    .memory_words(runtime)
                    .unwrap()[&2]
                    .to_u64(),
                Some(0xab)
            );
            let input = cx.new(|cx| crate::input::TextField::new("a", cx));
            app.dialog = Some(Dialog::Terminal {
                gate: GateId(3),
                input,
                error: None,
            });
            app.apply_dialog(window, cx);
            assert!(app.dialog.is_none());
            app.navigate_scope("main/u1".into(), "devices".into(), cx);
            let runtime = app.simulation_gate(GateId(2)).unwrap();
            assert!(
                !app.simulation
                    .as_ref()
                    .unwrap()
                    .memory_words(runtime)
                    .unwrap()
                    .contains_key(&2)
            );
            app.navigate_scope("main/u0".into(), "devices".into(), cx);
            cx.notify();
        })
    });
    cx.run_until_parked();
    let position = view.read_with(cx, |app, _| {
        let p = app
            .editor
            .viewport
            .to_screen(app.editor.module().gate(GateId(1)).unwrap().position);
        app.canvas_bounds.origin + point(px(p.x), px(p.y))
    });
    cx.simulate_click(position, Modifiers::default());
    view.read_with(cx, |app, _| {
        let sim = app.simulation.as_ref().unwrap();
        assert_eq!(
            sim.gate_value(sim.scoped_gate("main/u0", GateId(1)).unwrap())
                .unwrap()
                .to_u64(),
            Some(1)
        );
        assert_eq!(
            sim.gate_value(sim.scoped_gate("main/u1", GateId(1)).unwrap())
                .unwrap()
                .to_u64(),
            Some(0)
        );
    });
}

#[gpui::test]
fn builtin_gate_parity_properties_and_runtime_dip_input(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        GateApp::new(rgate_core::Circuit::default(), None, window, cx)
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            let frame = app
                .editor
                .place(GateKind::Frame, Point::new(300.0, 200.0))
                .unwrap();
            app.command(Command::Properties, window, cx);
            let Some(Dialog::Properties(dialog)) = &mut app.dialog else {
                panic!("missing properties")
            };
            dialog
                .frame_width
                .update(cx, |field, cx| field.set_value("400".into(), cx));
            app.apply_dialog(window, cx);
            assert_eq!(
                app.editor.module().gate(frame).unwrap().config.frame_width,
                400.0
            );
            let clock = app
                .editor
                .place(GateKind::Clock, Point::new(100.0, 100.0))
                .unwrap();
            app.command(Command::Properties, window, cx);
            let Some(Dialog::Properties(dialog)) = &mut app.dialog else {
                panic!("missing properties")
            };
            dialog
                .phase
                .update(cx, |field, cx| field.set_value("20".into(), cx));
            dialog
                .duty
                .update(cx, |field, cx| field.set_value("25".into(), cx));
            app.apply_dialog(window, cx);
            assert_eq!(
                app.editor.module().gate(clock).unwrap().config.clock_duty,
                25
            );
            let dip = app
                .editor
                .place(GateKind::Dip, Point::new(200.0, 100.0))
                .unwrap();
            app.command(Command::ClockStep, window, cx);
            let value = cx.new(|cx| crate::input::TextField::new("AB", cx));
            app.dialog = Some(Dialog::Input {
                gate: dip,
                value,
                error: None,
            });
            app.apply_dialog(window, cx);
            assert_eq!(
                app.simulation
                    .as_ref()
                    .unwrap()
                    .gate_value(app.simulation_gate(dip).unwrap())
                    .unwrap()
                    .to_u64(),
                Some(0xab)
            );
        })
    });
    cx.run_until_parked();
}

#[gpui::test]
fn symbol_editor_draws_shapes_and_updates_instances_without_cpu_behavior(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        GateApp::new(demo::hierarchical_inverters(), None, window, cx)
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.switch_module("wrapped_inverter".into(), cx);
            app.command(Command::EditSymbol, window, cx);
        })
    });
    cx.run_until_parked();
    let bounds = cx.debug_bounds("symbol-editor-canvas").unwrap();
    let a = bounds.center() - point(px(50.0), px(30.0));
    let b = bounds.center() + point(px(50.0), px(30.0));
    cx.simulate_mouse_down(a, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(b, MouseButton::Left, Modifiers::default());
    cx.update(|window, cx| view.update(cx, |app, cx| app.apply_dialog(window, cx)));
    assert!(view.read_with(cx, |app, _| app.dialog.is_none()));
    view.read_with(cx, |app, _| {
        assert_eq!(app.editor.module().symbol.len(), 1);
        assert_eq!(
            app.editor
                .circuit()
                .module("main")
                .unwrap()
                .gate(GateId(2))
                .unwrap()
                .config
                .custom_symbol
                .len(),
            1
        );
    });
}

#[gpui::test]
fn wide_dip_value_and_rich_comment_render_in_both_themes(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        GateApp::new(rgate_core::Circuit::default(), None, window, cx)
    });
    cx.run_until_parked();
    cx.update(|window,cx|view.update(cx,|app,cx|{
        let id=app.editor.place(GateKind::Dip,Point::new(100.0,100.0)).unwrap();app.command(Command::Properties,window,cx);
        let Some(Dialog::Properties(dialog))=&mut app.dialog else{panic!("missing properties")};
        dialog.width.update(cx,|field,cx|field.set_value("128".into(),cx));dialog.value.update(cx,|field,cx|field.set_value("0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF".into(),cx));
        app.apply_dialog(window,cx);assert!(app.dialog.is_none());assert_eq!(app.editor.module().gate(id).unwrap().initial.width(),128);
        let comment=app.editor.place(GateKind::Comment,Point::new(200.0,150.0)).unwrap();app.editor.edit_module(|module|module.gate_mut(comment).unwrap().text="<h2>Heading</h2><font color='red'><b>Bold red</b></font><br><a href='https://example.com'>Docs</a><img src='diagram.png' alt='Diagram' width=70 height=60>".into()).unwrap();cx.notify();
    }));
    cx.run_until_parked();
    for theme in [crate::theme::Theme::Classic, crate::theme::Theme::Modern] {
        cx.update(|window, cx| {
            view.update(cx, |app, cx| {
                app.command(Command::SetTheme(theme), window, cx)
            })
        });
        cx.run_until_parked();
    }
}

#[gpui::test]
fn marquee_selects_wires_and_delete_removes_only_selected_branches(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(demo::full_adder(), None, window, cx));
    cx.run_until_parked();
    let screen = |world: Point, cx: &gpui::VisualTestContext| {
        view.read_with(cx, |app, _| {
            let p = app.editor.viewport.to_screen(world);
            app.canvas_bounds.origin + point(px(p.x), px(p.y))
        })
    };
    let start = screen(Point::new(160.0, 123.0), cx);
    let end = screen(Point::new(175.0, 131.0), cx);
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
    view.read_with(cx, |app, _| {
        assert!(app.editor.selection.is_empty());
        assert_eq!(app.editor.selected_wires.len(), 1);
        assert!(app.scene().selected_wires.contains(&rgate_core::WireId(1)));
    });
    cx.update(|window, cx| view.update(cx, |app, cx| app.command(Command::Delete, window, cx)));
    view.read_with(cx, |app, _| {
        assert!(
            !app.editor
                .module()
                .wires
                .iter()
                .any(|wire| wire.id == rgate_core::WireId(1))
        );
        assert!(
            app.editor
                .module()
                .wires
                .iter()
                .any(|wire| wire.id == rgate_core::WireId(2))
        );
        app.editor.circuit().validate().unwrap();
    });
}

#[gpui::test]
fn modules_nets_separator_resizes_and_long_hierarchy_rows_stay_single_line(
    cx: &mut TestAppContext,
) {
    let mut circuit = demo::hierarchical_inverters();
    circuit.modules[0].gates[1].name =
        "very_long_instance_name_that_used_to_wrap_over_the_next_row".into();
    let (view, cx) = cx.add_window_view(|window, cx| GateApp::new(circuit, None, window, cx));
    cx.simulate_resize(size(px(1240.0), px(820.0)));
    cx.run_until_parked();
    let first = cx.debug_bounds("modules-panel").unwrap();
    assert!(
        (f32::from(first.size.height) - 196.0).abs() < 2.0,
        "initial height: {:?}",
        first.size
    );
    let position = cx.debug_bounds("modules-nets-splitter").unwrap().center();
    cx.simulate_mouse_down(position, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        position + point(px(0.0), px(150.0)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_up(
        position + point(px(0.0), px(150.0)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.run_until_parked();
    let resized = cx.debug_bounds("modules-panel").unwrap();
    assert!(resized.size.height > first.size.height + px(140.0));
    assert!((view.read_with(cx, |app, _| app.modules_height) - 346.0).abs() < 2.0);
    let module = cx.debug_bounds("module-wrapped_inverter").unwrap();
    assert_eq!(module.size.height, px(25.0));
    assert!(view.read_with(cx, |app, _| app.drag.is_none()));
    assert!(!view.read_with(cx, |app, _| app.editor.is_dirty()));
}

#[gpui::test]
fn lc3_terminal_runs_live_and_explains_held_reset(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        GateApp::new(rgate_core::Circuit::default(), None, window, cx)
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.command(Command::Example("lc3"), window, cx);
            app.command(Command::ClockStep, window, cx);
            assert!(app.held_reset_input().is_some());
            let console = app
                .editor
                .module()
                .gates
                .iter()
                .find(|gate| gate.name == "console")
                .unwrap()
                .id;
            let input = cx.new(|cx| crate::input::TextField::new("", cx));
            app.dialog = Some(Dialog::Terminal {
                gate: console,
                input,
                error: None,
            });
            app.release_terminal_reset(cx);
            assert!(app.held_reset_input().is_none());
            app.command(Command::PlayPause, window, cx);
            assert!(app.running);
            for _ in 0..400 {
                app.advance_running(cx);
            }
            assert_eq!(
                app.simulation
                    .as_ref()
                    .unwrap()
                    .terminal_output(app.simulation_gate(console).unwrap())
                    .unwrap(),
                "HI\n"
            );
            assert!(app.dialog.is_some());
            let time = app.simulation.as_ref().unwrap().time();
            app.command(Command::ClockStep, window, cx);
            assert_eq!(app.simulation.as_ref().unwrap().time(), time + 500);
            assert!(!app.running);
        })
    });
    cx.run_until_parked();
}

#[gpui::test]
fn message_log_drag_select_and_copy_spans_lines(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        GateApp::new(rgate_core::Circuit::default(), None, window, cx)
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        view.update(cx, |app, cx| {
            app.messages = vec!["first message".into(), "Error: second message".into()];
            cx.notify();
        })
    });
    cx.run_until_parked();
    let bounds = cx.debug_bounds("selectable-message-log").unwrap();
    let start = bounds.origin + point(px(0.0), px(5.0));
    let end = bounds.origin + point(px(200.0), px(25.0));
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
    cx.simulate_keystrokes("cmd-c");
    assert_eq!(
        cx.read_from_clipboard().unwrap().text().unwrap(),
        "first message\nError: second message"
    );
    cx.simulate_keystrokes("cmd-a cmd-c");
    assert_eq!(
        cx.read_from_clipboard().unwrap().text().unwrap(),
        "first message\nError: second message"
    );
    assert_eq!(
        view.read_with(cx, |app, _| app.editor.tool.clone()),
        Tool::Select
    );
    assert!(!view.read_with(cx, |app, _| app.editor.is_dirty()));
}

#[gpui::test]
fn hierarchy_self_instance_button_is_absent_but_other_definitions_can_be_placed(
    cx: &mut TestAppContext,
) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        GateApp::new(demo::hierarchical_inverters(), None, window, cx)
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("instantiate-main").is_none());
    assert!(cx.debug_bounds("instantiate-main/u0").is_some());
    cx.update(|_, cx| {
        view.update(cx, |app, cx| {
            app.switch_module("wrapped_inverter".into(), cx)
        })
    });
    cx.run_until_parked();
    let row = cx.debug_bounds("module-wrapped_inverter").unwrap();
    assert_eq!(row.size.height, px(25.0));
}

#[gpui::test]
fn waveform_mouse_cursors_zoom_scroll_and_radix_do_not_change_circuit(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(demo::bus_memory(), None, window, cx));
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.bottom_height = 300.0;
            app.command(Command::ClockStep, window, cx);
            app.toggle_probe(NetId(5), cx);
            app.waveform.follow = false;
            app.waveform.start = 0.0;
            app.waveform.span = 200.0;
            cx.notify();
        })
    });
    cx.run_until_parked();
    let bounds = cx.debug_bounds("wave-canvas").unwrap();
    let a = bounds.origin + point(px(300.0), px(40.0));
    let b = bounds.origin + point(px(350.0), px(40.0));
    cx.simulate_click(a, Modifiers::default());
    cx.simulate_click(
        b,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    view.read_with(cx, |app, _| {
        assert!(app.waveform.cursor_a.is_some());
        assert!(app.waveform.cursor_b.is_some());
        assert!(app.waveform.delta().unwrap() > 0);
    });
    cx.simulate_click(
        bounds.origin + point(px(30.0), px(35.0)),
        Modifiers::default(),
    );
    let bin = crate::waveform::Radix::Binary;
    cx.update(|_, cx| {
        view.update(cx, |app, cx| {
            let name = app.waveform.selected.clone().unwrap();
            app.waveform.radices.insert(name, bin);
            cx.notify();
        })
    });
    let previous = view.read_with(cx, |app, _| app.waveform.span);
    let zoom = cx.debug_bounds("wave-zoom-in").unwrap().center();
    cx.simulate_click(zoom, Modifiers::default());
    assert_eq!(
        view.read_with(cx, |app, _| app.waveform.span),
        previous / 2.0
    );
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: a,
        delta: gpui::ScrollDelta::Pixels(point(px(-40.0), px(0.0))),
        ..Default::default()
    });
    assert!(view.read_with(cx, |app, _| app.waveform.start) > 0.0);
    assert!(!view.read_with(cx, |app, _| app.editor.is_dirty()));
    cx.run_until_parked();
}

#[gpui::test]
fn find_gates_and_nets_selects_result_without_editing_document(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(demo::full_adder(), None, window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| view.update(cx, |app, cx| app.command(Command::Find, window, cx)));
    cx.run_until_parked();
    cx.simulate_input("xor_sum");
    cx.run_until_parked();
    let row = cx.debug_bounds("find-0").unwrap().center();
    cx.simulate_click(row, Modifiers::default());
    view.read_with(cx, |app, _| {
        assert!(app.dialog.is_none());
        assert!(app.editor.selection.contains(&GateId(5)));
        assert!(!app.editor.is_dirty());
    });
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.editor.select(GateId(1), false);
            app.editor.select(GateId(2), true);
            app.command(Command::Align(rgate_editor::Align::Top), window, cx);
            assert_eq!(
                app.editor.module().gate(GateId(1)).unwrap().position.y,
                app.editor.module().gate(GateId(2)).unwrap().position.y
            );
            app.command(Command::Undo, window, cx);
        })
    });
    assert!(!view.read_with(cx, |app, _| app.editor.is_dirty()));
}

#[gpui::test]
fn tiny_vga_visual_peripheral_runs_controller_and_draws_frame(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        GateApp::new(rgate_core::Circuit::default(), None, window, cx)
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.command(Command::Example("tiny-vga"), window, cx);
            app.command(Command::ClockStep, window, cx);
            app.release_terminal_reset(cx);
            let gate = app
                .editor
                .module()
                .gates
                .iter()
                .find(|gate| gate.kind == GateKind::Vga)
                .unwrap()
                .id;
            app.dialog = Some(Dialog::Vga { gate });
            app.advance_vga_frame(cx);
            app.advance_vga_frame(cx);
            let frame = app
                .simulation
                .as_ref()
                .unwrap()
                .vga_frame(app.simulation_gate(gate).unwrap())
                .unwrap();
            assert!(frame.frames >= 1);
            assert_eq!(frame.pixels[0], 0xff0000);
            assert_eq!(frame.sync_errors, 0);
        })
    });
    cx.run_until_parked();
}

#[cfg(feature = "icarus")]
#[gpui::test]
fn icarus_pwm_drives_editor_inputs_leds_hierarchy_and_waveforms(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(demo::full_adder(), None, window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.command(Command::Example("pwm-icarus"), window, cx);
            app.command(Command::ClockStep, window, cx);
            assert!(
                app.simulation
                    .as_ref()
                    .unwrap()
                    .backend_name()
                    .contains("Icarus")
            );
            let net = |app: &GateApp, name: &str| {
                app.editor
                    .module()
                    .nets
                    .iter()
                    .find(|n| n.name == name)
                    .unwrap()
                    .id
            };
            let led = net(app, "led");
            let count = net(app, "count");
            let duty_net = net(app, "duty");
            assert_eq!(app.displayed_value(count).unwrap().to_u64(), Some(0));
            app.release_terminal_reset(cx);
            let duty = app
                .editor
                .module()
                .gates
                .iter()
                .find(|g| g.name == "DUTY")
                .unwrap()
                .id;
            app.toggle_probe(led, cx);
            app.toggle_probe(count, cx);
            app.dialog = Some(Dialog::Input {
                gate: duty,
                value: cx.new(|cx| crate::input::TextField::new("01", cx)),
                error: None,
            });
            app.apply_dialog(window, cx);
            assert!(app.dialog.is_none());
            assert_eq!(app.displayed_value(duty_net).unwrap().to_u64(), Some(1));
            app.command(Command::Step, window, cx);
            assert_eq!(app.displayed_value(count).unwrap().to_u64(), Some(1));
            assert_eq!(app.displayed_value(led).unwrap().to_u64(), Some(0));
            app.simulation.as_mut().unwrap().advance(25500).unwrap();
            assert_eq!(app.displayed_value(count).unwrap().to_u64(), Some(0));
            assert_eq!(app.displayed_value(led).unwrap().to_u64(), Some(1));
            let time = app.simulation.as_ref().unwrap().time();
            app.navigate_scope("main/dimmer".into(), "PwmDimmer".into(), cx);
            let local_count = net(app, "count");
            assert_eq!(app.displayed_value(local_count).unwrap().to_u64(), Some(0));
            let diff = net(app, "difference");
            assert_eq!(app.displayed_value(diff).unwrap().to_u64(), Some(255));
            app.toggle_probe(diff, cx);
            assert_eq!(app.simulation.as_ref().unwrap().time(), time);
            assert!(
                app.simulation
                    .as_ref()
                    .unwrap()
                    .traces()
                    .values()
                    .any(|trace| trace.name == "main/dimmer/difference")
            );
            app.command(Command::Stop, window, cx);
            assert!(app.simulation.is_none());
            app.command(Command::Example("full-adder"), window, cx);
            app.command(Command::ClockStep, window, cx);
            assert_eq!(
                app.simulation.as_ref().unwrap().backend_name(),
                "Rust schematic"
            );
        })
    });
    cx.run_until_parked();
}

#[cfg(feature = "icarus")]
#[gpui::test]
fn create_edit_save_and_run_verilog_module(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(demo::full_adder(), None, window, cx));
    cx.run_until_parked();
    cx.update(|window,cx|view.update(cx,|app,cx|{
  app.command(Command::NewVerilog,window,cx);
  let Some(Dialog::Verilog {name,source,interface,..})=&app.dialog else{panic!("source dialog")};
  name.update(cx,|f,cx|f.set_value("hdl1".into(),cx));
  source.update(cx,|f,cx|f.set_value("`timescale 1ns/1ps\nmodule hdl1(input wire clk, input wire reset, input wire enable, output reg [7:0] count);\n always @(posedge clk) if(reset) count<=0; else if(enable) count<=count+1;\nendmodule\n".into(),cx));
  interface.update(cx,|f,cx|f.set_value("input clk 1\ninput reset 1\ninput enable 1\noutput count 8".into(),cx));
  app.apply_dialog(window,cx);assert!(app.dialog.is_none());assert_eq!(app.editor.active_module(),"hdl1");
  let encoded=rgate_format::native::encode(app.editor.circuit()).unwrap();assert_eq!(rgate_format::native::decode(&encoded).unwrap(),*app.editor.circuit());
  app.command(Command::ClockStep,window,cx);assert!(app.simulation.as_ref().unwrap().backend_name().contains("Icarus"));
  let count=app.editor.module().nets.iter().find(|n|n.name=="count").unwrap().id;app.toggle_probe(count,cx);
  let mut set=|app:&mut GateApp,name:&str,value:&str,cx:&mut gpui::Context<GateApp>|{
    let net=app.editor.module().nets.iter().find(|n|n.name==name).unwrap().id;
    app.dialog=Some(Dialog::HdlInput{net,value:cx.new(|cx|crate::input::TextField::new(value,cx)),error:None});app.apply_dialog(window,cx);assert!(app.dialog.is_none());
  };
  set(app,"reset","1",cx);set(app,"clk","1",cx);assert_eq!(app.displayed_value(count).unwrap().to_u64(),Some(0));
  set(app,"clk","0",cx);set(app,"reset","0",cx);set(app,"enable","1",cx);set(app,"clk","1",cx);assert_eq!(app.displayed_value(count).unwrap().to_u64(),Some(1));
  app.command(Command::Stop,window,cx);app.command(Command::EditVerilog,window,cx);assert!(app.dialog.is_none());assert_eq!(app.tab,crate::app::WorkspaceTab::Edit);
 }));
    cx.run_until_parked();
}

#[cfg(feature = "icarus")]
#[gpui::test]
fn editable_verilog_pwm_is_instantiated_in_schematic_and_inspected_live(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(demo::full_adder(), None, window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.command(Command::Example("pwm-verilog"), window, cx);
            app.command(Command::ClockStep, window, cx);
            let sim = app.simulation.as_mut().unwrap();
            let count = sim.scoped_net("main/dimmer", NetId(4)).unwrap();
            assert_eq!(sim.value(count).unwrap().to_u64(), Some(1));
            sim.set_input(GateId(2), rgate_core::Signal::from_u64(0, 8))
                .unwrap();
            let led = app
                .editor
                .module()
                .nets
                .iter()
                .find(|n| n.name == "pwm_out")
                .unwrap()
                .id;
            assert_eq!(app.displayed_value(led).unwrap().to_u64(), Some(0));
            let time = app.simulation.as_ref().unwrap().time();
            app.navigate_scope("main/dimmer".into(), "pwm".into(), cx);
            assert_eq!(app.displayed_value(NetId(4)).unwrap().to_u64(), Some(1));
            assert_eq!(app.simulation.as_ref().unwrap().time(), time);
            app.toggle_probe(NetId(4), cx);
            app.command(Command::Stop, window, cx);
            app.command(Command::VerilogInterface, window, cx);
            assert!(matches!(app.dialog, Some(Dialog::Verilog { .. })));
        })
    });
    cx.run_until_parked();
}

#[cfg(not(target_family = "wasm"))]
#[gpui::test]
fn verilog_edit_tab_uses_highlighted_component_and_updates_document(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(demo::full_adder(), None, window, cx));
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.command(Command::Example("pwm-verilog"), window, cx);
            app.switch_module("pwm".into(), cx);
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            assert!(app.source_editor.is_some());
            let input = app.source_editor.as_ref().unwrap().input.clone();
            let source = input.read(cx).value().to_string() + "\n// Unicode: λ — edited\n";
            input.update(cx, |state, cx| {
                state.replace_all(source.clone(), window, cx)
            });
        })
    });
    cx.run_until_parked();
    view.read_with(cx, |app, _| {
        assert!(
            app.editor
                .module()
                .verilog
                .as_ref()
                .unwrap()
                .contains("Unicode: λ")
        );
        assert!(app.editor.is_dirty());
    });
}

#[cfg(not(target_family = "wasm"))]
#[gpui::test]
fn code_editor_keyboard_input_newlines_and_undo_do_not_trigger_canvas(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(demo::full_adder(), None, window, cx));
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.command(Command::Example("pwm-verilog"), window, cx);
            app.switch_module("pwm".into(), cx);
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            let input = app.source_editor.as_ref().unwrap().input.clone();
            input.update(cx, |state, cx| state.focus_handle(cx).focus(window, cx));
        })
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("module pwm;");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("endmodule");
    cx.run_until_parked();
    view.read_with(cx, |app, _| {
        assert!(app.editor.module().verilog.as_ref().unwrap().contains("\n"));
        assert!(app.dialog.is_none());
        assert!(app.editor.module().gates.is_empty());
    });
    cx.simulate_keystrokes("cmd-z");
    cx.run_until_parked();
    view.read_with(cx, |app, _| {
        assert!(
            !app.editor
                .module()
                .verilog
                .as_ref()
                .unwrap()
                .ends_with("endmodule")
        )
    });
}
