//! Zed's editor and language buffer, fetched at the same revision as GPUI.
use crate::{
    app::{GateApp, WorkspaceTab},
    commands::Command,
};
use gpui::{AppContext, Context, Entity, Focusable, Window, div, prelude::*, px, rgb};
use gpui_component::input::{Editor, EditorState, InputEvent};
use std::sync::Arc;

pub struct SourceEditor {
    pub module: String,
    pub input: Entity<EditorState>,
    pub _subscription: gpui::Subscription,
}
pub fn init(cx: &mut gpui::App) {
    if !cx.has_global::<gpui_component::Theme>() {
        gpui_component::init(cx);
    }
    if gpui_component::highlighter::LanguageRegistry::singleton()
        .language("verilog")
        .is_none()
    {
        gpui_component::highlighter::LanguageRegistry::singleton().register(
            "verilog",
            &gpui_component::highlighter::LanguageConfig::new(
                "verilog",
                tree_sitter_verilog::LANGUAGE.into(),
                vec![],
                include_str!("verilog-highlights.scm"),
                "",
                "",
            ),
        );
    }
}
fn syntax_style(color: u32) -> gpui_component::highlighter::ThemeStyle {
    serde_json::from_value(serde_json::json!({"color":gpui::Hsla::from(rgb(color)),"font_style":null,"font_weight":null})).expect("valid syntax style")
}
fn sync_theme(theme: crate::theme::Theme, window: &mut Window, cx: &mut gpui::App) {
    let dark = theme == crate::theme::Theme::Dark;
    let mode = if dark {
        gpui_component::ThemeMode::Dark
    } else {
        gpui_component::ThemeMode::Light
    };
    if gpui_component::Theme::global(cx).mode != mode {
        gpui_component::Theme::change(mode, Some(window), cx);
    }
    let palette = theme.palette();
    let current = gpui_component::Theme::global_mut(cx);
    current.colors.background = rgb(palette.panel).into();
    current.colors.foreground = rgb(palette.text).into();
    current.colors.muted = rgb(palette.panel).into();
    current.colors.muted_foreground = rgb(palette.muted).into();
    current.colors.border = rgb(palette.shadow).into();
    current.colors.selection = rgb(palette.selection).into();
    current.mono_font_family = if cfg!(target_os = "macos") {
        "Menlo"
    } else {
        "DejaVu Sans Mono"
    }
    .into();
    current.mono_font_size = px(14.0);
    let mut highlight = (*current.highlight_theme).clone();
    highlight.style.editor_background = Some(rgb(palette.panel).into());
    highlight.style.editor_foreground = Some(rgb(palette.text).into());
    highlight.style.editor_active_line = Some(rgb(palette.light).into());
    highlight.style.editor_line_number = Some(rgb(palette.muted).into());
    highlight.style.editor_active_line_number = Some(rgb(palette.text).into());
    highlight.style.syntax.keyword = Some(syntax_style(if dark { 0xc4b5fd } else { 0x7c3aed }));
    highlight.style.syntax.comment = Some(syntax_style(if dark { 0x94a3b8 } else { 0x64748b }));
    highlight.style.syntax.number = Some(syntax_style(if dark { 0xfbbf24 } else { 0xb45309 }));
    highlight.style.syntax.string = Some(syntax_style(if dark { 0x86efac } else { 0x15803d }));
    highlight.style.syntax.type_ = Some(syntax_style(if dark { 0x67e8f9 } else { 0x0e7490 }));
    highlight.style.syntax.function = Some(syntax_style(if dark { 0x93c5fd } else { 0x1d4ed8 }));
    highlight.style.syntax.operator = Some(syntax_style(if dark { 0x7dd3fc } else { 0x0369a1 }));
    highlight.style.syntax.variable = Some(syntax_style(palette.text));
    if *current.highlight_theme != highlight {
        current.highlight_theme = Arc::new(highlight);
    }
}
impl GateApp {
    pub fn source_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        init(cx);
        sync_theme(self.theme, window, cx);
        let module = self.editor.active_module().to_owned();
        let value = self.editor.module().verilog.clone().unwrap_or_default();
        if self
            .source_editor
            .as_ref()
            .is_none_or(|editor| editor.module != module)
        {
            let input = cx.new(|cx| {
                EditorState::new(window, cx)
                    .language("verilog")
                    .line_number(true)
                    .soft_wrap(false)
                    .default_value(value)
            });
            let name = module.clone();
            let subscription = cx.subscribe(&input, move |this, input, event, cx| {
                if matches!(event, InputEvent::Change)
                    && this.simulation.is_none()
                    && this.editor.active_module() == name
                {
                    let source = input.read(cx).value().to_string();
                    if let Err(error) = this
                        .editor
                        .edit_module(|module| module.verilog = Some(source))
                    {
                        this.log(error.to_string());
                    } else {
                        this.changed();
                    }
                    cx.notify();
                }
            });
            self.source_editor = Some(SourceEditor {
                module,
                input,
                _subscription: subscription,
            });
        }
        let input = self.source_editor.as_ref().unwrap().input.clone();
        let source = self.editor.module().verilog.as_ref().unwrap();
        if input.read(cx).value().as_ref() != source {
            let source = source.clone();
            input.update(cx, |editor, cx| editor.set_value(source, window, cx));
        }
        let palette = self.theme.palette();
        div()
            .id("verilog-edit-tab")
            .flex_1()
            .size_full()
            .min_h_0()
            .min_w_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .p(px(8.0))
                    .bg(rgb(palette.chrome))
                    .child(format!(
                        "{}.sv · Verilog/SystemVerilog",
                        self.editor.active_module()
                    ))
                    .child(
                        div()
                            .id("verilog-interface-button")
                            .cursor_pointer()
                            .child("Interface…")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.command(Command::VerilogInterface, window, cx)
                            })),
                    )
                    .child("Changes saved to document"),
            )
            .child(
                div().flex_1().min_h_0().min_w_0().child(
                    Editor::new(&input)
                        .h(gpui::relative(1.0))
                        .readonly(self.simulation.is_some()),
                ),
            )
            .into_any_element()
    }
    pub fn source_editor_focused(&self, window: &Window, cx: &gpui::App) -> bool {
        self.tab == WorkspaceTab::Edit
            && self
                .source_editor
                .as_ref()
                .is_some_and(|editor| editor.input.read(cx).focus_handle(cx).is_focused(window))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[gpui::test]
    fn published_editor_highlights_verilog_and_matches_dark_palette(cx: &mut gpui::TestAppContext) {
        let (_, cx) = cx.add_window_view(|window, cx| {
            init(cx);
            sync_theme(crate::theme::Theme::Dark, window, cx);
            crate::input::TextField::new("", cx)
        });
        cx.update(|_, cx| {
            let theme = gpui_component::Theme::global(cx);
            assert_eq!(
                theme.highlight_theme.style.editor_background,
                Some(rgb(crate::theme::Theme::Dark.palette().panel).into())
            );
            assert_eq!(
                theme.mono_font_family.as_ref(),
                if cfg!(target_os = "macos") {
                    "Menlo"
                } else {
                    "DejaVu Sans Mono"
                }
            );
            let mut highlighter = gpui_component::highlighter::SyntaxHighlighter::new("verilog");
            let source = "module test; wire [7:0] data = 8'hFF; // comment\nendmodule";
            highlighter.update(None, &source.into(), None);
            let styles = highlighter.styles(&(0..source.len()), theme.highlight_theme.as_ref());
            for (token, capture) in [
                ("module", "keyword"),
                ("7", "number"),
                ("// comment", "comment"),
            ] {
                let offset = source.find(token).unwrap();
                let actual = styles
                    .iter()
                    .find(|(range, _)| range.contains(&offset))
                    .unwrap()
                    .1
                    .color;
                assert_eq!(
                    actual,
                    theme.highlight_theme.style(capture).unwrap().color,
                    "{token}"
                );
            }
        });
    }
}
