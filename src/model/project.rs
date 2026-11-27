use std::path::Path as FsPath;

use anyhow::{Context as _, Result, bail};
use serde::{Deserialize, Serialize};

use super::map::Map;
use crate::geom::{Path, Robot};

pub const PROJECT_VERSION: u32 = 1;

/// Everything that's edited and undone together.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Doc {
    pub map: Map,
    pub robot: Robot,
    #[serde(default)]
    pub paths: Vec<Path>,
}

/// A project file: plain JSON holding the map, robot and paths. The
/// background image path is stored relative to the file when possible.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub version: u32,
    #[serde(flatten)]
    pub doc: Doc,
}

impl Project {
    pub fn load(path: &FsPath) -> Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let mut project: Project =
            serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        if project.version > PROJECT_VERSION {
            bail!("{} was saved by a newer Waypoint (format {})", path.display(), project.version);
        }
        let dir = path.parent().unwrap_or(FsPath::new(""));
        if let Some(p) = &mut project.doc.map.background
            && p.is_relative()
            && !p.to_string_lossy().starts_with(super::map::ASSET_PREFIX)
        {
            *p = dir.join(&*p);
        }
        Ok(project)
    }

    pub fn save(&self, path: &FsPath) -> Result<()> {
        let mut project = self.clone();
        project.version = PROJECT_VERSION;
        if let (Some(dir), Some(p)) = (path.parent(), &mut project.doc.map.background)
            && let Ok(rel) = p.strip_prefix(dir)
        {
            *p = rel.to_path_buf();
        }
        let text = serde_json::to_string_pretty(&project)?;
        std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{Pose, vec2};

    #[test]
    fn save_load_roundtrip_relativises_paths() {
        let dir = std::env::temp_dir().join(format!("waypoint-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("demo.json");

        let mut doc = Doc::default();
        doc.map.background = Some(dir.join("table.png"));
        let mut path = Path::new("Match A", Pose { x: 250., y: 250., theta: 0. });
        path.push_straight(vec2(1000., 250.), false, true);
        doc.paths.push(path);
        let project = Project { version: PROJECT_VERSION, doc };

        project.save(&file).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains(r#""background": "table.png""#), "{text}");
        assert_eq!(Project::load(&file).unwrap(), project);
        std::fs::remove_dir_all(&dir).ok();
    }
}
