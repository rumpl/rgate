//! UI preferences and recovery are separate from circuit documents and simulation state.
use crate::{
    app::{Dialog, GateApp, WorkspaceTab},
    theme::Theme,
};
use anyhow::{Context, Result, bail};
use gpui::{Context as UiContext, Window};
use rgate_core::{Circuit, Point};
use rgate_editor::{Editor, Viewport};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

const VERSION: u32 = 1;
const MAX_STATE_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ViewState {
    pub zoom: f32,
    pub pan: Point,
}
impl Default for ViewState {
    fn default() -> Self {
        let view = Viewport::default();
        Self {
            zoom: view.zoom,
            pan: view.pan,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd)]
pub struct SavedProbe {
    pub root: String,
    pub path: String,
    pub net: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct DocumentWorkspace {
    pub active_module: String,
    pub debug_path: Option<String>,
    pub views: BTreeMap<String, ViewState>,
    pub collapsed: BTreeSet<String>,
    pub probes: BTreeSet<SavedProbe>,
    pub waveform: crate::waveform::WaveformSettings,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Preferences {
    pub version: u32,
    pub theme: Theme,
    pub sidebar_width: f32,
    pub components_width: f32,
    pub modules_height: f32,
    pub bottom_height: f32,
    pub show_grid: bool,
    pub snap: bool,
    pub grid: f32,
    pub bottom_scope: bool,
    pub module_list: bool,
    pub nets_ports: bool,
    pub documents: BTreeMap<String, DocumentWorkspace>,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            version: VERSION,
            theme: Theme::default(),
            sidebar_width: 210.0,
            components_width: 205.0,
            modules_height: 196.0,
            bottom_height: 176.0,
            show_grid: false,
            snap: true,
            grid: 5.0,
            bottom_scope: false,
            module_list: false,
            nets_ports: false,
            documents: BTreeMap::new(),
        }
    }
}
impl Preferences {
    fn validate(&mut self) -> Result<()> {
        if self.version != VERSION {
            bail!("unsupported workspace settings version {}", self.version);
        }
        self.sidebar_width = finite_clamp(self.sidebar_width, 210.0, 150.0, 400.0);
        self.components_width = finite_clamp(self.components_width, 205.0, 150.0, 600.0);
        self.modules_height = finite_clamp(self.modules_height, 196.0, 80.0, 4000.0);
        self.bottom_height = finite_clamp(self.bottom_height, 176.0, 80.0, 380.0);
        self.grid = finite_clamp(self.grid, 5.0, 1.0, 100.0);
        for document in self.documents.values_mut() {
            document
                .views
                .retain(|_, view| view.zoom.is_finite() && view.pan.is_finite());
            for view in document.views.values_mut() {
                view.zoom = view.zoom.clamp(0.25, 6.0);
            }
        }
        Ok(())
    }
}
fn finite_clamp(value: f32, fallback: f32, min: f32, max: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Recovery {
    version: u32,
    pub circuit: Circuit,
    pub file: Option<PathBuf>,
    pub workspace: DocumentWorkspace,
}
impl Recovery {
    fn validate(&self) -> Result<()> {
        if self.version != VERSION {
            bail!("unsupported recovery version");
        }
        self.circuit.validate().context("invalid recovery circuit")
    }
}

pub struct Persistence {
    pub preferences: Preferences,
    pub recovery: Option<Recovery>,
    last_settings: Option<String>,
    last_recovery: Option<String>,
    pub error: Option<String>,
    ticks: u8,
    #[cfg(not(target_family = "wasm"))]
    directory: PathBuf,
}
impl Persistence {
    pub fn load() -> Self {
        #[cfg(not(target_family = "wasm"))]
        let directory = state_directory();
        let mut store = Self {
            preferences: Preferences::default(),
            recovery: None,
            last_settings: None,
            last_recovery: None,
            error: None,
            ticks: 0,
            #[cfg(not(target_family = "wasm"))]
            directory,
        };
        match store
            .read("settings.json")
            .and_then(|source| source.map(|source| decode_preferences(&source)).transpose())
        {
            Ok(Some(preferences)) => store.preferences = preferences,
            Ok(None) => {}
            Err(error) => store.error = Some(format!("Workspace settings ignored: {error:#}")),
        }
        match store
            .read("recovery.json")
            .and_then(|source| source.map(|source| decode_recovery(&source)).transpose())
        {
            Ok(recovery) => store.recovery = recovery,
            Err(error) => store.error = Some(format!("Recovery could not load: {error:#}")),
        }
        store
    }
    fn read(&self, name: &str) -> Result<Option<String>> {
        #[cfg(not(target_family = "wasm"))]
        {
            let path = self.directory.join(name);
            match std::fs::read(&path) {
                Ok(bytes) => {
                    if bytes.len() > MAX_STATE_BYTES {
                        bail!("state file exceeds 32 MB");
                    }
                    Ok(Some(String::from_utf8(bytes)?))
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(error) => Err(error).with_context(|| format!("reading {}", path.display())),
            }
        }
        #[cfg(target_family = "wasm")]
        {
            crate::browser::storage_read(name).map_err(crate::browser::js_error)
        }
    }
    fn write(&self, name: &str, content: &str) -> Result<()> {
        if content.len() > MAX_STATE_BYTES {
            bail!("workspace/recovery exceeds 32 MB; save the circuit manually");
        }
        #[cfg(not(target_family = "wasm"))]
        {
            std::fs::create_dir_all(&self.directory)?;
            rgate_format::native::save_atomic(&self.directory.join(name), content)
        }
        #[cfg(target_family = "wasm")]
        {
            crate::browser::storage_write(name, content).map_err(crate::browser::js_error)
        }
    }
    pub fn clear_recovery(&mut self) -> Result<()> {
        #[cfg(not(target_family = "wasm"))]
        {
            match std::fs::remove_file(self.directory.join("recovery.json")) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        #[cfg(target_family = "wasm")]
        crate::browser::storage_remove("recovery.json").map_err(crate::browser::js_error)?;
        self.recovery = None;
        self.last_recovery = None;
        Ok(())
    }
    fn save(&mut self, preferences: Preferences, recovery: Option<Recovery>) -> Result<()> {
        let source = serde_json::to_string(&preferences)?;
        if self.last_settings.as_ref() != Some(&source) {
            self.write("settings.json", &source)?;
            self.last_settings = Some(source);
        }
        self.preferences = preferences;
        if let Some(recovery) = recovery {
            let source = serde_json::to_string(&recovery)?;
            if self.last_recovery.as_ref() != Some(&source) {
                self.write("recovery.json", &source)?;
                self.last_recovery = Some(source);
            }
            self.recovery = Some(recovery);
        } else {
            self.clear_recovery()?;
        }
        Ok(())
    }
}
fn decode_preferences(source: &str) -> Result<Preferences> {
    if source.len() > MAX_STATE_BYTES {
        bail!("settings too large");
    }
    let mut preferences: Preferences = serde_json::from_str(source)?;
    preferences.validate()?;
    Ok(preferences)
}
fn decode_recovery(source: &str) -> Result<Recovery> {
    if source.len() > MAX_STATE_BYTES {
        bail!("recovery too large");
    }
    let recovery: Recovery = serde_json::from_str(source)?;
    recovery.validate()?;
    Ok(recovery)
}

#[cfg(not(target_family = "wasm"))]
fn state_directory() -> PathBuf {
    if let Some(path) = std::env::var_os("RGATE_STATE_DIR") {
        return path.into();
    }
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_else(|| ".".into()));
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/RGate")
    } else if cfg!(target_os = "windows") {
        PathBuf::from(std::env::var_os("APPDATA").unwrap_or_else(|| home.into_os_string()))
            .join("RGate")
    } else {
        PathBuf::from(
            std::env::var_os("XDG_STATE_HOME")
                .unwrap_or_else(|| home.join(".local/state").into_os_string()),
        )
        .join("rgate")
    }
}

impl GateApp {
    pub fn workspace_key_public(&self) -> String {
        self.workspace_key()
    }
    fn workspace_key(&self) -> String {
        if let Some(path) = &self.file {
            format!("file:{}", path.to_string_lossy())
        } else {
            format!(
                "untitled:{}:{}",
                self.editor.circuit().root,
                self.editor.circuit().title
            )
        }
    }
    fn view_key(&self) -> String {
        self.debug_path
            .as_ref()
            .map(|path| format!("instance:{path}"))
            .unwrap_or_else(|| format!("module:{}", self.editor.active_module()))
    }
    pub fn remember_view(&mut self) {
        let key = self.view_key();
        self.workspace_views.insert(
            key,
            ViewState {
                zoom: self.editor.viewport.zoom,
                pan: self.editor.viewport.pan,
            },
        );
    }
    pub fn restore_view(&mut self) {
        if let Some(view) = self.workspace_views.get(&self.view_key()) {
            self.editor.viewport = Viewport {
                zoom: view.zoom,
                pan: view.pan,
            };
            self.need_fit = false;
        } else {
            self.need_fit = true;
        }
    }
    pub fn capture_workspace(&mut self) -> DocumentWorkspace {
        self.remember_view();
        DocumentWorkspace {
            active_module: self.editor.active_module().into(),
            debug_path: self.debug_path.clone(),
            views: self.workspace_views.clone(),
            collapsed: self.collapsed_modules.iter().cloned().collect(),
            probes: self.saved_probes.clone(),
            waveform: self.waveform.clone(),
        }
    }
    pub fn apply_workspace(&mut self, workspace: DocumentWorkspace) {
        self.workspace_views = workspace
            .views
            .into_iter()
            .filter(|(_, view)| view.zoom.is_finite() && view.pan.is_finite())
            .collect();
        self.collapsed_modules = workspace.collapsed.into_iter().collect();
        self.saved_probes = workspace.probes;
        self.waveform = workspace.waveform;
        self.waveform.sanitize();
        if self
            .editor
            .circuit()
            .module(&workspace.active_module)
            .is_some()
        {
            let _ = self.editor.switch_module(&workspace.active_module);
        }
        // Restore the view only; never auto-run or pretend volatile CPU state was saved.
        self.debug_path = None;
        if let Some(path) = workspace.debug_path
            && let Some(view) = self
                .workspace_views
                .get(&format!("instance:{path}"))
                .cloned()
        {
            self.workspace_views
                .insert(format!("module:{}", self.editor.active_module()), view);
        }
        self.restore_view();
    }
    pub fn enable_persistence(&mut self, cx: &mut UiContext<Self>) {
        let store = Persistence::load();
        let prefs = &store.preferences;
        self.theme = prefs.theme;
        cx.set_global(prefs.theme);
        self.sidebar_width = prefs.sidebar_width;
        self.components_width = prefs.components_width;
        self.modules_height = prefs.modules_height;
        self.bottom_height = prefs.bottom_height;
        self.editor.show_grid = prefs.show_grid;
        self.editor.snap = prefs.snap;
        self.editor.grid = prefs.grid;
        self.bottom_scope = prefs.bottom_scope;
        self.module_list = prefs.module_list;
        self.nets_ports = prefs.nets_ports;
        if let Some(workspace) = prefs.documents.get(&self.workspace_key()) {
            self.apply_workspace(workspace.clone());
        }
        if let Some(error) = &store.error {
            self.log(error.clone());
        }
        if store.recovery.is_some() {
            self.dialog = Some(Dialog::Recovery);
        }
        self.persistence = Some(store);
        cx.notify();
    }
    pub fn persistence_tick(&mut self) {
        if let Some(store) = &mut self.persistence {
            store.ticks = store.ticks.wrapping_add(1);
            if store.ticks < 50 {
                return;
            }
            store.ticks = 0;
        } else {
            return;
        }
        if self.drag.is_none()
            && self.editor.draft.is_none()
            && !self.prompting
            && !matches!(self.dialog, Some(Dialog::Recovery))
        {
            self.flush_workspace();
        }
    }
    pub fn flush_workspace(&mut self) {
        if matches!(self.dialog, Some(Dialog::Recovery)) {
            return;
        }
        let Some(mut store) = self.persistence.take() else {
            return;
        };
        let workspace = self.capture_workspace();
        let mut preferences = store.preferences.clone();
        preferences.theme = self.theme;
        preferences.sidebar_width = self.sidebar_width;
        preferences.components_width = self.components_width;
        preferences.modules_height = self.modules_height;
        preferences.bottom_height = self.bottom_height;
        preferences.show_grid = self.editor.show_grid;
        preferences.snap = self.editor.snap;
        preferences.grid = self.editor.grid;
        preferences.bottom_scope = self.bottom_scope;
        preferences.module_list = self.module_list;
        preferences.nets_ports = self.nets_ports;
        preferences
            .documents
            .insert(self.workspace_key(), workspace.clone());
        while preferences.documents.len() > 32 {
            let key = preferences.documents.keys().next().unwrap().clone();
            preferences.documents.remove(&key);
        }
        let recovery = self.editor.is_dirty().then(|| Recovery {
            version: VERSION,
            circuit: self.editor.circuit().clone(),
            file: self.file.clone(),
            workspace,
        });
        if let Err(error) = store.save(preferences, recovery) {
            let message = format!("Workspace/recovery save failed: {error:#}");
            if store.error.as_ref() != Some(&message) {
                self.log(message.clone());
            }
            store.error = Some(message);
        } else {
            store.error = None;
        }
        self.persistence = Some(store);
    }
    pub fn restore_recovery(&mut self, window: &mut Window, cx: &mut UiContext<Self>) {
        let Some(recovery) = self
            .persistence
            .as_ref()
            .and_then(|store| store.recovery.clone())
        else {
            return;
        };
        match Editor::new(recovery.circuit.clone()) {
            Ok(mut editor) => {
                // Recovery restores an unsaved document, not a silently saved file.
                editor.mark_recovered();
                self.editor = editor;
                self.file = recovery.file;
                self.simulation = None;
                self.running = false;
                self.probes.clear();
                self.debug_path = None;
                self.tab = WorkspaceTab::Edit;
                self.apply_workspace(recovery.workspace);
                self.dialog = None;
                self.log("Recovered unsaved circuit. Save it to keep these changes.".into());
                self.focus.focus(window, cx);
                cx.notify();
            }
            Err(error) => self.log(format!("Recovery failed: {error}")),
        }
    }
    pub fn discard_recovery(&mut self, window: &mut Window, cx: &mut UiContext<Self>) {
        if let Some(store) = &mut self.persistence
            && let Err(error) = store.clear_recovery()
        {
            self.log(format!("Could not discard recovery: {error:#}"));
        }
        self.dialog = None;
        self.focus.focus(window, cx);
        cx.notify();
    }
    pub fn restore_saved_probes(&mut self) {
        let Some(sim) = &mut self.simulation else {
            return;
        };
        self.saved_probes.retain(|probe| {
            if probe.root != sim.root_path() {
                return true;
            }
            let Some(name) = sim.scope_module(&probe.path) else {
                return false;
            };
            let Some(module) = self.editor.circuit().module(name) else {
                return false;
            };
            let Some(net) = module.nets.iter().find(|net| net.name == probe.net) else {
                return false;
            };
            let Some(actual) = sim.scoped_net(&probe.path, net.id) else {
                return false;
            };
            if probe.root != sim.root_path() {
                return true;
            }
            if sim.probe(actual).is_err() {
                return false;
            }
            sim.label_probe(actual, format!("{}/{}", probe.path, probe.net));
            self.probes.insert(actual);
            true
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::Command;
    use gpui::TestAppContext;
    use rgate_core::demo;

    fn directory() -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "rgate-workspace-test-{}-{nonce}",
            std::process::id()
        ))
    }
    fn store(directory: PathBuf) -> Persistence {
        Persistence {
            preferences: Preferences::default(),
            recovery: None,
            last_settings: None,
            last_recovery: None,
            error: None,
            ticks: 0,
            directory,
        }
    }
    #[test]
    fn settings_sanitize_panel_sizes_and_reject_future_versions_and_corruption() {
        let mut prefs = Preferences {
            sidebar_width: -100.0,
            components_width: 5000.0,
            ..Default::default()
        };
        prefs.validate().unwrap();
        assert_eq!(prefs.sidebar_width, 150.0);
        assert_eq!(prefs.components_width, 600.0);
        assert!(decode_preferences("bad JSON").is_err());
        let future = Preferences {
            version: 99,
            ..Default::default()
        };
        assert!(decode_preferences(&serde_json::to_string(&future).unwrap()).is_err());
        assert!(decode_recovery("{\"version\":1}").is_err());
    }
    #[test]
    fn recovery_roundtrip_and_atomic_storage_do_not_touch_original_document() {
        let directory = directory();
        let mut store = store(directory.clone());
        let circuit = demo::full_adder();
        let recovery = Recovery {
            version: VERSION,
            circuit: circuit.clone(),
            file: Some(PathBuf::from("original.rgate")),
            workspace: DocumentWorkspace::default(),
        };
        store.save(Preferences::default(), Some(recovery)).unwrap();
        let loaded =
            decode_recovery(&std::fs::read_to_string(directory.join("recovery.json")).unwrap())
                .unwrap();
        assert_eq!(loaded.circuit, circuit);
        assert_eq!(loaded.file, Some(PathBuf::from("original.rgate")));
        assert!(directory.join("settings.json").exists());
        assert!(!directory.join("original.rgate").exists());
        store.clear_recovery().unwrap();
        assert!(!directory.join("recovery.json").exists());
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[gpui::test]
    fn views_and_named_probes_roundtrip_and_stale_probes_are_ignored(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| {
            GateApp::new(demo::hierarchical_inverters(), None, window, cx)
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            view.update(cx, |app, cx| {
                app.editor.viewport = Viewport {
                    zoom: 2.0,
                    pan: Point::new(120.0, 70.0),
                };
                app.remember_view();
                app.switch_module("inverter".into(), cx);
                app.editor.viewport = Viewport {
                    zoom: 3.0,
                    pan: Point::new(35.0, 45.0),
                };
                app.remember_view();
                app.switch_module("main".into(), cx);
                assert_eq!(app.editor.viewport.zoom, 2.0);
                assert_eq!(app.editor.viewport.pan, Point::new(120.0, 70.0));
                app.command(Command::ClockStep, window, cx);
                app.navigate_scope("main/u0/inner".into(), "inverter".into(), cx);
                app.toggle_probe(rgate_core::NetId(2), cx);
                let saved = app.capture_workspace();
                app.simulation = None;
                app.probes.clear();
                app.debug_path = None;
                app.apply_workspace(saved);
                app.switch_module("main".into(), cx);
                app.saved_probes.insert(SavedProbe {
                    root: "main".into(),
                    path: "main/missing".into(),
                    net: "bad".into(),
                });
                app.command(Command::ClockStep, window, cx);
                assert_eq!(app.simulation.as_ref().unwrap().traces().len(), 1);
                assert_eq!(app.saved_probes.len(), 1);
                assert_eq!(
                    app.simulation
                        .as_ref()
                        .unwrap()
                        .traces()
                        .values()
                        .next()
                        .unwrap()
                        .name,
                    "main/u0/inner/Y"
                );
            })
        });
    }
    #[gpui::test]
    fn recovered_document_stays_dirty_until_saved_and_persistence_handles_restart(
        cx: &mut TestAppContext,
    ) {
        let directory = directory();
        let (view, cx) =
            cx.add_window_view(|window, cx| GateApp::new(demo::full_adder(), None, window, cx));
        cx.run_until_parked();
        cx.update(|window, cx| {
            view.update(cx, |app, cx| {
                app.persistence = Some(store(directory.clone()));
                app.editor
                    .place(rgate_core::GateKind::Not, Point::ZERO)
                    .unwrap();
                app.components_width = 300.0;
                app.theme = Theme::Dark;
                app.flush_workspace();
                let recovery = decode_recovery(
                    &std::fs::read_to_string(directory.join("recovery.json")).unwrap(),
                )
                .unwrap();
                let prefs = decode_preferences(
                    &std::fs::read_to_string(directory.join("settings.json")).unwrap(),
                )
                .unwrap();
                assert_eq!(prefs.theme, Theme::Dark);
                assert_eq!(prefs.components_width, 300.0);
                app.editor = Editor::new(demo::full_adder()).unwrap();
                app.persistence.as_mut().unwrap().recovery = Some(recovery);
                app.dialog = Some(Dialog::Recovery);
                app.restore_recovery(window, cx);
                assert!(app.editor.is_dirty());
                assert_eq!(app.editor.module().gates.len(), 13);
                assert!(!app.running);
                assert!(app.simulation.is_none());
                app.editor.mark_saved();
                app.flush_workspace();
                assert!(!directory.join("recovery.json").exists());
                app.persistence = None;
            })
        });
        std::fs::remove_dir_all(directory).unwrap();
    }
}
