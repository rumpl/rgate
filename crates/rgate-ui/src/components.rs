use crate::{
    app::{GateApp, WorkspaceTab},
    commands::{self, Command},
    theme::{Palette, Theme},
};
use gpui::{AnyElement, Context, FontWeight, SharedString, Window, div, prelude::*, px};
use gpui_component::{
    ActiveTheme, Sizable as _, h_flex, input::Input, list::ListItem,
    scroll::ScrollableElement as _, v_flex,
};
use gpui_kit_assets::IconName;
use rgate_core::GateKind;
use rgate_editor::Tool;

const GROUPS: &[&str] = &[
    "Logic",
    "Inputs & outputs",
    "Sequential & memory",
    "Routing & buses",
    "Arithmetic",
    "Transistors",
    "Annotations",
];

fn group(kind: &GateKind) -> &'static str {
    match kind {
        GateKind::InputPort
        | GateKind::OutputPort
        | GateKind::InoutPort
        | GateKind::Switch
        | GateKind::Dip
        | GateKind::Led
        | GateKind::Clock
        | GateKind::Ground
        | GateKind::Vdd
        | GateKind::Tty
        | GateKind::Peripheral
        | GateKind::Vga => "Inputs & outputs",
        GateKind::Dff | GateKind::Jkff | GateKind::Register | GateKind::Ram | GateKind::Rom => {
            "Sequential & memory"
        }
        GateKind::Mux
        | GateKind::Demux
        | GateKind::Decoder
        | GateKind::Splitter
        | GateKind::Concat
        | GateKind::Tap => "Routing & buses",
        GateKind::Add
        | GateKind::Multiply
        | GateKind::Divide
        | GateKind::ShiftLeft
        | GateKind::ShiftRight
        | GateKind::ArithmeticShiftRight
        | GateKind::RotateLeft
        | GateKind::RotateRight => "Arithmetic",
        GateKind::Nmos | GateKind::Pmos => "Transistors",
        GateKind::Comment | GateKind::Frame => "Annotations",
        _ => "Logic",
    }
}

pub struct Component {
    pub label: String,
    pub shortcut: &'static str,
    pub kind: GateKind,
}

pub fn catalog() -> Vec<Component> {
    commands::entries("Components")
        .into_iter()
        .filter_map(|entry| match entry.command {
            Some(Command::Tool(Tool::Place(kind))) => Some(Component {
                label: entry.label.into(),
                shortcut: entry.shortcut,
                kind,
            }),
            _ => None,
        })
        .collect()
}

pub fn matches_search(component: &Component, query: &str) -> bool {
    let haystack = format!(
        "{} {} {}",
        component.label,
        component.kind.name(),
        group(&component.kind)
    )
    .to_lowercase();
    query
        .split_whitespace()
        .all(|word| haystack.contains(&word.to_lowercase()))
}

#[derive(Clone)]
pub struct ComponentDrag {
    pub kind: GateKind,
    label: String,
    theme: Theme,
    palette: Palette,
}

fn preview(kind: GateKind, theme: Theme, palette: Palette) -> AnyElement {
    if kind == GateKind::Comment || kind == GateKind::Frame {
        div()
            .w(px(20.))
            .text_center()
            .child(if kind == GateKind::Comment {
                "T"
            } else {
                "□"
            })
            .into_any_element()
    } else if theme == Theme::Classic && crate::classic_gates::supports(&kind) {
        crate::classic_gates::preview(kind, palette)
    } else {
        crate::modern_gates::preview(kind, palette)
    }
}

impl Render for ComponentDrag {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        h_flex()
            .gap_2()
            .px_2p5()
            .h(px(32.))
            .bg(theme.popover)
            .text_color(theme.popover_foreground)
            .text_sm()
            .border_1()
            .border_color(theme.border)
            .rounded(theme.radius)
            .shadow_md()
            .child(preview(self.kind.clone(), self.theme, self.palette))
            .child(self.label.clone())
    }
}

impl GateApp {
    pub(super) fn component_sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let enabled =
            self.dialog.is_none() && !self.prompting && self.tab != WorkspaceTab::Interface;
        let query = self.component_search.read(cx).value().trim().to_owned();
        let catalog = catalog()
            .into_iter()
            .filter(|component| matches_search(component, &query))
            .collect::<Vec<_>>();
        let modules = self
            .editor
            .circuit()
            .modules
            .iter()
            .filter(|module| module.name != self.editor.active_module())
            .map(|module| Component {
                label: module.name.clone(),
                shortcut: "",
                kind: GateKind::Module(module.name.clone()),
            })
            .filter(|component| matches_search(component, &query))
            .collect::<Vec<_>>();
        let theme = cx.theme();
        let (muted, border) = (theme.muted_foreground, theme.sidebar_border);
        let heading = move |label: &'static str| {
            div()
                .pt_2p5()
                .pb_1()
                .px_1()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(muted)
                .child(label)
        };
        let mut list = v_flex().id("component-list").flex_1().min_h_0().p_1();
        for group_name in GROUPS {
            if !catalog
                .iter()
                .any(|component| group(&component.kind) == *group_name)
            {
                continue;
            }
            list = list.child(heading(group_name));
            for component in catalog
                .iter()
                .filter(|component| group(&component.kind) == *group_name)
            {
                list = list.child(self.component_row(component, enabled, cx));
            }
        }
        if !modules.is_empty() {
            list = list.child(heading("Module instances"));
            for component in &modules {
                list = list.child(self.component_row(component, enabled, cx));
            }
        }
        if catalog.is_empty() && modules.is_empty() {
            list = list.child(
                div()
                    .p_2()
                    .text_color(muted)
                    .child("No matching components."),
            );
        }
        v_flex()
            .id("component-sidebar")
            .size_full()
            .child(
                h_flex()
                    .h(px(34.))
                    .flex_none()
                    .px_2()
                    .border_b_1()
                    .border_color(border)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Components"),
            )
            .child(
                div().flex_none().p_1p5().child(
                    Input::new(&self.component_search)
                        .small()
                        .cleanable(true)
                        .prefix(gpui_component::Icon::new(IconName::Search).small()),
                ),
            )
            .child(
                div()
                    .px_2()
                    .pb_1()
                    .text_xs()
                    .text_color(muted)
                    .child(if enabled {
                        "Drag onto canvas · click to place"
                    } else {
                        "Open Edit or Simulate to place"
                    }),
            )
            .child(list.overflow_y_scrollbar())
            .child(
                h_flex()
                    .flex_none()
                    .gap_1p5()
                    .p_1p5()
                    .border_t_1()
                    .border_color(border)
                    .child(
                        self.button(
                            "grid",
                            "Grid",
                            Command::ToggleGrid,
                            self.editor.show_grid,
                            cx,
                        )
                        .icon(IconName::Grid3x3),
                    )
                    .child(
                        self.button("snap", "Snap", Command::ToggleSnap, self.editor.snap, cx)
                            .icon(IconName::Magnet),
                    ),
            )
            .into_any_element()
    }

    fn component_row(
        &self,
        component: &Component,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let kind = component.kind.clone();
        let palette = self.palette();
        let drag = ComponentDrag {
            kind: kind.clone(),
            label: component.label.clone(),
            theme: self.theme,
            palette,
        };
        let muted = cx.theme().muted_foreground;
        let mut row = ListItem::new(SharedString::from(format!("component-{}", component.label)))
            .selected(self.editor.tool == Tool::Place(kind.clone()))
            .disabled(!enabled)
            .py_0p5()
            .child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .gap_2()
                    .child(preview(kind.clone(), self.theme, palette))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_xs()
                            .child(component.label.clone()),
                    )
                    .child(div().text_xs().text_color(muted).child(component.shortcut)),
            );
        if enabled {
            row = row
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.focus.focus(window, cx);
                    this.command(Command::Tool(Tool::Place(kind.clone())), window, cx);
                }))
                .on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone()));
        }
        row.into_any_element()
    }
}
