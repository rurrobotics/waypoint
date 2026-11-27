//! Project templates: a table (map), robot and no paths, to start new
//! projects from. One blank template is built in; users save their own into
//! `<config dir>/templates/*.json` (ordinary project files without paths).

use std::path::{Path as FsPath, PathBuf};

use anyhow::Result;

use super::{Doc, Project, project::PROJECT_VERSION};

#[derive(Clone, Debug, PartialEq)]
pub struct Template {
    pub name: String,
    /// The file it came from; `None` for the built-in template.
    pub file: Option<PathBuf>,
    pub doc: Doc,
}

/// The empty 3000 × 2000 Eurobot table.
pub fn blank() -> Template {
    Template { name: "Blank 3000 × 2000".into(), file: None, doc: Doc::default() }
}

pub fn dir() -> Option<PathBuf> {
    Some(super::config_dir()?.join("templates"))
}

/// The built-in template followed by every readable template in `dir`,
/// sorted by name. Unreadable files are skipped.
pub fn list(dir: Option<&FsPath>) -> Vec<Template> {
    let mut user: Vec<Template> = dir
        .and_then(|d| std::fs::read_dir(d).ok())
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter_map(|p| {
            let project = Project::load(&p).ok()?;
            let name = p.file_stem()?.to_string_lossy().into_owned();
            Some(Template { name, file: Some(p), doc: Doc { paths: Vec::new(), ..project.doc } })
        })
        .collect();
    user.sort_by_key(|t| t.name.to_lowercase());
    std::iter::once(blank()).chain(user).collect()
}

/// Saves `doc`'s map and robot as a template called `name` in `dir`,
/// replacing one with the same name.
pub fn save(dir: &FsPath, name: &str, doc: &Doc) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let stem: String =
        name.chars().map(|c| if c.is_alphanumeric() || " -_".contains(c) { c } else { '_' }).collect();
    let stem = stem.trim();
    let file = dir.join(format!("{}.json", if stem.is_empty() { "template" } else { stem }));
    let doc = Doc { paths: Vec::new(), ..doc.clone() };
    Project { version: PROJECT_VERSION, doc }.save(&file)?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{Path, Pose, Shape, vec2};
    use crate::model::Obstacle;

    #[test]
    fn save_and_list_templates() {
        let dir = std::env::temp_dir().join(format!("waypoint-templates-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();

        let mut doc = Doc::default();
        doc.map.name = "Eurobot".into();
        doc.map.obstacles.push(Obstacle {
            name: "Ramp".into(),
            shape: Shape::Rect { center: vec2(1500., 1850.), size: vec2(1200., 300.), rotation: 0. },
        });
        doc.robot.width = 280.;
        doc.paths.push(Path::new("Not part of a template", Pose::default()));

        let file = save(&dir, "Eurobot 2026/v2", &doc).unwrap();
        assert_eq!(file.file_name().unwrap(), "Eurobot 2026_v2.json");
        std::fs::write(dir.join("broken.json"), "{ not json").unwrap();

        let all = list(Some(&dir));
        assert_eq!(all.len(), 2, "built-in + saved; the broken file is skipped");
        assert_eq!(all[0], blank());
        let t = &all[1];
        assert_eq!(t.name, "Eurobot 2026_v2");
        assert_eq!(t.doc.map.obstacles, doc.map.obstacles);
        assert_eq!(t.doc.robot.width, 280.);
        assert!(t.doc.paths.is_empty());

        assert_eq!(list(None), vec![blank()]);
        std::fs::remove_dir_all(&dir).ok();
    }
}
