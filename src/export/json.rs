use anyhow::Result;
use serde::Serialize;

use super::{ExportOptions, Exporter};
use crate::geom::{Pose, Robot, SegmentKind, Vec2};
use crate::model::{Doc, Origin};

pub struct JsonExporter;

#[derive(Serialize)]
struct File<'a> {
    format: &'static str,
    version: u32,
    units: Units,
    table: Table<'a>,
    robot: &'a Robot,
    paths: Vec<PathOut<'a>>,
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

#[derive(Serialize)]
struct PathOut<'a> {
    name: &'a str,
    length: f64,
    start: Pose,
    segments: Vec<SegmentOut>,
    #[serde(skip_serializing_if = "Option::is_none")]
    samples: Option<Vec<SampleOut>>,
}

#[derive(Serialize)]
struct SegmentOut {
    #[serde(flatten)]
    kind: KindOut,
    reverse: bool,
    start: Pose,
    end: Pose,
    length: f64,
}

/// Like [`SegmentKind`] but with every parameter spelled out for consumers.
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum KindOut {
    Straight { to: Vec2 },
    Turn { angle: f64, radius: f64, in_place: bool },
    Hermite { to: Pose, t0: f64, t1: f64 },
}

#[derive(Serialize)]
struct SampleOut {
    segment: usize,
    s: f64,
    x: f64,
    y: f64,
    theta: f64,
    curvature: f64,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    in_place: bool,
}

impl Exporter for JsonExporter {
    fn name(&self) -> &'static str {
        "JSON"
    }

    fn extension(&self) -> &'static str {
        "json"
    }

    fn export(&self, doc: &Doc, options: &ExportOptions) -> Result<String> {
        let paths = doc
            .paths
            .iter()
            .map(|path| {
                let poses = path.poses();
                let segments = path
                    .segments
                    .iter()
                    .zip(poses.windows(2))
                    .map(|(seg, w)| SegmentOut {
                        kind: match seg.kind {
                            SegmentKind::Straight { to } => KindOut::Straight { to },
                            SegmentKind::Turn { angle, radius, .. } => {
                                KindOut::Turn { angle, radius, in_place: seg.is_in_place() }
                            }
                            SegmentKind::Hermite { to, t0, t1 } => KindOut::Hermite { to, t0, t1 },
                        },
                        reverse: seg.reverse,
                        start: w[0],
                        end: w[1],
                        length: seg.length(w[0]),
                    })
                    .collect();
                let samples = options.sample_step.map(|step| {
                    path.sample(step)
                        .into_iter()
                        .map(|s| SampleOut {
                            segment: s.segment,
                            s: s.s,
                            x: s.pose.x,
                            y: s.pose.y,
                            theta: s.pose.theta,
                            curvature: s.curvature,
                            in_place: s.in_place,
                        })
                        .collect()
                });
                PathOut { name: &path.name, length: path.length(), start: path.start, segments, samples }
            })
            .collect();

        let file = File {
            format: "waypoint-paths",
            version: 1,
            units: Units { length: "mm", angle: "rad" },
            table: Table {
                name: &doc.map.name,
                year: doc.map.year,
                width: doc.map.width,
                height: doc.map.height,
                origin: doc.map.origin,
            },
            robot: &doc.robot,
            paths,
        };
        Ok(serde_json::to_string_pretty(&file)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{Path, vec2};

    #[test]
    fn export_contains_segments_and_samples() {
        let mut doc = Doc::default();
        let mut path = Path::new("Match A", Pose { x: 250., y: 250., theta: 0. });
        path.push_straight(vec2(250., 1000.), false, true);
        path.push_hermite(vec2(1200., 1500.), false);
        doc.paths.push(path);

        let text = JsonExporter.export(&doc, &ExportOptions::default()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        let segs = v["paths"][0]["segments"].as_array().unwrap();
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[0]["type"], "turn");
        assert_eq!(segs[0]["in_place"], true);
        assert_eq!(segs[1]["type"], "straight");
        assert_eq!(segs[2]["type"], "hermite");
        assert!(v["paths"][0]["samples"].as_array().unwrap().len() > 50);

        let text = JsonExporter.export(&doc, &ExportOptions { sample_step: None }).unwrap();
        assert!(!text.contains("samples"));
    }
}
