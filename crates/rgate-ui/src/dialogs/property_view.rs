use super::{Dialog, NetDialog, PropertyDialog, error_text, field_row};
use crate::{app::GateApp, commands::Command};
use gpui::{AnyElement, Context, FontWeight, SharedString, Window, div, prelude::*};
use gpui_component::{
    ActiveTheme,
    button::{Button, ButtonVariants as _},
    h_flex,
    label::Label,
    radio::Radio,
    v_flex,
};
use gpui_kit_assets::IconName;
use rgate_core::GateKind;

impl GateApp {
    pub fn select_led_display(
        &mut self,
        mode: rgate_core::LedDisplay,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(Dialog::Properties(dialog)) = &mut self.dialogs.active {
            let connected = self
                .editor
                .module()
                .gate(dialog.gate)
                .is_some_and(|gate| gate.pins.iter().any(|pin| pin.net.is_some()));
            dialog.select_led_display(mode, connected, window, cx);
        }
        cx.notify();
    }

    pub fn show_properties(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(net) = self.editor.selection.net {
            if let Some(definition) = self.editor.module().net(net) {
                self.dialogs.show(Dialog::Net(Box::new(NetDialog::from_net(
                    definition, window, cx,
                ))));
            }
            return;
        }
        let Some(id) = self.editor.selection.gates.iter().next().copied() else {
            self.log("Select a gate first; module ports are edited in the Interface tab.".into());
            return;
        };
        let Some(gate) = self.editor.module().gate(id) else {
            return;
        };
        let dialog = PropertyDialog::from_gate(gate, window, cx);
        self.dialogs.show(Dialog::Properties(Box::new(dialog)));
        self.sim.pause();
    }

    pub(super) fn render_net_dialog(&self, properties: &NetDialog, cx: &mut Context<Self>) -> AnyElement {
        let mut content = v_flex()
            .gap_3()
            .pb_1()
            .child(field_row("Name", &properties.name))
            .child(field_row("Bit width", &properties.width))
            .child(self.dialog_checkbox(
                "net-show-name",
                "Show net label",
                properties.show_name,
                cx,
                |dialog, checked| {
                    if let Dialog::Net(dialog) = dialog {
                        dialog.show_name = checked;
                    }
                },
            ));
        if let Some(error) = &properties.error {
            content = content.child(error_text(error, cx));
        }
        content.into_any_element()
    }

    pub(super) fn render_properties_dialog(
        &self,
        properties: &PropertyDialog,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let mut content = v_flex().gap_3().pb_1();
        let Some(gate) = self.editor.module().gate(properties.gate) else {
            return content.into_any_element();
        };
        content = content
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(gate.kind.name().to_owned()),
            )
            .child(field_row("Name", &properties.name));
        let fields = crate::properties::fields(&gate.kind);
        for (label, field, visible) in [
            ("Bit width", &properties.width, fields.width),
            ("Initial value", &properties.value, fields.initial),
            ("Delay (ns)", &properties.delay, fields.delay),
            ("Clock period (ns)", &properties.period, fields.clock),
        ] {
            if visible {
                content = content.child(field_row(label, field));
            }
        }
        if gate.kind.is_logic() {
            content = content.child(self.dialog_checkbox(
                "gate-reduction",
                "Reduce input bus to one bit",
                properties.reduction,
                cx,
                |dialog, checked| {
                    if let Dialog::Properties(dialog) = dialog {
                        dialog.reduction = checked;
                    }
                },
            ));
        }
        if gate.kind == GateKind::TriState {
            content = content
                .child(self.dialog_checkbox(
                    "tri-enable-low",
                    "Active-low enable",
                    properties.enable_low,
                    cx,
                    |dialog, checked| {
                        if let Dialog::Properties(dialog) = dialog {
                            dialog.enable_low = checked;
                        }
                    },
                ))
                .child(self.dialog_checkbox(
                    "tri-invert",
                    "Invert enabled output",
                    properties.invert_output,
                    cx,
                    |dialog, checked| {
                        if let Dialog::Properties(dialog) = dialog {
                            dialog.invert_output = checked;
                        }
                    },
                ));
        }
        if matches!(gate.kind, GateKind::Module(_)) {
            content = content
                .child(field_row("Block width", &properties.module_width))
                .child(field_row("Port positions JSON", &properties.custom_ports))
                .child(
                    Label::new(
                        "Coordinates are relative to the block center; e.g. {\"A\":{\"x\":-50,\"y\":0}}.",
                    )
                    .text_xs()
                    .text_color(muted),
                );
        }
        if gate.kind == GateKind::Tty {
            content = content.child(self.dialog_checkbox(
                "tty-tkgate",
                "TkGate TD/RD/RTS/CTS/DSR/DTR protocol (otherwise TX/RX/WR/RD)",
                properties.tty_tkgate,
                cx,
                |dialog, checked| {
                    if let Dialog::Properties(dialog) = dialog {
                        dialog.tty_tkgate = checked;
                    }
                },
            ));
        }
        if gate.kind == GateKind::Clock {
            content = content
                .child(field_row("Phase delay (ns)", &properties.phase))
                .child(field_row("High duty (%)", &properties.duty));
        }
        if matches!(gate.kind, GateKind::Multiply | GateKind::Divide) {
            content = content
                .child(field_row("Operand A width", &properties.operand_a))
                .child(field_row("Operand B width", &properties.operand_b));
        }
        if gate.kind == GateKind::Frame {
            content = content
                .child(field_row("Frame width", &properties.frame_width))
                .child(field_row("Frame height", &properties.frame_height))
                .child(field_row("Title", &properties.comment));
        }
        if gate.kind == GateKind::Led {
            content = content.child(self.render_led_properties(gate, properties, cx));
        }
        if matches!(
            gate.kind,
            GateKind::Mux | GateKind::Decoder | GateKind::Demux
        ) || gate.kind.is_logic()
        {
            content = content.child(field_row("Input/output count", &properties.inputs));
        }
        if matches!(gate.kind, GateKind::Concat | GateKind::Splitter) {
            content = content.child(field_row("Partitions (LSB first)", &properties.partitions));
        }
        if gate.kind == GateKind::Tap {
            content = content
                .child(field_row("Tap bit offset", &properties.tap_offset))
                .child(field_row("Tap output width", &properties.tap_width));
        }
        if matches!(gate.kind, GateKind::Ram | GateKind::Rom) {
            content = content
                .child(field_row("Address bits", &properties.address_bits))
                .child(field_row("Initial hex words", &properties.memory))
                .child(h_flex().child(
                    self.button(
                        "load-memory-image",
                        "Load hex file…",
                        Command::LoadMemory,
                        false,
                        cx,
                    )
                    .icon(IconName::FolderOpen),
                ))
                .child(
                    Label::new("Hex words, e.g. 41 42 FF or @10 AB CD. Unspecified words are X. RAM CS/OE/WE and ROM OE are active-low.")
                        .text_xs()
                        .text_color(muted),
                );
        }
        if gate.kind == GateKind::Comment {
            for (index, link) in rgate_core::comment_links(&gate.text)
                .into_iter()
                .enumerate()
            {
                if link.target.starts_with("https://") || link.target.starts_with("http://") {
                    content = content.child(
                        h_flex().child(
                            Button::new(SharedString::from(format!("comment-link-{index}")))
                                .link()
                                .icon(IconName::ExternalLink)
                                .label(format!("Open link: {}", link.label))
                                .on_click(move |_, _, cx| cx.open_url(&link.target)),
                        ),
                    );
                }
            }
            content = content.child(field_row("Text (\\n = newline)", &properties.comment));
        }
        content = content.child(self.dialog_checkbox(
            "property-show-name",
            "Show instance name",
            properties.show_name,
            cx,
            |dialog, checked| {
                if let Dialog::Properties(dialog) = dialog {
                    dialog.show_name = checked;
                }
            },
        ));
        if let Some(error) = &properties.error {
            content = content.child(error_text(error, cx));
        }
        content.into_any_element()
    }

    fn render_led_properties(
        &self,
        gate: &rgate_core::Gate,
        properties: &PropertyDialog,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let mut modes = v_flex().gap_1p5().child(Label::new("Display type"));
        for mode in rgate_core::LedDisplay::ALL {
            modes = modes.child(
                Radio::new(SharedString::from(format!("led-mode-{mode:?}")))
                    .label(mode.label())
                    .checked(properties.led_display == mode)
                    .on_click(cx.listener(move |this, _: &bool, window, cx| {
                        this.select_led_display(mode, window, cx);
                    })),
            );
        }
        let width = properties
            .width
            .read(cx)
            .value()
            .parse::<u16>()
            .ok()
            .filter(|width| (1..=4096).contains(width));
        let connected = gate.pins.iter().any(|pin| pin.net.is_some());
        let mut section = v_flex()
            .gap_2()
            .child(modes)
            .child(
                div()
                    .id("led-width-preview")
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(
                        width
                            .map(|width| properties.led_display.preview(width))
                            .unwrap_or_else(|| "Enter a width from 1 to 4096.".into()),
                    ),
            )
            .child(
                Label::new(if connected {
                    "Wired LED: its bus width is preserved when you change display type."
                } else {
                    "Choosing a display type sets the recommended width automatically. You can override it."
                })
                .text_color(muted),
            );
        if properties.led_display == rgate_core::LedDisplay::SevenSegment {
            section = section.child(
                Label::new("Segment map: bit 0 top; 1 upper-left; 2 upper-right; 3 middle; 4 lower-left; 5 lower-right; 6 bottom.")
                    .text_xs()
                    .text_color(muted),
            );
        }
        section.into_any_element()
    }
}
