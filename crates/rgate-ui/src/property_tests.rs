use crate::{
    app::{Dialog, GateApp},
    commands::Command,
};
use gpui::{TestAppContext, px, size};
use rgate_core::{Circuit, GateKind, Point};

#[gpui::test]
fn component_property_forms_only_show_applicable_fields(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(Circuit::default(), None, window, cx));
    cx.simulate_resize(size(px(1240.), px(820.)));
    cx.run_until_parked();
    for (kind, width, initial, delay, clock) in [
        (GateKind::Frame, false, false, false, false),
        (GateKind::Comment, false, false, false, false),
        (GateKind::And, true, false, true, false),
        (GateKind::Switch, true, true, false, false),
        (GateKind::Clock, false, false, false, true),
        (GateKind::Led, true, false, false, false),
        (GateKind::Ram, true, false, true, false),
    ] {
        cx.update(|window, cx| {
            view.update(cx, |app, cx| {
                app.dialog = None;
                app.editor
                    .place(kind.clone(), Point::new(200., 150.))
                    .unwrap();
                app.command(Command::Properties, window, cx);
            })
        });
        cx.run_until_parked();
        for (selector, expected) in [
            ("property-field-Bit width", width),
            ("property-field-Initial value", initial),
            ("property-field-Delay (ns)", delay),
            ("property-field-Clock period (ns)", clock),
        ] {
            assert_eq!(
                cx.debug_bounds(selector).is_some(),
                expected,
                "{kind:?}: {selector}"
            );
        }
        assert!(cx.debug_bounds("property-field-Name").is_some());
        assert_eq!(
            cx.debug_bounds("property-field-Frame width").is_some(),
            kind == GateKind::Frame
        );
    }
}

#[gpui::test]
fn annotations_ignore_hidden_fields_preserve_electrical_state_and_undo(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(Circuit::default(), None, window, cx));
    cx.run_until_parked();
    for kind in [GateKind::Frame, GateKind::Comment] {
        cx.update(|window, cx| {
            view.update(cx, |app, cx| {
                let id = app
                    .editor
                    .place(kind.clone(), Point::new(200., 150.))
                    .unwrap();
                let original = app.editor.module().gate(id).unwrap().clone();
                app.command(Command::Properties, window, cx);
                let Some(Dialog::Properties(dialog)) = &app.dialog else {
                    panic!("no properties")
                };
                for field in [
                    &dialog.width,
                    &dialog.value,
                    &dialog.delay,
                    &dialog.period,
                    &dialog.phase,
                    &dialog.duty,
                    &dialog.inputs,
                    &dialog.partitions,
                    &dialog.tap_offset,
                    &dialog.address_bits,
                ] {
                    field.update(cx, |field, cx| field.set_value("not a number".into(), cx));
                }
                dialog
                    .name
                    .update(cx, |field, cx| field.set_value("annotation".into(), cx));
                dialog
                    .comment
                    .update(cx, |field, cx| field.set_value("ALU\\nDetails".into(), cx));
                dialog.frame_width.update(cx, |field, cx| {
                    field.set_value(
                        if kind == GateKind::Frame {
                            "400"
                        } else {
                            "invalid"
                        }
                        .into(),
                        cx,
                    )
                });
                dialog.frame_height.update(cx, |field, cx| {
                    field.set_value(
                        if kind == GateKind::Frame {
                            "250"
                        } else {
                            "invalid"
                        }
                        .into(),
                        cx,
                    )
                });
                app.apply_dialog(window, cx);
                assert!(app.dialog.is_none());
                let updated = app.editor.module().gate(id).unwrap();
                assert_eq!(updated.text, "ALU\nDetails");
                assert_eq!(updated.width, original.width);
                assert_eq!(updated.initial, original.initial);
                assert_eq!(updated.delay, original.delay);
                assert_eq!(updated.period, original.period);
                assert_eq!(updated.pins, original.pins);
                if kind == GateKind::Frame {
                    assert_eq!(updated.config.frame_width, 400.);
                } else {
                    assert_eq!(updated.config, original.config);
                }
                app.command(Command::Undo, window, cx);
                assert_eq!(app.editor.module().gate(id).unwrap(), &original);
            })
        });
    }
}

#[gpui::test]
fn invalid_frame_dimensions_are_transactional(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(Circuit::default(), None, window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            let id = app.editor.place(GateKind::Frame, Point::ZERO).unwrap();
            let original = app.editor.module().gate(id).unwrap().clone();
            app.command(Command::Properties, window, cx);
            for value in ["NaN", "0", "10001", "invalid"] {
                let Some(Dialog::Properties(dialog)) = &app.dialog else {
                    panic!("no properties")
                };
                dialog
                    .frame_width
                    .update(cx, |field, cx| field.set_value(value.into(), cx));
                app.apply_dialog(window, cx);
                let Some(Dialog::Properties(dialog)) = &app.dialog else {
                    panic!("invalid frame accepted")
                };
                assert!(dialog.error.is_some());
                assert_eq!(app.editor.module().gate(id).unwrap(), &original);
            }
        })
    });
}
