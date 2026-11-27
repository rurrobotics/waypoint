use std::path::{Path as FsPath, PathBuf};

use gpui::{Context, PathPromptOptions, PromptLevel, Window};

use super::{ExportJson, NewProject, OpenMap, OpenProject, SaveMap, SaveProject, SaveProjectAs, Workspace};
use crate::export::{Exporter, JsonExporter};
use crate::model::{Doc, Map, Project, project::PROJECT_VERSION};
use crate::ui::canvas::Selection;

impl Workspace {
    fn dialog_dir(&self) -> PathBuf {
        self.project_path
            .as_deref()
            .or(self.map_file.as_deref())
            .and_then(FsPath::parent)
            .map(FsPath::to_path_buf)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default()
    }

    fn reset_with(&mut self, doc: Doc, cx: &mut Context<Self>) {
        self.doc = doc;
        self.history.clear();
        self.active_path = (!self.doc.paths.is_empty()).then_some(0);
        self.selection = Selection::None;
        self.polygon.clear();
        self.measure = None;
        self.edit = None;
        self.auto_fit = true;
        self.last_canvas_size = Default::default();
        self.revision += 1;
        self.dirty = false;
        cx.notify();
    }

    /// Runs `then` right away, or after the user agrees to drop unsaved changes.
    fn confirm_discard(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) {
        if !self.dirty {
            then(self, window, cx);
            return;
        }
        let answer = window.prompt(
            PromptLevel::Warning,
            "Discard unsaved changes?",
            Some("The current project has changes that haven't been saved."),
            &["Discard", "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if answer.await == Ok(0) {
                this.update_in(cx, |this, window, cx| then(this, window, cx)).ok();
            }
        })
        .detach();
    }

    pub(super) fn new_project(&mut self, _: &NewProject, window: &mut Window, cx: &mut Context<Self>) {
        self.confirm_discard(window, cx, |this, _, cx| {
            // Keep the table and robot: a new strategy usually targets the same year.
            let doc = Doc { map: this.doc.map.clone(), robot: this.doc.robot.clone(), paths: Vec::new() };
            this.project_path = None;
            this.reset_with(doc, cx);
            this.dirty = false;
        });
    }

    pub(super) fn open_project(&mut self, _: &OpenProject, window: &mut Window, cx: &mut Context<Self>) {
        self.confirm_discard(window, cx, |_, window, cx| {
            let rx = cx.prompt_for_paths(PathPromptOptions {
                files: true,
                directories: false,
                multiple: false,
                prompt: Some("Open project".into()),
            });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = rx.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                this.update(cx, |this, cx| this.load_project(path, cx)).ok();
            })
            .detach();
        });
    }

    /// Replaces the document with a project file (used by Open and the command line).
    pub fn load_project(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        match Project::load(&path) {
            Ok(project) => {
                self.map_file = project.map_file.clone();
                self.project_path = Some(path.clone());
                self.reset_with(project.doc, cx);
                self.set_status(format!("Opened {}", file_name(&path)), false);
            }
            Err(e) => self.set_status(format!("{e:#}"), true),
        }
        cx.notify();
    }

    pub(super) fn save_project(&mut self, _: &SaveProject, window: &mut Window, cx: &mut Context<Self>) {
        match self.project_path.clone() {
            Some(path) => self.write_project(path, cx),
            None => self.save_project_as(&SaveProjectAs, window, cx),
        }
    }

    pub(super) fn save_project_as(&mut self, _: &SaveProjectAs, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_new_path(&self.dialog_dir(), Some("strategy.waypoint"));
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(path))) = rx.await else { return };
            let path = with_extension(path, "waypoint");
            this.update(cx, |this, cx| this.write_project(path, cx)).ok();
        })
        .detach();
    }

    fn write_project(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let project = Project { version: PROJECT_VERSION, map_file: self.map_file.clone(), doc: self.doc.clone() };
        match project.save(&path) {
            Ok(()) => {
                self.set_status(format!("Saved {}", file_name(&path)), false);
                self.project_path = Some(path);
                self.dirty = false;
            }
            Err(e) => self.set_status(format!("{e:#}"), true),
        }
        cx.notify();
    }

    pub(super) fn open_map(&mut self, _: &OpenMap, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open map".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else { return };
            let Some(path) = paths.into_iter().next() else { return };
            this.update(cx, |this, cx| match Map::load(&path) {
                Ok(map) => {
                    this.edit_doc(cx, |d| d.map = map);
                    this.map_file = Some(path.clone());
                    this.selection = Selection::None;
                    this.auto_fit = true;
                    this.last_canvas_size = Default::default();
                    this.set_status(format!("Loaded map {}", file_name(&path)), false);
                }
                Err(e) => this.set_status(format!("{e:#}"), true),
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn save_map(&mut self, _: &SaveMap, window: &mut Window, cx: &mut Context<Self>) {
        let suggested = format!("eurobot-{}.wmap.toml", self.doc.map.year);
        let rx = cx.prompt_for_new_path(&self.dialog_dir(), Some(&suggested));
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(path))) = rx.await else { return };
            let path = if path.to_string_lossy().ends_with(".toml") { path } else { with_extension(path, "wmap.toml") };
            this.update(cx, |this, cx| {
                match this.doc.map.save(&path) {
                    Ok(()) => {
                        this.set_status(format!("Saved map {}", file_name(&path)), false);
                        this.map_file = Some(path);
                    }
                    Err(e) => this.set_status(format!("{e:#}"), true),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn pick_background(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Table image (PNG, JPG or SVG)".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else { return };
            let Some(path) = paths.into_iter().next() else { return };
            this.update(cx, |this, cx| this.edit_doc(cx, |d| d.map.background = Some(path))).ok();
        })
        .detach();
    }

    pub(super) fn export_json(&mut self, _: &ExportJson, window: &mut Window, cx: &mut Context<Self>) {
        if self.doc.paths.is_empty() {
            self.set_status("Nothing to export — draw a path first", true);
            cx.notify();
            return;
        }
        let exporter = JsonExporter;
        let stem = self
            .project_path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map_or("paths".to_string(), |s| s.to_string_lossy().into_owned());
        let suggested = format!("{stem}.{}", exporter.extension());
        let text = match exporter.export(&self.doc) {
            Ok(text) => text,
            Err(e) => {
                self.set_status(format!("{e:#}"), true);
                cx.notify();
                return;
            }
        };
        let rx = cx.prompt_for_new_path(&self.dialog_dir(), Some(&suggested));
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(path))) = rx.await else { return };
            let path = with_extension(path, exporter.extension());
            let result = std::fs::write(&path, text);
            this.update(cx, |this, cx| {
                match result {
                    Ok(()) => this.set_status(format!("Exported {} to {}", exporter.name(), file_name(&path)), false),
                    Err(e) => this.set_status(format!("Export failed: {e}"), true),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

/// Appends `ext` unless the path already ends with it.
fn with_extension(path: PathBuf, ext: &str) -> PathBuf {
    if path.to_string_lossy().ends_with(&format!(".{ext}")) {
        path
    } else {
        let mut s = path.into_os_string();
        s.push(format!(".{ext}"));
        s.into()
    }
}

fn file_name(path: &FsPath) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}
