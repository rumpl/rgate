use super::{Dialog, error_text};
use crate::{app::GateApp, view::mono_font};
use anyhow::{Context as _, Result};
use gpui::{AnyElement, Context, Entity, SharedString, div, prelude::*, px};
use gpui_component::{
    ActiveTheme, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputState, Textarea, TextareaState},
    label::Label,
    list::ListItem,
    radio::Radio,
    scroll::ScrollableElement as _,
    v_flex,
};
use rgate_core::{Circuit, Direction, GateKind, Module, NetId, Signal};

impl GateApp {
    pub(super) fn render_verilog_ports_dialog(
        &self,
        module: &str,
        error: Option<&str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let mut content = v_flex()
            .gap_3()
            .pb_1()
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
            Some((source, nets)) => match rgate_format::hdl::inferred_nets(source, module, nets) {
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
                Err(err) => {
                    ports = ports.child(error_text(
                        &format!("Correct the source declarations first: {err}"),
                        cx,
                    ));
                }
            },
            None => ports = ports.child("Source module is unavailable."),
        }
        content = content.child(ports.overflow_y_scrollbar()).child(
            Label::new("During simulation, use Simulate → Browse HDL signals… to inspect internal signals and add waveform probes.")
                .text_color(muted),
        );
        if let Some(error) = error {
            content = content.child(error_text(error, cx));
        }
        content.into_any_element()
    }

    pub(super) fn render_hdl_signals_dialog(
        &self,
        query: &Entity<InputState>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let mut list = v_flex().id("hdl-signals-list").h(px(350.0));
        if let Some(sim) = &self.sim.live {
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
        v_flex()
            .gap_3()
            .pb_1()
            .child(Input::new(query).small().cleanable(true))
            .child(
                Label::new("Click a signal to inspect its live value; Probe records its waveform. Filter by an instance path to explore that scope.")
                    .text_color(muted),
            )
            .child(list.overflow_y_scrollbar())
            .into_any_element()
    }

    pub(super) fn render_hdl_top_dialog(
        &self,
        circuit: &Circuit,
        selected: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let mut content = v_flex().gap_3().pb_1().child(
            Label::new("Choose the module to run first. All source definitions will be available in Modules.")
                .text_color(muted),
        );
        for module in &circuit.modules {
            let name = module.name.clone();
            content = content.child(
                Radio::new(SharedString::from(format!("top-{name}")))
                    .label(name.clone())
                    .checked(name == selected)
                    .on_click(cx.listener(move |this, _: &bool, _, cx| {
                        if let Some(Dialog::HdlTop { selected, .. }) = &mut this.dialogs.active {
                            *selected = name.clone();
                        }
                        cx.notify();
                    })),
            );
        }
        content.into_any_element()
    }

    pub(super) fn render_verilog_dialog(
        &self,
        creating: bool,
        name: &Entity<InputState>,
        source: &Entity<TextareaState>,
        interface: &Entity<TextareaState>,
        error: Option<&str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut content = v_flex()
            .gap_3()
            .pb_1()
            .child(Label::new("Module name (must match source declaration)"))
            .child(Input::new(name).small());
        if creating || cfg!(target_family = "wasm") {
            content = content
                .child(Label::new(
                    "Initial source — trusted HDL only. After creation, edit in the Edit tab.",
                ))
                .child(
                    Textarea::new(source)
                        .small()
                        .h(px(300.0))
                        .font_family(mono_font()),
                );
        }
        content = content
            .child(Label::new(
                "Ports are inferred from source. Add internal observations: signal name width",
            ))
            .child(
                Textarea::new(interface)
                    .small()
                    .h(px(130.0))
                    .font_family(mono_font()),
            );
        if let Some(error) = error {
            content = content.child(error_text(error, cx));
        }
        content.into_any_element()
    }

    pub(super) fn render_hdl_input_dialog(
        &self,
        value: &Entity<InputState>,
        error: Option<&str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut content = v_flex().gap_3().pb_1().child(Input::new(value).small());
        if let Some(error) = error {
            content = content.child(error_text(error, cx));
        }
        content.into_any_element()
    }

    pub(super) fn apply_verilog_dialog(
        &mut self,
        creating: bool,
        name: String,
        source: String,
        interface: String,
    ) -> Result<()> {
        if !crate::verilog_editor::identifier(&name) {
            anyhow::bail!("Use a simple Verilog module name");
        }
        let observations =
            crate::verilog_editor::interface(&interface).map_err(anyhow::Error::msg)?;
        let mut existing = if creating {
            Vec::new()
        } else {
            self.editor.module().nets.clone()
        };
        for net in observations.into_iter().filter(|n| n.port.is_none()) {
            if !existing.iter().any(|n| n.name == net.name) {
                let mut net = net;
                net.id = NetId(existing.iter().map(|n| n.id.0).max().unwrap_or(0) + 1);
                existing.push(net);
            }
        }
        let mut nets = rgate_format::hdl::inferred_nets(&source, &name, &existing)?;
        if source.trim().is_empty() || source.len() > 1_000_000 {
            anyhow::bail!("Source must contain 1–1000000 bytes");
        }
        if !creating
            && self
                .editor
                .module()
                .nets
                .iter()
                .filter(|n| n.port.is_some())
                .map(|n| (&n.name, n.width, n.port))
                .collect::<Vec<_>>()
                != nets
                    .iter()
                    .filter(|n| n.port.is_some())
                    .map(|n| (&n.name, n.width, n.port))
                    .collect::<Vec<_>>()
            && self.editor.circuit().modules.iter().any(|m| {
                m.gates.iter().any(|g| {
                    g.kind == GateKind::Module(name.clone())
                        && g.pins.iter().any(|p| p.net.is_some())
                })
            })
        {
            anyhow::bail!("Disconnect module instances before changing HDL interface ports");
        }
        if !creating && name != self.editor.active_module() {
            anyhow::bail!("Renaming source modules is not supported yet");
        }
        if creating {
            let mut definition = Module::new(name);
            definition.verilog = Some(source);
            definition.nets = nets;
            self.editor.create_definition(definition)?;
            return Ok(());
        }
        let mut next = self.editor.module().next_net_id().0;
        for net in &mut nets {
            net.id = if let Some(old) = self
                .editor
                .module()
                .nets
                .iter()
                .find(|n| n.name == net.name)
            {
                old.id
            } else {
                let id = NetId(next);
                next += 1;
                id
            };
        }
        self.editor.edit_module(|module| {
            module.verilog = Some(source);
            module.nets = nets;
        })?;
        Ok(())
    }

    pub(super) fn apply_hdl_input(&mut self, net: NetId, raw: &str) -> Result<()> {
        let sim = self.sim.live.as_mut().context("Simulation inactive")?;
        let gate = sim.source_input(net).context("Not an interactive input")?;
        let width = sim.value(net).context("Missing input")?.width();
        let value =
            Signal::parse(raw.trim_start_matches("0x"), width, 16).map_err(anyhow::Error::msg)?;
        sim.set_input(gate, value)?;
        Ok(())
    }
}
