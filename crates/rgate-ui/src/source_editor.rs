//! Published code editor and Verilog language integration.
use crate::{app::GateApp, commands::Command};
use gpui::{AppContext, Context, Entity, Window, div, prelude::*, px, rgb};
use gpui_component::{
    ActiveTheme, Sizable as _,
    button::Button,
    h_flex,
    input::{Editor, EditorState, InputEvent},
};
use std::sync::Arc;

pub struct SourceEditor {
    pub module: String,
    pub input: Entity<EditorState>,
    pub _subscription: gpui::Subscription,
}
pub fn init() {
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
/// Tunes syntax colors and the code font; the editor surface itself follows the GPUI Kit theme.
fn sync_syntax_theme(dark: bool, cx: &mut gpui::App) {
    let current = gpui_component::Theme::global_mut(cx);
    current.mono_font_family = crate::view::mono_font().into();
    current.mono_font_size = px(14.0);
    let mut highlight = (*current.highlight_theme).clone();
    highlight.style.syntax.keyword = Some(syntax_style(if dark { 0xc4b5fd } else { 0x7c3aed }));
    highlight.style.syntax.comment = Some(syntax_style(if dark { 0x94a3b8 } else { 0x64748b }));
    highlight.style.syntax.number = Some(syntax_style(if dark { 0xfbbf24 } else { 0xb45309 }));
    highlight.style.syntax.string = Some(syntax_style(if dark { 0x86efac } else { 0x15803d }));
    highlight.style.syntax.type_ = Some(syntax_style(if dark { 0x67e8f9 } else { 0x0e7490 }));
    highlight.style.syntax.function = Some(syntax_style(if dark { 0x93c5fd } else { 0x1d4ed8 }));
    highlight.style.syntax.operator = Some(syntax_style(if dark { 0x7dd3fc } else { 0x0369a1 }));
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
        init();
        sync_syntax_theme(self.appearance.is_dark(), cx);
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
                    this.hdl_diagnostics.clear();
                    #[cfg(feature = "icarus")]
                    {
                        this.lint_due =
                            Some(std::time::Instant::now() + std::time::Duration::from_millis(350));
                    }
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
        let diagnostics = self
            .hdl_diagnostics
            .iter()
            .filter(|d| d.module == self.editor.active_module())
            .cloned()
            .collect::<Vec<_>>();
        let text = input.read(cx).value().to_string();
        input.update(cx, |state, cx| {
            if let Some(set) = state.diagnostics_mut() {
                set.clear();
                for diagnostic in &diagnostics {
                    let row = diagnostic.line.saturating_sub(1);
                    let len = text
                        .lines()
                        .nth(row as usize)
                        .map_or(1, |line| line.encode_utf16().count().max(1))
                        as u32;
                    set.push(
                        gpui_component::highlighter::Diagnostic::new(
                            gpui_component::input::Position::new(row, 0)
                                ..gpui_component::input::Position::new(row, len),
                            diagnostic.message.clone(),
                        )
                        .with_severity(gpui_component::highlighter::DiagnosticSeverity::Error),
                    );
                }
            }
            if let Some(line) = self.diagnostic_line.take() {
                state.set_cursor_position(
                    gpui_component::input::Position::new(line.saturating_sub(1), 0),
                    window,
                    cx,
                );
            }
        });
        let theme = cx.theme();
        let border = theme.border;
        let muted = theme.muted_foreground;

        div()
            .id("verilog-edit-tab")
            .flex_1()
            .size_full()
            .min_h_0()
            .min_w_0()
            .flex()
            .flex_col()
            .child(
                h_flex()
                    .gap_3()
                    .px_2()
                    .py_1()
                    .border_b_1()
                    .border_color(border)
                    .child(
                        div()
                            .flex_1()
                            .text_color(muted)
                            .font_family(crate::view::mono_font())
                            .child(format!("{}.sv", self.editor.active_module())),
                    )
                    .child(
                        Button::new("verilog-interface-button")
                            .xsmall()
                            .label("Ports…")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.command(Command::VerilogInterface, window, cx)
                            })),
                    ),
            )
            .child(
                div()
                    .id("source-editor-content")
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .child(
                        Editor::new(&input)
                            .h(gpui::relative(1.0))
                            .readonly(self.simulation.is_some()),
                    ),
            )
            .into_any_element()
    }
}
