use super::property_dialog::PropertyDialog;
use anyhow::{Context as _, Result};
use gpui::{Context, Entity, Window};
use gpui_component::input::{InputState, TextareaState};
use rgate_core::{Circuit, GateId, Net, NetId};
use rgate_editor::Editor;
use std::path::PathBuf;

/// The GPUI Kit dialog currently mirroring `DialogHost::active`.
pub struct OpenDialog {
    pub title: &'static str,
    pub width: f32,
}

pub struct NetDialog {
    pub net: NetId,
    pub name: Entity<InputState>,
    pub width: Entity<InputState>,
    pub show_name: bool,
    pub error: Option<String>,
}

impl NetDialog {
    pub fn from_net<T: 'static>(net: &Net, window: &mut Window, cx: &mut Context<T>) -> Self {
        Self {
            net: net.id,
            name: crate::input::text_field(&net.name, window, cx),
            width: crate::input::text_field(net.width.to_string(), window, cx),
            show_name: net.show_name,
            error: None,
        }
    }

    pub fn apply_to_editor<T: 'static>(&self, editor: &mut Editor, cx: &Context<T>) -> Result<()> {
        let id = self.net;
        let name = self.name.read(cx).value().trim().to_owned();
        if name.is_empty() {
            anyhow::bail!("Net name must not be empty.");
        }
        let width: u16 = self
            .width
            .read(cx)
            .value()
            .parse()
            .context("Width must be 1–4096.")?;
        if !(1..=4096).contains(&width) {
            anyhow::bail!("Width must be 1–4096.");
        }
        if editor.module().gates.iter().any(|gate| {
            gate.pins
                .iter()
                .any(|pin| pin.net == Some(id) && gate.pin_width(&pin.name) != width)
        }) {
            anyhow::bail!(
                "Width conflicts with connected pins. Disconnect or resize the components first."
            );
        }
        let show = self.show_name;
        editor.edit_module(|module| {
            if let Some(net) = module.nets.iter_mut().find(|net| net.id == id) {
                net.name = name;
                net.width = width;
                net.show_name = show;
            }
        })?;
        Ok(())
    }
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

    /// Returns `true` if a non-recovery dialog was dismissed.
    pub fn dismiss(&mut self) -> bool {
        if self.is_recovery() {
            return false;
        }
        self.active = None;
        true
    }

    /// Writes `error` into the active dialog's inline error slot, or returns it
    /// if the active dialog has no error banner.
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
