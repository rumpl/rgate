use crate::{
    app::{Dialog, GateApp},
    commands::Command,
    theme::Theme,
};
use gpui::{Modifiers, TestAppContext, px, size};
use rgate_core::{GateId, demo};

#[gpui::test]
fn close_button_dismisses_dialogs_without_applying_changes(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(demo::full_adder(), None, window, cx));
    cx.simulate_resize(size(px(1240.), px(820.)));
    cx.run_until_parked();
    for theme in [Theme::Classic, Theme::Modern, Theme::Dark] {
        cx.update(|window, cx| {
            view.update(cx, |app, cx| {
                app.command(Command::SetTheme(theme), window, cx)
            })
        });
        for command in [
            Command::Help,
            Command::About,
            Command::Find,
            Command::NewModule,
            Command::Properties,
        ] {
            let original = view.read_with(cx, |app, _| {
                serde_json::to_string(app.editor.circuit()).unwrap()
            });
            cx.update(|window, cx| {
                view.update(cx, |app, cx| {
                    app.editor.select(GateId(1), false);
                    app.command(command, window, cx);
                    if let Some(Dialog::Properties(dialog)) = &app.dialog {
                        dialog
                            .name
                            .update(cx, |field, cx| field.set_value("unsaved_name".into(), cx));
                    }
                })
            });
            cx.run_until_parked();
            let close = cx.debug_bounds("dialog-header-close").unwrap().center();
            cx.simulate_click(close, Modifiers::default());
            cx.run_until_parked();
            cx.update(|window, cx| {
                view.read_with(cx, |app, _| {
                    assert!(app.dialog.is_none());
                    assert!(app.focus.is_focused(window));
                    assert_eq!(
                        serde_json::to_string(app.editor.circuit()).unwrap(),
                        original
                    );
                })
            });
        }
    }
}

#[gpui::test]
fn recovery_dialog_remains_explicitly_confirmed_and_has_no_close_button(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(demo::full_adder(), None, window, cx));
    cx.update(|_, cx| {
        view.update(cx, |app, cx| {
            app.dialog = Some(Dialog::Recovery);
            cx.notify();
        })
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("dialog-header-close").is_none());
    cx.simulate_keystrokes("escape");
    assert!(view.read_with(cx, |app, _| matches!(app.dialog, Some(Dialog::Recovery))));
}
