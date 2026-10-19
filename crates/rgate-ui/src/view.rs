use crate::{
    app::{ContextMenu, GateApp, WorkspaceTab},
    canvas,
    commands::{self, Command, MenuEntry},
    theme::{Appearance, Theme},
};
use gpui::{
    AnyElement, App, AppContext, Context, CursorStyle, DismissEvent, Entity, Focusable, FontWeight,
    MouseButton, Pixels, SharedString, WeakEntity, Window, anchored, canvas as drawing, deferred,
    div, prelude::*, px, rgb,
};
use gpui_component::{
    ActiveTheme, Icon, Selectable as _, Sizable as _, Theme as KitTheme, ThemeMode,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::Textarea,
    label::Label,
    list::ListItem,
    menu::{DropdownMenu as _, PopupMenu, PopupMenuItem},
    resizable::{ResizableState, h_resizable, resizable_panel, v_resizable},
    scroll::ScrollableElement as _,
    separator::Separator,
    status_bar::StatusBar,
    tab::{Tab, TabBar},
    toolbar::{Toolbar, ToolbarGroup},
    v_flex,
};
use gpui_kit_assets::IconName;
use rgate_core::{Direction, GateKind, Point};
use rgate_editor::Tool;

const MENUS: [&str; 9] = [
    "File", "Edit", "View", "Tool", "Simulate", "Module", "Gate", "Arrange", "Help",
];
const MESSAGE_HISTORY: usize = 30;

/// Persistent split state for the main window panes.
pub struct PanelLayout {
    main: Entity<ResizableState>,
    sidebar: Entity<ResizableState>,
    center: Entity<ResizableState>,
    extents: [Pixels; 3],
}

impl PanelLayout {
    pub fn new(cx: &mut App) -> Self {
        Self {
            main: cx.new(|_| ResizableState::default()),
            sidebar: cx.new(|_| ResizableState::default()),
            center: cx.new(|_| ResizableState::default()),
            extents: [Pixels::ZERO; 3],
        }
    }

    /// GPUI Kit rescales every pane proportionally when its container changes size.
    /// Side panes should keep their chosen size instead, leaving the canvas to absorb
    /// the difference (this matters most on the web, where the canvas grows after startup).
    fn keep_side_panes(&mut self, sizes: [&[(usize, f32)]; 3], window: &mut Window, cx: &mut App) {
        let states = [&self.main, &self.sidebar, &self.center];
        for ((state, extent), fixed) in states.into_iter().zip(&mut self.extents).zip(sizes) {
            let current = state.read(cx).container_size();
            if current == *extent || current <= Pixels::ZERO {
                continue;
            }
            *extent = current;
            state.update(cx, |state, cx| {
                for &(index, size) in fixed {
                    if state
                        .sizes()
                        .get(index)
                        .is_some_and(|actual| *actual != px(size))
                    {
                        state.resize_panel(index, px(size), window, cx);
                    }
                }
            });
        }
    }
}

pub fn apply_appearance(appearance: Appearance, window: Option<&mut Window>, cx: &mut App) {
    let mode = if appearance.is_dark() {
        ThemeMode::Dark
    } else {
        ThemeMode::Light
    };
    KitTheme::change(mode, window, cx);
    // The web platform has no system fonts, only the families bundled at startup.
    if cfg!(target_family = "wasm") {
        let theme = KitTheme::global_mut(cx);
        theme.font_family = "IBM Plex Sans".into();
        theme.mono_font_family = "Lilex".into();
    }
}

pub fn mono_font() -> &'static str {
    if cfg!(target_family = "wasm") {
        "Lilex"
    } else {
        "Menlo"
    }
}

/// Which View-menu entries currently carry a check mark.
#[derive(Clone, Copy)]
struct MenuChecks {
    theme: Theme,
    appearance: Appearance,
    grid: bool,
    snap: bool,
}

impl MenuChecks {
    fn checked(self, command: &Command) -> bool {
        match command {
            Command::SetTheme(theme) => *theme == self.theme,
            Command::SetAppearance(appearance) => *appearance == self.appearance,
            Command::ToggleGrid => self.grid,
            Command::ToggleSnap => self.snap,
            _ => false,
        }
    }
}

fn menu_items(
    mut menu: PopupMenu,
    entries: Vec<MenuEntry>,
    app: WeakEntity<GateApp>,
    checks: Option<MenuChecks>,
) -> PopupMenu {
    for entry in entries {
        let Some(command) = entry.command else {
            menu = menu.separator();
            continue;
        };
        let checked = checks.is_some_and(|checks| checks.checked(&command));
        let (label, shortcut) = (entry.label, entry.shortcut);
        let app = app.clone();
        menu = menu.item(
            PopupMenuItem::element(move |_, cx| {
                h_flex()
                    .w_full()
                    .gap_6()
                    .justify_between()
                    .child(label)
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(shortcut),
                    )
            })
            .checked(checked)
            .on_click(move |_, window, cx| {
                if let Err(error) =
                    app.update(cx, |this, cx| this.command(command.clone(), window, cx))
                {
                    eprintln!("menu command ignored after window closed: {error}");
                }
            }),
        );
    }
    menu
}

impl GateApp {
    fn menu_checks(&self) -> MenuChecks {
        MenuChecks {
            theme: self.theme,
            appearance: self.appearance,
            grid: self.editor.show_grid,
            snap: self.editor.snap,
        }
    }

    pub(super) fn button(
        &self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        command: Command,
        active: bool,
        cx: &mut Context<Self>,
    ) -> Button {
        Button::new(id.into())
            .small()
            .label(label)
            .selected(active)
            .on_click(
                cx.listener(move |this, _, window, cx| this.command(command.clone(), window, cx)),
            )
    }

    pub(super) fn release_reset_button(&self, id: &'static str, cx: &mut Context<Self>) -> Button {
        Button::new(id)
            .small()
            .primary()
            .label("Release reset (RESET_N → 1)")
            .on_click(cx.listener(|this, _, _, cx| this.release_terminal_reset(cx)))
    }

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
            .icon(icon)
            .tooltip(tooltip)
            .selected(active)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.focus.focus(window, cx);
                this.command(command.clone(), window, cx);
            }))
    }

    fn menu_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let app = cx.weak_entity();
        let checks = self.menu_checks();
        let mut menus = h_flex().gap_0p5();
        for name in MENUS {
            let app = app.clone();
            menus = menus.child(
                Button::new(SharedString::from(format!("menu-{name}")))
                    .ghost()
                    .xsmall()
                    .label(name)
                    .dropdown_menu(move |menu, _, _| {
                        menu_items(
                            menu.min_w(px(240.0)),
                            commands::entries(name),
                            app.clone(),
                            Some(checks),
                        )
                    }),
            );
        }
        let theme = cx.theme();
        h_flex()
            .h(px(32.0))
            .px_2()
            .flex_none()
            .bg(theme.title_bar)
            .border_b_1()
            .border_color(theme.title_bar_border)
            .child(menus)
            .child(div().flex_1())
            .child(
                Label::new("RGate 0.1 · Rust / GPUI Kit")
                    .text_xs()
                    .text_color(theme.muted_foreground),
            )
            .into_any_element()
    }

    fn toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
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
            .child(self.tool_button(
                "tool-undo",
                IconName::Undo2,
                "Undo (⌘Z)",
                C::Undo,
                false,
                cx,
            ))
            .child(self.tool_button(
                "tool-redo",
                IconName::Redo2,
                "Redo (⇧⌘Z)",
                C::Redo,
                false,
                cx,
            ));
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
            .child(self.tool_button(
                "tool-stop",
                IconName::Square,
                "Stop simulation",
                C::Stop,
                false,
                cx,
            ))
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
            .w_full()
            .px_2()
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

    fn modules_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let rows = if self.module_list {
            self.editor
                .circuit()
                .modules
                .iter()
                .map(|module| rgate_core::HierarchyRow {
                    path: format!("definition/{}", module.name),
                    module: module.name.clone(),
                    label: module.name.clone(),
                    depth: 0,
                    has_children: false,
                })
                .collect::<Vec<_>>()
        } else {
            self.editor
                .circuit()
                .hierarchy_rows(&self.collapsed_modules)
        };
        let mut list = v_flex().id("module-list").flex_1().min_h_0().p_1();
        for row in rows {
            let name = row.module.clone();
            let active = if self.simulation.is_some() {
                self.debug_path.as_deref() == Some(row.path.as_str())
            } else {
                row.module == self.editor.active_module()
            };
            let expander = if row.has_children {
                let path = row.path.clone();
                let expanded = !self.collapsed_modules.contains(&row.path);
                Button::new(SharedString::from(format!("expand-{}", row.path)))
                    .ghost()
                    .xsmall()
                    .icon(if expanded {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.collapsed_modules.remove(&path) {
                            this.collapsed_modules.insert(path.clone());
                        }
                        cx.stop_propagation();
                        cx.notify();
                    }))
                    .into_any_element()
            } else {
                div().size(px(20.0)).flex_none().into_any_element()
            };
            let mut content = h_flex()
                .w_full()
                .min_w_0()
                .gap_1()
                .child(expander)
                .child(
                    Icon::new(if row.path == self.editor.circuit().root {
                        IconName::Cpu
                    } else {
                        IconName::Box
                    })
                    .small()
                    .text_color(muted),
                )
                .child(div().flex_1().min_w_0().truncate().child(row.label));
            if row.module != self.editor.active_module() {
                let name = name.clone();
                content = content.child(
                    Button::new(SharedString::from(format!("instantiate-{}", row.path)))
                        .ghost()
                        .xsmall()
                        .icon(IconName::Plus)
                        .tooltip("Place an instance of this module")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.command(
                                Command::Tool(Tool::Place(GateKind::Module(name.clone()))),
                                window,
                                cx,
                            );
                            cx.stop_propagation();
                        })),
                );
            }
            let navigate_path = row.path.clone();
            list = list.child(
                ListItem::new(SharedString::from(format!("hierarchy-{}", row.path)))
                    .text_sm()
                    .selected(active)
                    .py_0p5()
                    .pl(px(4.0 + row.depth as f32 * 14.0))
                    .child(content)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if this.module_list {
                            this.switch_module(name.clone(), cx)
                        } else {
                            this.navigate_scope(navigate_path.clone(), name.clone(), cx)
                        }
                    })),
            );
        }
        v_flex()
            .size_full()
            .child(
                h_flex()
                    .h(px(34.0))
                    .flex_none()
                    .px_2()
                    .gap_1()
                    .border_b_1()
                    .border_color(cx.theme().sidebar_border)
                    .child(
                        div()
                            .flex_1()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Modules"),
                    )
                    .child(
                        Button::new("modules-tree")
                            .ghost()
                            .xsmall()
                            .icon(IconName::ListTree)
                            .tooltip("Instance hierarchy")
                            .selected(!self.module_list)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.module_list = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("modules-flat")
                            .ghost()
                            .xsmall()
                            .icon(IconName::List)
                            .tooltip("Module definitions")
                            .selected(self.module_list)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.module_list = true;
                                cx.notify();
                            })),
                    ),
            )
            .child(list.overflow_y_scrollbar())
            .into_any_element()
    }

    fn nets_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let (muted, border) = (theme.muted_foreground, theme.sidebar_border);
        let mut definitions = self
            .editor
            .module()
            .nets
            .iter()
            .filter(|net| !self.nets_ports || net.port.is_some())
            .collect::<Vec<_>>();
        definitions.sort_by(|a, b| a.name.cmp(&b.name));
        let mut list = v_flex().id("net-list").flex_1().min_h_0().p_1();
        for net in definitions {
            let id = net.id;
            let value = self
                .displayed_value(id)
                .map(ToString::to_string)
                .unwrap_or_else(|| "—".into());
            let color = self
                .displayed_value(id)
                .map(|value| self.palette().signal_color(value))
                .unwrap_or(muted);
            let name = if net.width > 1 {
                format!("{} [{}:0]", net.name, net.width - 1)
            } else {
                net.name.clone()
            };
            let probed = self.is_probed(id);
            list = list.child(
                ListItem::new(SharedString::from(format!("net-{}", id.0)))
                    .selected(self.editor.selected_net == Some(id))
                    .py_0p5()
                    .child(
                        h_flex()
                            .w_full()
                            .min_w_0()
                            .gap_1p5()
                            .child(
                                Icon::new(if net.port == Some(Direction::Output) {
                                    IconName::ArrowRight
                                } else {
                                    IconName::Spline
                                })
                                .small()
                                .text_color(muted),
                            )
                            .child(div().flex_1().min_w_0().truncate().text_xs().child(name))
                            .child(
                                div()
                                    .text_color(color)
                                    .font_family(mono_font())
                                    .text_xs()
                                    .child(value),
                            )
                            .child(
                                Button::new(SharedString::from(format!("probe-{}", id.0)))
                                    .ghost()
                                    .xsmall()
                                    .icon(if probed {
                                        IconName::CircleDot
                                    } else {
                                        IconName::Circle
                                    })
                                    .tooltip(if probed {
                                        "Remove waveform probe"
                                    } else {
                                        "Add waveform probe"
                                    })
                                    .selected(probed)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.toggle_probe(id, cx);
                                        cx.stop_propagation();
                                    })),
                            ),
                    )
                    .on_click(cx.listener(move |this, event: &gpui::ClickEvent, _, cx| {
                        this.editor.selected_net = Some(id);
                        this.editor.selection.clear();
                        this.editor.selected_wires.clear();
                        if event.click_count() >= 2 {
                            this.toggle_probe(id, cx);
                        }
                        cx.notify();
                    })),
            );
        }
        let empty_hint = if !self.nets_ports && self.editor.module().nets.is_empty() {
            Some(
                "No nets yet. Placing gates does not create nets. Press W, then connect two pins to add a net here.",
            )
        } else if self.nets_ports
            && self
                .editor
                .module()
                .nets
                .iter()
                .all(|net| net.port.is_none())
        {
            Some("No ports. Select a net in the Interface tab to expose it as an input or output.")
        } else {
            None
        };
        if let Some(hint) = empty_hint {
            list = list.child(div().p_2().text_xs().text_color(muted).child(hint));
        }
        v_flex()
            .size_full()
            .child(
                h_flex().h(px(34.0)).flex_none().px_2().child(
                    TabBar::new("nets-tabs")
                        .segmented()
                        .xsmall()
                        .selected_index(usize::from(self.nets_ports))
                        .on_click(cx.listener(|this, index: &usize, _, cx| {
                            this.nets_ports = *index == 1;
                            cx.notify();
                        }))
                        .child(Tab::new().label("Nets"))
                        .child(Tab::new().label("Ports")),
                ),
            )
            .child(list.overflow_y_scrollbar())
            .child(
                div()
                    .flex_none()
                    .px_2()
                    .py_1()
                    .border_t_1()
                    .border_color(border)
                    .text_xs()
                    .text_color(muted)
                    .child(format!(
                        "{} nets · click ○ to probe",
                        self.editor.module().nets.len()
                    )),
            )
            .into_any_element()
    }

    fn sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let app = cx.weak_entity();
        v_resizable("sidebar-split")
            .with_state(&self.layout.sidebar)
            .on_resize(move |state, _, cx| {
                let height = state.read(cx).sizes().first().copied();
                if let Some(height) = height
                    && let Err(error) =
                        app.update(cx, |this, _| this.modules_height = f32::from(height))
                {
                    eprintln!("sidebar resize ignored: {error}");
                }
            })
            .child(
                resizable_panel()
                    .size(px(self.modules_height))
                    .size_range(px(80.0)..Pixels::MAX)
                    .child(self.modules_panel(cx)),
            )
            .child(
                resizable_panel()
                    .size_range(px(110.0)..Pixels::MAX)
                    .child(self.nets_panel(cx)),
            )
            .into_any_element()
    }

    fn workspace_tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        const TABS: [WorkspaceTab; 3] = [
            WorkspaceTab::Edit,
            WorkspaceTab::Interface,
            WorkspaceTab::Simulate,
        ];
        let muted = cx.theme().muted_foreground;
        let mut status = h_flex().gap_2().pr_2().text_xs();
        if let Some(path) = &self.debug_path {
            if let Some((parent, _)) = path.rsplit_once('/') {
                let parent = parent.to_owned();
                status = status.child(
                    Button::new("debug-up")
                        .ghost()
                        .xsmall()
                        .icon(IconName::ArrowUp)
                        .label("Parent")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(name) = this
                                .simulation
                                .as_ref()
                                .and_then(|sim| sim.scope_module(&parent))
                                .map(ToOwned::to_owned)
                            {
                                this.navigate_scope(parent.clone(), name, cx);
                            }
                        })),
                );
            }
            if let Some(simulation) = &self.simulation {
                status = status.child(
                    div()
                        .text_color(muted)
                        .child(format!("Live: {path} · {}", simulation.backend_name())),
                );
            }
        }
        #[cfg(feature = "icarus")]
        let compiling = self.compilation.is_some();
        #[cfg(not(feature = "icarus"))]
        let compiling = false;
        if compiling || self.simulation.is_some() {
            let state = if compiling {
                "Compiling · Stop cancels"
            } else if self.running {
                "Running"
            } else {
                "Paused"
            };
            let theme = cx.theme();
            status = status.child(
                h_flex()
                    .gap_1p5()
                    .child(div().size(px(6.0)).rounded_full().bg(if self.running {
                        theme.success
                    } else {
                        theme.muted_foreground
                    }))
                    .child(state),
            );
        }
        TabBar::new("workspace-tabs")
            .underline()
            .small()
            .selected_index(TABS.iter().position(|tab| *tab == self.tab).unwrap_or(0))
            .on_click(cx.listener(move |this, index: &usize, window, cx| {
                this.set_tab(TABS[*index], window, cx)
            }))
            .child(
                Tab::new()
                    .label("Edit")
                    .prefix(Icon::new(IconName::Pencil).small()),
            )
            .child(
                Tab::new()
                    .label("Interface")
                    .prefix(Icon::new(IconName::Plug).small()),
            )
            .child(
                Tab::new()
                    .label("Simulate")
                    .prefix(Icon::new(IconName::Activity).small()),
            )
            .suffix(status)
            .into_any_element()
    }

    fn schematic(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        #[cfg(not(target_family = "wasm"))]
        if self.tab == WorkspaceTab::Edit && self.editor.module().verilog.is_some() {
            return self.source_editor(window, cx);
        }
        #[cfg(target_family = "wasm")]
        let _ = window;
        let scene = self.scene();
        let entity = cx.entity().downgrade();
        let cursor = match self.editor.tool {
            Tool::Wire | Tool::Place(_) | Tool::Delete => CursorStyle::Crosshair,
            Tool::Pan => CursorStyle::OpenHand,
            _ => CursorStyle::Arrow,
        };
        div()
            .id("schematic")
            .flex_1()
            .size_full()
            .min_h_0()
            .min_w_0()
            .relative()
            .overflow_hidden()
            .bg(rgb(self.palette().panel))
            .cursor(cursor)
            .child(
                drawing(
                    move |bounds, _, cx| match entity.update(cx, |this, _| {
                        let previous_size = this.canvas_bounds.size;
                        this.canvas_bounds = bounds;
                        // The browser canvas grows over several frames after startup; keep
                        // refitting until the user moves the view themselves.
                        if cfg!(target_family = "wasm")
                            && previous_size != bounds.size
                            && this.auto_fit.as_ref() == Some(&this.editor.viewport)
                        {
                            this.need_fit = true;
                        }
                        if this.need_fit {
                            let content_bounds = this.editor.module().bounds();
                            this.editor.viewport.fit(
                                content_bounds,
                                Point::new(
                                    f32::from(bounds.size.width),
                                    f32::from(bounds.size.height),
                                ),
                            );
                            this.need_fit = false;
                            this.auto_fit = Some(this.editor.viewport.clone());
                        }
                        this.scene()
                    }) {
                        Ok(scene) => scene,
                        Err(error) => {
                            eprintln!("canvas prepaint failed: {error}");
                            scene
                        }
                    },
                    canvas::paint,
                )
                .size_full(),
            )
            .on_drop(cx.listener(
                |this, component: &crate::components::ComponentDrag, window, cx| {
                    this.drop_component(
                        component.kind.clone(),
                        window.mouse_position(),
                        window,
                        cx,
                    );
                },
            ))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::mouse_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::mouse_up))
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_scroll_wheel(cx.listener(Self::scroll))
            .on_pinch(cx.listener(Self::pinch))
            .into_any_element()
    }

    fn interface(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let (muted, border, radius) = (theme.muted_foreground, theme.border, theme.radius);
        let mut rows = v_flex().id("interface-ports").p_1();
        for net in &self.editor.module().nets {
            let id = net.id;
            rows = rows.child(
                ListItem::new(SharedString::from(format!("interface-net-{}", id.0)))
                    .text_sm()
                    .selected(self.editor.selected_net == Some(id))
                    .child(
                        h_flex()
                            .w_full()
                            .gap_3()
                            .child(div().w(px(170.0)).truncate().child(net.name.clone()))
                            .child(
                                div()
                                    .w(px(60.0))
                                    .text_color(muted)
                                    .child(format!("{} bit", net.width)),
                            )
                            .child(div().flex_1().child(match net.port {
                                Some(Direction::Input) => "input",
                                Some(Direction::Output) => "output",
                                Some(Direction::InOut) => "inout",
                                None => "internal",
                            })),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.editor.selected_net = Some(id);
                        cx.notify();
                    })),
            );
        }
        v_flex()
            .flex_1()
            .min_h_0()
            .p_4()
            .gap_3()
            .child(
                div()
                    .text_lg()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(format!("Module interface: {}", self.editor.active_module())),
            )
            .child(
                Label::new("Select a net, then set its direction. Ports are included in Verilog exports.")
                    .text_color(muted),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(self.button("port-input", "Input", Command::Port(Some(Direction::Input)), false, cx))
                    .child(self.button("port-output", "Output", Command::Port(Some(Direction::Output)), false, cx))
                    .child(self.button("port-inout", "InOut", Command::Port(Some(Direction::InOut)), false, cx))
                    .child(self.button("port-internal", "Internal", Command::Port(None), false, cx)),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .border_1()
                    .border_color(border)
                    .rounded(radius)
                    .child(rows.size_full().overflow_y_scrollbar()),
            )
            .child(
                Label::new("Ports become pins on module instances. Click + beside a module to place it in another module. Recursive instances are rejected.")
                    .text_xs()
                    .text_color(muted),
            )
            .into_any_element()
    }

    fn message_log(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let start = self.messages.len().saturating_sub(MESSAGE_HISTORY);
        let text = self.messages[start..].join("\n");
        if self.message_log.read(cx).value().as_ref() != text {
            let last_row = self.messages.len().saturating_sub(start + 1) as u32;
            self.message_log.update(cx, |log, cx| {
                log.set_value(text, window, cx);
                log.set_cursor_position(
                    gpui_component::input::Position::new(last_row, 0),
                    window,
                    cx,
                );
            });
        }
        div()
            .id("messages-content")
            .flex_1()
            .min_h_0()
            .child(
                Textarea::new(&self.message_log)
                    .readonly(true)
                    .bordered(false)
                    .small()
                    .h(gpui::relative(1.0)),
            )
            .into_any_element()
    }

    fn bottom_panel(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let content = if self.bottom_scope {
            self.waveform_panel(window, cx)
        } else {
            self.message_log(window, cx)
        };
        v_flex()
            .size_full()
            .child(
                TabBar::new("bottom-tabs")
                    .underline()
                    .xsmall()
                    .selected_index(usize::from(self.bottom_scope))
                    .on_click(cx.listener(|this, index: &usize, _, cx| {
                        this.bottom_scope = *index == 1;
                        cx.notify();
                    }))
                    .child(
                        Tab::new()
                            .label("Messages")
                            .prefix(Icon::new(IconName::ScrollText).small()),
                    )
                    .child(
                        Tab::new()
                            .label("Waveforms")
                            .prefix(Icon::new(IconName::AudioWaveform).small()),
                    )
                    .suffix(div().pr_2().text_xs().text_color(muted).child(format!(
                        "Time: {} ns",
                        self.simulation.as_ref().map_or(0, |sim| sim.time())
                    ))),
            )
            .child(content)
            .into_any_element()
    }

    fn status_bar(&self) -> AnyElement {
        let file = self
            .file
            .as_ref()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.editor.circuit().title.clone());
        let dirty = if self.editor.is_dirty() { " *" } else { "" };
        let tool = match &self.editor.tool {
            Tool::Select => "Select",
            Tool::Wire => "Wire",
            Tool::Pan => "Scroll",
            Tool::Delete => "Cut",
            Tool::Place(kind) => kind.name(),
        };
        StatusBar::new()
            .left(format!("File: {file}{dirty}"))
            .left(Separator::vertical().h(px(12.0)))
            .left(format!(
                "Module: {}",
                self.debug_path
                    .as_deref()
                    .unwrap_or(self.editor.active_module())
            ))
            .child(div().min_w_0().truncate().child(self.status.clone()))
            .right(format!(
                "{tool}  {:.0}%  ({:.0}, {:.0})",
                self.editor.viewport.zoom * 100.0,
                self.mouse.x,
                self.mouse.y
            ))
            .into_any_element()
    }

    pub(crate) fn canvas_menu_entries(&self) -> Vec<MenuEntry> {
        let menu = if !self.editor.selection.is_empty() {
            "CanvasComponent"
        } else if !self.editor.selected_wires.is_empty() {
            "CanvasWire"
        } else {
            "Canvas"
        };
        let inputs = (self.editor.selection.len() == 1)
            .then(|| {
                self.editor
                    .module()
                    .gate(*self.editor.selection.iter().next().unwrap())
            })
            .flatten()
            .map_or(0, |gate| {
                gate.pins
                    .iter()
                    .filter(|pin| pin.direction == rgate_core::Direction::Input)
                    .count()
            });
        let mut entries = commands::entries(menu);
        entries.retain(|entry| match &entry.command {
            Some(Command::SwapInputs) => inputs >= 2 && self.simulation.is_none(),
            Some(Command::Delete | Command::Rotate(_) | Command::Paste | Command::Tool(_)) => {
                self.simulation.is_none()
            }
            _ => true,
        });
        if inputs > 2 {
            for entry in &mut entries {
                if matches!(entry.command, Some(Command::SwapInputs)) {
                    entry.label = "Swap inputs…";
                }
            }
        }
        // Filtering unavailable actions must not leave dangling separators.
        let mut previous_separator = true;
        entries.retain(|entry| {
            let separator = entry.command.is_none();
            let keep = !separator || !previous_separator;
            previous_separator = separator;
            keep
        });
        if entries.last().is_some_and(|entry| entry.command.is_none()) {
            entries.pop();
        }
        entries
    }

    pub(crate) fn open_canvas_menu(
        &mut self,
        position: gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entries = self.canvas_menu_entries();
        if entries.is_empty() {
            return;
        }
        let app = cx.weak_entity();
        let menu = PopupMenu::build(window, cx, move |menu, _, _| {
            menu_items(menu.min_w(px(220.0)), entries, app, None)
        });
        menu.focus_handle(cx).focus(window, cx);
        let dismiss = cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, window, cx| {
            this.context_menu = None;
            this.focus.focus(window, cx);
            cx.notify();
        });
        self.context_menu = Some(ContextMenu {
            position,
            menu,
            _dismiss: dismiss,
        });
    }

    fn context_menu_layer(&self) -> Option<AnyElement> {
        let menu = self.context_menu.as_ref()?;
        Some(
            deferred(
                anchored()
                    .position(menu.position)
                    .snap_to_window_with_margin(px(8.0))
                    .child(menu.menu.clone()),
            )
            .with_priority(gpui_base::POPUP_PRIORITY)
            .into_any_element(),
        )
    }

    fn panes(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        self.layout.keep_side_panes(
            [
                &[(0, self.sidebar_width), (2, self.components_width)],
                &[(0, self.modules_height)],
                &[(1, self.bottom_height)],
            ],
            window,
            cx,
        );
        let content = if self.tab == WorkspaceTab::Interface {
            self.interface(cx)
        } else {
            self.schematic(window, cx)
        };
        let theme = cx.theme();
        let (sidebar_bg, background) = (theme.sidebar, theme.background);
        let main_app = cx.weak_entity();
        let center_app = cx.weak_entity();
        let center = v_resizable("center-split")
            .with_state(&self.layout.center)
            .on_resize(move |state, _, cx| {
                let height = state.read(cx).sizes().get(1).copied();
                if let Some(height) = height
                    && let Err(error) =
                        center_app.update(cx, |this, _| this.bottom_height = f32::from(height))
                {
                    eprintln!("panel resize ignored: {error}");
                }
            })
            .child(
                resizable_panel().size_range(px(160.0)..Pixels::MAX).child(
                    v_flex()
                        .size_full()
                        .bg(background)
                        .child(self.workspace_tabs(cx))
                        .child(content),
                ),
            )
            .child(
                resizable_panel()
                    .size(px(self.bottom_height))
                    .size_range(px(80.0)..px(380.0))
                    .flex_none()
                    .child(self.bottom_panel(window, cx)),
            );
        h_resizable("main-split")
            .with_state(&self.layout.main)
            .on_resize(move |state, _, cx| {
                let sizes = state.read(cx).sizes().clone();
                if let [sidebar, _, components] = sizes[..]
                    && let Err(error) = main_app.update(cx, |this, _| {
                        this.sidebar_width = f32::from(sidebar);
                        this.components_width = f32::from(components);
                    })
                {
                    eprintln!("panel resize ignored: {error}");
                }
            })
            .child(
                resizable_panel()
                    .size(px(self.sidebar_width))
                    .size_range(px(150.0)..px(400.0))
                    .flex_none()
                    .bg(sidebar_bg)
                    .child(self.sidebar(cx)),
            )
            .child(
                resizable_panel()
                    .size_range(px(200.0)..Pixels::MAX)
                    .child(center),
            )
            .child(
                resizable_panel()
                    .size(px(self.components_width))
                    .size_range(px(150.0)..px(600.0))
                    .flex_none()
                    .bg(sidebar_bg)
                    .child(self.component_sidebar(cx)),
            )
            .into_any_element()
    }
}

impl Render for GateApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use commands::*;
        #[cfg(target_family = "wasm")]
        crate::browser::set_dirty(self.editor.is_dirty());
        window.set_window_title(&format!(
            "{}{} — RGate",
            self.editor.circuit().title,
            if self.editor.is_dirty() { " *" } else { "" }
        ));
        self.sync_dialog(window, cx);
        let theme = cx.theme();
        let (background, foreground) = (theme.background, theme.foreground);
        div()
            .id("rgate")
            .key_context(if self.dialog.is_some() {
                "RGateDialog"
            } else {
                "RGate"
            })
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .relative()
            .bg(background)
            .text_color(foreground)
            .text_sm()
            .on_action(
                cx.listener(|this, _: &Find, window, cx| this.command(Command::Find, window, cx)),
            )
            .on_key_down(cx.listener(Self::key_down))
            .on_mouse_move(
                cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                    if this.drag.is_some() && !this.canvas_bounds.contains(&event.position) {
                        this.mouse_move(event, window, cx);
                    }
                }),
            )
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::mouse_up))
            .on_action(
                cx.listener(|this, _: &New, window, cx| this.command(Command::New, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &Open, window, cx| this.command(Command::Open, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &Save, window, cx| this.command(Command::Save, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &SaveAs, window, cx| {
                    this.command(Command::SaveAs, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &Export, window, cx| {
                    this.command(Command::Export, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &Undo, window, cx| this.command(Command::Undo, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &Redo, window, cx| this.command(Command::Redo, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &Cut, window, cx| this.command(Command::Cut, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &Copy, window, cx| this.command(Command::Copy, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &Paste, window, cx| this.command(Command::Paste, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &Delete, window, cx| {
                    this.command(Command::Delete, window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &SelectAll, window, cx| {
                this.command(Command::SelectAll, window, cx)
            }))
            .on_action(cx.listener(|this, _: &Rotate, window, cx| {
                this.command(Command::Rotate(true), window, cx)
            }))
            .on_action(cx.listener(|this, _: &RotateBack, window, cx| {
                this.command(Command::Rotate(false), window, cx)
            }))
            .on_action(cx.listener(|this, _: &PlayPause, window, cx| {
                this.command(Command::PlayPause, window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &Stop, window, cx| this.command(Command::Stop, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &Step, window, cx| this.command(Command::Step, window, cx)),
            )
            .on_action(cx.listener(|this, _: &ClockStep, window, cx| {
                this.command(Command::ClockStep, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ZoomIn, window, cx| {
                this.command(Command::Zoom(1.25), window, cx)
            }))
            .on_action(cx.listener(|this, _: &ZoomOut, window, cx| {
                this.command(Command::Zoom(0.8), window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &Fit, window, cx| this.command(Command::Fit, window, cx)),
            )
            .on_action(cx.listener(|this, _: &ClassicTheme, window, cx| {
                this.command(Command::SetTheme(Theme::Classic), window, cx)
            }))
            .on_action(cx.listener(|this, _: &ModernTheme, window, cx| {
                this.command(Command::SetTheme(Theme::Modern), window, cx)
            }))
            .on_action(cx.listener(|this, _: &LightAppearance, window, cx| {
                this.command(Command::SetAppearance(Appearance::Light), window, cx)
            }))
            .on_action(cx.listener(|this, _: &DarkAppearance, window, cx| {
                this.command(Command::SetAppearance(Appearance::Dark), window, cx)
            }))
            .on_action(cx.listener(|this, _: &ToggleGrid, window, cx| {
                this.command(Command::ToggleGrid, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ToggleSnap, window, cx| {
                this.command(Command::ToggleSnap, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ToggleScope, window, cx| {
                this.command(Command::ToggleScope, window, cx)
            }))
            .on_action(cx.listener(|this, _: &Properties, window, cx| {
                this.command(Command::Properties, window, cx)
            }))
            .on_action(cx.listener(|this, _: &NewModule, window, cx| {
                this.command(Command::NewModule, window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &About, window, cx| this.command(Command::About, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &Help, window, cx| this.command(Command::Help, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &Quit, window, cx| this.command(Command::Quit, window, cx)),
            )
            .child(self.menu_bar(cx))
            .child(self.toolbar(cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .child(self.panes(window, cx)),
            )
            .child(self.status_bar())
            .children(self.context_menu_layer())
    }
}
