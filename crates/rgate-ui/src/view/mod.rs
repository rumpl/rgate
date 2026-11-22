mod menus;
mod sidebar;
mod toolbar;
mod workspace;

use crate::{
    app::{GateApp, WorkspaceTab},
    commands::{self, Command},
    theme::{Appearance, Theme},
};
use gpui::{
    AnyElement, App, AppContext, Context, Entity, MouseButton, Pixels, SharedString, Window, div,
    prelude::*, px,
};
use gpui_component::{
    ActiveTheme, Selectable as _, Sizable as _, Theme as KitTheme, ThemeMode,
    button::{Button, ButtonVariants as _},
    resizable::{ResizableState, h_resizable, resizable_panel, v_resizable},
    separator::Separator,
    status_bar::StatusBar,
    v_flex,
};
use rgate_editor::Tool;

/// Persistent split state for the main window panes.
pub struct PanelLayout {
    pub(super) main: Entity<ResizableState>,
    pub(super) sidebar: Entity<ResizableState>,
    pub(super) center: Entity<ResizableState>,
    extents: [Pixels; 3],
}

impl PanelLayout {
    pub fn new(cx: &mut App) -> Self {
        Self {
            main: cx.new(|_| ResizableState::default()),
            sidebar: cx.new(|_| ResizableState::default()),
            center: cx.new(|_| ResizableState::default()),
            extents: [Pixels::ZERO; 3],
        }
    }

    /// GPUI Kit rescales every pane proportionally when its container changes size.
    /// Side panes should keep their chosen size instead, leaving the canvas to absorb
    /// the difference (this matters most on the web, where the canvas grows after startup).
    fn keep_side_panes(&mut self, sizes: [&[(usize, f32)]; 3], window: &mut Window, cx: &mut App) {
        let states = [&self.main, &self.sidebar, &self.center];
        for ((state, extent), fixed) in states.into_iter().zip(&mut self.extents).zip(sizes) {
            let current = state.read(cx).container_size();
            if current == *extent || current <= Pixels::ZERO {
                continue;
            }
            *extent = current;
            state.update(cx, |state, cx| {
                for &(index, size) in fixed {
                    if state
                        .sizes()
                        .get(index)
                        .is_some_and(|actual| *actual != px(size))
                    {
                        state.resize_panel(index, px(size), window, cx);
                    }
                }
            });
        }
    }
}

pub fn apply_appearance(appearance: Appearance, window: Option<&mut Window>, cx: &mut App) {
    let mode = if appearance.is_dark() {
        ThemeMode::Dark
    } else {
        ThemeMode::Light
    };
    KitTheme::change(mode, window, cx);
    crate::theme::apply_chrome(appearance, cx);
}

pub fn mono_font() -> &'static str {
    if cfg!(target_family = "wasm") {
        "Lilex"
    } else {
        "Menlo"
    }
}

impl GateApp {
    pub(crate) fn button(
        &self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        command: Command,
        active: bool,
        cx: &mut Context<Self>,
    ) -> Button {
        Button::new(id.into())
            .small()
            .label(label)
            .selected(active)
            .on_click(
                cx.listener(move |this, _, window, cx| this.command(command.clone(), window, cx)),
            )
    }

    pub(crate) fn release_reset_button(&self, id: &'static str, cx: &mut Context<Self>) -> Button {
        Button::new(id)
            .small()
            .primary()
            .label("Release reset (RESET_N → 1)")
            .on_click(cx.listener(|this, _, _, cx| this.release_terminal_reset(cx)))
    }

    fn status_bar(&self) -> AnyElement {
        let file = self
            .file
            .as_ref()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.editor.circuit().title.clone());
        let dirty = if self.editor.is_dirty() { " *" } else { "" };
        let tool = match &self.editor.tool {
            Tool::Select => "Select",
            Tool::Wire => "Wire",
            Tool::Pan => "Scroll",
            Tool::Delete => "Cut",
            Tool::Place(kind) => kind.name(),
        };
        StatusBar::new()
            .left(format!("File: {file}{dirty}"))
            .left(Separator::vertical().h(px(12.0)))
            .left(format!(
                "Module: {}",
                self.debug_path
                    .as_deref()
                    .unwrap_or(self.editor.active_module())
            ))
            .child(div().min_w_0().truncate().child(self.status.clone()))
            .right(format!(
                "{tool}  {:.0}%  ({:.0}, {:.0})",
                self.editor.viewport.zoom * 100.0,
                self.mouse.x,
                self.mouse.y
            ))
            .into_any_element()
    }

    fn panes(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        self.layout.keep_side_panes(
            [
                &[(0, self.sidebar_width), (2, self.components_width)],
                &[(0, self.modules_height)],
                &[(1, self.bottom_height)],
            ],
            window,
            cx,
        );
        let content = if self.tab == WorkspaceTab::Interface {
            self.interface(cx)
        } else {
            self.schematic(window, cx)
        };
        let theme = cx.theme();
        let (sidebar_bg, background) = (theme.sidebar, theme.background);
        let main_app = cx.weak_entity();
        let center_app = cx.weak_entity();
        let center = v_resizable("center-split")
            .with_state(&self.layout.center)
            .on_resize(move |state, _, cx| {
                let height = state.read(cx).sizes().get(1).copied();
                if let Some(height) = height
                    && let Err(error) =
                        center_app.update(cx, |this, _| this.bottom_height = f32::from(height))
                {
                    eprintln!("panel resize ignored: {error}");
                }
            })
            .child(
                resizable_panel().size_range(px(160.0)..Pixels::MAX).child(
                    v_flex()
                        .size_full()
                        .bg(background)
                        .child(self.workspace_tabs(cx))
                        .child(content),
                ),
            )
            .child(
                resizable_panel()
                    .size(px(self.bottom_height))
                    .size_range(px(80.0)..px(380.0))
                    .flex_none()
                    .child(self.bottom_panel(window, cx)),
            );
        h_resizable("main-split")
            .with_state(&self.layout.main)
            .on_resize(move |state, _, cx| {
                let sizes = state.read(cx).sizes().clone();
                if let [sidebar, _, components] = sizes[..]
                    && let Err(error) = main_app.update(cx, |this, _| {
                        this.sidebar_width = f32::from(sidebar);
                        this.components_width = f32::from(components);
                    })
                {
                    eprintln!("panel resize ignored: {error}");
                }
            })
            .child(
                resizable_panel()
                    .size(px(self.sidebar_width))
                    .size_range(px(150.0)..px(400.0))
                    .flex_none()
                    .bg(sidebar_bg)
                    .child(self.sidebar(cx)),
            )
            .child(
                resizable_panel()
                    .size_range(px(200.0)..Pixels::MAX)
                    .child(center),
            )
            .child(
                resizable_panel()
                    .size(px(self.components_width))
                    .size_range(px(150.0)..px(600.0))
                    .flex_none()
                    .bg(sidebar_bg)
                    .child(self.component_sidebar(cx)),
            )
            .into_any_element()
    }
}

impl Render for GateApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use commands::*;
        #[cfg(target_family = "wasm")]
        crate::browser::set_dirty(self.editor.is_dirty());
        window.set_window_title(&format!(
            "{}{} — RGate",
            self.editor.circuit().title,
            if self.editor.is_dirty() { " *" } else { "" }
        ));
        self.sync_dialog(window, cx);
        let theme = cx.theme();
        let (background, foreground) = (theme.background, theme.foreground);
        div()
            .id("rgate")
            .key_context(if self.dialog.is_some() {
                "RGateDialog"
            } else {
                "RGate"
            })
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .relative()
            .bg(background)
            .text_color(foreground)
            .text_sm()
            .on_action(
                cx.listener(|this, _: &Find, window, cx| this.command(Command::Find, window, cx)),
            )
            .on_key_down(cx.listener(Self::key_down))
            .on_mouse_move(
                cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                    if this.drag.is_some() && !this.canvas_bounds.contains(&event.position) {
                        this.mouse_move(event, window, cx);
                    }
                }),
            )
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::mouse_up))
            .on_action(
                cx.listener(|this, _: &New, window, cx| this.command(Command::New, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &Open, window, cx| this.command(Command::Open, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &Save, window, cx| this.command(Command::Save, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &SaveAs, window, cx| {
                    this.command(Command::SaveAs, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &Export, window, cx| {
                    this.command(Command::Export, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &Undo, window, cx| this.command(Command::Undo, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &Redo, window, cx| this.command(Command::Redo, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &Cut, window, cx| this.command(Command::Cut, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &Copy, window, cx| this.command(Command::Copy, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &Paste, window, cx| this.command(Command::Paste, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &Delete, window, cx| {
                    this.command(Command::Delete, window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &SelectAll, window, cx| {
                this.command(Command::SelectAll, window, cx)
            }))
            .on_action(cx.listener(|this, _: &Rotate, window, cx| {
                this.command(Command::Rotate(true), window, cx)
            }))
            .on_action(cx.listener(|this, _: &RotateBack, window, cx| {
                this.command(Command::Rotate(false), window, cx)
            }))
            .on_action(cx.listener(|this, _: &PlayPause, window, cx| {
                this.command(Command::PlayPause, window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &Stop, window, cx| this.command(Command::Stop, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &Step, window, cx| this.command(Command::Step, window, cx)),
            )
            .on_action(cx.listener(|this, _: &ClockStep, window, cx| {
                this.command(Command::ClockStep, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ZoomIn, window, cx| {
                this.command(Command::Zoom(1.25), window, cx)
            }))
            .on_action(cx.listener(|this, _: &ZoomOut, window, cx| {
                this.command(Command::Zoom(0.8), window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &Fit, window, cx| this.command(Command::Fit, window, cx)),
            )
            .on_action(cx.listener(|this, _: &ClassicTheme, window, cx| {
                this.command(Command::SetTheme(Theme::Classic), window, cx)
            }))
            .on_action(cx.listener(|this, _: &ModernTheme, window, cx| {
                this.command(Command::SetTheme(Theme::Modern), window, cx)
            }))
            .on_action(cx.listener(|this, _: &LightAppearance, window, cx| {
                this.command(Command::SetAppearance(Appearance::Light), window, cx)
            }))
            .on_action(cx.listener(|this, _: &DarkAppearance, window, cx| {
                this.command(Command::SetAppearance(Appearance::Dark), window, cx)
            }))
            .on_action(cx.listener(|this, _: &ToggleGrid, window, cx| {
                this.command(Command::ToggleGrid, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ToggleSnap, window, cx| {
                this.command(Command::ToggleSnap, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ToggleScope, window, cx| {
                this.command(Command::ToggleScope, window, cx)
            }))
            .on_action(cx.listener(|this, _: &Properties, window, cx| {
                this.command(Command::Properties, window, cx)
            }))
            .on_action(cx.listener(|this, _: &NewModule, window, cx| {
                this.command(Command::NewModule, window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &About, window, cx| this.command(Command::About, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &Help, window, cx| this.command(Command::Help, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &Quit, window, cx| this.command(Command::Quit, window, cx)),
            )
            .child(self.menu_bar(cx))
            .child(self.toolbar(cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .child(self.panes(window, cx)),
            )
            .child(self.status_bar())
            .children(self.context_menu_layer())
    }
}
