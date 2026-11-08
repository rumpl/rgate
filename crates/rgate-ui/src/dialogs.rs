//! Bridges RGate's declarative `GateApp::dialog` state to GPUI Kit's window dialogs.
//!
//! GPUI Kit opens dialogs imperatively through the window `Root`, so the app
//! reconciles its state against the open dialog on every render. The body and
//! footer are small views that observe the app, keeping dialog contents live
//! while a simulation runs.
use crate::{
    app::{Dialog, GateApp, OpenDialog},
    commands::Command,
    view::mono_font,
};
use gpui::{
    AnyElement, App, AppContext, Context, Entity, FocusHandle, Focusable, FontWeight, SharedString,
    Subscription, WeakEntity, Window, div, prelude::*, px,
};
use gpui_component::{
    ActiveTheme, Icon, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex,
    input::{Input, InputState, Textarea},
    label::Label,
    list::ListItem,
    radio::Radio,
    scroll::ScrollableElement as _,
    v_flex,
};
use gpui_kit_assets::IconName;
use rgate_core::{Direction, GateKind};
use std::cell::Cell;

#[derive(Clone, Copy, PartialEq)]
enum Slot {
    Body,
    Footer,
}

struct DialogPart {
    app: WeakEntity<GateApp>,
    slot: Slot,
    _observe: Option<Subscription>,
}

impl DialogPart {
    fn new(app: WeakEntity<GateApp>, slot: Slot, cx: &mut Context<Self>) -> Self {
        let observe = app
            .upgrade()
            .map(|entity| cx.observe(&entity, |_, _, cx| cx.notify()));
        Self {
            app,
            slot,
            _observe: observe,
        }
    }
}

impl Render for DialogPart {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let slot = self.slot;
        self.app
            .update(cx, |app, cx| match slot {
                Slot::Body => app.dialog_body(cx),
                Slot::Footer => app.dialog_footer(cx),
            })
            .unwrap_or_else(|_| div().into_any_element())
    }
}

fn field_row(label: &'static str, field: &Entity<InputState>) -> AnyElement {
    h_flex()
        .gap_3()
        .child(Label::new(label).w(px(140.0)).flex_none())
        .child(Input::new(field).small().flex_1())
        .into_any_element()
}

impl GateApp {
    fn dialog_frame(&self) -> Option<(&'static str, f32)> {
        let gate_kind = |gate| {
            self.editor
                .module()
                .gate(gate)
                .map(|gate| gate.kind.clone())
        };
        Some(match self.dialog.as_ref()? {
            Dialog::HdlSignals { .. } => ("Live HDL signals", 700.0),
            Dialog::HdlTop { .. } => ("Choose top module", 440.0),
            Dialog::VerilogPorts { .. } => ("Verilog module ports", 520.0),
            Dialog::Verilog { .. } => ("Verilog module source", 900.0),
            Dialog::HdlInput { .. } => ("Set HDL input (hex)", 400.0),
            Dialog::Symbol(_) => ("Module symbol editor", 640.0),
            Dialog::Find { .. } => ("Find gates and nets", 580.0),
            Dialog::Vga { .. } => ("VGA display", 620.0),
            Dialog::Recovery => ("Recover unsaved circuit", 500.0),
            Dialog::Properties(properties) => match gate_kind(properties.gate) {
                Some(GateKind::Frame) => ("Frame properties", 500.0),
                Some(GateKind::Comment) => ("Comment properties", 500.0),
                _ => ("Gate properties", 500.0),
            },
            Dialog::Input { .. } => ("Set input value", 400.0),
            Dialog::Memory { .. } => ("Memory inspector", 580.0),
            Dialog::Net(_) => ("Net properties", 440.0),
            Dialog::Terminal { .. } => ("Terminal I/O", 580.0),
            Dialog::NewModule { .. } => ("New module", 380.0),
            Dialog::Help => ("Getting started", 590.0),
            Dialog::About => ("About RGate", 550.0),
        })
    }

    fn dialog_initial_focus(&self, cx: &App) -> Option<FocusHandle> {
        let field = match self.dialog.as_ref()? {
            Dialog::Find { query } | Dialog::HdlSignals { query } => query,
            Dialog::NewModule { name, .. } => name,
            Dialog::Input { value, .. } | Dialog::HdlInput { value, .. } => value,
            Dialog::Memory { address, .. } => address,
            Dialog::Terminal { input, .. } => input,
            Dialog::Properties(properties) => &properties.name,
            Dialog::Net(net) => &net.name,
            Dialog::Verilog { source, .. } => return Some(source.read(cx).focus_handle(cx)),
            _ => return None,
        };
        Some(field.read(cx).focus_handle(cx))
    }

    /// Opens, replaces or closes the GPUI Kit dialog so it matches `self.dialog`.
    pub(crate) fn sync_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let wanted = self.dialog_frame();
        let shown = self
            .open_dialog
            .as_ref()
            .map(|open| (open.title, open.width));
        if wanted == shown {
            return;
        }
        self.open_dialog = wanted.map(|(title, width)| OpenDialog { title, width });
        let app = cx.weak_entity();
        let closable = !matches!(self.dialog, Some(crate::app::Dialog::Recovery));
        let focus = Cell::new(self.dialog_initial_focus(cx));
        window.defer(cx, move |window, cx| {
            if shown.is_some() {
                window.close_dialog(cx);
            }
            let Some((title, width)) = wanted else {
                return;
            };
            let body = cx.new(|cx| DialogPart::new(app.clone(), Slot::Body, cx));
            let footer = cx.new(|cx| DialogPart::new(app.clone(), Slot::Footer, cx));
            window.open_dialog(cx, move |dialog, window, cx| {
                // The builder reruns whenever the dialog layer rebuilds; focus only once.
                if let Some(handle) = focus.take() {
                    handle.focus(window, cx);
                }
                let (confirm, cancel) = (app.clone(), app.clone());
                dialog
                    .title(title)
                    .w(px(width))
                    .close_button(closable)
                    .keyboard(closable)
                    .overlay_closable(false)
                    .child(body.clone())
                    .footer(footer.clone())
                    // The app owns the dialog state; returning false leaves closing to `sync_dialog`.
                    .on_ok(move |_, window, cx| {
                        let _ = confirm.update(cx, |this, cx| {
                            if !matches!(this.dialog, Some(Dialog::Verilog { .. })) {
                                this.apply_dialog(window, cx);
                            }
                        });
                        false
                    })
                    .on_cancel(move |_, window, cx| {
                        let _ = cancel.update(cx, |this, cx| this.dismiss_dialog(window, cx));
                        false
                    })
            });
        });
    }

    fn error_text(&self, error: &str, cx: &App) -> AnyElement {
        div()
            .text_color(cx.theme().danger)
            .child(error.to_owned())
            .into_any_element()
    }

    fn dialog_checkbox(
        &self,
        id: &'static str,
        label: &'static str,
        checked: bool,
        cx: &mut Context<Self>,
        toggle: impl Fn(&mut Dialog, bool) + 'static,
    ) -> Checkbox {
        Checkbox::new(id)
            .label(label)
            .checked(checked)
            .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                if let Some(dialog) = &mut this.dialog {
                    toggle(dialog, *checked);
                }
                cx.notify();
            }))
    }

    pub(crate) fn dialog_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(dialog) = &self.dialog else {
            return div().into_any_element();
        };
        let apply = matches!(
            dialog,
            Dialog::VerilogPorts { .. }
                | Dialog::HdlTop { .. }
                | Dialog::Verilog { .. }
                | Dialog::HdlInput { .. }
                | Dialog::Symbol(_)
                | Dialog::Input { .. }
                | Dialog::Memory { .. }
                | Dialog::Net(_)
                | Dialog::Terminal { .. }
                | Dialog::Properties(_)
                | Dialog::NewModule { .. }
        );
        if matches!(dialog, Dialog::Recovery) {
            return div().into_any_element();
        }
        let ports = matches!(dialog, Dialog::VerilogPorts { .. });
        let mut buttons = h_flex().justify_end().gap_2();
        if apply {
            buttons = buttons.child(
                Button::new("dialog-apply")
                    .primary()
                    .label(if ports {
                        "Refresh from source"
                    } else {
                        "Apply"
                    })
                    .on_click(cx.listener(|this, _, window, cx| this.apply_dialog(window, cx))),
            );
        }
        buttons
            .child(
                Button::new("dialog-close")
                    .label(if apply && !ports { "Cancel" } else { "Close" })
                    .on_click(cx.listener(|this, _, window, cx| this.dismiss_dialog(window, cx))),
            )
            .into_any_element()
    }

    pub(crate) fn dialog_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(dialog) = &self.dialog else {
            return div().into_any_element();
        };
        let theme = cx.theme();
        let (muted, border, radius, panel) = (
            theme.muted_foreground,
            theme.border,
            theme.radius,
            theme.secondary,
        );
        let mut content = v_flex().gap_3().pb_1();
        match dialog {
            Dialog::VerilogPorts { module, error } => {
                content = content
                    .child(format!("{module} · ports defined in Verilog source"))
                    .child(
                        Label::new("Use these pins to connect an instance in your parent schematic. Edit declarations in the source, then refresh to update instance pins.")
                            .text_color(muted),
                    );
                let mut ports = v_flex().id("verilog-ports-list").max_h(px(240.0)).gap_1();
                match self
                    .editor
                    .circuit()
                    .module(module)
                    .and_then(|m| m.verilog.as_ref().map(|source| (source, &m.nets)))
                {
                    Some((source, nets)) => {
                        match rgate_format::hdl::inferred_nets(source, module, nets) {
                            Ok(nets) => {
                                ports = ports.child(
                                    h_flex()
                                        .gap_3()
                                        .text_xs()
                                        .text_color(muted)
                                        .child(div().w(px(90.0)).child("Direction"))
                                        .child(div().flex_1().child("Pin"))
                                        .child("Width"),
                                );
                                let mut count = 0;
                                for net in nets.iter().filter(|n| n.port.is_some()) {
                                    count += 1;
                                    ports = ports.child(
                                        h_flex()
                                            .gap_3()
                                            .child(div().w(px(90.0)).child(match net.port {
                                                Some(Direction::Input) => "Input →",
                                                Some(Direction::Output) => "Output ←",
                                                _ => "InOut ↔",
                                            }))
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .font_family(mono_font())
                                                    .child(net.name.clone()),
                                            )
                                            .child(format!(
                                                "{} bit{}",
                                                net.width,
                                                if net.width == 1 { "" } else { "s" }
                                            )),
                                    );
                                }
                                if count == 0 {
                                    ports = ports.child("This module has no external ports.");
                                }
                            }
                            Err(error) => {
                                ports = ports.child(self.error_text(
                                    &format!("Correct the source declarations first: {error}"),
                                    cx,
                                ));
                            }
                        }
                    }
                    None => ports = ports.child("Source module is unavailable."),
                }
                content = content.child(ports.overflow_y_scrollbar()).child(
                    Label::new("During simulation, use Simulate → Browse HDL signals… to inspect internal signals and add waveform probes.")
                        .text_color(muted),
                );
                if let Some(error) = error {
                    content = content.child(self.error_text(error, cx));
                }
            }
            Dialog::HdlSignals { query } => {
                content = content.child(Input::new(query).small().cleanable(true)).child(
                    Label::new("Click a signal to inspect its live value; Probe records its waveform. Filter by an instance path to explore that scope.")
                        .text_color(muted),
                );
                let mut list = v_flex().id("hdl-signals-list").h(px(350.0));
                #[cfg(feature = "icarus")]
                if let Some(sim) = &self.simulation {
                    let filter = query.read(cx).value().to_ascii_lowercase();
                    for (index, signal) in sim
                        .discovered_signals()
                        .iter()
                        .filter(|s| s.path.to_ascii_lowercase().contains(&filter))
                        .take(2000)
                        .enumerate()
                    {
                        let path = signal.path.clone();
                        let probe_path = path.clone();
                        let value = sim
                            .hdl_bound_value(&path)
                            .map(ToString::to_string)
                            .unwrap_or_else(|| "Inspect".into());
                        list = list.child(
                            ListItem::new(("hdl-signal", index))
                                .text_sm()
                                .child(
                                    h_flex()
                                        .w_full()
                                        .gap_2()
                                        .child(div().flex_1().min_w_0().truncate().child(format!(
                                            "{} [{} bits]  {value}",
                                            path, signal.width
                                        )))
                                        .child(
                                            Button::new(("hdl-probe", index))
                                                .xsmall()
                                                .ghost()
                                                .label("Probe / Remove")
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.inspect_hdl_signal(&probe_path, true, cx);
                                                    cx.stop_propagation();
                                                })),
                                        ),
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.inspect_hdl_signal(&path, false, cx)
                                })),
                        );
                    }
                }
                #[cfg(not(feature = "icarus"))]
                {
                    list = list.child(
                        "Open a Verilog circuit in desktop RGate to inspect live HDL signals.",
                    );
                }
                content = content.child(list.overflow_y_scrollbar());
            }
            Dialog::HdlTop {
                circuit, selected, ..
            } => {
                content = content.child(
                    Label::new("Choose the module to run first. All source definitions will be available in Modules.")
                        .text_color(muted),
                );
                for module in &circuit.modules {
                    let name = module.name.clone();
                    content = content.child(
                        Radio::new(SharedString::from(format!("top-{name}")))
                            .label(name.clone())
                            .checked(name == *selected)
                            .on_click(cx.listener(move |this, _: &bool, _, cx| {
                                if let Some(Dialog::HdlTop { selected, .. }) = &mut this.dialog {
                                    *selected = name.clone();
                                }
                                cx.notify();
                            })),
                    );
                }
            }
            Dialog::Verilog {
                creating,
                name,
                source,
                interface,
                error,
                ..
            } => {
                content = content
                    .child(Label::new("Module name (must match source declaration)"))
                    .child(Input::new(name).small());
                if *creating || cfg!(target_family = "wasm") {
                    content = content
                        .child(Label::new("Initial source — trusted HDL only. After creation, edit in the Edit tab."))
                        .child(Textarea::new(source).small().h(px(300.0)).font_family(mono_font()));
                }
                content = content
                    .child(Label::new("Ports are inferred from source. Add internal observations: signal name width"))
                    .child(Textarea::new(interface).small().h(px(130.0)).font_family(mono_font()));
                if let Some(error) = error {
                    content = content.child(self.error_text(error, cx));
                }
            }
            Dialog::HdlInput { value, error, .. } => {
                content = content.child(Input::new(value).small());
                if let Some(error) = error {
                    content = content.child(self.error_text(error, cx));
                }
            }
            Dialog::Vga { gate } => {
                content = content.child(self.vga_display(*gate, cx));
                if self.held_reset_input().is_some() {
                    content = content
                        .child(h_flex().child(self.release_reset_button("vga-release-reset", cx)));
                }
            }
            Dialog::Find { query } => {
                content = content.child(Input::new(query).small().cleanable(true));
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
                content = content.child(list.overflow_y_scrollbar());
            }
            Dialog::Recovery => {
                content = content
                    .child("An unsaved circuit was found from your previous session. Simulation runtime state is not restored.")
                    .child(
                        h_flex()
                            .justify_end()
                            .gap_2()
                            .child(
                                Button::new("discard-recovery")
                                    .label("Discard recovery")
                                    .on_click(cx.listener(|this, _, window, cx| this.discard_recovery(window, cx))),
                            )
                            .child(
                                Button::new("recover-circuit")
                                    .primary()
                                    .label("Recover circuit")
                                    .on_click(cx.listener(|this, _, window, cx| this.restore_recovery(window, cx))),
                            ),
                    );
            }
            Dialog::Symbol(_) => {
                content = content.child(crate::symbol_editor::editor(self, cx));
            }
            Dialog::Input { value, error, .. } => {
                content = content.child(field_row("Hex value", value));
                if let Some(error) = error {
                    content = content.child(self.error_text(error, cx));
                }
            }
            Dialog::Memory {
                gate,
                address,
                value,
                error,
            } => {
                let words = self
                    .simulation
                    .as_ref()
                    .and_then(|sim| {
                        self.simulation_gate(*gate)
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
                content = content
                    .child(listing.overflow_y_scrollbar())
                    .child(
                        Label::new("Runtime edits are volatile; set initial contents in gate properties to save them.")
                            .text_color(muted),
                    )
                    .child(field_row("Address (decimal)", address))
                    .child(field_row("Value (hex)", value));
                if let Some(error) = error {
                    content = content.child(self.error_text(error, cx));
                }
            }
            Dialog::Net(properties) => {
                content = content
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
                    content = content.child(self.error_text(error, cx));
                }
            }
            Dialog::Terminal { gate, input, error } => {
                let output = self
                    .simulation
                    .as_ref()
                    .and_then(|sim| {
                        self.simulation_gate(*gate)
                            .and_then(|id| sim.terminal_output(id).ok())
                    })
                    .unwrap_or_default();
                let tkgate = self
                    .editor
                    .module()
                    .gate(*gate)
                    .is_some_and(|gate| gate.config.tty_tkgate);
                content = content.child(
                    h_flex()
                        .gap_2()
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
                        .child(div().text_color(muted).child(format!(
                            "{} · {} ns",
                            if self.running { "Running" } else { "Paused" },
                            self.simulation.as_ref().map_or(0, |sim| sim.time())
                        ))),
                );
                if self.held_reset_input().is_some() {
                    content = content
                        .child(self.error_text(
                            "RESET_N is 0: the circuit is held in reset and cannot execute.",
                            cx,
                        ))
                        .child(
                            h_flex().child(self.release_reset_button("terminal-release-reset", cx)),
                        );
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
                                "No output bytes received yet. Run/step the circuit after releasing reset.".into()
                            } else {
                                output
                            })
                            .overflow_y_scrollbar(),
                    )
                    .child(Label::new(if tkgate {
                        "TkGate mode: RD/DSR send output; TD/RTS/CTS receive input."
                    } else {
                        "TX writes a byte on WR rising edge. RX/READY expose queued input; RD rising edge consumes a byte."
                    }).text_color(muted))
                    .child(field_row("Send UTF-8 bytes", input));
                if let Some(error) = error {
                    content = content.child(self.error_text(error, cx));
                }
            }
            Dialog::Properties(properties) => {
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
                            Label::new("Coordinates are relative to the block center; e.g. {\"A\":{\"x\":-50,\"y\":0}}.")
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
                    content = content.child(modes).child(
                        div()
                            .id("led-width-preview")
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(
                                width
                                    .map(|width| properties.led_display.preview(width))
                                    .unwrap_or_else(|| "Enter a width from 1 to 4096.".into()),
                            ),
                    );
                    let connected = gate.pins.iter().any(|pin| pin.net.is_some());
                    content = content.child(Label::new(if connected {
                        "Wired LED: its bus width is preserved when you change display type."
                    } else {
                        "Choosing a display type sets the recommended width automatically. You can override it."
                    }).text_color(muted));
                    if properties.led_display == rgate_core::LedDisplay::SevenSegment {
                        content = content.child(
                            Label::new("Segment map: bit 0 top; 1 upper-left; 2 upper-right; 3 middle; 4 lower-left; 5 lower-right; 6 bottom.")
                                .text_xs()
                                .text_color(muted),
                        );
                    }
                }
                if matches!(
                    gate.kind,
                    GateKind::Mux | GateKind::Decoder | GateKind::Demux
                ) || gate.kind.is_logic()
                {
                    content = content.child(field_row("Input/output count", &properties.inputs));
                }
                if matches!(gate.kind, GateKind::Concat | GateKind::Splitter) {
                    content =
                        content.child(field_row("Partitions (LSB first)", &properties.partitions));
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
                            self.button("load-memory-image", "Load hex file…", Command::LoadMemory, false, cx)
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
                        if link.target.starts_with("https://") || link.target.starts_with("http://")
                        {
                            content = content.child(
                                h_flex().child(
                                    Button::new(SharedString::from(format!(
                                        "comment-link-{index}"
                                    )))
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
                    content = content.child(self.error_text(error, cx));
                }
            }
            Dialog::NewModule { name, error } => {
                content = content.child(field_row("Module name", name));
                if let Some(error) = error {
                    content = content.child(self.error_text(error, cx));
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
                    content = content.child(
                        v_flex()
                            .gap_1()
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(label))
                            .child(Label::new(text).text_color(muted)),
                    );
                }
            }
            Dialog::About => {
                content = content
                    .child(
                        h_flex()
                            .gap_4()
                            .child(Icon::new(IconName::Cpu).size(px(56.0)))
                            .child(
                                v_flex()
                                    .gap_1()
                                    .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child("RGate 0.1"))
                                    .child(Label::new("A Rust / GPUI reimplementation of TkGate").text_color(muted)),
                            ),
                    )
                    .child("Inspired by TkGate. Classic artwork is adapted from TkGate reference images; original artwork © Jeffery P. Hansen and contributors, GPL-2.0-or-later.")
                    .child("RGate and Classic artwork adaptations are GPL-3.0-or-later. GPUI/GPUI Kit are Apache-2.0; bundled Lucide/Feather icons are ISC/MIT. See LICENSE, NOTICE and the distribution license inventory.")
                    .child(Label::new("Independent reimplementation; not an official TkGate release. Distributed without warranty.").text_color(muted));
            }
        }
        content.into_any_element()
    }
}
