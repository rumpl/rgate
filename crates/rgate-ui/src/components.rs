use crate::{
    app::{GateApp, WorkspaceTab},
    commands::{self, Command},
    theme::Theme,
};
use gpui::{AnyElement, Context, FontWeight, SharedString, Window, div, prelude::*, px, rgb};
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
        GateKind::Switch
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
}

fn preview(kind: GateKind, theme: Theme) -> AnyElement {
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
        crate::classic_gates::preview(kind, theme.palette())
    } else {
        crate::modern_gates::preview(kind, theme.palette())
    }
}

impl Render for ComponentDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let palette = self.theme.palette();
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(10.))
            .h(px(32.))
            .bg(rgb(palette.panel))
            .text_color(rgb(palette.text))
            .text_size(px(12.))
            .border_1()
            .border_color(rgb(palette.gate))
            .rounded(px(palette.radius))
            .shadow_md()
            .child(preview(self.kind.clone(), self.theme))
            .child(self.label.clone())
    }
}

impl GateApp {
    pub(super) fn component_sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let palette = self.theme.palette();
        let enabled =
            self.dialog.is_none() && !self.prompting && self.tab != WorkspaceTab::Interface;
        let query = self.component_search.read(cx).value.trim().to_owned();
        let catalog = catalog()
            .into_iter()
            .filter(|component| matches_search(component, &query))
            .collect::<Vec<_>>();
        let mut result_count = catalog.len();
        let mut list = div()
            .id("component-list")
            .debug_selector(|| "component-list".into())
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(px(5.));
        for heading in GROUPS {
            if !catalog
                .iter()
                .any(|component| group(&component.kind) == *heading)
            {
                continue;
            }
            list = list.child(
                div()
                    .pt(px(10.))
                    .pb(px(5.))
                    .px(px(5.))
                    .text_size(px(11.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(palette.muted))
                    .child(*heading),
            );
            for component in catalog
                .iter()
                .filter(|component| group(&component.kind) == *heading)
            {
                list = list.child(self.component_row(component, enabled, cx));
            }
        }
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
        if !modules.is_empty() {
            list = list.child(
                div()
                    .pt(px(10.0))
                    .pb(px(5.0))
                    .px(px(5.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(palette.muted))
                    .child("Module instances"),
            );
            result_count += modules.len();
            for component in &modules {
                list = list.child(self.component_row(component, enabled, cx));
            }
        }
        if result_count == 0 {
            list = list.child(
                div()
                    .id("component-search-empty")
                    .debug_selector(|| "component-search-empty".into())
                    .p(px(10.0))
                    .text_color(rgb(palette.muted))
                    .child("No matching components."),
            );
        }
        div()
            .id("component-sidebar")
            .debug_selector(|| "component-sidebar".into())
            .w(px(self.components_width))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .bg(rgb(palette.panel))
            .border_1()
            .border_color(rgb(palette.shadow))
            .child(
                div()
                    .h(px(31.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .px(px(10.))
                    .gap(px(8.))
                    .bg(rgb(palette.chrome))
                    .border_b_1()
                    .border_color(rgb(palette.shadow))
                    .font_weight(FontWeight::BOLD)
                    .child("Components")
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_size(px(10.))
                            .font_weight(FontWeight::NORMAL)
                            .text_color(rgb(palette.muted))
                            .child(format!("{result_count} matches")),
                    ),
            )
            .child(
                div().flex_none().p(px(6.0)).h(px(40.0)).w_full().child(
                    div()
                        .id("component-search")
                        .debug_selector(|| "component-search".into())
                        .w_full()
                        .min_w_0()
                        .h(px(28.0))
                        .child(self.component_search.clone()),
                ),
            )
            .child(
                div()
                    .px(px(10.))
                    .py(px(7.))
                    .text_size(px(10.))
                    .text_color(rgb(palette.muted))
                    .child(if enabled {
                        "Drag onto canvas · click to place"
                    } else {
                        "Open Edit or Simulate to place"
                    }),
            )
            .child(list)
            .child(
                div()
                    .flex_none()
                    .flex()
                    .gap(px(5.))
                    .p(px(6.))
                    .bg(rgb(palette.chrome))
                    .child(self.button(
                        "grid",
                        "Grid",
                        Command::ToggleGrid,
                        self.editor.show_grid,
                        cx,
                    ))
                    .child(self.button("snap", "Snap", Command::ToggleSnap, self.editor.snap, cx)),
            )
            .into_any_element()
    }

    fn component_row(
        &self,
        component: &Component,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let palette = self.theme.palette();
        let kind = component.kind.clone();
        let drag = ComponentDrag {
            kind: kind.clone(),
            label: component.label.clone(),
            theme: self.theme,
        };
        let id = format!("component-{}", component.label);
        let mut row = div()
            .id(SharedString::from(id.clone()))
            .debug_selector(move || id)
            .h(px(29.))
            .w_full()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(5.))
            .rounded(px(palette.radius))
            .bg(rgb(if self.editor.tool == Tool::Place(kind.clone()) {
                palette.accent
            } else {
                palette.panel
            }))
            .text_color(rgb(if enabled { palette.text } else { palette.muted }))
            .child(preview(kind.clone(), self.theme))
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .text_size(px(11.))
                    .child(component.label.clone()),
            )
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(rgb(palette.muted))
                    .child(component.shortcut),
            );
        if enabled {
            row = row
                .cursor_pointer()
                .hover(|style| style.bg(rgb(palette.hover)))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.focus.focus(window, cx);
                    this.command(Command::Tool(Tool::Place(kind.clone())), window, cx);
                }))
                .on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone()));
        }
        row.into_any_element()
    }
}
