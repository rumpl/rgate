use super::{error_text, field_row};
use crate::{app::GateApp, commands::Command, view::mono_font};
use anyhow::{Context as _, Result};
use gpui::{AnyElement, Context, Entity, div, prelude::*, px};
use gpui_component::{
    ActiveTheme, h_flex, input::InputState, label::Label, scroll::ScrollableElement as _, v_flex,
};
use rgate_core::{GateId, Signal};

impl GateApp {
    pub(super) fn render_vga_dialog(&self, gate: GateId, cx: &mut Context<Self>) -> AnyElement {
        let mut content = v_flex().gap_3().pb_1().child(self.vga_display(gate, cx));
        if self.sim.held_reset_input().is_some() {
            content =
                content.child(h_flex().child(self.release_reset_button("vga-release-reset", cx)));
        }
        content.into_any_element()
    }

    pub(super) fn render_input_dialog(
        &self,
        value: &Entity<InputState>,
        error: Option<&str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut content = v_flex().gap_3().pb_1().child(field_row("Hex value", value));
        if let Some(error) = error {
            content = content.child(error_text(error, cx));
        }
        content.into_any_element()
    }

    pub(super) fn render_memory_dialog(
        &self,
        gate: GateId,
        address: &Entity<InputState>,
        value: &Entity<InputState>,
        error: Option<&str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let (muted, radius, panel) = (theme.muted_foreground, theme.radius, theme.secondary);
        let words = self
            .sim
            .live
            .as_ref()
            .and_then(|sim| {
                self.sim
                    .scoped_gate(gate)
                    .and_then(|id| sim.memory_words(id).ok())
            })
            .unwrap_or_default();
        let mut listing = v_flex()
            .id("memory-words")
            .h(px(220.0))
            .p_2()
            .bg(panel)
            .rounded(radius)
            .font_family(mono_font());
        for (address, word) in words.iter().take(512) {
            listing = listing.child(format!("{address:04X}: {}", word.display_value()));
        }
        let mut content = v_flex()
            .gap_3()
            .pb_1()
            .child(listing.overflow_y_scrollbar())
            .child(
                Label::new("Runtime edits are volatile; set initial contents in gate properties to save them.")
                    .text_color(muted),
            )
            .child(field_row("Address (decimal)", address))
            .child(field_row("Value (hex)", value));
        if let Some(error) = error {
            content = content.child(error_text(error, cx));
        }
        content.into_any_element()
    }

    pub(super) fn render_terminal_dialog(
        &self,
        gate: GateId,
        input: &Entity<InputState>,
        error: Option<&str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let (muted, border, radius, panel) = (
            theme.muted_foreground,
            theme.border,
            theme.radius,
            theme.secondary,
        );
        let output = self
            .sim
            .live
            .as_ref()
            .and_then(|sim| {
                self.sim
                    .scoped_gate(gate)
                    .and_then(|id| sim.terminal_output(id).ok())
            })
            .unwrap_or_default();
        let tkgate = self
            .editor
            .module()
            .gate(gate)
            .is_some_and(|gate| gate.config.tty_tkgate);
        let mut content = v_flex().gap_3().pb_1().child(
            h_flex()
                .gap_2()
                .child(self.button(
                    "terminal-run",
                    if self.sim.running { "Pause" } else { "Run" },
                    Command::PlayPause,
                    self.sim.running,
                    cx,
                ))
                .child(self.button(
                    "terminal-step",
                    "Clock step",
                    Command::ClockStep,
                    false,
                    cx,
                ))
                .child(div().text_color(muted).child(format!(
                    "{} · {} ns",
                    if self.sim.running { "Running" } else { "Paused" },
                    self.sim.time()
                ))),
        );
        if self.sim.held_reset_input().is_some() {
            content = content
                .child(error_text(
                    "RESET_N is 0: the circuit is held in reset and cannot execute.",
                    cx,
                ))
                .child(h_flex().child(self.release_reset_button("terminal-release-reset", cx)));
        }
        content = content
            .child(
                v_flex()
                    .id("terminal-output")
                    .h(px(200.0))
                    .p_2()
                    .bg(panel)
                    .border_1()
                    .border_color(border)
                    .rounded(radius)
                    .font_family(mono_font())
                    .child(if output.is_empty() {
                        "No output bytes received yet. Run/step the circuit after releasing reset."
                            .into()
                    } else {
                        output
                    })
                    .overflow_y_scrollbar(),
            )
            .child(
                Label::new(if tkgate {
                    "TkGate mode: RD/DSR send output; TD/RTS/CTS receive input."
                } else {
                    "TX writes a byte on WR rising edge. RX/READY expose queued input; RD rising edge consumes a byte."
                })
                .text_color(muted),
            )
            .child(field_row("Send UTF-8 bytes", input));
        if let Some(error) = error {
            content = content.child(error_text(error, cx));
        }
        content.into_any_element()
    }

    pub(super) fn apply_input_dialog(
        &mut self,
        gate: GateId,
        runtime_gate: Option<GateId>,
        raw: &str,
    ) -> Result<()> {
        let width = self
            .editor
            .module()
            .gate(gate)
            .context("Input no longer exists")?
            .width;
        let word =
            Signal::parse(raw.trim_start_matches("0x"), width, 16).map_err(anyhow::Error::msg)?;
        let sim = self.sim.live.as_mut().context("Simulation inactive")?;
        sim.set_input(runtime_gate.context("No runtime input")?, word)?;
        sim.advance(30)?;
        Ok(())
    }

    pub(super) fn apply_memory_dialog(
        &mut self,
        gate: GateId,
        runtime_gate: Option<GateId>,
        address_raw: &str,
        value_raw: &str,
    ) -> Result<()> {
        let address: usize = address_raw
            .parse()
            .context("Address must be a decimal number.")?;
        let width = self
            .editor
            .module()
            .gate(gate)
            .context("Memory no longer exists.")?
            .width;
        let word = Signal::parse(value_raw.trim_start_matches("0x"), width, 16)
            .map_err(anyhow::Error::msg)?;
        let runtime_id = runtime_gate.context("Memory is not in this live instance.")?;
        self.sim
            .live
            .as_mut()
            .context("Simulation is not active.")?
            .set_memory_word(runtime_id, address, word)?;
        Ok(())
    }

    pub(super) fn apply_terminal_dialog(
        &mut self,
        runtime_gate: Option<GateId>,
        text: &str,
    ) -> Result<()> {
        self.sim
            .live
            .as_mut()
            .context("Start simulation before sending terminal input.")
            .and_then(|sim| {
                sim.send_terminal(
                    runtime_gate.context("Terminal is not in this live instance.")?,
                    text,
                )
                .map_err(Into::into)
            })
    }
}
