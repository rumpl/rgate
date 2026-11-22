use crate::{
    app::{GateApp, WorkspaceTab},
    canvas,
    commands::Command,
};
use gpui::{
    AnyElement, Context, CursorStyle, FontWeight, MouseButton, SharedString, Window,
    canvas as drawing, div, prelude::*, px, rgb,
};
use gpui_component::{
    ActiveTheme, Icon, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::Textarea,
    label::Label,
    list::ListItem,
    scroll::ScrollableElement as _,
    tab::{Tab, TabBar},
    v_flex,
};
use gpui_kit_assets::IconName;
use rgate_core::Direction;
use rgate_editor::Tool;

impl GateApp {
    pub(super) fn workspace_tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        const TABS: [WorkspaceTab; 3] = [
            WorkspaceTab::Edit,
            WorkspaceTab::Interface,
            WorkspaceTab::Simulate,
        ];
        let muted = cx.theme().muted_foreground;
        let mut status = h_flex().gap_2().pr_2().text_xs();
        if let Some(path) = &self.sim.debug_path {
            if let Some((parent, _)) = path.rsplit_once('/') {
                let parent = parent.to_owned();
                status = status.child(
                    Button::new("debug-up")
                        .ghost()
                        .xsmall()
                        .icon(IconName::ArrowUp)
                        .label("Parent")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(name) = this
                                .sim
                                .live
                                .as_ref()
                                .and_then(|sim| sim.scope_module(&parent))
                                .map(ToOwned::to_owned)
                            {
                                this.navigate_scope(parent.clone(), name, cx);
                            }
                        })),
                );
            }
            if let Some(simulation) = &self.sim.live {
                status = status.child(
                    div()
                        .text_color(muted)
                        .child(format!("Live: {path} · {}", simulation.backend_name())),
                );
            }
        }
        let compiling = self.hdl.is_compiling();
        if compiling || self.sim.is_active() {
            let state = if compiling {
                "Compiling · Stop cancels"
            } else if self.sim.running {
                "Running"
            } else {
                "Paused"
            };
            let theme = cx.theme();
            status = status.child(
                h_flex()
                    .gap_1p5()
                    .px_2()
                    .py_0p5()
                    .rounded(px(3.0))
                    .bg(theme.secondary)
                    .child(
                        Icon::new(if self.sim.running {
                            IconName::Play
                        } else {
                            IconName::Pause
                        })
                        .small()
                        .text_color(if self.sim.running {
                            theme.success
                        } else {
                            theme.foreground
                        }),
                    )
                    .child(state),
            );
        }
        TabBar::new("workspace-tabs")
            .underline()
            .small()
            .selected_index(
                TABS.iter()
                    .position(|tab| *tab == self.chrome.tab)
                    .unwrap_or(0),
            )
            .on_click(cx.listener(move |this, index: &usize, window, cx| {
                this.set_tab(TABS[*index], window, cx)
            }))
            .child(
                Tab::new()
                    .label("Edit")
                    .prefix(Icon::new(IconName::Pencil).small()),
            )
            .child(
                Tab::new()
                    .label("Interface")
                    .prefix(Icon::new(IconName::Plug).small()),
            )
            .child(
                Tab::new()
                    .label("Simulate")
                    .prefix(Icon::new(IconName::Activity).small()),
            )
            .suffix(status)
            .into_any_element()
    }

    pub(super) fn schematic(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        #[cfg(not(target_family = "wasm"))]
        if self.chrome.tab == WorkspaceTab::Edit && self.editor.module().verilog.is_some() {
            return self.source_editor(window, cx);
        }
        #[cfg(target_family = "wasm")]
        let _ = window;
        let scene = self.scene();
        let entity = cx.entity().downgrade();
        let cursor = match self.editor.tool {
            Tool::Wire | Tool::Place(_) | Tool::Delete => CursorStyle::Crosshair,
            Tool::Pan => CursorStyle::OpenHand,
            _ => CursorStyle::Arrow,
        };
        div()
            .id("schematic")
            .flex_1()
            .size_full()
            .min_h_0()
            .min_w_0()
            .relative()
            .overflow_hidden()
            .bg(rgb(self.palette().panel))
            .cursor(cursor)
            .child(
                drawing(
                    move |bounds, _, cx| match entity.update(cx, |this, _| {
                        this.canvas.update_bounds_and_fit(&mut this.editor, bounds);
                        this.scene()
                    }) {
                        Ok(scene) => scene,
                        Err(error) => {
                            eprintln!("canvas prepaint failed: {error}");
                            scene
                        }
                    },
                    canvas::paint,
                )
                .size_full(),
            )
            .on_drop(cx.listener(
                |this, component: &crate::components::ComponentDrag, window, cx| {
                    this.drop_component(
                        component.kind.clone(),
                        window.mouse_position(),
                        window,
                        cx,
                    );
                },
            ))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::mouse_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::mouse_up))
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_scroll(cx.listener(Self::scroll))
            .on_pinch(cx.listener(Self::pinch))
            .into_any_element()
    }

    pub(super) fn interface(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let (muted, border, radius) = (theme.muted_foreground, theme.border, theme.radius);
        let mut rows = v_flex().id("interface-ports").p_1();
        for net in &self.editor.module().nets {
            let id = net.id;
            rows = rows.child(
                ListItem::new(SharedString::from(format!("interface-net-{}", id.0)))
                    .text_sm()
                    .selected(self.editor.selection.net == Some(id))
                    .child(
                        h_flex()
                            .w_full()
                            .gap_3()
                            .child(div().w(px(170.0)).truncate().child(net.name.clone()))
                            .child(
                                div()
                                    .w(px(60.0))
                                    .text_color(muted)
                                    .child(format!("{} bit", net.width)),
                            )
                            .child(div().flex_1().child(match net.port {
                                Some(Direction::Input) => "input",
                                Some(Direction::Output) => "output",
                                Some(Direction::InOut) => "inout",
                                None => "internal",
                            })),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.editor.selection.net = Some(id);
                        cx.notify();
                    })),
            );
        }
        v_flex()
            .flex_1()
            .min_h_0()
            .p_4()
            .gap_3()
            .child(
                div()
                    .text_lg()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(format!("Module interface: {}", self.editor.active_module())),
            )
            .child(
                Label::new(
                    "Select a net, then set its direction. Ports are included in Verilog exports.",
                )
                .text_color(muted),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(self.button(
                        "port-input",
                        "Input",
                        Command::Port(Some(Direction::Input)),
                        false,
                        cx,
                    ))
                    .child(self.button(
                        "port-output",
                        "Output",
                        Command::Port(Some(Direction::Output)),
                        false,
                        cx,
                    ))
                    .child(self.button(
                        "port-inout",
                        "InOut",
                        Command::Port(Some(Direction::InOut)),
                        false,
                        cx,
                    ))
                    .child(self.button(
                        "port-internal",
                        "Internal",
                        Command::Port(None),
                        false,
                        cx,
                    )),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .border_1()
                    .border_color(border)
                    .rounded(radius)
                    .child(rows.size_full().overflow_y_scrollbar()),
            )
            .child(
                Label::new("Ports become pins on module instances. Click + beside a module to place it in another module. Recursive instances are rejected.")
                    .text_xs()
                    .text_color(muted),
            )
            .into_any_element()
    }

    fn message_log(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        self.console.sync_view(window, cx);
        div()
            .id("messages-content")
            .flex_1()
            .min_h_0()
            .child(
                Textarea::new(&self.console.log_view)
                    .readonly(true)
                    .bordered(false)
                    .small()
                    .h(gpui::relative(1.0)),
            )
            .into_any_element()
    }

    pub(super) fn bottom_panel(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let content = if self.chrome.bottom_scope {
            self.waveform_panel(window, cx)
        } else {
            self.message_log(window, cx)
        };
        v_flex()
            .size_full()
            .child(
                TabBar::new("bottom-tabs")
                    .underline()
                    .xsmall()
                    .selected_index(usize::from(self.chrome.bottom_scope))
                    .on_click(cx.listener(|this, index: &usize, _, cx| {
                        this.chrome.bottom_scope = *index == 1;
                        cx.notify();
                    }))
                    .child(
                        Tab::new()
                            .label("Messages")
                            .prefix(Icon::new(IconName::ScrollText).small()),
                    )
                    .child(
                        Tab::new()
                            .label("Waveforms")
                            .prefix(Icon::new(IconName::AudioWaveform).small()),
                    )
                    .suffix(div().pr_2().text_xs().text_color(muted).child(format!(
                        "Time: {} ns",
                        self.sim.time()
                    ))),
            )
            .child(content)
            .into_any_element()
    }
}
