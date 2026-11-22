//! Unified dialog state, GPUI Kit dialog bridge, rendering dispatch, and commit handlers.

mod general;
mod hdl;
mod property_model;
mod property_view;
mod simulation;

pub use property_model::{NetDialog, PropertyDialog};

use crate::app::{GateApp, WorkspaceTab};
use anyhow::Result;
use gpui::{
    AnyElement, App, AppContext, Context, Entity, FocusHandle, Focusable, Subscription, WeakEntity,
    Window, div, prelude::*, px,
};
use gpui_component::{
    ActiveTheme, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex,
    input::{Input, InputState, TextareaState},
    label::Label,
};
use rgate_core::{Circuit, GateId, GateKind, NetId};
use rgate_editor::Tool;
use std::{cell::Cell, path::PathBuf};

/// The GPUI Kit dialog currently mirroring `DialogHost::active`.
pub struct OpenDialog {
    pub title: &'static str,
    pub width: f32,
}

pub enum Dialog {
    #[cfg_attr(target_family = "wasm", allow(dead_code))]
    VerilogPorts {
        module: String,
        error: Option<String>,
    },
    HdlSignals {
        query: Entity<InputState>,
    },
    HdlTop {
        circuit: Circuit,
        path: Option<PathBuf>,
        selected: String,
    },
    Verilog {
        creating: bool,
        name: Entity<InputState>,
        source: Entity<TextareaState>,
        interface: Entity<TextareaState>,
        error: Option<String>,
    },
    HdlInput {
        net: NetId,
        value: Entity<InputState>,
        error: Option<String>,
    },
    Vga {
        gate: GateId,
    },
    Find {
        query: Entity<InputState>,
    },
    Recovery,
    Symbol(crate::symbol_editor::SymbolDialog),
    Input {
        gate: GateId,
        value: Entity<InputState>,
        error: Option<String>,
    },
    Memory {
        gate: GateId,
        address: Entity<InputState>,
        value: Entity<InputState>,
        error: Option<String>,
    },
    Net(Box<NetDialog>),
    Terminal {
        gate: GateId,
        input: Entity<InputState>,
        error: Option<String>,
    },
    Properties(Box<PropertyDialog>),
    NewModule {
        name: Entity<InputState>,
        error: Option<String>,
    },
    Help,
    About,
}

/// Owns the active modal dialog and its GPUI overlay descriptor.
#[derive(Default)]
pub struct DialogHost {
    pub active: Option<Dialog>,
    pub open: Option<OpenDialog>,
}

impl DialogHost {
    pub fn is_open(&self) -> bool {
        self.active.is_some()
    }

    pub fn is_recovery(&self) -> bool {
        matches!(self.active, Some(Dialog::Recovery))
    }

    pub fn is_properties(&self) -> bool {
        matches!(self.active, Some(Dialog::Properties(_)))
    }

    pub fn allows_sim_transport(&self) -> bool {
        matches!(
            self.active,
            Some(Dialog::Terminal { .. } | Dialog::Vga { .. } | Dialog::HdlSignals { .. })
        )
    }

    pub fn show(&mut self, dialog: Dialog) {
        self.active = Some(dialog);
    }

    pub fn close(&mut self) {
        self.active = None;
    }

    pub fn dismiss(&mut self) -> bool {
        if self.is_recovery() {
            return false;
        }
        self.active = None;
        true
    }

    pub fn set_error(&mut self, error: anyhow::Error) -> Option<anyhow::Error> {
        let text = format!("{error:#}");
        match &mut self.active {
            Some(
                Dialog::Verilog { error: slot, .. }
                | Dialog::HdlInput { error: slot, .. }
                | Dialog::Input { error: slot, .. }
                | Dialog::Memory { error: slot, .. }
                | Dialog::Terminal { error: slot, .. }
                | Dialog::NewModule { error: slot, .. }
                | Dialog::VerilogPorts { error: slot, .. },
            ) => {
                *slot = Some(text);
                None
            }
            Some(Dialog::Net(dialog)) => {
                dialog.error = Some(text);
                None
            }
            Some(Dialog::Properties(dialog)) => {
                dialog.error = Some(text);
                None
            }
            _ => Some(error),
        }
    }
}

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

pub(super) fn field_row(label: &'static str, field: &Entity<InputState>) -> AnyElement {
    h_flex()
        .gap_3()
        .child(Label::new(label).w(px(140.0)).flex_none())
        .child(Input::new(field).small().flex_1())
        .into_any_element()
}

pub(super) fn error_text(error: &str, cx: &App) -> AnyElement {
    div()
        .text_color(cx.theme().danger)
        .child(error.to_owned())
        .into_any_element()
}

impl GateApp {
    pub(super) fn dialog_checkbox(
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
                if let Some(dialog) = &mut this.dialogs.active {
                    toggle(dialog, *checked);
                }
                cx.notify();
            }))
    }

    fn dialog_frame(&self) -> Option<(&'static str, f32)> {
        let gate_kind = |gate| {
            self.editor
                .module()
                .gate(gate)
                .map(|gate| gate.kind.clone())
        };
        Some(match self.dialogs.active.as_ref()? {
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
        let field = match self.dialogs.active.as_ref()? {
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

    /// Opens, replaces or closes the GPUI Kit dialog so it matches `self.dialogs.active`.
    pub(crate) fn sync_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let wanted = self.dialog_frame();
        let shown = self
            .dialogs
            .open
            .as_ref()
            .map(|open| (open.title, open.width));
        if wanted == shown {
            return;
        }
        self.dialogs.open = wanted.map(|(title, width)| OpenDialog { title, width });
        let app = cx.weak_entity();
        let closable = !self.dialogs.is_recovery();
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
                    .on_ok(move |_, window, cx| {
                        let _ = confirm.update(cx, |this, cx| {
                            if !matches!(this.dialogs.active, Some(Dialog::Verilog { .. })) {
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

    pub(crate) fn dialog_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(dialog) = &self.dialogs.active else {
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
        let Some(dialog) = &self.dialogs.active else {
            return div().into_any_element();
        };
        match dialog {
            Dialog::VerilogPorts { module, error } => {
                self.render_verilog_ports_dialog(module, error.as_deref(), cx)
            }
            Dialog::HdlSignals { query } => self.render_hdl_signals_dialog(query, cx),
            Dialog::HdlTop {
                circuit, selected, ..
            } => self.render_hdl_top_dialog(circuit, selected, cx),
            Dialog::Verilog {
                creating,
                name,
                source,
                interface,
                error,
            } => self.render_verilog_dialog(*creating, name, source, interface, error.as_deref(), cx),
            Dialog::HdlInput { value, error, .. } => {
                self.render_hdl_input_dialog(value, error.as_deref(), cx)
            }
            Dialog::Vga { gate } => self.render_vga_dialog(*gate, cx),
            Dialog::Find { query } => self.render_find_dialog(query, cx),
            Dialog::Recovery => self.render_recovery_dialog(cx),
            Dialog::Symbol(_) => crate::symbol_editor::editor(self, cx),
            Dialog::Input { value, error, .. } => {
                self.render_input_dialog(value, error.as_deref(), cx)
            }
            Dialog::Memory {
                gate,
                address,
                value,
                error,
            } => self.render_memory_dialog(*gate, address, value, error.as_deref(), cx),
            Dialog::Net(properties) => self.render_net_dialog(properties, cx),
            Dialog::Terminal { gate, input, error } => {
                self.render_terminal_dialog(*gate, input, error.as_deref(), cx)
            }
            Dialog::Properties(properties) => self.render_properties_dialog(properties, cx),
            Dialog::NewModule { name, error } => {
                self.render_new_module_dialog(name, error.as_deref(), cx)
            }
            Dialog::Help => self.render_help_dialog(cx),
            Dialog::About => self.render_about_dialog(cx),
        }
    }

    pub fn dismiss_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.dialogs.dismiss() {
            return;
        }
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub fn apply_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.dialogs.active, Some(Dialog::VerilogPorts { .. })) {
            match crate::verilog_editor::refresh_ports(&mut self.editor) {
                Ok(()) => self.log("Module ports refreshed from source.".into()),
                Err(error) => {
                    let _ = self.dialogs.set_error(error);
                }
            }
            cx.notify();
            return;
        }
        if let Some(Dialog::HdlTop {
            circuit,
            path,
            selected,
        }) = self.dialogs.active.as_ref()
        {
            let mut circuit = circuit.clone();
            circuit.root = selected.clone();
            let path = path.clone();
            self.dialogs.close();
            self.load_document(circuit, path);
            cx.notify();
            return;
        }
        if let Some(Dialog::Find { query }) = &self.dialogs.active {
            if let Some(result) =
                rgate_editor::search(self.editor.circuit(), &query.read(cx).value())
                    .into_iter()
                    .next()
            {
                self.select_search_result(result, window, cx);
            }
            return;
        }
        if self.dialogs.is_recovery() {
            self.restore_recovery(window, cx);
            return;
        }
        if matches!(
            self.dialogs.active,
            Some(Dialog::Help | Dialog::About | Dialog::Vga { .. } | Dialog::HdlSignals { .. })
        ) {
            self.dialogs.close();
            self.focus.focus(window, cx);
            cx.notify();
            return;
        }

        let terminal_dialog = matches!(
            self.dialogs.active,
            Some(
                Dialog::Terminal { .. }
                    | Dialog::Memory { .. }
                    | Dialog::Input { .. }
                    | Dialog::HdlInput { .. }
            )
        );
        let creating_module = matches!(self.dialogs.active, Some(Dialog::NewModule { .. }));
        let runtime_dialog_gate = match self.dialogs.active.as_ref() {
            Some(
                Dialog::Memory { gate, .. }
                | Dialog::Terminal { gate, .. }
                | Dialog::Input { gate, .. },
            ) => self.sim.scoped_gate(*gate),
            _ => None,
        };

        let result = self.commit_dialog(runtime_dialog_gate, cx);
        match result {
            Ok(()) => {
                self.dialogs.close();
                if !terminal_dialog {
                    self.changed();
                }
                if creating_module {
                    self.sim.probes.clear();
                    self.canvas.clear_interaction();
                    self.editor.set_tool(Tool::Select);
                    self.chrome.tab = WorkspaceTab::Edit;
                    self.canvas.need_fit = true;
                    self.log(format!(
                        "Created module {}. Place gates, then connect pins to create nets.",
                        self.editor.active_module()
                    ));
                }
                self.focus.focus(window, cx);
            }
            Err(error) => {
                if let Some(unhandled) = self.dialogs.set_error(error) {
                    self.error(unhandled);
                }
            }
        }
        cx.notify();
    }

    fn commit_dialog(
        &mut self,
        runtime_dialog_gate: Option<GateId>,
        cx: &Context<Self>,
    ) -> Result<()> {
        match self.dialogs.active.as_ref() {
            Some(Dialog::Verilog {
                creating,
                name,
                source,
                interface,
                ..
            }) => {
                let creating = *creating;
                let name = name.read(cx).value().trim().to_owned();
                let source = source.read(cx).value().to_string();
                let interface = interface.read(cx).value().to_string();
                self.apply_verilog_dialog(creating, name, source, interface)
            }
            Some(Dialog::HdlInput { net, value, .. }) => {
                let net = *net;
                let raw = value.read(cx).value().to_string();
                self.apply_hdl_input(net, &raw)
            }
            Some(Dialog::Symbol(dialog)) => self
                .editor
                .set_module_symbol(&dialog.module, dialog.shapes.clone())
                .map_err(Into::into),
            Some(Dialog::Input { gate, value, .. }) => {
                let gate = *gate;
                let raw = value.read(cx).value().to_string();
                self.apply_input_dialog(gate, runtime_dialog_gate, &raw)
            }
            Some(Dialog::Memory {
                gate,
                address,
                value,
                ..
            }) => {
                let gate = *gate;
                let address = address.read(cx).value().to_string();
                let raw = value.read(cx).value().to_string();
                self.apply_memory_dialog(gate, runtime_dialog_gate, &address, &raw)
            }
            Some(Dialog::Terminal { input, .. }) => {
                let text = input.read(cx).value().to_string();
                self.apply_terminal_dialog(runtime_dialog_gate, &text)
            }
            Some(Dialog::Net(dialog)) => dialog.apply_to_editor(&mut self.editor, cx),
            Some(Dialog::Properties(dialog)) => dialog.apply_to_editor(&mut self.editor, cx),
            Some(Dialog::NewModule { name, .. }) => self
                .editor
                .create_module(name.read(cx).value().trim().to_owned())
                .map_err(Into::into),
            _ => Ok(()),
        }
    }
}
