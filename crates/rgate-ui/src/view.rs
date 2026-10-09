use crate::{
    app::{Dialog, Drag, GateApp, Popup, WorkspaceTab},
    canvas,
    commands::{self, Command},
};
use gpui::{
    AnyElement, Context, CursorStyle, FontWeight, MouseButton, SharedString, Window, anchored,
    canvas as drawing, div, point, prelude::*, px, rgb,
};
use rgate_core::{Direction, GateKind, Point};
use rgate_editor::Tool;

impl GateApp {
    fn themed_icon(&self, name: &str) -> AnyElement {
        match self.theme {
            crate::theme::Theme::Classic => crate::classic_icons::icon(name, 16.0),
            crate::theme::Theme::Modern | crate::theme::Theme::Dark => {
                crate::icons::icon(name, self.theme.palette(), 16.0)
            }
        }
    }

    fn themed_logo(&self, dimension: f32) -> AnyElement {
        crate::icons::icon("gatelogo", self.theme.palette(), dimension)
    }

    pub(super) fn button(
        &self,
        id: impl Into<SharedString>,
        label: impl Into<String>,
        command: Command,
        active: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id(id.into())
            .flex()
            .items_center()
            .justify_center()
            .px(px(9.0))
            .h(px(25.0))
            .rounded(px(self.theme.palette().radius))
            .bg(rgb(if active {
                self.theme.palette().accent
            } else {
                self.theme.palette().chrome
            }))
            .border_1()
            .border_color(rgb(if active {
                self.theme.palette().shadow
            } else {
                self.theme.palette().light
            }))
            .hover(|style| {
                style
                    .bg(rgb(self.theme.palette().hover))
                    .border_color(rgb(self.theme.palette().shadow))
            })
            .cursor_pointer()
            .child(label.into())
            .on_click(
                cx.listener(move |this, _, window, cx| this.command(command.clone(), window, cx)),
            )
            .into_any_element()
    }

    fn release_reset_button(&self, id: &'static str, cx: &mut Context<Self>) -> AnyElement {
        let palette = self.theme.palette();
        div()
            .flex()
            .child(
                div()
                    .id(id)
                    .debug_selector(move || id.into())
                    .flex()
                    .items_center()
                    .justify_center()
                    .h(px(30.0))
                    .px(px(12.0))
                    .rounded(px(palette.radius))
                    .bg(rgb(palette.accent))
                    .text_color(rgb(palette.text))
                    .border_1()
                    .border_color(rgb(palette.shadow))
                    .hover(|style| style.bg(rgb(palette.hover)).border_color(rgb(palette.gate)))
                    .active(|style| style.bg(rgb(palette.selection)))
                    .cursor_pointer()
                    .child("Release reset (RESET_N → 1)")
                    .on_click(cx.listener(|this, _, _, cx| this.release_terminal_reset(cx))),
            )
            .into_any_element()
    }

    fn icon_button(
        &self,
        icon: &'static str,
        label: &'static str,
        command: Command,
        active: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id(SharedString::from(format!("tool-{icon}")))
            .w(px(27.0))
            .h(px(27.0))
            .rounded(px(self.theme.palette().radius))
            .flex()
            .items_center()
            .justify_center()
            .bg(rgb(if active {
                self.theme.palette().accent
            } else {
                self.theme.palette().chrome
            }))
            .border_1()
            .border_color(rgb(if active {
                self.theme.palette().shadow
            } else {
                self.theme.palette().chrome
            }))
            .hover(|style| {
                style
                    .bg(rgb(self.theme.palette().hover))
                    .border_color(rgb(self.theme.palette().shadow))
            })
            .cursor_pointer()
            .child(self.themed_icon(icon))
            .on_mouse_move(cx.listener(move |this, _, _, cx| {
                if this.drag.is_none() && this.status != label {
                    this.status = label.into();
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.focus.focus(window, cx);
                this.command(command.clone(), window, cx);
            }))
            .into_any_element()
    }

    fn separator(&self) -> AnyElement {
        div()
            .w(px(2.0))
            .h(px(22.0))
            .mx(px(5.0))
            .bg(rgb(self.theme.palette().shadow))
            .border_r_1()
            .border_color(rgb(self.theme.palette().light))
            .into_any_element()
    }

    fn menu_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let entity = cx.entity().downgrade();
        let mut menus = div()
            .flex()
            .items_center()
            .gap(px(2.0))
            .h(px(27.0))
            .px(px(3.0))
            .flex_none();
        for (index, name) in [
            "File", "Edit", "View", "Tool", "Simulate", "Module", "Gate", "Arrange", "Help",
        ]
        .into_iter()
        .enumerate()
        {
            let active = self.popup.as_ref().is_some_and(|popup| popup.menu == name);
            menus = menus.child(
                div()
                    .id(SharedString::from(format!("menu-{name}")))
                    .debug_selector(|| format!("menu-{name}"))
                    .h(px(24.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .bg(rgb(if active {
                        self.theme.palette().accent
                    } else {
                        self.theme.palette().chrome
                    }))
                    .hover(|style| style.bg(rgb(self.theme.palette().light)))
                    .cursor_pointer()
                    .child(name)
                    .on_mouse_move(cx.listener(move |this, _, _, cx| {
                        if this
                            .popup
                            .as_ref()
                            .is_some_and(|popup| popup.menu != name && popup.menu != "Canvas")
                            && let Some(bounds) = this.menu_bounds.get(index)
                        {
                            this.popup = Some(Popup {
                                menu: name.into(),
                                position: Point::new(
                                    f32::from(bounds.left()),
                                    f32::from(bounds.bottom()),
                                ),
                            });
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.focus.focus(window, cx);
                        this.popup = if this.popup.as_ref().is_some_and(|popup| popup.menu == name)
                        {
                            None
                        } else {
                            Some(Popup {
                                menu: name.into(),
                                position: this.menu_bounds.get(index).map_or(
                                    Point::ZERO,
                                    |bounds| {
                                        Point::new(
                                            f32::from(bounds.left()),
                                            f32::from(bounds.bottom()),
                                        )
                                    },
                                ),
                            })
                        };
                        cx.notify();
                    })),
            );
        }
        menus
            .on_children_prepainted(move |bounds, _, cx| {
                if let Err(error) = entity.update(cx, |this, _| {
                    this.menu_bounds = bounds.into_iter().take(9).collect();
                }) {
                    eprintln!("menu layout update failed: {error}");
                }
            })
            .child(div().flex_1())
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(self.theme.palette().muted))
                    .mr(px(12.0))
                    .child("RGate 0.1 · Rust / GPUI"),
            )
            .into_any_element()
    }

    fn toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        use Command as C;
        let mode = &self.editor.tool;
        div()
            .flex()
            .items_center()
            .h(px(37.0))
            .px(px(7.0))
            .border_t_1()
            .border_b_1()
            .border_color(rgb(self.theme.palette().shadow))
            .flex_none()
            .child(self.icon_button("file_new", "New circuit (⌘N)", C::New, false, cx))
            .child(self.icon_button(
                "file_open",
                "Open TkGate .v / RGate circuit (⌘O)",
                C::Open,
                false,
                cx,
            ))
            .child(self.icon_button("file_save", "Save circuit (⌘S)", C::Save, false, cx))
            .child(self.icon_button(
                "file_saveas",
                "Save circuit as… (⇧⌘S)",
                C::SaveAs,
                false,
                cx,
            ))
            .child(self.icon_button(
                "sim_dump",
                "Export Verilog with lossless layout metadata",
                C::Export,
                false,
                cx,
            ))
            .child(self.separator())
            .child(self.icon_button("back", "Undo (⌘Z)", C::Undo, false, cx))
            .child(self.icon_button("forward", "Redo (⇧⌘Z)", C::Redo, false, cx))
            .child(self.separator())
            .child(self.icon_button(
                "edit_cut",
                "Cut selected components (⌘X)",
                C::Cut,
                false,
                cx,
            ))
            .child(self.icon_button(
                "edit_copy",
                "Copy selected components (⌘C)",
                C::Copy,
                false,
                cx,
            ))
            .child(self.icon_button("edit_paste", "Paste components (⌘V)", C::Paste, false, cx))
            .child(self.separator())
            .child(self.icon_button(
                "mov_curs",
                "Select / move (V)",
                C::Tool(Tool::Select),
                *mode == Tool::Select,
                cx,
            ))
            .child(self.icon_button(
                "net_wire",
                "Connect pins with orthogonal wires (W)",
                C::Tool(Tool::Wire),
                *mode == Tool::Wire,
                cx,
            ))
            .child(self.icon_button(
                "scroll_curs",
                "Scroll / pan canvas (P)",
                C::Tool(Tool::Pan),
                *mode == Tool::Pan,
                cx,
            ))
            .child(self.icon_button(
                "cut_curs",
                "Cut wire / delete gate (D)",
                C::Tool(Tool::Delete),
                *mode == Tool::Delete,
                cx,
            ))
            .child(self.separator())
            .child(self.icon_button(
                "edit_rotate",
                "Rotate clockwise (R)",
                C::Rotate(true),
                false,
                cx,
            ))
            .child(self.icon_button(
                "edit_brotate",
                "Rotate counterclockwise (⇧R)",
                C::Rotate(false),
                false,
                cx,
            ))
            .child(self.icon_button(
                "gateprops",
                "Gate properties (Enter)",
                C::Properties,
                false,
                cx,
            ))
            .child(self.separator())
            .child(self.icon_button(
                "sim_go",
                "Run / pause simulation (Space)",
                C::PlayPause,
                self.running,
                cx,
            ))
            .child(self.icon_button(
                "sim_pause",
                "Pause / resume simulation (Space)",
                C::PlayPause,
                self.simulation.is_some() && !self.running,
                cx,
            ))
            .child(self.icon_button("sim_stop", "Stop simulation", C::Stop, false, cx))
            .child(self.icon_button(
                "sim_step",
                "Step one simulation event (F6)",
                C::Step,
                false,
                cx,
            ))
            .child(self.icon_button(
                "sim_clock",
                "Advance one clock cycle (Tab)",
                C::ClockStep,
                false,
                cx,
            ))
            .child(self.separator())
            .child(self.icon_button("zoom_in", "Zoom in (+)", C::Zoom(1.25), false, cx))
            .child(self.icon_button("zoom_out", "Zoom out (−)", C::Zoom(0.8), false, cx))
            .child(self.button("fit", "Fit", C::Fit, false, cx))
            .child(div().flex_1())
            .child(div().mr(px(3.0)).child(self.themed_logo(35.0)))
            .into_any_element()
    }

    fn sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let sidebar_entity = cx.weak_entity();
        let mut modules = div()
            .id("module-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .bg(rgb(self.theme.palette().panel));
        let rows = if self.module_list {
            self.editor
                .circuit()
                .modules
                .iter()
                .map(|module| rgate_core::HierarchyRow {
                    path: format!("definition/{}", module.name),
                    module: module.name.clone(),
                    label: module.name.clone(),
                    depth: 0,
                    has_children: false,
                })
                .collect::<Vec<_>>()
        } else {
            self.editor
                .circuit()
                .hierarchy_rows(&self.collapsed_modules)
        };
        for row in rows {
            let name = row.module.clone();
            let path = row.path.clone();
            let active = if self.simulation.is_some() {
                self.debug_path.as_deref() == Some(row.path.as_str())
            } else {
                row.module == self.editor.active_module()
            };
            let navigate_path = row.path.clone();
            let expanded = !self.collapsed_modules.contains(&row.path);
            let mut item = div()
                .id(SharedString::from(format!("hierarchy-{}", row.path)))
                .debug_selector(|| format!("module-{}", row.module))
                .h(px(25.0))
                .flex_none()
                .overflow_hidden()
                .whitespace_nowrap()
                .flex()
                .items_center()
                .gap(px(5.0))
                .pl(px(4.0 + row.depth as f32 * 14.0))
                .pr(px(4.0))
                .bg(rgb(if active {
                    self.theme.palette().selection
                } else {
                    self.theme.palette().panel
                }))
                .hover(|style| style.bg(rgb(self.theme.palette().hover)))
                .cursor_pointer();
            if row.has_children {
                item = item.child(
                    div()
                        .id(SharedString::from(format!("expand-{}", row.path)))
                        .debug_selector(|| format!("expand-{}", row.path))
                        .w(px(12.0))
                        .flex_none()
                        .child(if expanded { "▾" } else { "▸" })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if !this.collapsed_modules.remove(&path) {
                                this.collapsed_modules.insert(path.clone());
                            }
                            cx.stop_propagation();
                            cx.notify();
                        })),
                );
            } else {
                item = item.child(div().w(px(12.0)).flex_none().child("·"));
            }
            item = item
                .child(self.themed_icon(if row.path == self.editor.circuit().root {
                    "module_root"
                } else {
                    "mod_net"
                }))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .h(px(25.0))
                        .flex()
                        .items_center()
                        .truncate()
                        .child(row.label),
                );
            if row.module != self.editor.active_module() {
                item = item.child(
                    div()
                        .id(SharedString::from(format!("instantiate-{}", row.path)))
                        .debug_selector(|| format!("instantiate-{}", row.path))
                        .w(px(22.0))
                        .flex_none()
                        .child(if cfg!(target_family = "wasm") {
                            "+"
                        } else {
                            "＋"
                        })
                        .on_click(cx.listener({
                            let name = name.clone();
                            move |this, _, window, cx| {
                                this.command(
                                    Command::Tool(Tool::Place(GateKind::Module(name.clone()))),
                                    window,
                                    cx,
                                );
                                cx.stop_propagation();
                            }
                        })),
                );
            }
            item = item.on_click(cx.listener(move |this, _, _, cx| {
                if this.module_list {
                    this.switch_module(name.clone(), cx)
                } else {
                    this.navigate_scope(navigate_path.clone(), name.clone(), cx)
                }
            }));
            modules = modules.child(item);
        }
        let mut nets = div()
            .id("net-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .bg(rgb(self.theme.palette().panel));
        let mut definitions = self
            .editor
            .module()
            .nets
            .iter()
            .filter(|net| !self.nets_ports || net.port.is_some())
            .collect::<Vec<_>>();
        definitions.sort_by(|a, b| a.name.cmp(&b.name));
        for net in definitions {
            let id = net.id;
            let selected = self.editor.selected_net == Some(id);
            let value = self
                .displayed_value(id)
                .map(ToString::to_string)
                .unwrap_or_else(|| "—".into());
            let color = self
                .displayed_value(id)
                .map(|value| self.theme.palette().signal_color(value))
                .unwrap_or_else(|| rgb(self.theme.palette().muted).into());
            let name = if net.width > 1 {
                format!("{} [{}:0]", net.name, net.width - 1)
            } else {
                net.name.clone()
            };
            nets = nets.child(
                div()
                    .id(SharedString::from(format!("net-{}", id.0)))
                    .h(px(24.0))
                    .flex()
                    .items_center()
                    .px(px(4.0))
                    .gap(px(4.0))
                    .bg(rgb(if selected {
                        self.theme.palette().selection
                    } else {
                        self.theme.palette().panel
                    }))
                    .hover(|style| style.bg(rgb(self.theme.palette().hover)))
                    .cursor_pointer()
                    .child(self.themed_icon(if net.port == Some(Direction::Output) {
                        "output"
                    } else {
                        "net_wire"
                    }))
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .text_size(px(11.0))
                            .child(name),
                    )
                    .child(
                        div()
                            .text_color(color)
                            .font_family(if cfg!(target_family = "wasm") {
                                "Lilex"
                            } else {
                                "Menlo"
                            })
                            .text_size(px(10.0))
                            .child(value),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("probe-{}", id.0)))
                            .w(px(20.0))
                            .flex()
                            .justify_center()
                            .text_color(rgb(if self.is_probed(id) {
                                self.theme.palette().selected_wire
                            } else {
                                self.theme.palette().muted
                            }))
                            .child(if self.is_probed(id) { "●" } else { "○" })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.toggle_probe(id, cx);
                                cx.stop_propagation();
                            })),
                    )
                    .on_click(cx.listener(move |this, event: &gpui::ClickEvent, _, cx| {
                        this.editor.selected_net = Some(id);
                        this.editor.selection.clear();
                        this.editor.selected_wires.clear();
                        if event.click_count() >= 2 {
                            this.toggle_probe(id, cx);
                        }
                        cx.notify();
                    })),
            );
        }
        if !self.nets_ports && self.editor.module().nets.is_empty() {
            nets = nets.child(
                div().id("nets-empty").debug_selector(|| "nets-empty".into())
                    .p(px(10.0)).text_color(rgb(self.theme.palette().muted)).text_size(px(11.0))
                    .child("No nets yet. Placing gates does not create nets. Press W, then connect two pins to add a net here."),
            );
        }
        if self.nets_ports
            && self
                .editor
                .module()
                .nets
                .iter()
                .all(|net| net.port.is_none())
        {
            nets = nets.child(div().p(px(10.0)).text_color(rgb(self.theme.palette().muted)).text_size(px(11.0)).child("No ports. Select a net in the Interface tab to expose it as an input or output."));
        }
        div()
            .w(px(self.sidebar_width))
            .flex_none()
            .flex()
            .flex_col()
            .gap_0()
            .min_h_0()
            .on_children_prepainted(move |bounds, _, cx| {
                if let Err(error) = sidebar_entity.update(cx, |this, _| {
                    if let Some(first) = bounds.first()
                        && let Some(last) = bounds.last()
                    {
                        this.sidebar_bounds =
                            gpui::Bounds::from_corners(first.origin, last.bottom_right());
                    }
                }) {
                    eprintln!("sidebar layout: {error}");
                }
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .id("modules-panel")
                    .debug_selector(|| "modules-panel".into())
                    .h(px(self.modules_height))
                    .max_h(px(if f32::from(self.sidebar_bounds.size.height) > 0.0 {
                        (f32::from(self.sidebar_bounds.size.height) - 110.0).max(80.0)
                    } else {
                        10000.0
                    }))
                    .flex_none()
                    .min_h_0()
                    .overflow_hidden()
                    .border_1()
                    .border_color(rgb(self.theme.palette().shadow))
                    .child(
                        div()
                            .h(px(28.0))
                            .flex_none()
                            .flex()
                            .items_center()
                            .bg(rgb(self.theme.palette().chrome))
                            .child(
                                div()
                                    .id("modules-tree")
                                    .debug_selector(|| "modules-tree".into())
                                    .h_full()
                                    .flex_none()
                                    .w(px(28.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .bg(rgb(if !self.module_list {
                                        self.theme.palette().accent
                                    } else {
                                        self.theme.palette().chrome
                                    }))
                                    .child(self.themed_icon("modtree"))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.module_list = false;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .id("modules-flat")
                                    .debug_selector(|| "modules-flat".into())
                                    .h_full()
                                    .flex_none()
                                    .w(px(28.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .bg(rgb(if self.module_list {
                                        self.theme.palette().accent
                                    } else {
                                        self.theme.palette().chrome
                                    }))
                                    .child(self.themed_icon("modlist"))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.module_list = true;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .debug_selector(|| "modules-heading".into())
                                    .flex_1()
                                    .h_full()
                                    .px(px(7.0))
                                    .flex()
                                    .items_center()
                                    .child("Modules"),
                            ),
                    )
                    .child(modules),
            )
            .child(
                div()
                    .id("modules-nets-splitter")
                    .debug_selector(|| "modules-nets-splitter".into())
                    .h(px(7.0))
                    .flex_none()
                    .cursor(CursorStyle::ResizeUpDown)
                    .hover(|style| style.bg(rgb(self.theme.palette().shadow)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                            this.drag = Some(Drag::Modules {
                                start_y: f32::from(event.position.y),
                                height: this.modules_height,
                            });
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .border_1()
                    .border_color(rgb(self.theme.palette().shadow))
                    .child(
                        div()
                            .h(px(28.0))
                            .flex()
                            .items_center()
                            .child(
                                div()
                                    .id("nets-tab")
                                    .h_full()
                                    .px(px(12.0))
                                    .flex()
                                    .items_center()
                                    .bg(rgb(if !self.nets_ports {
                                        self.theme.palette().panel
                                    } else {
                                        self.theme.palette().chrome
                                    }))
                                    .border_r_1()
                                    .border_color(rgb(self.theme.palette().shadow))
                                    .cursor_pointer()
                                    .child("Nets")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.nets_ports = false;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .id("ports-tab")
                                    .h_full()
                                    .px(px(12.0))
                                    .flex()
                                    .items_center()
                                    .bg(rgb(if self.nets_ports {
                                        self.theme.palette().panel
                                    } else {
                                        self.theme.palette().chrome
                                    }))
                                    .border_r_1()
                                    .border_color(rgb(self.theme.palette().shadow))
                                    .cursor_pointer()
                                    .child("Ports")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.nets_ports = true;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(nets)
                    .child(
                        div()
                            .h(px(22.0))
                            .border_t_1()
                            .border_color(rgb(self.theme.palette().shadow))
                            .px(px(6.0))
                            .text_size(px(10.0))
                            .flex()
                            .items_center()
                            .child(format!(
                                "{} nets · click ○ to probe",
                                self.editor.module().nets.len()
                            )),
                    ),
            )
            .into_any_element()
    }

    fn workspace_tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut bar = div()
            .h(px(31.0))
            .flex()
            .items_center()
            .border_b_1()
            .border_color(rgb(self.theme.palette().shadow))
            .flex_none();
        for (tab, label, icon) in [
            (WorkspaceTab::Edit, "Edit", "editmode"),
            (WorkspaceTab::Interface, "Interface", "iface"),
            (WorkspaceTab::Simulate, "Simulate", "simulate"),
        ] {
            bar = bar.child(
                div()
                    .id(SharedString::from(format!("workspace-{label}")))
                    .w(px(142.0))
                    .h_full()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .px(px(12.0))
                    .bg(rgb(if self.tab == tab {
                        self.theme.palette().panel
                    } else {
                        self.theme.palette().chrome
                    }))
                    .border_r_1()
                    .border_t_1()
                    .border_color(rgb(if self.tab == tab {
                        self.theme.palette().light
                    } else {
                        self.theme.palette().shadow
                    }))
                    .font_weight(if self.tab == tab {
                        FontWeight::BOLD
                    } else {
                        FontWeight::NORMAL
                    })
                    .hover(|style| style.bg(rgb(self.theme.palette().light)))
                    .cursor_pointer()
                    .child(self.themed_icon(icon))
                    .child(label)
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.set_tab(tab, window, cx)),
                    ),
            );
        }
        if self.editor.module().verilog.is_some() {
            bar = bar.child(
                div()
                    .id("edit-verilog-source")
                    .px(px(8.0))
                    .cursor_pointer()
                    .child("Verilog source…")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.command(Command::EditVerilog, window, cx)
                    })),
            );
        }
        if let Some(path) = &self.debug_path {
            if let Some((parent, _)) = path.rsplit_once('/') {
                let parent = parent.to_owned();
                bar = bar.child(
                    div()
                        .id("debug-up")
                        .debug_selector(|| "debug-up".into())
                        .px(px(8.0))
                        .cursor_pointer()
                        .child("↑ Parent")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(name) = this
                                .simulation
                                .as_ref()
                                .and_then(|sim| sim.scope_module(&parent))
                                .map(ToOwned::to_owned)
                            {
                                this.navigate_scope(parent.clone(), name, cx);
                            }
                        })),
                );
            }
            bar = bar.child(div().px(px(8.0)).text_size(px(11.0)).child(format!(
                "Live: {path} · {}",
                self.simulation.as_ref().unwrap().backend_name()
            )));
        }
        let state = if self.running {
            "Running"
        } else if self.simulation.is_some() {
            "Paused"
        } else {
            "Edit mode"
        };
        bar.child(div().flex_1())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .mr(px(9.0))
                    .text_size(px(11.0))
                    .child(div().size(px(6.0)).rounded_full().bg(rgb(if self.running {
                        self.theme.palette().high
                    } else {
                        self.theme.palette().muted
                    })))
                    .child(state),
            )
            .into_any_element()
    }

    fn schematic(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        #[cfg(not(target_family = "wasm"))]
        if self.tab == WorkspaceTab::Edit && self.editor.module().verilog.is_some() {
            return self.source_editor(window, cx);
        }
        #[cfg(target_family = "wasm")]
        let _ = window;
        let scene = self.scene();
        let entity = cx.entity().downgrade();
        let cursor = match self.editor.tool {
            Tool::Wire | Tool::Place(_) => CursorStyle::Crosshair,
            Tool::Pan => CursorStyle::OpenHand,
            Tool::Delete => CursorStyle::Crosshair,
            _ => CursorStyle::Arrow,
        };
        div()
            .id("schematic")
            .debug_selector(|| "schematic".into())
            .flex_1()
            .size_full()
            .min_h_0()
            .min_w_0()
            .relative()
            .overflow_hidden()
            .bg(rgb(self.theme.palette().panel))
            .cursor(cursor)
            .child(
                drawing(
                    move |bounds, _, cx| match entity.update(cx, |this, _| {
                        let previous_size = this.canvas_bounds.size;
                        this.canvas_bounds = bounds;
                        if cfg!(target_family = "wasm")
                            && (f32::from(previous_size.width) < 100.0
                                || f32::from(previous_size.height) < 100.0)
                            && f32::from(bounds.size.width) >= 100.0
                            && f32::from(bounds.size.height) >= 100.0
                        {
                            this.need_fit = true;
                        }
                        if this.need_fit {
                            let content_bounds = this.editor.module().bounds();
                            this.editor.viewport.fit(
                                content_bounds,
                                Point::new(
                                    f32::from(bounds.size.width),
                                    f32::from(bounds.size.height),
                                ),
                            );
                            this.need_fit = false;
                        }
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
            .on_scroll_wheel(cx.listener(Self::scroll))
            .on_pinch(cx.listener(Self::pinch))
            .into_any_element()
    }

    fn interface(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut rows = div()
            .id("interface-ports")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .border_1()
            .border_color(rgb(self.theme.palette().shadow))
            .bg(rgb(self.theme.palette().panel));
        for net in &self.editor.module().nets {
            let id = net.id;
            let selected = self.editor.selected_net == Some(id);
            rows = rows.child(
                div()
                    .id(SharedString::from(format!("interface-net-{}", id.0)))
                    .flex()
                    .h(px(29.0))
                    .items_center()
                    .gap(px(12.0))
                    .px(px(10.0))
                    .cursor_pointer()
                    .bg(rgb(if selected {
                        self.theme.palette().selection
                    } else {
                        self.theme.palette().panel
                    }))
                    .border_b_1()
                    .border_color(rgb(self.theme.palette().hover))
                    .hover(|style| style.bg(rgb(self.theme.palette().hover)))
                    .child(div().w(px(170.0)).child(net.name.clone()))
                    .child(div().w(px(60.0)).child(format!("{} bit", net.width)))
                    .child(div().flex_1().child(match net.port {
                        Some(Direction::Input) => "input",
                        Some(Direction::Output) => "output",
                        Some(Direction::InOut) => "inout",
                        None => "internal",
                    }))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.editor.selected_net = Some(id);
                        cx.notify();
                    })),
            );
        }
        div().flex_1().min_h_0().flex().flex_col().p(px(20.0)).gap(px(12.0)).bg(rgb(self.theme.palette().light))
            .child(div().text_size(px(16.0)).font_weight(FontWeight::BOLD).child(format!("Module interface: {}", self.editor.active_module())))
            .child("Select a net, then set its direction. Ports are included in Verilog exports.")
            .child(div().flex().gap(px(7.0))
                .child(self.button("port-input", "Input", Command::Port(Some(Direction::Input)), false, cx))
                .child(self.button("port-output", "Output", Command::Port(Some(Direction::Output)), false, cx))
                .child(self.button("port-inout", "InOut", Command::Port(Some(Direction::InOut)), false, cx))
                .child(self.button("port-internal", "Internal", Command::Port(None), false, cx)))
            .child(rows)
            .child(div().text_size(px(11.0)).text_color(rgb(self.theme.palette().muted)).child("Ports become pins on module instances. Click ＋ beside a module to place it in another module. Recursive instances are rejected."))
            .into_any_element()
    }

    fn bottom_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut panel = div()
            .h(px(self.bottom_height))
            .flex_none()
            .flex()
            .flex_col()
            .border_1()
            .border_color(rgb(self.theme.palette().shadow));
        panel = panel.child(
            div()
                .h(px(25.0))
                .flex()
                .items_center()
                .child(
                    div()
                        .id("messages-tab")
                        .h_full()
                        .px(px(12.0))
                        .flex()
                        .items_center()
                        .gap(px(5.0))
                        .bg(rgb(if !self.bottom_scope {
                            self.theme.palette().panel
                        } else {
                            self.theme.palette().chrome
                        }))
                        .cursor_pointer()
                        .child(self.themed_icon("log"))
                        .child("Messages")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.bottom_scope = false;
                            cx.notify();
                        })),
                )
                .child(
                    div()
                        .id("waveforms-tab")
                        .h_full()
                        .px(px(12.0))
                        .flex()
                        .items_center()
                        .gap(px(5.0))
                        .bg(rgb(if self.bottom_scope {
                            self.theme.palette().panel
                        } else {
                            self.theme.palette().chrome
                        }))
                        .cursor_pointer()
                        .child(self.themed_icon("sim_view"))
                        .child("Waveforms")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.bottom_scope = true;
                            cx.notify();
                        })),
                )
                .child(div().flex_1())
                .child(div().text_size(px(11.0)).mr(px(9.0)).child(format!(
                    "Time: {} ns",
                    self.simulation.as_ref().map_or(0, |sim| sim.time())
                ))),
        );
        if self.bottom_scope {
            panel = panel.child(self.waveform_panel(cx));
        } else {
            self.message_log
                .update(cx, |log, _| log.set_messages(&self.messages));
            let messages = div()
                .id("messages-content")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .bg(rgb(self.theme.palette().panel))
                .p(px(8.0))
                .child(self.message_log.clone());
            panel = panel.child(messages);
        }
        panel.into_any_element()
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
        div()
            .h(px(25.0))
            .flex_none()
            .flex()
            .gap(px(4.0))
            .items_center()
            .px(px(5.0))
            .pb(px(3.0))
            .text_size(px(11.0))
            .child(
                self.theme
                    .palette()
                    .sunken()
                    .w(px(225.0))
                    .h(px(20.0))
                    .px(px(5.0))
                    .overflow_hidden()
                    .child(format!("File: {file}{dirty}")),
            )
            .child(
                self.theme
                    .palette()
                    .sunken()
                    .w(px(150.0))
                    .h(px(20.0))
                    .px(px(5.0))
                    .child(format!(
                        "Module: {}",
                        self.debug_path
                            .as_deref()
                            .unwrap_or(self.editor.active_module())
                    )),
            )
            .child(
                self.theme
                    .palette()
                    .sunken()
                    .flex_1()
                    .min_w_0()
                    .h(px(20.0))
                    .px(px(5.0))
                    .overflow_hidden()
                    .child(self.status.clone()),
            )
            .child(
                self.theme
                    .palette()
                    .sunken()
                    .w(px(187.0))
                    .h(px(20.0))
                    .px(px(5.0))
                    .child(format!(
                        "{tool}  {:.0}%  ({:.0}, {:.0})",
                        self.editor.viewport.zoom * 100.0,
                        self.mouse.x,
                        self.mouse.y
                    )),
            )
            .into_any_element()
    }

    fn popup(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let popup = self.popup.as_ref()?;
        let mut items = div()
            .occlude()
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .on_pinch(|_, _, cx| cx.stop_propagation())
            .on_mouse_down_out(cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                if this.dialog.is_none()
                    && !this
                        .menu_bounds
                        .iter()
                        .any(|bounds| bounds.contains(&event.position))
                {
                    this.popup = None;
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .w(px(280.0))
            .id("popup-items")
            .debug_selector(|| "popup-items".into())
            .max_h(px(480.0))
            .overflow_y_scroll()
            .bg(rgb(self.theme.palette().chrome))
            .border_1()
            .border_color(rgb(self.theme.palette().shadow))
            .rounded(px(self.theme.palette().radius))
            .shadow_lg()
            .py(px(3.0))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        for (index, entry) in commands::entries(&popup.menu).into_iter().enumerate() {
            if let Some(command) = entry.command {
                let selected = matches!(command, Command::SetTheme(theme) if theme == self.theme);
                items = items.child(
                    div()
                        .id(SharedString::from(format!("popup-{index}")))
                        .debug_selector(|| format!("popup-item-{index}"))
                        .h(px(25.0))
                        .flex()
                        .items_center()
                        .justify_between()
                        .px(px(11.0))
                        .cursor_pointer()
                        .hover(|style| style.bg(rgb(self.theme.palette().accent)))
                        .child(format!(
                            "{}{}",
                            if selected { "✓  " } else { "" },
                            entry.label
                        ))
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(rgb(self.theme.palette().muted))
                                .child(entry.shortcut),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.command(command.clone(), window, cx);
                            cx.stop_propagation();
                        })),
                );
            } else {
                items = items.child(
                    div()
                        .h(px(1.0))
                        .my(px(4.0))
                        .mx(px(4.0))
                        .bg(rgb(self.theme.palette().shadow)),
                );
            }
        }
        Some(
            div()
                .absolute()
                .inset_0()
                .child(
                    anchored()
                        .position(point(px(popup.position.x), px(popup.position.y)))
                        .snap_to_window()
                        .child(items),
                )
                .into_any_element(),
        )
    }

    fn dialog_header(
        &self,
        title: &'static str,
        closable: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let palette = self.theme.palette();
        let mut header = div()
            .h(px(32.0))
            .flex_none()
            .flex()
            .items_center()
            .pl(px(10.0))
            .pr(px(4.0))
            .gap(px(8.0))
            .bg(rgb(palette.accent))
            .font_weight(FontWeight::BOLD)
            .child(div().flex_1().child(title));
        if closable {
            header = header.child(
                div()
                    .id("dialog-header-close")
                    .debug_selector(|| "dialog-header-close".into())
                    .size(px(24.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(palette.radius))
                    .text_size(px(20.0))
                    .font_weight(FontWeight::NORMAL)
                    .text_color(rgb(palette.text))
                    .hover(|style| style.bg(rgb(palette.hover)))
                    .active(|style| style.bg(rgb(palette.selection)))
                    .cursor_pointer()
                    .child("×")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.dismiss_dialog(window, cx);
                        cx.stop_propagation();
                    })),
            );
        }
        header.into_any_element()
    }

    fn dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dialog = self.dialog.as_ref()?;
        let (title, width) = match dialog {
            Dialog::Verilog { .. } => ("Verilog module source", 900.0),
            Dialog::HdlInput { .. } => ("Set HDL input (hex)", 400.0),
            Dialog::Symbol(_) => ("Module symbol editor", 640.0),
            Dialog::Find { .. } => ("Find gates and nets", 580.0),
            Dialog::Vga { .. } => ("VGA display", 620.0),
            Dialog::Recovery => ("Recover unsaved circuit", 500.0),
            Dialog::Properties(properties)
                if self
                    .editor
                    .module()
                    .gate(properties.gate)
                    .is_some_and(|gate| gate.kind == GateKind::Frame) =>
            {
                ("Frame properties", 500.0)
            }
            Dialog::Properties(properties)
                if self
                    .editor
                    .module()
                    .gate(properties.gate)
                    .is_some_and(|gate| gate.kind == GateKind::Comment) =>
            {
                ("Comment properties", 500.0)
            }
            Dialog::Properties(_) => ("Gate properties", 500.0),
            Dialog::Input { .. } => ("Set DIP input", 400.0),
            Dialog::Memory { .. } => ("Memory inspector", 580.0),
            Dialog::Net(_) => ("Net properties", 440.0),
            Dialog::Terminal { .. } => ("Terminal I/O", 580.0),
            Dialog::NewModule { .. } => ("New module", 380.0),
            Dialog::Help => ("Getting started", 590.0),
            Dialog::About => ("About RGate", 550.0),
        };
        let mut content = div().flex().flex_col().gap(px(10.0)).p(px(16.0));
        let mut apply = false;
        match dialog {
            Dialog::Verilog {
                creating,
                name,
                source,
                interface,
                error,
                ..
            } => {
                apply = true;
                content = content
                    .child("Module name (must match source declaration)")
                    .child(name.clone());
                if *creating || cfg!(target_family = "wasm") {
                    content=content.child("Initial source — trusted HDL only. After creation, edit in the Edit tab.")
                        .child(div().id("verilog-source-scroll").h(px(300.0)).overflow_y_scroll().child(source.clone()));
                }
                content = content
                    .child("Interface / observed signals: input|output|inout|signal name width")
                    .child(
                        div()
                            .id("verilog-interface-scroll")
                            .h(px(130.0))
                            .overflow_y_scroll()
                            .child(interface.clone()),
                    );
                if let Some(error) = error {
                    content = content.child(
                        div()
                            .text_color(rgb(self.theme.palette().error))
                            .child(error.clone()),
                    );
                }
            }
            Dialog::HdlInput { value, error, .. } => {
                apply = true;
                content = content.child(value.clone());
                if let Some(error) = error {
                    content = content.child(error.clone());
                }
            }
            Dialog::Vga { gate } => {
                content = content.child(self.vga_display(*gate, cx));
                if self.held_reset_input().is_some() {
                    content = content.child(self.release_reset_button("vga-release-reset", cx));
                }
            }
            Dialog::Find { query } => {
                content = content.child(query.clone());
                let results = rgate_editor::search(self.editor.circuit(), &query.read(cx).value);
                let mut list = div().id("find-results").h(px(300.0)).overflow_y_scroll();
                if results.is_empty() {
                    list = list.child(
                        "Type to find gates/nets across module definitions (up to 200 results).",
                    );
                }
                for (index, result) in results.into_iter().enumerate() {
                    list = list.child(
                        div()
                            .id(SharedString::from(format!("find-{index}")))
                            .debug_selector(|| format!("find-{index}"))
                            .h(px(28.0))
                            .cursor_pointer()
                            .truncate()
                            .child(format!(
                                "{} / {} — {}",
                                result.module, result.name, result.description
                            ))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.select_search_result(result.clone(), window, cx)
                            })),
                    );
                }
                content = content.child(list);
            }
            Dialog::Recovery => {
                content=content.child("An unsaved circuit was found from your previous session. Simulation runtime state is not restored.")
                    .child(div().flex().gap(px(12.0))
                        .child(div().id("recover-circuit").debug_selector(||"recover-circuit".into()).cursor_pointer().child("Recover circuit").on_click(cx.listener(|this,_,window,cx|this.restore_recovery(window,cx))))
                        .child(div().id("discard-recovery").cursor_pointer().child("Discard recovery").on_click(cx.listener(|this,_,window,cx|this.discard_recovery(window,cx)))));
            }
            Dialog::Symbol(_) => {
                apply = true;
                content = content.child(crate::symbol_editor::editor(self, cx));
            }
            Dialog::Input { value, error, .. } => {
                apply = true;
                content = content.child(Self::field_row("Hex value", value.clone()));
                if let Some(error) = error {
                    content = content.child(error.clone());
                }
            }
            Dialog::Memory {
                gate,
                address,
                value,
                error,
            } => {
                apply = true;
                let words = self
                    .simulation
                    .as_ref()
                    .and_then(|sim| {
                        self.simulation_gate(*gate)
                            .and_then(|id| sim.memory_words(id).ok())
                    })
                    .unwrap_or_default();
                let mut listing = div()
                    .id("memory-words")
                    .h(px(220.0))
                    .overflow_y_scroll()
                    .bg(rgb(self.theme.palette().panel))
                    .font_family(if cfg!(target_family = "wasm") {
                        "Lilex"
                    } else {
                        "Menlo"
                    })
                    .p(px(8.0));
                for (address, word) in words.iter().take(512) {
                    listing = listing.child(format!("{address:04X}: {}", word.display_value()));
                }
                content = content.child(listing).child("Runtime edits are volatile; set initial contents in gate properties to save them.")
                    .child(Self::field_row("Address (decimal)",address.clone())).child(Self::field_row("Value (hex)",value.clone()));
                if let Some(error) = error {
                    content = content.child(
                        div()
                            .text_color(rgb(self.theme.palette().error))
                            .child(error.clone()),
                    );
                }
            }
            Dialog::Net(properties) => {
                apply = true;
                content = content
                    .child(Self::field_row("Name", properties.name.clone()))
                    .child(Self::field_row("Bit width", properties.width.clone()))
                    .child(
                        div()
                            .id("net-show-name")
                            .cursor_pointer()
                            .child(if properties.show_name {
                                "☑ Show net label"
                            } else {
                                "☐ Show net label"
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(Dialog::Net(dialog)) = &mut this.dialog {
                                    dialog.show_name = !dialog.show_name;
                                }
                                cx.notify();
                            })),
                    );
                if let Some(error) = &properties.error {
                    content = content.child(
                        div()
                            .text_color(rgb(self.theme.palette().error))
                            .child(error.clone()),
                    );
                }
            }
            Dialog::Terminal { gate, input, error } => {
                apply = true;
                let output = self
                    .simulation
                    .as_ref()
                    .and_then(|sim| {
                        self.simulation_gate(*gate)
                            .and_then(|id| sim.terminal_output(id).ok())
                    })
                    .unwrap_or_default();
                let empty = output.is_empty();
                let mode = self
                    .editor
                    .module()
                    .gate(*gate)
                    .is_some_and(|gate| gate.config.tty_tkgate);
                content = content.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(self.button(
                            "terminal-run",
                            if self.running { "Pause" } else { "Run" },
                            Command::PlayPause,
                            self.running,
                            cx,
                        ))
                        .child(self.button(
                            "terminal-step",
                            "Clock step",
                            Command::ClockStep,
                            false,
                            cx,
                        ))
                        .child(format!(
                            "{} · {} ns",
                            if self.running { "Running" } else { "Paused" },
                            self.simulation.as_ref().map_or(0, |sim| sim.time())
                        )),
                );
                if self.held_reset_input().is_some() {
                    content = content
                        .child(div().text_color(rgb(self.theme.palette().error)).child(
                            "RESET_N is 0: the circuit is held in reset and cannot execute.",
                        ))
                        .child(self.release_reset_button("terminal-release-reset", cx));
                }
                content=content.child(div().id("terminal-output").h(px(200.0)).overflow_y_scroll().bg(rgb(0x202020)).text_color(rgb(0xeeeeee)).font_family(if cfg!(target_family="wasm"){"Lilex"}else{"Menlo"}).p(px(8.0))
                    .child(if empty{"No output bytes received yet. Run/step the circuit after releasing reset.".into()}else{output}))
                    .child(if mode {"TkGate mode: RD/DSR send output; TD/RTS/CTS receive input."}else{"TX writes a byte on WR rising edge. RX/READY expose queued input; RD rising edge consumes a byte."})
                    .child(Self::field_row("Send UTF-8 bytes",input.clone()));
                if let Some(error) = error {
                    content = content.child(
                        div()
                            .text_color(rgb(self.theme.palette().error))
                            .child(error.clone()),
                    );
                }
            }
            Dialog::Properties(properties) => {
                apply = true;
                let gate = self.editor.module().gate(properties.gate).unwrap();
                content = content.child(
                    div()
                        .font_weight(FontWeight::BOLD)
                        .child(gate.kind.name().to_owned()),
                );
                content = content.child(Self::field_row("Name", properties.name.clone()));
                let fields = crate::properties::fields(&gate.kind);
                for (label, field, visible) in [
                    ("Bit width", &properties.width, fields.width),
                    ("Initial value", &properties.value, fields.initial),
                    ("Delay (ns)", &properties.delay, fields.delay),
                    ("Clock period (ns)", &properties.period, fields.clock),
                ] {
                    if visible {
                        content = content.child(Self::field_row(label, field.clone()));
                    }
                }
                if gate.kind.is_logic() {
                    content = content.child(
                        div()
                            .id("gate-reduction")
                            .cursor_pointer()
                            .child(if properties.reduction {
                                "☑ Reduce input bus to one bit"
                            } else {
                                "☐ Reduce input bus to one bit"
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(Dialog::Properties(dialog)) = &mut this.dialog {
                                    dialog.reduction = !dialog.reduction;
                                }
                                cx.notify();
                            })),
                    );
                }
                if gate.kind == GateKind::TriState {
                    content = content
                        .child(
                            div()
                                .id("tri-enable-low")
                                .cursor_pointer()
                                .child(if properties.enable_low {
                                    "☑ Active-low enable"
                                } else {
                                    "☐ Active-low enable"
                                })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(Dialog::Properties(dialog)) = &mut this.dialog {
                                        dialog.enable_low = !dialog.enable_low;
                                    }
                                    cx.notify();
                                })),
                        )
                        .child(
                            div()
                                .id("tri-invert")
                                .cursor_pointer()
                                .child(if properties.invert_output {
                                    "☑ Invert enabled output"
                                } else {
                                    "☐ Invert enabled output"
                                })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(Dialog::Properties(dialog)) = &mut this.dialog {
                                        dialog.invert_output = !dialog.invert_output;
                                    }
                                    cx.notify();
                                })),
                        );
                }
                if matches!(gate.kind, GateKind::Module(_)) {
                    content=content.child(Self::field_row("Block width",properties.module_width.clone()))
                        .child(Self::field_row("Port positions JSON",properties.custom_ports.clone()))
                        .child("Coordinates are relative to the block center; e.g. {\"A\":{\"x\":-50,\"y\":0}}.");
                }
                if gate.kind == GateKind::Tty {
                    content = content.child(
                        div()
                            .id("tty-tkgate")
                            .cursor_pointer()
                            .child(if properties.tty_tkgate {
                                "☑ TkGate TD/RD/RTS/CTS/DSR/DTR protocol"
                            } else {
                                "☐ TkGate protocol (otherwise TX/RX/WR/RD)"
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(Dialog::Properties(dialog)) = &mut this.dialog {
                                    dialog.tty_tkgate = !dialog.tty_tkgate;
                                }
                                cx.notify();
                            })),
                    );
                }
                if gate.kind == GateKind::Clock {
                    content = content
                        .child(Self::field_row(
                            "Phase delay (ns)",
                            properties.phase.clone(),
                        ))
                        .child(Self::field_row("High duty (%)", properties.duty.clone()));
                }
                if matches!(gate.kind, GateKind::Multiply | GateKind::Divide) {
                    content = content
                        .child(Self::field_row(
                            "Operand A width",
                            properties.operand_a.clone(),
                        ))
                        .child(Self::field_row(
                            "Operand B width",
                            properties.operand_b.clone(),
                        ));
                }
                if gate.kind == GateKind::Frame {
                    content = content
                        .child(Self::field_row(
                            "Frame width",
                            properties.frame_width.clone(),
                        ))
                        .child(Self::field_row(
                            "Frame height",
                            properties.frame_height.clone(),
                        ))
                        .child(Self::field_row("Title", properties.comment.clone()));
                }
                if gate.kind == GateKind::Led {
                    let mut modes = div().flex().flex_col().gap(px(3.0)).child("Display type");
                    for mode in rgate_core::LedDisplay::ALL {
                        modes = modes.child(
                            div()
                                .id(SharedString::from(format!("led-mode-{mode:?}")))
                                .debug_selector(|| format!("led-mode-{mode:?}"))
                                .cursor_pointer()
                                .h(px(22.0))
                                .child(format!(
                                    "{} {}",
                                    if properties.led_display == mode {
                                        "◉"
                                    } else {
                                        "○"
                                    },
                                    mode.label()
                                ))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.select_led_display(mode, cx);
                                })),
                        );
                    }
                    let width = properties
                        .width
                        .read(cx)
                        .value
                        .parse::<u16>()
                        .ok()
                        .filter(|width| (1..=4096).contains(width));
                    content = content.child(modes).child(
                        div()
                            .id("led-width-preview")
                            .font_weight(FontWeight::BOLD)
                            .child(
                                width
                                    .map(|width| properties.led_display.preview(width))
                                    .unwrap_or_else(|| "Enter a width from 1 to 4096.".into()),
                            ),
                    );
                    let connected = gate.pins.iter().any(|pin| pin.net.is_some());
                    content = content.child(if connected {
                        "Wired LED: its bus width is preserved when you change display type."
                    } else {
                        "Choosing a display type sets the recommended width automatically. You can override it."
                    });
                    if properties.led_display == rgate_core::LedDisplay::SevenSegment {
                        content = content.child("Segment map: bit 0 top; 1 upper-left; 2 upper-right; 3 middle; 4 lower-left; 5 lower-right; 6 bottom.");
                    }
                }
                if matches!(
                    gate.kind,
                    GateKind::Mux | GateKind::Decoder | GateKind::Demux
                ) || gate.kind.is_logic()
                {
                    content = content.child(Self::field_row(
                        "Input/output count",
                        properties.inputs.clone(),
                    ));
                }
                if matches!(gate.kind, GateKind::Concat | GateKind::Splitter) {
                    content = content.child(Self::field_row(
                        "Partitions (LSB first)",
                        properties.partitions.clone(),
                    ));
                }
                if gate.kind == GateKind::Tap {
                    content = content
                        .child(Self::field_row(
                            "Tap bit offset",
                            properties.tap_offset.clone(),
                        ))
                        .child(Self::field_row(
                            "Tap output width",
                            properties.tap_width.clone(),
                        ));
                }
                if matches!(gate.kind, GateKind::Ram | GateKind::Rom) {
                    content = content.child(Self::field_row("Address bits", properties.address_bits.clone()))
                        .child(Self::field_row("Initial hex words", properties.memory.clone()))
                        .child(self.button("load-memory-image", "Load hex file…", Command::LoadMemory, false, cx))
                        .child("Hex words, e.g. 41 42 FF or @10 AB CD. Unspecified words are X. RAM CS/OE/WE and ROM OE are active-low.");
                }
                if gate.kind == GateKind::Comment {
                    for (index, link) in rgate_core::comment_links(&gate.text)
                        .into_iter()
                        .enumerate()
                    {
                        if link.target.starts_with("https://") || link.target.starts_with("http://")
                        {
                            content = content.child(
                                div()
                                    .id(SharedString::from(format!("comment-link-{index}")))
                                    .cursor_pointer()
                                    .child(format!("Open link: {}", link.label))
                                    .on_click(move |_, _, cx| cx.open_url(&link.target)),
                            );
                        }
                    }
                    content = content.child(Self::field_row(
                        "Text (\\n = newline)",
                        properties.comment.clone(),
                    ));
                }
                content = content.child(
                    div()
                        .id("property-show-name")
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .cursor_pointer()
                        .child(if properties.show_name { "☑" } else { "☐" })
                        .child("Show instance name")
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(Dialog::Properties(dialog)) = &mut this.dialog {
                                dialog.show_name = !dialog.show_name;
                            }
                            cx.notify();
                        })),
                );
                if let Some(error) = &properties.error {
                    content = content.child(
                        div()
                            .text_color(rgb(self.theme.palette().error))
                            .child(error.clone()),
                    );
                }
            }
            Dialog::NewModule { name, error } => {
                apply = true;
                content = content.child(Self::field_row("Module name", name.clone()));
                if let Some(error) = error {
                    content = content.child(
                        div()
                            .text_color(rgb(self.theme.palette().error))
                            .child(error.clone()),
                    );
                }
            }
            Dialog::Help => {
                for (label, text) in [
                    (
                        "Build a circuit",
                        "Choose a gate in Make or the parts toolbar, then click the canvas. W starts wiring: click a pin, add corners, and click the destination pin. V returns to selection.",
                    ),
                    (
                        "Edit",
                        "Drag to move gates; Shift-click selects more than one. R rotates. Delete removes the selection. Double-click a gate for properties. ⌘/Ctrl-Z undoes an edit.",
                    ),
                    (
                        "Simulate",
                        "Space runs or pauses. Click switches to toggle; DIP switches increment. F6 steps one event; Tab advances a clock cycle. The square Stop button returns to editing.",
                    ),
                    (
                        "Watch signals",
                        "Double-click a wire or click ○ in Nets to add a waveform probe. X means unknown; Z means floating. Buses support up to 64 bits.",
                    ),
                    (
                        "Navigate and save",
                        "Scroll to pan; ⌘/Ctrl-scroll zooms; ⌘/Ctrl-0 fits. Save .rgate for native files, or export .v with layout metadata. Use File → Open to load a circuit from the examples folder.",
                    ),
                    (
                        "Current scope",
                        "This is a working foundation, not a complete TkGate port. Arbitrary HDL, Tcl peripheral plugins, timing checks, and symbol editing are not implemented. Unsupported imported elements are reported in Messages.",
                    ),
                ] {
                    content = content
                        .child(div().font_weight(FontWeight::BOLD).child(label))
                        .child(text);
                }
            }
            Dialog::About => {
                content = content.child(div().flex().gap(px(16.0)).items_center()
                    .child(self.themed_logo(70.0))
                    .child(div().flex().flex_col().gap(px(4.0)).child(div().text_size(px(20.0)).font_weight(FontWeight::BOLD).child("RGate 0.1"))
                        .child("A Rust / GPUI reimplementation of TkGate")))
                    .child("Inspired by TkGate. Classic artwork is adapted from TkGate reference images; original artwork © Jeffery P. Hansen and contributors, GPL-2.0-or-later.")
                    .child("RGate and Classic artwork adaptations are GPL-3.0-or-later. GPUI/GPUI Kit are Apache-2.0; bundled Lucide/Feather icons are ISC/MIT. See LICENSE, NOTICE and the distribution license inventory.")
                    .child("Independent reimplementation; not an official TkGate release. Distributed without warranty.");
            }
        }
        let mut buttons = div()
            .flex()
            .justify_end()
            .gap(px(8.0))
            .px(px(16.0))
            .pb(px(14.0));
        if apply {
            if !matches!(dialog, Dialog::Recovery) {
                buttons = buttons.child(
                    div()
                        .id("dialog-apply")
                        .rounded(px(self.theme.palette().radius))
                        .h(px(27.0))
                        .px(px(18.0))
                        .flex()
                        .items_center()
                        .bg(rgb(self.theme.palette().accent))
                        .border_1()
                        .border_color(rgb(self.theme.palette().shadow))
                        .cursor_pointer()
                        .child("Apply")
                        .on_click(cx.listener(|this, _, window, cx| this.apply_dialog(window, cx))),
                );
            }
            buttons = buttons.child(
                div()
                    .id("dialog-close")
                    .rounded(px(self.theme.palette().radius))
                    .h(px(27.0))
                    .px(px(18.0))
                    .flex()
                    .items_center()
                    .bg(rgb(self.theme.palette().chrome))
                    .border_1()
                    .border_color(rgb(self.theme.palette().shadow))
                    .cursor_pointer()
                    .child(if apply { "Cancel" } else { "Close" })
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.dismiss_dialog(window, cx);
                    })),
            );
        }
        Some(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(gpui::rgba(0x00000040))
                .occlude()
                .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                .on_pinch(|_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .id("dialog-content")
                        .debug_selector(|| "dialog-content".into())
                        .w(px(width))
                        .rounded(px(self.theme.palette().radius))
                        .overflow_hidden()
                        .max_h(px(740.0))
                        .bg(rgb(self.theme.palette().chrome))
                        .border_1()
                        .border_color(rgb(self.theme.palette().shadow))
                        .shadow_lg()
                        .flex()
                        .flex_col()
                        .child(self.dialog_header(title, !matches!(dialog, Dialog::Recovery), cx))
                        .child(content)
                        .child(buttons),
                )
                .into_any_element(),
        )
    }

    fn field_row(label: &'static str, field: gpui::Entity<crate::input::TextField>) -> AnyElement {
        div()
            .debug_selector(move || format!("property-field-{label}"))
            .flex()
            .items_center()
            .gap(px(10.0))
            .child(div().w(px(130.0)).flex_none().child(label))
            .child(div().flex_1().child(field))
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
        let mut root = div()
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
            .bg(rgb(self.theme.palette().chrome))
            .text_color(rgb(self.theme.palette().text))
            .font_family(if cfg!(target_family = "wasm") {
                "IBM Plex Sans"
            } else {
                self.theme.palette().font
            })
            .text_size(px(12.0))
            .on_action(
                cx.listener(|this, _: &Find, window, cx| this.command(Command::Find, window, cx)),
            )
            .on_key_down(cx.listener(Self::key_down))
            .on_mouse_move(
                cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                    if matches!(
                        this.drag,
                        Some(
                            Drag::Sidebar
                                | Drag::Bottom
                                | Drag::Modules { .. }
                                | Drag::Components { .. }
                        )
                    ) || (this.drag.is_some() && !this.canvas_bounds.contains(&event.position))
                    {
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
                this.command(Command::SetTheme(crate::theme::Theme::Classic), window, cx)
            }))
            .on_action(cx.listener(|this, _: &ModernTheme, window, cx| {
                this.command(Command::SetTheme(crate::theme::Theme::Modern), window, cx)
            }))
            .on_action(cx.listener(|this, _: &DarkTheme, window, cx| {
                this.command(Command::SetTheme(crate::theme::Theme::Dark), window, cx)
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
            );
        let content = if self.tab == WorkspaceTab::Interface {
            self.interface(cx)
        } else {
            self.schematic(window, cx)
        };
        root = root
            .child(self.menu_bar(cx))
            .child(self.toolbar(cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .flex()
                    .p(px(7.0))
                    .child(self.sidebar(cx))
                    .child(
                        div()
                            .id("sidebar-splitter")
                            .debug_selector(|| "sidebar-splitter".into())
                            .w(px(7.0))
                            .flex_none()
                            .cursor(CursorStyle::ResizeLeftRight)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    this.drag = Some(Drag::Sidebar);
                                    cx.notify();
                                }),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .flex_1()
                                    .min_h_0()
                                    .flex()
                                    .flex_col()
                                    .border_1()
                                    .border_color(rgb(self.theme.palette().shadow))
                                    .child(self.workspace_tabs(cx))
                                    .child(content),
                            )
                            .child(
                                div()
                                    .id("bottom-splitter")
                                    .debug_selector(|| "bottom-splitter".into())
                                    .h(px(6.0))
                                    .flex_none()
                                    .cursor(CursorStyle::ResizeUpDown)
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _, _, cx| {
                                            this.drag = Some(Drag::Bottom);
                                            cx.notify();
                                        }),
                                    ),
                            )
                            .child(self.bottom_panel(cx)),
                    )
                    .child(
                        div()
                            .id("components-splitter")
                            .debug_selector(|| "components-splitter".into())
                            .w(px(7.0))
                            .flex_none()
                            .cursor(CursorStyle::ResizeLeftRight)
                            .hover(|style| style.bg(rgb(self.theme.palette().shadow)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                                    this.drag = Some(Drag::Components {
                                        start_x: f32::from(event.position.x),
                                        width: this.components_width,
                                    });
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ),
                    )
                    .child(self.component_sidebar(cx)),
            )
            .child(self.status_bar());
        if let Some(popup) = self.popup(cx) {
            root = root.child(popup);
        }
        if let Some(dialog) = self.dialog(cx) {
            root = root.child(dialog);
        }
        root
    }
}
