use crate::{
    app::{ContextMenu, GateApp, OpenMenu},
    commands::{self, Command, MenuEntry},
    theme::{Appearance, Theme},
};
use gpui::{
    AnyElement, ClickEvent, Context, DismissEvent, Focusable, MouseButton, Pixels, SharedString,
    WeakEntity, Window, anchored, deferred, div, prelude::*, px,
};
use gpui_component::{
    ActiveTheme, Selectable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    label::Label,
    menu::{PopupMenu, PopupMenuItem},
};

const MENUS: [&str; 9] = [
    "File", "Edit", "View", "Tool", "Simulate", "Module", "Gate", "Arrange", "Help",
];

/// Which View-menu entries currently carry a check mark.
#[derive(Clone, Copy)]
struct MenuChecks {
    theme: Theme,
    appearance: Appearance,
    grid: bool,
    snap: bool,
}

impl MenuChecks {
    fn checked(self, command: &Command) -> bool {
        match command {
            Command::SetTheme(theme) => *theme == self.theme,
            Command::SetAppearance(appearance) => *appearance == self.appearance,
            Command::ToggleGrid => self.grid,
            Command::ToggleSnap => self.snap,
            _ => false,
        }
    }
}

fn menu_items(
    mut menu: PopupMenu,
    entries: Vec<MenuEntry>,
    app: WeakEntity<GateApp>,
    checks: Option<MenuChecks>,
) -> PopupMenu {
    for entry in entries {
        let Some(command) = entry.command else {
            menu = menu.separator();
            continue;
        };
        let checked = checks.is_some_and(|checks| checks.checked(&command));
        let (label, shortcut) = (entry.label, entry.shortcut);
        let app = app.clone();
        menu = menu.item(
            PopupMenuItem::element(move |_, cx| {
                h_flex()
                    .w_full()
                    .gap_6()
                    .justify_between()
                    .child(label)
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(shortcut),
                    )
            })
            .checked(checked)
            .on_click(move |_, window, cx| {
                if let Err(error) =
                    app.update(cx, |this, cx| this.command(command.clone(), window, cx))
                {
                    eprintln!("menu command ignored after window closed: {error}");
                }
            }),
        );
    }
    menu
}

impl GateApp {
    fn menu_checks(&self) -> MenuChecks {
        MenuChecks {
            theme: self.theme,
            appearance: self.appearance,
            grid: self.editor.show_grid,
            snap: self.editor.snap,
        }
    }

    fn open_top_menu(&mut self, name: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        self.context_menu = None;
        let app = cx.weak_entity();
        let checks = self.menu_checks();
        let menu = PopupMenu::build(window, cx, move |menu, _, _| {
            menu_items(
                menu.min_w(px(240.0)),
                commands::entries(name),
                app,
                Some(checks),
            )
        });
        menu.focus_handle(cx).focus(window, cx);
        let dismiss = cx.subscribe_in(
            &menu,
            window,
            move |this, _, _: &DismissEvent, window, cx| {
                if this
                    .open_menu
                    .as_ref()
                    .is_some_and(|open| open.name == name)
                {
                    this.open_menu = None;
                    this.focus.focus(window, cx);
                    cx.notify();
                }
            },
        );
        self.open_menu = Some(OpenMenu {
            name,
            menu,
            _dismiss: dismiss,
        });
        cx.notify();
    }

    fn toggle_top_menu(&mut self, name: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .open_menu
            .as_ref()
            .is_some_and(|open| open.name == name)
        {
            self.open_menu = None;
            self.focus.focus(window, cx);
            cx.notify();
        } else {
            self.open_top_menu(name, window, cx);
        }
    }

    pub(super) fn menu_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut menus = h_flex().gap_0p5();
        for name in MENUS {
            let active_menu = self
                .open_menu
                .as_ref()
                .filter(|open| open.name == name)
                .map(|open| open.menu.clone());
            let is_open = active_menu.is_some();
            menus = menus.child(
                div()
                    .id(SharedString::from(format!("menu-item-{name}")))
                    .relative()
                    .on_hover(cx.listener(move |this, hovered: &bool, window, cx| {
                        if *hovered
                            && this
                                .open_menu
                                .as_ref()
                                .is_some_and(|open| open.name != name)
                        {
                            this.open_top_menu(name, window, cx);
                        }
                    }))
                    .child(
                        Button::new(SharedString::from(format!("menu-{name}")))
                            .ghost()
                            .xsmall()
                            .label(name)
                            .open(is_open)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _, window, cx| {
                                    window.prevent_default();
                                    cx.stop_propagation();
                                    this.toggle_top_menu(name, window, cx);
                                }),
                            )
                            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                                if matches!(event, ClickEvent::Mouse(_)) {
                                    return;
                                }
                                this.toggle_top_menu(name, window, cx);
                            })),
                    )
                    .when_some(active_menu, |this, menu| {
                        this.child(
                            deferred(
                                anchored()
                                    .anchor(gpui::Anchor::TopLeft)
                                    .snap_to_window_with_margin(px(8.0))
                                    .child(div().occlude().top_1().child(menu)),
                            )
                            .with_priority(gpui_base::POPUP_PRIORITY),
                        )
                    }),
            );
        }
        let theme = cx.theme();
        h_flex()
            .h(px(32.0))
            .px_2()
            .flex_none()
            .bg(theme.title_bar)
            .border_b_1()
            .border_color(theme.title_bar_border)
            .child(menus)
            .child(div().flex_1())
            .child(
                Label::new("RGate")
                    .text_xs()
                    .text_color(theme.muted_foreground),
            )
            .into_any_element()
    }

    pub(crate) fn canvas_menu_entries(&self) -> Vec<MenuEntry> {
        let menu = if !self.editor.selection.is_empty() {
            "CanvasComponent"
        } else if !self.editor.selected_wires.is_empty() {
            "CanvasWire"
        } else {
            "Canvas"
        };
        let inputs = (self.editor.selection.len() == 1)
            .then(|| {
                self.editor
                    .module()
                    .gate(*self.editor.selection.iter().next().unwrap())
            })
            .flatten()
            .map_or(0, |gate| {
                gate.pins
                    .iter()
                    .filter(|pin| pin.direction == rgate_core::Direction::Input)
                    .count()
            });
        let mut entries = commands::entries(menu);
        entries.retain(|entry| match &entry.command {
            Some(Command::SwapInputs) => inputs >= 2 && self.simulation.is_none(),
            Some(Command::Delete | Command::Rotate(_) | Command::Paste | Command::Tool(_)) => {
                self.simulation.is_none()
            }
            _ => true,
        });
        if inputs > 2 {
            for entry in &mut entries {
                if matches!(entry.command, Some(Command::SwapInputs)) {
                    entry.label = "Swap inputs…";
                }
            }
        }
        // Filtering unavailable actions must not leave dangling separators.
        let mut previous_separator = true;
        entries.retain(|entry| {
            let separator = entry.command.is_none();
            let keep = !separator || !previous_separator;
            previous_separator = separator;
            keep
        });
        if entries.last().is_some_and(|entry| entry.command.is_none()) {
            entries.pop();
        }
        entries
    }

    pub(crate) fn open_canvas_menu(
        &mut self,
        position: gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entries = self.canvas_menu_entries();
        if entries.is_empty() {
            return;
        }
        self.open_menu = None;
        let app = cx.weak_entity();
        let menu = PopupMenu::build(window, cx, move |menu, _, _| {
            menu_items(menu.min_w(px(220.0)), entries, app, None)
        });
        menu.focus_handle(cx).focus(window, cx);
        let dismiss = cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, window, cx| {
            this.context_menu = None;
            this.focus.focus(window, cx);
            cx.notify();
        });
        self.context_menu = Some(ContextMenu {
            position,
            menu,
            _dismiss: dismiss,
        });
    }

    pub(super) fn context_menu_layer(&self) -> Option<AnyElement> {
        let menu = self.context_menu.as_ref()?;
        Some(
            deferred(
                anchored()
                    .position(menu.position)
                    .snap_to_window_with_margin(px(8.0))
                    .child(menu.menu.clone()),
            )
            .with_priority(gpui_base::POPUP_PRIORITY)
            .into_any_element(),
        )
    }
}
