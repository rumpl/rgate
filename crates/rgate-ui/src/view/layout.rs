use crate::workspace::Preferences;
use gpui::{App, AppContext, Context, Entity, Pixels, Subscription, Window, px};
use gpui_component::{
    input::{InputEvent, InputState},
    menu::PopupMenu,
    resizable::ResizableState,
};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WorkspaceTab {
    Edit,
    Interface,
    Simulate,
}

pub struct ContextMenu {
    pub position: gpui::Point<Pixels>,
    pub menu: Entity<PopupMenu>,
    pub _dismiss: Subscription,
}

pub struct OpenMenu {
    pub name: &'static str,
    pub menu: Entity<PopupMenu>,
    pub _dismiss: Subscription,
}

/// Persistent split state for the main window panes.
pub struct PanelLayout {
    pub(super) main: Entity<ResizableState>,
    pub(super) sidebar: Entity<ResizableState>,
    pub(super) center: Entity<ResizableState>,
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
    pub(super) fn keep_side_panes(
        &mut self,
        sizes: [&[(usize, f32)]; 3],
        window: &mut Window,
        cx: &mut App,
    ) {
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

/// Owns window chrome layout, panel dimensions, sidebar modes, menus, and active tabs.
pub struct ChromeLayout {
    pub tab: WorkspaceTab,
    pub bottom_scope: bool,
    pub nets_ports: bool,
    pub module_list: bool,
    pub collapsed_modules: HashSet<String>,
    pub sidebar_width: f32,
    pub components_width: f32,
    pub modules_height: f32,
    pub bottom_height: f32,
    pub panels: PanelLayout,
    pub component_search: Entity<InputState>,
    pub context_menu: Option<ContextMenu>,
    pub open_menu: Option<OpenMenu>,
}

impl ChromeLayout {
    pub fn new<T: 'static>(window: &mut Window, cx: &mut Context<T>) -> Self {
        let component_search = crate::input::search_field("Search components…", window, cx);
        cx.subscribe(&component_search, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        })
        .detach();
        Self {
            tab: WorkspaceTab::Edit,
            bottom_scope: false,
            nets_ports: false,
            module_list: false,
            collapsed_modules: HashSet::new(),
            sidebar_width: 210.0,
            components_width: 205.0,
            modules_height: 196.0,
            bottom_height: 176.0,
            panels: PanelLayout::new(cx),
            component_search,
            context_menu: None,
            open_menu: None,
        }
    }

    pub fn apply_preferences(&mut self, prefs: &Preferences) {
        self.sidebar_width = prefs.sidebar_width;
        self.components_width = prefs.components_width;
        self.modules_height = prefs.modules_height;
        self.bottom_height = prefs.bottom_height;
        self.bottom_scope = prefs.bottom_scope;
        self.module_list = prefs.module_list;
        self.nets_ports = prefs.nets_ports;
    }

    pub fn write_preferences(&self, prefs: &mut Preferences) {
        prefs.sidebar_width = self.sidebar_width;
        prefs.components_width = self.components_width;
        prefs.modules_height = self.modules_height;
        prefs.bottom_height = self.bottom_height;
        prefs.bottom_scope = self.bottom_scope;
        prefs.module_list = self.module_list;
        prefs.nets_ports = self.nets_ports;
    }

    pub fn keep_side_panes(&mut self, window: &mut Window, cx: &mut App) {
        self.panels.keep_side_panes(
            [
                &[(0, self.sidebar_width), (2, self.components_width)],
                &[(0, self.modules_height)],
                &[(1, self.bottom_height)],
            ],
            window,
            cx,
        );
    }
}
