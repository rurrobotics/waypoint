use std::path::PathBuf;

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

/// One year's playing table, embedded in every project (and template).
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
    #[default]
    Blue,
    Green,
    Amber,
    Red,
    Violet,
    Gray,
}

impl RegionColor {
    pub const ALL: [RegionColor; 6] = [Self::Blue, Self::Green, Self::Amber, Self::Red, Self::Violet, Self::Gray];
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_roundtrip_is_flat() {
        let mut map = Map::default();
        map.obstacles.push(Obstacle {
            name: "Ramp".into(),
            shape: Shape::Rect { center: vec2(1500., 1800.), size: vec2(1200., 400.), rotation: 0. },
        });
        map.regions.push(Region {
            name: "Start".into(),
            color: RegionColor::Blue,
            shape: Shape::Circle { center: vec2(200., 200.), radius: 150. },
        });
        let json = serde_json::to_string(&map).unwrap();
        assert!(json.contains(r#"{"name":"Ramp","type":"rect","center":[1500.0,1800.0]"#), "{json}");
        assert_eq!(serde_json::from_str::<Map>(&json).unwrap(), map);
    }

    #[test]
    fn hand_written_map() {
        let map: Map = serde_json::from_str(
            r#"{
                "name": "Eurobot 2026", "width": 3000, "height": 2000, "origin": "top-left",
                "obstacles": [{ "name": "Pillar", "type": "circle", "center": [1500, 1000], "radius": 40 }]
            }"#,
        )
        .unwrap();
        assert_eq!(map.origin, Origin::TopLeft);
        assert_eq!(map.obstacles.len(), 1);
    }
}
