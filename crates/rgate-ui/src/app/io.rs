use super::{Dialog, GateApp};
use anyhow::Result;
#[cfg(not(target_family = "wasm"))]
use gpui::PathPromptOptions;
use gpui::{Context, Window};
use rgate_core::Circuit;
use rgate_editor::Editor;
use std::path::PathBuf;

impl GateApp {
    pub(super) fn replace_circuit(&mut self, circuit: Circuit, path: Option<PathBuf>) {
        self.canvas.input_swap = None;
        if path
            .as_ref()
            .is_some_and(|p| p.extension().is_some_and(|s| s == "v" || s == "sv"))
            && circuit.modules.iter().all(|m| m.verilog.is_some())
            && circuit.modules.len() > 1
        {
            let selected = circuit.root.clone();
            self.dialogs.show(Dialog::HdlTop {
                circuit,
                path,
                selected,
            });
            return;
        }
        self.load_document(circuit, path);
    }

    pub(crate) fn load_document(&mut self, circuit: Circuit, path: Option<PathBuf>) {
        self.flush_workspace();
        match Editor::new(circuit) {
            Ok(editor) => {
                self.editor = editor;
                self.file = path;
                self.changed();
                self.sim.clear_all();
                self.dialogs.close();
                self.canvas.clear_interaction();
                self.workspace.views.clear();
                if let Some(workspace) = self.workspace.saved_document(&self.workspace_key_public())
                {
                    self.apply_workspace(workspace);
                } else {
                    self.canvas.need_fit = true;
                }
                self.flush_workspace();
                self.log("Circuit loaded.".into());
            }
            Err(error) => self.error(error),
        }
    }

    fn apply_memory_image_text(
        &mut self,
        image: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        if let Some(Dialog::Properties(dialog)) = &mut self.dialogs.active {
            dialog.apply_memory_image(image, window, cx)?;
        }
        Ok(())
    }
}

#[cfg(target_family = "wasm")]
impl GateApp {
    pub(super) fn open(&mut self, window: &Window, cx: &mut Context<Self>) {
        let picker = crate::browser::picker(".rgate,.v,.sv");
        self.prompting = true;
        cx.spawn_in(window, async move |entity, cx| {
            let result = crate::browser::read_file(picker).await;
            if let Err(error) = entity.update_in(cx, |this, _, cx| {
                this.prompting = false;
                match result {
                    Ok(Some((name, source))) => match rgate_format::parse(&source) {
                        Ok(result) => {
                            this.replace_circuit(result.circuit, Some(PathBuf::from(name)));
                            for warning in result.warnings {
                                this.log(warning);
                            }
                        }
                        Err(error) => this.error(format!("{error:#}")),
                    },
                    Ok(None) => {}
                    Err(error) => this.error(error),
                }
                cx.notify();
            }) {
                eprintln!("Upload completed after app closed: {error}");
            }
        })
        .detach();
    }

    pub(super) fn save(
        &mut self,
        _save_as: bool,
        export: bool,
        _window: &Window,
        cx: &mut Context<Self>,
    ) {
        let content = if export {
            rgate_format::verilog::export(self.editor.circuit())
        } else {
            rgate_format::native::encode(self.editor.circuit())
        };
        let extension = if export { "v" } else { "rgate" };
        let base = self
            .file
            .as_ref()
            .and_then(|path| path.file_stem())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| {
                self.editor
                    .circuit()
                    .title
                    .trim_end_matches(".v")
                    .to_owned()
            });
        let filename = format!("{}.{}", base.replace(['/', '\\'], "-"), extension);
        let result = content.and_then(|content| {
            crate::browser::download(&filename, &content).map_err(crate::browser::js_error)
        });
        match result {
            Ok(()) => {
                if !export {
                    self.file = Some(PathBuf::from(&filename));
                    self.editor.mark_saved();
                }
                self.flush_workspace();
                self.log(format!("Downloaded {filename}"));
            }
            Err(error) => self.error(format!("{error:#}")),
        }
        cx.notify();
    }

    pub(super) fn load_memory_image(&mut self, window: &Window, cx: &mut Context<Self>) {
        let picker = crate::browser::picker(".hex,.mem,.txt");
        self.prompting = true;
        cx.spawn_in(window, async move |entity, cx| {
            let result = crate::browser::read_file(picker).await;
            if let Err(error) = entity.update_in(cx, |this, window, cx| {
                this.prompting = false;
                let result: Result<()> = (|| {
                    if let Some((_, image)) = result? {
                        this.apply_memory_image_text(&image, window, cx)?;
                    }
                    Ok(())
                })();
                if let Err(error) = result
                    && let Some(Dialog::Properties(dialog)) = &mut this.dialogs.active
                {
                    dialog.error = Some(format!("{error:#}"));
                }
                cx.notify();
            }) {
                eprintln!("Memory upload completed after app closed: {error}");
            }
        })
        .detach();
    }
}

#[cfg(not(target_family = "wasm"))]
impl GateApp {
    pub(super) fn open(&mut self, window: &Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open a .rgate circuit or Verilog .v/.sv source".into()),
        });
        self.prompting = true;
        cx.spawn_in(window, async move |entity, cx| {
            let result = paths.await;
            if let Err(error) = entity.update_in(cx, |this, _, cx| {
                this.prompting = false;
                match result {
                    Ok(Ok(Some(paths))) => {
                        if let Some(path) = paths.into_iter().next() {
                            match rgate_format::load(&path) {
                                Ok(result) => {
                                    this.replace_circuit(result.circuit, Some(path));
                                    for warning in result.warnings {
                                        this.log(warning);
                                    }
                                }
                                Err(error) => this.error(format!("{error:#}")),
                            }
                        }
                    }
                    Ok(Ok(None)) => {}
                    Ok(Err(error)) => this.error(error),
                    Err(error) => this.error(error),
                }
                cx.notify();
            }) {
                eprintln!("open dialog ended after window closed: {error}");
            }
        })
        .detach();
    }

    pub(super) fn save(
        &mut self,
        save_as: bool,
        export: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if !save_as
            && let Some(path) = &self.file
            && path
                .extension()
                .is_some_and(|extension| extension == "rgate")
        {
            let path = path.clone();
            self.save_to(path, false);
            return;
        }
        let directory = self
            .file
            .as_ref()
            .and_then(|path| path.parent())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").unwrap_or_else(|| ".".into()))
            });
        let extension = if export { "v" } else { "rgate" };
        let name = format!(
            "{}.{}",
            self.editor
                .circuit()
                .title
                .trim_end_matches(".v")
                .replace('/', "-"),
            extension
        );
        let path = cx.prompt_for_new_path(&directory, Some(&name));
        self.prompting = true;
        cx.spawn_in(window, async move |entity, cx| {
            let result = path.await;
            if let Err(error) = entity.update_in(cx, |this, _, cx| {
                this.prompting = false;
                match result {
                    Ok(Ok(Some(mut path))) => {
                        if export || path.extension().is_none() {
                            path.set_extension(extension);
                        }
                        this.save_to(path, export);
                    }
                    Ok(Ok(None)) => {}
                    Ok(Err(error)) => this.error(error),
                    Err(error) => this.error(error),
                }
                cx.notify();
            }) {
                eprintln!("save dialog ended after window closed: {error}");
            }
        })
        .detach();
    }

    fn save_to(&mut self, path: PathBuf, export: bool) {
        match rgate_format::save(&path, self.editor.circuit()) {
            Ok(()) => {
                if !export {
                    self.file = Some(path.clone());
                    self.editor.mark_saved();
                }
                self.flush_workspace();
                self.log(format!(
                    "{} {}",
                    if export { "Exported" } else { "Saved" },
                    path.display()
                ));
            }
            Err(error) => self.error(format!("{error:#}")),
        }
    }

    pub(super) fn load_memory_image(&mut self, window: &Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Load hex memory image".into()),
        });
        self.prompting = true;
        cx.spawn_in(window, async move |entity, cx| {
            let result = paths.await;
            if let Err(error) = entity.update_in(cx, |this, window, cx| {
                this.prompting = false;
                let result: Result<()> = (|| {
                    let paths = result??;
                    if let Some(path) = paths.and_then(|paths| paths.into_iter().next()) {
                        let image = std::fs::read_to_string(path)?;
                        this.apply_memory_image_text(&image, window, cx)?;
                    }
                    Ok(())
                })();
                if let Err(error) = result
                    && let Some(Dialog::Properties(dialog)) = &mut this.dialogs.active
                {
                    dialog.error = Some(format!("{error:#}"));
                }
                cx.notify();
            }) {
                eprintln!("memory image dialog closed: {error}");
            }
        })
        .detach();
    }
}
