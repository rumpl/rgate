//! UI preferences and recovery are separate from circuit documents and simulation state.
use crate::{
    app::{Dialog, GateApp, WorkspaceTab},
    theme::{Appearance, Theme},
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
    pub appearance: Appearance,
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
            appearance: Appearance::default(),
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

/// Owns workspace persistence and per-module / per-instance viewport memory.
#[derive(Default)]
pub struct WorkspaceStore {
    pub persistence: Option<Persistence>,
    pub views: BTreeMap<String, ViewState>,
}

impl WorkspaceStore {
    pub fn document_key(file: Option<&PathBuf>, circuit: &Circuit) -> String {
        if let Some(path) = file {
            format!("file:{}", path.to_string_lossy())
        } else {
            format!("untitled:{}:{}", circuit.root, circuit.title)
        }
    }

    pub fn view_key(debug_path: Option<&str>, active_module: &str) -> String {
        debug_path
            .map(|path| format!("instance:{path}"))
            .unwrap_or_else(|| format!("module:{active_module}"))
    }

    pub fn remember_view(
        &mut self,
        debug_path: Option<&str>,
        active_module: &str,
        viewport: &Viewport,
    ) {
        let key = Self::view_key(debug_path, active_module);
        self.views.insert(
            key,
            ViewState {
                zoom: viewport.zoom,
                pan: viewport.pan,
            },
        );
    }

    /// Restores the saved viewport for the current scope. Returns `true` if restored,
    /// `false` if the caller should auto-fit.
    pub fn restore_view(
        &self,
        debug_path: Option<&str>,
        active_module: &str,
        viewport: &mut Viewport,
    ) -> bool {
        if let Some(view) = self.views.get(&Self::view_key(debug_path, active_module)) {
            *viewport = Viewport {
                zoom: view.zoom,
                pan: view.pan,
            };
            true
        } else {
            false
        }
    }

    pub fn saved_document(&self, key: &str) -> Option<DocumentWorkspace> {
        self.persistence
            .as_ref()
            .and_then(|store| store.preferences.documents.get(key).cloned())
    }

    pub fn tick_ready(&mut self) -> bool {
        let Some(store) = &mut self.persistence else {
            return false;
        };
        store.ticks = store.ticks.wrapping_add(1);
        if store.ticks < 50 {
            return false;
        }
        store.ticks = 0;
        true
    }
}

impl GateApp {
    pub fn workspace_key_public(&self) -> String {
        WorkspaceStore::document_key(self.file.as_ref(), self.editor.circuit())
    }

    pub fn remember_view(&mut self) {
        self.workspace.remember_view(
            self.sim.debug_path.as_deref(),
            self.editor.active_module(),
            &self.editor.viewport,
        );
    }

    pub fn restore_view(&mut self) {
        let active_module = self.editor.active_module().to_owned();
        let restored = self.workspace.restore_view(
            self.sim.debug_path.as_deref(),
            &active_module,
            &mut self.editor.viewport,
        );
        self.canvas.need_fit = !restored;
    }

    pub fn capture_workspace(&mut self) -> DocumentWorkspace {
        self.remember_view();
        DocumentWorkspace {
            active_module: self.editor.active_module().into(),
            debug_path: self.sim.debug_path.clone(),
            views: self.workspace.views.clone(),
            collapsed: self.chrome.collapsed_modules.iter().cloned().collect(),
            probes: self.sim.saved_probes.clone(),
            waveform: self.waveform.settings.clone(),
        }
    }

    pub fn apply_workspace(&mut self, workspace: DocumentWorkspace) {
        self.workspace.views = workspace
            .views
            .into_iter()
            .filter(|(_, view)| view.zoom.is_finite() && view.pan.is_finite())
            .collect();
        self.chrome.collapsed_modules = workspace.collapsed.into_iter().collect();
        self.sim.saved_probes = workspace.probes;
        self.waveform.settings = workspace.waveform;
        self.waveform.settings.sanitize();
        if self
            .editor
            .circuit()
            .module(&workspace.active_module)
            .is_some()
        {
            let _ = self.editor.switch_module(&workspace.active_module);
        }
        // Restore the view only; never auto-run or pretend volatile CPU state was saved.
        self.sim.debug_path = None;
        if let Some(path) = workspace.debug_path
            && let Some(view) = self
                .workspace
                .views
                .get(&format!("instance:{path}"))
                .cloned()
        {
            self.workspace
                .views
                .insert(format!("module:{}", self.editor.active_module()), view);
        }
        self.restore_view();
    }

    pub fn enable_persistence(&mut self, cx: &mut UiContext<Self>) {
        let store = Persistence::load();
        let prefs = &store.preferences;
        self.theme = prefs.theme;
        self.appearance = prefs.appearance;
        crate::view::apply_appearance(prefs.appearance, None, cx);
        self.chrome.apply_preferences(prefs);
        self.editor.show_grid = prefs.show_grid;
        self.editor.snap = prefs.snap;
        self.editor.grid = prefs.grid;
        if let Some(workspace) = prefs.documents.get(&self.workspace_key_public()) {
            self.apply_workspace(workspace.clone());
        }
        if let Some(error) = &store.error {
            self.log(error.clone());
        }
        if store.recovery.is_some() {
            self.dialogs.show(Dialog::Recovery);
        }
        self.workspace.persistence = Some(store);
        cx.notify();
    }

    pub fn persistence_tick(&mut self) {
        if !self.workspace.tick_ready() {
            return;
        }
        if self.canvas.drag.is_none()
            && self.editor.draft.is_none()
            && !self.prompting
            && !self.dialogs.is_recovery()
        {
            self.flush_workspace();
        }
    }

    pub fn flush_workspace(&mut self) {
        if self.dialogs.is_recovery() {
            return;
        }
        let Some(mut store) = self.workspace.persistence.take() else {
            return;
        };
        let workspace = self.capture_workspace();
        let mut preferences = store.preferences.clone();
        preferences.theme = self.theme;
        preferences.appearance = self.appearance;
        self.chrome.write_preferences(&mut preferences);
        preferences.show_grid = self.editor.show_grid;
        preferences.snap = self.editor.snap;
        preferences.grid = self.editor.grid;
        preferences
            .documents
            .insert(self.workspace_key_public(), workspace.clone());
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
        self.workspace.persistence = Some(store);
    }

    pub fn restore_recovery(&mut self, window: &mut Window, cx: &mut UiContext<Self>) {
        let Some(recovery) = self
            .workspace
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
                self.sim.reset();
                self.chrome.tab = WorkspaceTab::Edit;
                self.apply_workspace(recovery.workspace);
                self.dialogs.close();
                self.log("Recovered unsaved circuit. Save it to keep these changes.".into());
                self.focus.focus(window, cx);
                cx.notify();
            }
            Err(error) => self.log(format!("Recovery failed: {error}")),
        }
    }

    pub fn discard_recovery(&mut self, window: &mut Window, cx: &mut UiContext<Self>) {
        if let Some(store) = &mut self.workspace.persistence
            && let Err(error) = store.clear_recovery()
        {
            self.log(format!("Could not discard recovery: {error:#}"));
        }
        self.dialogs.close();
        self.focus.focus(window, cx);
        cx.notify();
    }
}
