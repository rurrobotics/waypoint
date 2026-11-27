use anyhow::Result;
use serde::Serialize;

use super::Exporter;
use crate::geom::{Path, Robot};
use crate::model::{Doc, Origin};

/// Writes the paths exactly as they are edited: a start pose and segments
/// with their parameters (straight `to`, turn `angle`/`radius`, Hermite
/// `to`/`t0`/`t1`, optional `reverse`). Nothing is sampled or derived.
pub struct JsonExporter;

#[derive(Serialize)]
struct File<'a> {
    format: &'static str,
    version: u32,
    units: Units,
    table: Table<'a>,
    robot: &'a Robot,
    paths: Vec<Path>,
}

#[derive(Serialize)]
struct Units {
    length: &'static str,
    angle: &'static str,
}

#[derive(Serialize)]
struct Table<'a> {
    name: &'a str,
    year: u32,
    width: f64,
    height: f64,
    origin: Origin,
}

impl Exporter for JsonExporter {
    fn name(&self) -> &'static str {
        "JSON"
    }

    fn extension(&self) -> &'static str {
        "json"
    }

    fn export(&self, doc: &Doc) -> Result<String> {
        let file = File {
            format: "waypoint-paths",
            version: 2,
            units: Units { length: "mm", angle: "rad" },
            table: Table {
                name: &doc.map.name,
                year: doc.map.year,
                width: doc.map.width,
                height: doc.map.height,
                origin: doc.map.origin,
            },
            robot: &doc.robot,
            // Visibility is an editor setting, not part of the path.
            paths: doc.paths.iter().map(|p| Path { hidden: false, ..p.clone() }).collect(),
        };
        Ok(serde_json::to_string_pretty(&file)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{Pose, vec2};

    #[test]
    fn exports_segments_with_their_parameters_only() {
        let mut doc = Doc::default();
        let mut path = Path::new("Match A", Pose { x: 250., y: 250., theta: 0. });
        path.push_straight(vec2(250., 1000.), false, true);
        path.push_arc_through(vec2(800., 1300.), false);
        path.push_hermite(vec2(1200., 1500.), true);
        doc.paths.push(path.clone());

        let text = JsonExporter.export(&doc).unwrap();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        let segs = v["paths"][0]["segments"].as_array().unwrap();
        let kinds: Vec<_> = segs.iter().map(|s| s["type"].as_str().unwrap()).collect();
        assert_eq!(kinds, ["turn", "straight", "turn", "hermite"]);
        assert_eq!(segs[0]["radius"], 0.0);
        assert!(segs[2]["radius"].as_f64().unwrap() > 0.);
        assert_eq!(segs[3]["reverse"], true);
        assert!(!text.contains("samples"));

        // The exported paths read back as the same paths.
        let back: Vec<Path> = serde_json::from_value(v["paths"].clone()).unwrap();
        assert_eq!(back, vec![path]);
    }

    #[test]
    fn hidden_paths_are_exported_without_the_flag() {
        let mut doc = Doc::default();
        let mut path = Path::new("Hidden", Pose::default());
        path.hidden = true;
        doc.paths.push(path);
        let text = JsonExporter.export(&doc).unwrap();
        assert!(text.contains("\"Hidden\""));
        assert!(!text.contains("hidden"), "{text}");
    }
}
