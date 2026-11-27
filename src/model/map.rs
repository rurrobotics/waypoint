use std::path::{Path as FsPath, PathBuf};

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};

use crate::geom::{Shape, Vec2, vec2};

/// Which table corner is the origin. With `TopLeft`, y grows downwards on screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Origin {
    #[default]
    BottomLeft,
    TopLeft,
}

/// One year's playing table. Saved on its own as `*.wmap.toml` so a whole
/// team can share it, and embedded in every project.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Map {
    pub name: String,
    #[serde(default)]
    pub year: u32,
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub origin: Origin,
    /// Table artwork (PNG/JPG/SVG), stretched over the whole table.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<PathBuf>,
    #[serde(default)]
    pub obstacles: Vec<Obstacle>,
    #[serde(default)]
    pub regions: Vec<Region>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Obstacle {
    pub name: String,
    #[serde(flatten)]
    pub shape: Shape,
}

/// A named area of interest (start zone, scoring area, …). Never collides.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Region {
    pub name: String,
    pub color: RegionColor,
    #[serde(flatten)]
    pub shape: Shape,
}

/// Region colours come from the brand palette.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RegionColor {
    Primary,
    #[default]
    Secondary,
    Accent,
    Tertiary,
    Muted,
}

impl RegionColor {
    pub const ALL: [RegionColor; 5] = [Self::Primary, Self::Secondary, Self::Accent, Self::Tertiary, Self::Muted];
}

impl Default for Map {
    fn default() -> Self {
        Self {
            name: "Eurobot".into(),
            year: 2026,
            width: 3000.,
            height: 2000.,
            origin: Origin::default(),
            background: None,
            obstacles: Vec::new(),
            regions: Vec::new(),
        }
    }
}

impl Map {
    pub fn size(&self) -> Vec2 {
        vec2(self.width, self.height)
    }

    /// Loads a map file; a relative background path is resolved against the file.
    pub fn load(path: &FsPath) -> Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let mut map: Map = toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        if let (Some(bg), Some(dir)) = (&map.background, path.parent())
            && bg.is_relative()
        {
            map.background = Some(dir.join(bg));
        }
        Ok(map)
    }

    /// Saves a map file; the background is stored relative to it when it sits
    /// in the same directory tree.
    pub fn save(&self, path: &FsPath) -> Result<()> {
        let mut map = self.clone();
        if let (Some(bg), Some(dir)) = (&self.background, path.parent())
            && let Ok(rel) = bg.strip_prefix(dir)
        {
            map.background = Some(rel.to_path_buf());
        }
        let text = toml::to_string_pretty(&map)?;
        std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_roundtrip_is_flat() {
        let mut map = Map::default();
        map.obstacles.push(Obstacle {
            name: "Ramp".into(),
            shape: Shape::Rect { center: vec2(1500., 1800.), size: vec2(1200., 400.), rotation: 0. },
        });
        map.regions.push(Region {
            name: "Start blue".into(),
            color: RegionColor::Tertiary,
            shape: Shape::Circle { center: vec2(200., 200.), radius: 150. },
        });
        let text = toml::to_string_pretty(&map).unwrap();
        assert!(text.contains("[[obstacles]]") && text.contains("type = \"rect\""), "{text}");
        assert_eq!(toml::from_str::<Map>(&text).unwrap(), map);
    }

    #[test]
    fn hand_written_map() {
        let map: Map = toml::from_str(
            r#"
            name = "Eurobot 2026"
            width = 3000
            height = 2000
            origin = "top-left"

            [[obstacles]]
            name = "Pillar"
            type = "circle"
            center = [1500, 1000]
            radius = 40
            "#,
        )
        .unwrap();
        assert_eq!(map.origin, Origin::TopLeft);
        assert_eq!(map.obstacles.len(), 1);
    }
}
