use crate::{
    app::{GateApp, WorkspaceTab},
    commands::Command,
};
use gpui::{TestAppContext, px, size};
use rgate_core::{Point, demo};
use rgate_editor::Viewport;

#[gpui::test]
fn edit_and_simulate_share_zoom_and_pan_in_both_directions(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|window, cx| GateApp::new(demo::full_adder(), None, window, cx));
    cx.simulate_resize(size(px(1240.), px(820.)));
    cx.run_until_parked();
    for (index, command) in [None, Some(Command::PlayPause), Some(Command::ClockStep)]
        .into_iter()
        .enumerate()
    {
        let edit = Viewport {
            zoom: 2.3 + index as f32 * 0.2,
            pan: Point::new(-80., 113.),
        };
        cx.update(|window, cx| {
            view.update(cx, |app, cx| {
                app.editor.viewport = edit.clone();
                if let Some(command) = command {
                    app.command(command, window, cx);
                } else {
                    app.set_tab(WorkspaceTab::Simulate, window, cx);
                }
            })
        });
        cx.run_until_parked();
        assert_eq!(
            view.read_with(cx, |app, _| app.editor.viewport.clone()),
            edit
        );
        let simulation = Viewport {
            zoom: 1.7,
            pan: Point::new(125., -60. - index as f32 * 20.),
        };
        cx.update(|window, cx| {
            view.update(cx, |app, cx| {
                app.editor.viewport = simulation.clone();
                if index == 1 {
                    app.command(Command::Stop, window, cx);
                } else {
                    app.set_tab(WorkspaceTab::Edit, window, cx);
                }
            })
        });
        cx.run_until_parked();
        view.read_with(cx, |app, _| {
            assert_eq!(app.editor.viewport, simulation);
            assert!(!app.need_fit);
            assert!(!app.editor.is_dirty());
        });
    }
}

#[gpui::test]
fn leaving_live_child_scope_preserves_its_view_in_edit_mode(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        GateApp::new(demo::hierarchical_inverters(), None, window, cx)
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.set_tab(WorkspaceTab::Simulate, window, cx);
            app.navigate_scope("main/u0/inner".into(), "inverter".into(), cx);
        })
    });
    cx.run_until_parked();
    let expected = Viewport {
        zoom: 4.,
        pan: Point::new(-123., 75.),
    };
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.editor.viewport = expected.clone();
            app.set_tab(WorkspaceTab::Edit, window, cx);
        })
    });
    cx.run_until_parked();
    view.read_with(cx, |app, _| {
        assert_eq!(app.editor.active_module(), "inverter");
        assert_eq!(app.editor.viewport, expected);
        assert!(app.debug_path.is_none());
    });
}
