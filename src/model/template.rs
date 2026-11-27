//! Project templates: a table (map), robot and no paths, to start new
//! projects from. Templates ship with the app from `assets/templates/*.json`
//! (`default.json` is what new projects start from); users save their own
//! into `<config dir>/templates/*.json`. Both are ordinary project files
//! without paths.

use std::path::{Path as FsPath, PathBuf};

use anyhow::Result;

use super::map::ASSET_PREFIX;
use super::{Doc, Project, project::PROJECT_VERSION};
use crate::assets::TEMPLATE_FILES;

#[derive(Clone, Debug, PartialEq)]
pub struct Template {
    pub name: String,
    /// The file it came from; `None` for templates shipped with the app.
    pub file: Option<PathBuf>,
    pub doc: Doc,
}

/// Parses a shipped template. A relative background refers to an image
/// shipped next to it and becomes an embedded asset path.
fn parse_shipped(path: &str, bytes: &[u8]) -> Option<Template> {
    let project: Project = serde_json::from_slice(bytes).ok()?;
    let mut doc = Doc { paths: Vec::new(), ..project.doc };
    if let Some(bg) = &doc.map.background
        && bg.is_relative()
        && !bg.to_string_lossy().starts_with(ASSET_PREFIX)
    {
        doc.map.background = Some(PathBuf::from(format!("{ASSET_PREFIX}templates/{}", bg.display())));
    }
    let name = format!("{} {}", doc.map.name, doc.map.year).trim().to_string();
    let name = if path.ends_with("/default.json") { format!("{name} (default)") } else { name };
    Some(Template { name, file: None, doc })
}

/// Templates shipped with the app, the default first.
pub fn shipped() -> Vec<Template> {
    let mut files: Vec<_> = TEMPLATE_FILES.iter().filter(|(p, _)| p.ends_with(".json")).collect();
    files.sort_by_key(|(p, _)| (!p.ends_with("/default.json"), *p));
    files.into_iter().filter_map(|(p, bytes)| parse_shipped(p, bytes)).collect()
}

/// The template new projects start from (`assets/templates/default.json`).
pub fn default() -> Template {
    shipped().into_iter().next().unwrap_or_else(|| Template {
        name: "Blank".into(),
        file: None,
        doc: Doc::default(),
    })
}

pub fn dir() -> Option<PathBuf> {
    Some(super::config_dir()?.join("templates"))
}

/// The shipped templates followed by every readable template in `dir`,
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
    shipped().into_iter().chain(user).collect()
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

    #[test]
    fn default_template_is_shipped_and_valid() {
        let t = default();
        assert!(t.name.ends_with("(default)"), "{}", t.name);
        assert!(t.file.is_none());
        assert!(t.doc.paths.is_empty());
        assert!(t.doc.map.width > 0. && t.doc.map.height > 0.);
        // Every shipped JSON file must parse.
        let json = TEMPLATE_FILES.iter().filter(|(p, _)| p.ends_with(".json")).count();
        assert_eq!(shipped().len(), json);
    }

    #[test]
    fn shipped_background_becomes_an_embedded_asset() {
        let t = parse_shipped(
            "templates/x.json",
            br#"{"version":1,"map":{"name":"T","year":1,"width":3000,"height":2000,"background":"x.png"},"robot":{"length":1,"width":1}}"#,
        )
        .unwrap();
        assert_eq!(t.doc.map.background_asset(), Some("templates/x.png"));
        assert_eq!(t.name, "T 1");
    }
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

        let shipped = shipped().len();
        let all = list(Some(&dir));
        assert_eq!(all.len(), shipped + 1, "shipped + saved; the broken file is skipped");
        assert_eq!(all[0], default());
        let t = &all[shipped];
        assert_eq!(t.name, "Eurobot 2026_v2");
        assert_eq!(t.doc.map.obstacles, doc.map.obstacles);
        assert_eq!(t.doc.robot.width, 280.);
        assert!(t.doc.paths.is_empty());

        assert_eq!(list(None), super::shipped());
        std::fs::remove_dir_all(&dir).ok();
    }
}
