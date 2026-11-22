use super::{error_text, field_row};
use crate::app::GateApp;
use gpui::{AnyElement, Context, Entity, FontWeight, SharedString, div, prelude::*, px};
use gpui_component::{
    ActiveTheme, Icon, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputState},
    label::Label,
    list::ListItem,
    scroll::ScrollableElement as _,
    v_flex,
};
use gpui_kit_assets::IconName;

impl GateApp {
    pub(super) fn render_find_dialog(
        &self,
        query: &Entity<InputState>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let results = rgate_editor::search(self.editor.circuit(), &query.read(cx).value());
        let mut list = v_flex().id("find-results").h(px(300.0));
        if results.is_empty() {
            list = list.child(div().p_2().text_color(muted).child(
                "Type to find gates/nets across module definitions (up to 200 results).",
            ));
        }
        for (index, result) in results.into_iter().enumerate() {
            list = list.child(
                ListItem::new(SharedString::from(format!("find-{index}")))
                    .text_sm()
                    .child(div().w_full().truncate().child(format!(
                        "{} / {} — {}",
                        result.module, result.name, result.description
                    )))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.select_search_result(result.clone(), window, cx)
                    })),
            );
        }
        v_flex()
            .gap_3()
            .pb_1()
            .child(Input::new(query).small().cleanable(true))
            .child(list.overflow_y_scrollbar())
            .into_any_element()
    }

    pub(super) fn render_recovery_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        v_flex()
            .gap_3()
            .pb_1()
            .child("An unsaved circuit was found from your previous session. Simulation runtime state is not restored.")
            .child(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("discard-recovery")
                            .label("Discard recovery")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.discard_recovery(window, cx)
                            })),
                    )
                    .child(
                        Button::new("recover-circuit")
                            .primary()
                            .label("Recover circuit")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.restore_recovery(window, cx)
                            })),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn render_new_module_dialog(
        &self,
        name: &Entity<InputState>,
        error: Option<&str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut content = v_flex().gap_3().pb_1().child(field_row("Module name", name));
        if let Some(error) = error {
            content = content.child(error_text(error, cx));
        }
        content.into_any_element()
    }

    pub(super) fn render_help_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let mut content = v_flex().gap_3().pb_1();
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
            content = content.child(
                v_flex()
                    .gap_1()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(label))
                    .child(Label::new(text).text_color(muted)),
            );
        }
        content.into_any_element()
    }

    pub(super) fn render_about_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        v_flex()
            .gap_3()
            .pb_1()
            .child(
                h_flex()
                    .gap_4()
                    .child(Icon::new(IconName::Cpu).size(px(56.0)))
                    .child(
                        v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_xl()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("RGate 0.1"),
                            )
                            .child(
                                Label::new("A Rust / GPUI reimplementation of TkGate")
                                    .text_color(muted),
                            ),
                    ),
            )
            .child("Inspired by TkGate. Classic artwork is adapted from TkGate reference images; original artwork © Jeffery P. Hansen and contributors, GPL-2.0-or-later.")
            .child("RGate and Classic artwork adaptations are GPL-3.0-or-later. GPUI/GPUI Kit are Apache-2.0; bundled Lucide/Feather icons are ISC/MIT. See LICENSE, NOTICE and the distribution license inventory.")
            .child(
                Label::new("Independent reimplementation; not an official TkGate release. Distributed without warranty.")
                    .text_color(muted),
            )
            .into_any_element()
    }
}
