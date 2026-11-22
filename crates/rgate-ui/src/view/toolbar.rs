use crate::{app::GateApp, commands::Command};
use gpui::{AnyElement, Context, prelude::*, px};
use gpui_component::{
    ActiveTheme, Disableable as _, Icon, Selectable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    separator::Separator,
    toolbar::{Toolbar, ToolbarGroup},
};
use gpui_kit_assets::IconName;
use rgate_editor::Tool;

impl GateApp {
    fn tool_button(
        &self,
        id: &'static str,
        icon: IconName,
        tooltip: &'static str,
        command: Command,
        active: bool,
        cx: &mut Context<Self>,
    ) -> Button {
        Button::new(id)
            .ghost()
            .small()
            .icon(Icon::new(icon).small())
            .tooltip(tooltip)
            .selected(active)
            .when(active, |button| {
                button.border_1().border_color(cx.theme().primary)
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.focus.focus(window, cx);
                this.command(command.clone(), window, cx);
            }))
    }

    pub(super) fn toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        use Command as C;
        let tool = self.editor.tool.clone();
        let file = ToolbarGroup::new("toolbar-file")
            .label("File")
            .child(self.tool_button(
                "tool-new",
                IconName::FilePlus,
                "New circuit (⌘N)",
                C::New,
                false,
                cx,
            ))
            .child(self.tool_button(
                "tool-open",
                IconName::FolderOpen,
                "Open TkGate .v / RGate circuit (⌘O)",
                C::Open,
                false,
                cx,
            ))
            .child(self.tool_button(
                "tool-save",
                IconName::Save,
                "Save circuit (⌘S)",
                C::Save,
                false,
                cx,
            ))
            .child(self.tool_button(
                "tool-save-as",
                IconName::SaveAll,
                "Save circuit as… (⇧⌘S)",
                C::SaveAs,
                false,
                cx,
            ))
            .child(self.tool_button(
                "tool-export",
                IconName::FileCode,
                "Export Verilog with lossless layout metadata",
                C::Export,
                false,
                cx,
            ));
        let history = ToolbarGroup::new("toolbar-history")
            .label("History")
            .child(
                self.tool_button(
                    "tool-undo",
                    IconName::Undo2,
                    "Undo (⌘Z)",
                    C::Undo,
                    false,
                    cx,
                )
                .disabled(!self.editor.can_undo()),
            )
            .child(
                self.tool_button(
                    "tool-redo",
                    IconName::Redo2,
                    "Redo (⇧⌘Z)",
                    C::Redo,
                    false,
                    cx,
                )
                .disabled(!self.editor.can_redo()),
            );
        let clipboard = ToolbarGroup::new("toolbar-clipboard")
            .label("Clipboard")
            .child(self.tool_button(
                "tool-cut",
                IconName::Scissors,
                "Cut selected components (⌘X)",
                C::Cut,
                false,
                cx,
            ))
            .child(self.tool_button(
                "tool-copy",
                IconName::Copy,
                "Copy selected components (⌘C)",
                C::Copy,
                false,
                cx,
            ))
            .child(self.tool_button(
                "tool-paste",
                IconName::ClipboardPaste,
                "Paste components (⌘V)",
                C::Paste,
                false,
                cx,
            ));
        let tools = ToolbarGroup::new("toolbar-tools")
            .label("Tools")
            .child(self.tool_button(
                "tool-select",
                IconName::MousePointer2,
                "Select / move (V)",
                C::Tool(Tool::Select),
                tool == Tool::Select,
                cx,
            ))
            .child(self.tool_button(
                "tool-wire",
                IconName::Spline,
                "Connect pins with orthogonal wires (W)",
                C::Tool(Tool::Wire),
                tool == Tool::Wire,
                cx,
            ))
            .child(self.tool_button(
                "tool-pan",
                IconName::Hand,
                "Scroll / pan canvas (P)",
                C::Tool(Tool::Pan),
                tool == Tool::Pan,
                cx,
            ))
            .child(self.tool_button(
                "tool-delete",
                IconName::Eraser,
                "Cut wire / delete gate (D)",
                C::Tool(Tool::Delete),
                tool == Tool::Delete,
                cx,
            ));
        let gate = ToolbarGroup::new("toolbar-gate")
            .label("Gate")
            .child(self.tool_button(
                "tool-rotate",
                IconName::RotateCw,
                "Rotate clockwise (R)",
                C::Rotate(true),
                false,
                cx,
            ))
            .child(self.tool_button(
                "tool-rotate-back",
                IconName::RotateCcw,
                "Rotate counterclockwise (⇧R)",
                C::Rotate(false),
                false,
                cx,
            ))
            .child(self.tool_button(
                "tool-properties",
                IconName::SlidersHorizontal,
                "Gate properties (Enter)",
                C::Properties,
                false,
                cx,
            ));
        let simulate = ToolbarGroup::new("toolbar-simulate")
            .label("Simulation")
            .child(self.tool_button(
                "tool-run",
                IconName::Play,
                "Run / pause simulation (Space)",
                C::PlayPause,
                self.running,
                cx,
            ))
            .child(self.tool_button(
                "tool-pause",
                IconName::Pause,
                "Pause / resume simulation (Space)",
                C::PlayPause,
                self.simulation.is_some() && !self.running,
                cx,
            ))
            .child(
                self.tool_button(
                    "tool-stop",
                    IconName::Square,
                    "Stop simulation",
                    C::Stop,
                    false,
                    cx,
                )
                .disabled(self.simulation.is_none()),
            )
            .child(self.tool_button(
                "tool-step",
                IconName::StepForward,
                "Step one simulation event (F6)",
                C::Step,
                false,
                cx,
            ))
            .child(self.tool_button(
                "tool-clock",
                IconName::Timer,
                "Advance one clock cycle (Tab)",
                C::ClockStep,
                false,
                cx,
            ));
        let view = ToolbarGroup::new("toolbar-view")
            .label("View")
            .child(self.tool_button(
                "tool-zoom-in",
                IconName::ZoomIn,
                "Zoom in (+)",
                C::Zoom(1.25),
                false,
                cx,
            ))
            .child(self.tool_button(
                "tool-zoom-out",
                IconName::ZoomOut,
                "Zoom out (−)",
                C::Zoom(0.8),
                false,
                cx,
            ))
            .child(self.tool_button(
                "tool-fit",
                IconName::Scan,
                "Fit circuit (⌘0)",
                C::Fit,
                false,
                cx,
            ));
        let separator = || Separator::vertical().h(px(18.0)).mx_1();
        Toolbar::new("main-toolbar")
            .h(px(32.0))
            .w_full()
            .px_2()
            .bg(cx.theme().background)
            .border_b_1()
            .border_color(cx.theme().border)
            .child(file)
            .content(separator())
            .child(history)
            .content(separator())
            .child(clipboard)
            .content(separator())
            .child(tools)
            .content(separator())
            .child(gate)
            .content(separator())
            .child(simulate)
            .content(separator())
            .child(view)
            .into_any_element()
    }
}
