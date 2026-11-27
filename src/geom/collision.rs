//! Clearance between the robot footprint and the table walls / obstacles.

use super::footprint::Robot;
use super::path::Sample;
use super::shape::{Shape, polygon_contains};
use super::vec::Vec2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Wall,
    Obstacle(usize),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub sample: usize,
    pub segment: usize,
    pub target: Target,
    pub clearance: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    /// One entry per sample: the body is closer than the margin to something.
    pub colliding: Vec<bool>,
    /// The closest offender of every colliding sample.
    pub hits: Vec<Hit>,
    /// Number of separate collision stretches along the path.
    pub events: usize,
    /// Smallest clearance along the path and the sample where it occurs.
    pub min_clearance: Option<(f64, usize)>,
    /// Smallest clearance per segment (∞ for segments without samples).
    pub segment_clearance: Vec<f64>,
}

/// Checks every sample's footprint against the table bounds and obstacles.
/// Clearance is measured from the bare body; a sample collides when its
/// clearance drops below the robot's margin.
pub fn check(samples: &[Sample], segments: usize, robot: &Robot, table: Vec2, obstacles: &[&Shape]) -> Report {
    let mut report = Report {
        colliding: vec![false; samples.len()],
        segment_clearance: vec![f64::INFINITY; segments],
        ..Default::default()
    };
    let mut prev = false;

    for (i, sample) in samples.iter().enumerate() {
        let body = robot.corners(sample.pose);
        let mut best = (wall_clearance(&body, table), Target::Wall);
        for (k, shape) in obstacles.iter().enumerate() {
            let d = shape_clearance(&body, shape);
            if d < best.0 {
                best = (d, Target::Obstacle(k));
            }
        }

        let (clearance, target) = best;
        if report.min_clearance.is_none_or(|(c, _)| clearance < c) {
            report.min_clearance = Some((clearance, i));
        }
        if let Some(c) = report.segment_clearance.get_mut(sample.segment) {
            *c = c.min(clearance);
        }

        let hit = clearance <= 0. || clearance < robot.margin;
        report.colliding[i] = hit;
        if hit {
            report.hits.push(Hit { sample: i, segment: sample.segment, target, clearance });
            if !prev {
                report.events += 1;
            }
        }
        prev = hit;
    }
    report
}

/// Distance from the body to the nearest wall; negative when outside the table.
pub fn wall_clearance(body: &[Vec2], table: Vec2) -> f64 {
    body.iter().map(|p| p.x.min(table.x - p.x).min(p.y).min(table.y - p.y)).fold(f64::INFINITY, f64::min)
}

/// Distance from a convex body polygon to a shape; 0 when they overlap.
pub fn shape_clearance(body: &[Vec2], shape: &Shape) -> f64 {
    match shape {
        Shape::Circle { center, radius } => {
            if polygon_contains(body, *center) {
                return 0.;
            }
            (polygon_point_distance(body, *center) - radius).max(0.)
        }
        _ => {
            let verts = shape.vertices().unwrap_or_default();
            if verts.len() < 2 {
                return f64::INFINITY;
            }
            if convex_overlap(body, &verts) {
                return 0.;
            }
            let a = verts.iter().map(|&p| polygon_point_distance(body, p)).fold(f64::INFINITY, f64::min);
            let b = body.iter().map(|&p| polygon_point_distance(&verts, p)).fold(f64::INFINITY, f64::min);
            a.min(b)
        }
    }
}

/// Separating-axis test for two convex polygons.
pub fn convex_overlap(a: &[Vec2], b: &[Vec2]) -> bool {
    for poly in [a, b] {
        let n = poly.len();
        for i in 0..n {
            let axis = (poly[(i + 1) % n] - poly[i]).perp();
            if axis.length() < 1e-12 {
                continue;
            }
            let (amin, amax) = project(a, axis);
            let (bmin, bmax) = project(b, axis);
            if amax < bmin || bmax < amin {
                return false;
            }
        }
    }
    true
}

fn project(poly: &[Vec2], axis: Vec2) -> (f64, f64) {
    poly.iter().map(|p| p.dot(axis)).fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), d| (lo.min(d), hi.max(d)))
}

/// Distance from a point to a polygon's outline.
pub fn polygon_point_distance(poly: &[Vec2], p: Vec2) -> f64 {
    let n = poly.len();
    (0..n).map(|i| segment_point_distance(poly[i], poly[(i + 1) % n], p)).fold(f64::INFINITY, f64::min)
}

pub fn segment_point_distance(a: Vec2, b: Vec2, p: Vec2) -> f64 {
    let ab = b - a;
    let len2 = ab.dot(ab);
    let t = if len2 < 1e-12 { 0. } else { ((p - a).dot(ab) / len2).clamp(0., 1.) };
    p.distance(a + ab * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::path::Path;
    use crate::geom::vec::{Pose, vec2};

    const TABLE: Vec2 = vec2(3000., 2000.);

    fn robot() -> Robot {
        Robot { length: 200., width: 200., offset: Vec2::ZERO, margin: 10. }
    }

    #[test]
    fn walls() {
        let body = robot().corners(Pose { x: 150., y: 1000., theta: 0. });
        assert!((wall_clearance(&body, TABLE) - 50.).abs() < 1e-9);
        let body = robot().corners(Pose { x: 50., y: 1000., theta: 0. });
        assert!(wall_clearance(&body, TABLE) < 0.);
    }

    #[test]
    fn sat_overlap_and_distance() {
        let body = robot().corners(Pose { x: 1000., y: 1000., theta: 0. });
        let near = Shape::Rect { center: vec2(1250., 1000.), size: vec2(100., 100.), rotation: 0. };
        // Body spans x 900..1100, the obstacle 1200..1300.
        assert!((shape_clearance(&body, &near) - 100.).abs() < 1e-9);
        let overlapping = Shape::Rect { center: vec2(1150., 1000.), size: vec2(100., 100.), rotation: 0.3 };
        assert_eq!(shape_clearance(&body, &overlapping), 0.);
        let circle = Shape::Circle { center: vec2(1000., 1300.), radius: 50. };
        assert!((shape_clearance(&body, &circle) - 150.).abs() < 1e-9);
    }

    #[test]
    fn diagonal_rect_gap_is_not_overlap() {
        // Axis-aligned bounding boxes overlap, but the rotated rectangles don't.
        let a = Shape::Rect { center: vec2(0., 0.), size: vec2(200., 20.), rotation: std::f64::consts::FRAC_PI_4 };
        let b = Shape::Rect { center: vec2(60., -60.), size: vec2(20., 20.), rotation: 0. };
        assert!(!convex_overlap(&a.vertices().unwrap(), &b.vertices().unwrap()));
    }

    #[test]
    fn path_report_counts_events() {
        let mut path = Path::new("p", Pose { x: 300., y: 1000., theta: 0. });
        path.push_straight(vec2(2700., 1000.), false, true);
        let obstacle = Shape::Rect { center: vec2(1500., 1000.), size: vec2(100., 100.), rotation: 0. };
        let samples = path.sample(10.);
        let report = check(&samples, path.segments.len(), &robot(), TABLE, &[&obstacle]);
        assert_eq!(report.events, 1);
        assert!(report.hits.iter().all(|h| h.target == Target::Obstacle(0)));
        assert_eq!(report.min_clearance.unwrap().0, 0.);

        let clear = check(&samples, path.segments.len(), &robot(), TABLE, &[]);
        assert_eq!(clear.events, 0);
        assert!((clear.min_clearance.unwrap().0 - 200.).abs() < 1e-6);
    }
}
