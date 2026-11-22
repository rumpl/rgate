use super::mono_font;
use crate::{app::GateApp, commands::Command};
use gpui::{AnyElement, Context, FontWeight, Pixels, SharedString, div, prelude::*, px};
use gpui_component::{
    ActiveTheme, Icon, Selectable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    list::ListItem,
    resizable::{resizable_panel, v_resizable},
    scroll::ScrollableElement as _,
    tab::{Tab, TabBar},
    v_flex,
};
use gpui_kit_assets::IconName;
use rgate_core::{Direction, GateKind};
use rgate_editor::Tool;

impl GateApp {
    fn modules_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let rows = if self.chrome.module_list {
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
                .hierarchy_rows(&self.chrome.collapsed_modules)
        };
        let mut list = v_flex().id("module-list").flex_1().min_h_0().p_1();
        for row in rows {
            let name = row.module.clone();
            let active = if self.sim.is_active() {
                self.sim.debug_path.as_deref() == Some(row.path.as_str())
            } else {
                row.module == self.editor.active_module()
            };
            let expander = if row.has_children {
                let path = row.path.clone();
                let expanded = !self.chrome.collapsed_modules.contains(&row.path);
                Button::new(SharedString::from(format!("expand-{}", row.path)))
                    .ghost()
                    .xsmall()
                    .icon(if expanded {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.chrome.collapsed_modules.remove(&path) {
                            this.chrome.collapsed_modules.insert(path.clone());
                        }
                        cx.stop_propagation();
                        cx.notify();
                    }))
                    .into_any_element()
            } else {
                div().size(px(20.0)).flex_none().into_any_element()
            };
            let mut content = h_flex()
                .w_full()
                .min_w_0()
                .gap_1()
                .child(expander)
                .child(
                    Icon::new(if row.path == self.editor.circuit().root {
                        IconName::Cpu
                    } else {
                        IconName::Box
                    })
                    .small()
                    .text_color(muted),
                )
                .child(div().flex_1().min_w_0().truncate().child(row.label));
            if row.module != self.editor.active_module() {
                let name = name.clone();
                content = content.child(
                    Button::new(SharedString::from(format!("instantiate-{}", row.path)))
                        .ghost()
                        .xsmall()
                        .icon(IconName::Plus)
                        .tooltip("Place an instance of this module")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.command(
                                Command::Tool(Tool::Place(GateKind::Module(name.clone()))),
                                window,
                                cx,
                            );
                            cx.stop_propagation();
                        })),
                );
            }
            let navigate_path = row.path.clone();
            list = list.child(
                ListItem::new(SharedString::from(format!("hierarchy-{}", row.path)))
                    .text_sm()
                    .selected(active)
                    .py_0p5()
                    .pl(px(4.0 + row.depth as f32 * 14.0))
                    .child(content)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if this.chrome.module_list {
                            this.switch_module(name.clone(), cx)
                        } else {
                            this.navigate_scope(navigate_path.clone(), name.clone(), cx)
                        }
                    })),
            );
        }
        v_flex()
            .size_full()
            .child(
                h_flex()
                    .h(px(34.0))
                    .flex_none()
                    .px_2()
                    .gap_1()
                    .border_b_1()
                    .border_color(cx.theme().sidebar_border)
                    .child(
                        div()
                            .flex_1()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Modules"),
                    )
                    .child(
                        Button::new("modules-tree")
                            .ghost()
                            .xsmall()
                            .icon(IconName::ListTree)
                            .tooltip("Instance hierarchy")
                            .selected(!self.chrome.module_list)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.chrome.module_list = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("modules-flat")
                            .ghost()
                            .xsmall()
                            .icon(IconName::List)
                            .tooltip("Module definitions")
                            .selected(self.chrome.module_list)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.chrome.module_list = true;
                                cx.notify();
                            })),
                    ),
            )
            .child(list.overflow_y_scrollbar())
            .into_any_element()
    }

    fn nets_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let (muted, border) = (theme.muted_foreground, theme.sidebar_border);
        let mut definitions = self
            .editor
            .module()
            .nets
            .iter()
            .filter(|net| !self.chrome.nets_ports || net.port.is_some())
            .collect::<Vec<_>>();
        definitions.sort_by(|a, b| a.name.cmp(&b.name));
        let mut list = v_flex().id("net-list").flex_1().min_h_0().p_1();
        for net in definitions {
            let id = net.id;
            let value = self
                .sim
                .displayed_value(id)
                .map(ToString::to_string)
                .unwrap_or_else(|| "—".into());
            let color = self
                .sim
                .displayed_value(id)
                .map(|value| self.palette().signal_color(value))
                .unwrap_or(muted);
            let name = if net.width > 1 {
                format!("{} [{}:0]", net.name, net.width - 1)
            } else {
                net.name.clone()
            };
            let probed = self.sim.is_probed(id);
            list = list.child(
                ListItem::new(SharedString::from(format!("net-{}", id.0)))
                    .selected(self.editor.selected_net == Some(id))
                    .py_0p5()
                    .child(
                        h_flex()
                            .w_full()
                            .min_w_0()
                            .gap_1p5()
                            .child(
                                Icon::new(if net.port == Some(Direction::Output) {
                                    IconName::ArrowRight
                                } else {
                                    IconName::Spline
                                })
                                .small()
                                .text_color(muted),
                            )
                            .child(div().flex_1().min_w_0().truncate().text_xs().child(name))
                            .child(
                                div()
                                    .text_color(color)
                                    .font_family(mono_font())
                                    .text_xs()
                                    .child(value),
                            )
                            .child(
                                Button::new(SharedString::from(format!("probe-{}", id.0)))
                                    .ghost()
                                    .xsmall()
                                    .icon(if probed {
                                        IconName::CircleDot
                                    } else {
                                        IconName::Circle
                                    })
                                    .tooltip(if probed {
                                        "Remove waveform probe"
                                    } else {
                                        "Add waveform probe"
                                    })
                                    .selected(probed)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.toggle_probe(id, cx);
                                        cx.stop_propagation();
                                    })),
                            ),
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
        let empty_hint = if !self.chrome.nets_ports && self.editor.module().nets.is_empty() {
            Some(
                "No nets yet. Placing gates does not create nets. Press W, then connect two pins to add a net here.",
            )
        } else if self.chrome.nets_ports
            && self
                .editor
                .module()
                .nets
                .iter()
                .all(|net| net.port.is_none())
        {
            Some("No ports. Select a net in the Interface tab to expose it as an input or output.")
        } else {
            None
        };
        if let Some(hint) = empty_hint {
            list = list.child(div().p_2().text_xs().text_color(muted).child(hint));
        }
        v_flex()
            .size_full()
            .child(
                h_flex().h(px(34.0)).flex_none().px_2().child(
                    TabBar::new("nets-tabs")
                        .segmented()
                        .xsmall()
                        .selected_index(usize::from(self.chrome.nets_ports))
                        .on_click(cx.listener(|this, index: &usize, _, cx| {
                            this.chrome.nets_ports = *index == 1;
                            cx.notify();
                        }))
                        .child(Tab::new().label("Nets"))
                        .child(Tab::new().label("Ports")),
                ),
            )
            .child(list.overflow_y_scrollbar())
            .child(
                div()
                    .flex_none()
                    .px_2()
                    .py_1()
                    .border_t_1()
                    .border_color(border)
                    .text_xs()
                    .text_color(muted)
                    .child(format!(
                        "{} nets · click ○ to probe",
                        self.editor.module().nets.len()
                    )),
            )
            .into_any_element()
    }

    pub(super) fn sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let app = cx.weak_entity();
        v_resizable("sidebar-split")
            .with_state(&self.chrome.panels.sidebar)
            .on_resize(move |state, _, cx| {
                let height = state.read(cx).sizes().first().copied();
                if let Some(height) = height
                    && let Err(error) = app.update(cx, |this, _| {
                        this.chrome.modules_height = f32::from(height)
                    })
                {
                    eprintln!("sidebar resize ignored: {error}");
                }
            })
            .child(
                resizable_panel()
                    .size(px(self.chrome.modules_height))
                    .size_range(px(80.0)..Pixels::MAX)
                    .child(self.modules_panel(cx)),
            )
            .child(
                resizable_panel()
                    .size_range(px(110.0)..Pixels::MAX)
                    .child(self.nets_panel(cx)),
            )
            .into_any_element()
    }
}
