//! Differential-drive paths: a start pose followed by chained segments.
//!
//! Every segment starts where the previous one ended. Headings are body
//! headings; a `reverse` segment is driven backwards, so its direction of
//! travel is the body heading plus π.

use std::f64::consts::PI;

use serde::{Deserialize, Serialize};

use super::vec::{Pose, Vec2, wrap_angle};

/// Headings closer than this are treated as aligned (≈0.06°).
const HEADING_EPS: f64 = 1e-3;
/// Radii below this are a turn in place (mm).
const IN_PLACE_RADIUS: f64 = 1e-6;
/// Angular step used to sample turns in place.
const TURN_SAMPLE_STEP: f64 = 5. * PI / 180.;
/// Resolution of the Hermite arc-length table.
const HERMITE_TABLE: usize = 256;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SegmentKind {
    /// Drive straight to `to`. Should start aligned with the current heading.
    Straight { to: Vec2 },
    /// Rotate by `angle` (CCW positive). `radius == 0` turns in place,
    /// otherwise drives a circular arc of that radius.
    Turn {
        angle: f64,
        radius: f64,
        /// Inserted by the editor in front of a straight; its angle is kept
        /// in sync so the straight stays aligned.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        auto: bool,
    },
    /// Cubic Hermite curve to a free end pose with tangent magnitudes `t0`, `t1` (mm).
    Hermite { to: Pose, t0: f64, t1: f64 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    #[serde(flatten)]
    pub kind: SegmentKind,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reverse: bool,
}

impl Segment {
    pub fn straight(to: Vec2, reverse: bool) -> Self {
        Self { kind: SegmentKind::Straight { to }, reverse }
    }

    pub fn turn(angle: f64, radius: f64, reverse: bool) -> Self {
        Self { kind: SegmentKind::Turn { angle, radius, auto: false }, reverse }
    }

    pub fn hermite(to: Pose, t0: f64, t1: f64, reverse: bool) -> Self {
        Self { kind: SegmentKind::Hermite { to, t0, t1 }, reverse }
    }

    pub fn is_in_place(&self) -> bool {
        matches!(self.kind, SegmentKind::Turn { radius, .. } if radius.abs() < IN_PLACE_RADIUS)
    }

    pub fn end_pose(&self, start: Pose) -> Pose {
        match self.kind {
            SegmentKind::Straight { to } => {
                let d = to - start.pos();
                if d.length() < 1e-9 {
                    Pose::new(to, start.theta)
                } else {
                    Pose::new(to, body_heading(d.angle(), self.reverse))
                }
            }
            SegmentKind::Turn { angle, radius, .. } => {
                if radius.abs() < IN_PLACE_RADIUS {
                    Pose { theta: start.theta + angle, ..start }
                } else {
                    let arc = Arc::new(start, angle, radius, self.reverse);
                    arc.pose_at(angle)
                }
            }
            SegmentKind::Hermite { to, .. } => to,
        }
    }

    /// Distance driven along the floor (turns in place have zero length).
    pub fn length(&self, start: Pose) -> f64 {
        match self.kind {
            SegmentKind::Straight { to } => start.pos().distance(to),
            SegmentKind::Turn { angle, radius, .. } => radius.abs() * angle.abs(),
            SegmentKind::Hermite { to, t0, t1 } => {
                let h = Hermite::new(start, to, t0, t1, self.reverse);
                h.arc_table().last().map_or(0., |&(_, s)| s)
            }
        }
    }

    /// True when a straight doesn't start aligned with the incoming heading.
    pub fn is_discontinuous(&self, start: Pose) -> bool {
        let SegmentKind::Straight { to } = self.kind else {
            return false;
        };
        let d = to - start.pos();
        d.length() > 1e-6 && wrap_angle(d.angle() - travel_heading(start.theta, self.reverse)).abs() > HEADING_EPS
    }
}

/// Direction of travel for a body heading.
pub fn travel_heading(theta: f64, reverse: bool) -> f64 {
    if reverse { theta + PI } else { theta }
}

/// Body heading for a direction of travel.
pub fn body_heading(travel: f64, reverse: bool) -> f64 {
    if reverse { wrap_angle(travel - PI) } else { travel }
}

/// A circular arc turn, resolved against its start pose.
#[derive(Clone, Copy, Debug)]
pub struct Arc {
    pub start: Pose,
    pub center: Vec2,
}

impl Arc {
    pub fn new(start: Pose, angle: f64, radius: f64, reverse: bool) -> Self {
        let travel = Vec2::from_angle(travel_heading(start.theta, reverse));
        let side = if angle >= 0. { 1. } else { -1. };
        Self { start, center: start.pos() + travel.perp() * (radius.abs() * side) }
    }

    /// Pose after sweeping `a` radians of the arc.
    pub fn pose_at(&self, a: f64) -> Pose {
        let p = self.center + (self.start.pos() - self.center).rotated(a);
        Pose::new(p, self.start.theta + a)
    }
}

/// A cubic Hermite curve, resolved against its start pose.
#[derive(Clone, Copy, Debug)]
pub struct Hermite {
    pub p0: Vec2,
    pub p1: Vec2,
    pub m0: Vec2,
    pub m1: Vec2,
    pub reverse: bool,
}

impl Hermite {
    pub fn new(start: Pose, to: Pose, t0: f64, t1: f64, reverse: bool) -> Self {
        Self {
            p0: start.pos(),
            p1: to.pos(),
            m0: Vec2::from_angle(travel_heading(start.theta, reverse)) * t0,
            m1: Vec2::from_angle(travel_heading(to.theta, reverse)) * t1,
            reverse,
        }
    }

    pub fn point(&self, t: f64) -> Vec2 {
        let (t2, t3) = (t * t, t * t * t);
        self.p0 * (2. * t3 - 3. * t2 + 1.)
            + self.m0 * (t3 - 2. * t2 + t)
            + self.p1 * (-2. * t3 + 3. * t2)
            + self.m1 * (t3 - t2)
    }

    pub fn derivative(&self, t: f64) -> Vec2 {
        let t2 = t * t;
        self.p0 * (6. * t2 - 6. * t)
            + self.m0 * (3. * t2 - 4. * t + 1.)
            + self.p1 * (-6. * t2 + 6. * t)
            + self.m1 * (3. * t2 - 2. * t)
    }

    pub fn second_derivative(&self, t: f64) -> Vec2 {
        self.p0 * (12. * t - 6.) + self.m0 * (6. * t - 4.) + self.p1 * (-12. * t + 6.) + self.m1 * (6. * t - 2.)
    }

    /// Bézier control points of the same curve, for drawing.
    pub fn bezier_controls(&self) -> (Vec2, Vec2) {
        (self.p0 + self.m0 * (1. / 3.), self.p1 - self.m1 * (1. / 3.))
    }

    pub fn curvature(&self, t: f64) -> f64 {
        let d1 = self.derivative(t);
        let len = d1.length();
        if len < 1e-9 { 0. } else { d1.cross(self.second_derivative(t)) / (len * len * len) }
    }

    /// Body pose at parameter `t`. Falls back to the chord direction where the
    /// derivative vanishes (zero tangent magnitudes at the ends).
    pub fn pose(&self, t: f64) -> Pose {
        let mut d = self.derivative(t);
        if d.length() < 1e-9 {
            d = self.derivative((t + if t < 0.5 { 1e-3 } else { -1e-3 }).clamp(0., 1.));
        }
        if d.length() < 1e-9 {
            d = self.p1 - self.p0;
        }
        Pose::new(self.point(t), body_heading(d.angle(), self.reverse))
    }

    /// `(t, cumulative length)` pairs from t = 0 to 1.
    pub fn arc_table(&self) -> Vec<(f64, f64)> {
        let mut table = Vec::with_capacity(HERMITE_TABLE + 1);
        let mut s = 0.;
        let mut prev = self.p0;
        table.push((0., 0.));
        for i in 1..=HERMITE_TABLE {
            let t = i as f64 / HERMITE_TABLE as f64;
            let p = self.point(t);
            s += p.distance(prev);
            prev = p;
            table.push((t, s));
        }
        table
    }
}

/// A point on a sampled path.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub segment: usize,
    /// Cumulative distance driven from the path start (mm).
    pub s: f64,
    pub pose: Pose,
    /// Signed curvature relative to the direction of travel (1/mm).
    pub curvature: f64,
    /// Part of a turn in place: the robot rotates without moving.
    pub in_place: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Path {
    pub name: String,
    pub start: Pose,
    #[serde(default)]
    pub segments: Vec<Segment>,
}

impl Path {
    pub fn new(name: impl Into<String>, start: Pose) -> Self {
        Self { name: name.into(), start, segments: Vec::new() }
    }

    /// Waypoint poses: the start followed by every segment's end.
    pub fn poses(&self) -> Vec<Pose> {
        let mut poses = Vec::with_capacity(self.segments.len() + 1);
        let mut pose = self.start;
        poses.push(pose);
        for seg in &self.segments {
            pose = seg.end_pose(pose);
            poses.push(pose);
        }
        poses
    }

    pub fn end_pose(&self) -> Pose {
        self.segments.iter().fold(self.start, |p, s| s.end_pose(p))
    }

    pub fn length(&self) -> f64 {
        let poses = self.poses();
        self.segments.iter().zip(&poses).map(|(s, &p)| s.length(p)).sum()
    }

    /// Indices of segments that start with a heading jump.
    pub fn discontinuities(&self) -> Vec<usize> {
        let poses = self.poses();
        self.segments
            .iter()
            .zip(&poses)
            .enumerate()
            .filter(|(_, (s, p))| s.is_discontinuous(**p))
            .map(|(i, _)| i)
            .collect()
    }

    /// Recomputes every auto turn in place that precedes a straight so the
    /// straight starts aligned.
    pub fn fix_auto_turns(&mut self) {
        let mut pose = self.start;
        for i in 0..self.segments.len() {
            let next = self.segments.get(i + 1).cloned();
            if let (SegmentKind::Turn { angle, radius, auto: true }, Some(next)) = (&mut self.segments[i].kind, next)
                && radius.abs() < IN_PLACE_RADIUS
                && let SegmentKind::Straight { to } = next.kind
            {
                let d = to - pose.pos();
                if d.length() > 1e-6 {
                    *angle = wrap_angle(d.angle() - travel_heading(pose.theta, next.reverse));
                }
            }
            pose = self.segments[i].end_pose(pose);
        }
    }

    /// Appends a straight to `to`. With `auto_turn`, first inserts a turn in
    /// place when the robot isn't already facing (or backing towards) it.
    pub fn push_straight(&mut self, to: Vec2, reverse: bool, auto_turn: bool) {
        let end = self.end_pose();
        let d = to - end.pos();
        if d.length() < 1e-6 {
            return;
        }
        let diff = wrap_angle(d.angle() - travel_heading(end.theta, reverse));
        if auto_turn && diff.abs() > HEADING_EPS {
            self.segments.push(Segment {
                kind: SegmentKind::Turn { angle: diff, radius: 0., auto: true },
                reverse: false,
            });
        }
        self.segments.push(Segment::straight(to, reverse));
    }

    /// Appends an arc that leaves tangent to the current heading and passes
    /// through `through`. Falls back to a straight when it lies dead ahead.
    pub fn push_arc_through(&mut self, through: Vec2, reverse: bool) {
        let end = self.end_pose();
        let travel = travel_heading(end.theta, reverse);
        let local = (through - end.pos()).rotated(-travel);
        if local.length() < 1e-6 {
            return;
        }
        if local.y.abs() < 1e-3 {
            self.push_straight(through, reverse, false);
            return;
        }
        let radius = local.dot(local) / (2. * local.y.abs());
        let angle = 2. * local.y.atan2(local.x);
        self.segments.push(Segment::turn(angle, radius, reverse));
    }

    /// Appends a turn in place to face (or back towards) `target`.
    pub fn push_turn_towards(&mut self, target: Vec2, reverse: bool) {
        let end = self.end_pose();
        let d = target - end.pos();
        if d.length() < 1e-6 {
            return;
        }
        let angle = wrap_angle(d.angle() - travel_heading(end.theta, reverse));
        self.segments.push(Segment::turn(angle, 0., false));
    }

    /// Appends a Hermite curve to `to`, ending along the chord direction.
    pub fn push_hermite(&mut self, to: Vec2, reverse: bool) {
        let end = self.end_pose();
        let chord = to - end.pos();
        if chord.length() < 1e-6 {
            return;
        }
        let theta = body_heading(chord.angle(), reverse);
        let len = chord.length();
        self.segments.push(Segment::hermite(Pose::new(to, theta), len, len, reverse));
    }

    /// Samples the path every `step` mm (turns in place every 5°).
    pub fn sample(&self, step: f64) -> Vec<Sample> {
        let step = step.max(0.1);
        let mut out = Vec::new();
        let mut pose = self.start;
        let mut s0 = 0.;
        out.push(Sample { segment: 0, s: 0., pose, curvature: 0., in_place: false });

        for (i, seg) in self.segments.iter().enumerate() {
            let len = seg.length(pose);
            match seg.kind {
                SegmentKind::Straight { to } => {
                    let end = seg.end_pose(pose);
                    let n = (len / step).ceil().max(1.) as usize;
                    for k in 1..=n {
                        let t = k as f64 / n as f64;
                        let p = Pose::new(pose.pos().lerp(to, t), end.theta);
                        out.push(Sample { segment: i, s: s0 + len * t, pose: p, curvature: 0., in_place: false });
                    }
                }
                SegmentKind::Turn { angle, radius, .. } if radius.abs() < IN_PLACE_RADIUS => {
                    let n = (angle.abs() / TURN_SAMPLE_STEP).ceil().max(1.) as usize;
                    for k in 1..=n {
                        let p = Pose { theta: pose.theta + angle * k as f64 / n as f64, ..pose };
                        out.push(Sample { segment: i, s: s0, pose: p, curvature: 0., in_place: true });
                    }
                }
                SegmentKind::Turn { angle, radius, .. } => {
                    let arc = Arc::new(pose, angle, radius, seg.reverse);
                    let n = (len / step).ceil().max(1.) as usize;
                    let curvature = angle.signum() / radius.abs();
                    for k in 1..=n {
                        let t = k as f64 / n as f64;
                        out.push(Sample {
                            segment: i,
                            s: s0 + len * t,
                            pose: arc.pose_at(angle * t),
                            curvature,
                            in_place: false,
                        });
                    }
                }
                SegmentKind::Hermite { to, t0, t1 } => {
                    let h = Hermite::new(pose, to, t0, t1, seg.reverse);
                    let table = h.arc_table();
                    let n = (len / step).ceil().max(1.) as usize;
                    let mut j = 1;
                    for k in 1..=n {
                        let target = len * k as f64 / n as f64;
                        while j < table.len() - 1 && table[j].1 < target {
                            j += 1;
                        }
                        let (ta, sa) = table[j - 1];
                        let (tb, sb) = table[j];
                        let t = if sb - sa > 1e-12 { ta + (tb - ta) * (target - sa) / (sb - sa) } else { tb };
                        out.push(Sample {
                            segment: i,
                            s: s0 + target,
                            pose: h.pose(t),
                            curvature: h.curvature(t),
                            in_place: false,
                        });
                    }
                }
            }
            s0 += len;
            pose = seg.end_pose(pose);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::vec::vec2;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn turn_in_place_keeps_position() {
        let start = Pose { x: 100., y: 200., theta: 0. };
        let end = Segment::turn(PI / 2., 0., false).end_pose(start);
        assert!(close(end.x, 100.) && close(end.y, 200.) && close(end.theta, PI / 2.));
        assert_eq!(Segment::turn(PI / 2., 0., false).length(start), 0.);
    }

    #[test]
    fn arc_turn_end_pose() {
        // Quarter circle left with r = 100 from the origin facing +x ends at (100, 100) facing +y.
        let start = Pose::default();
        let seg = Segment::turn(PI / 2., 100., false);
        let end = seg.end_pose(start);
        assert!(close(end.x, 100.) && close(end.y, 100.) && close(end.theta, PI / 2.), "{end:?}");
        assert!(close(seg.length(start), 50. * PI));

        // Right turn.
        let end = Segment::turn(-PI / 2., 100., false).end_pose(start);
        assert!(close(end.x, 100.) && close(end.y, -100.) && close(end.theta, -PI / 2.), "{end:?}");
    }

    #[test]
    fn reverse_arc_backs_up() {
        let start = Pose::default();
        let end = Segment::turn(PI / 2., 100., true).end_pose(start);
        // Travelling towards -x and turning left (CCW) curves towards -y.
        assert!(close(end.x, -100.) && close(end.y, -100.) && close(end.theta, PI / 2.), "{end:?}");
    }

    #[test]
    fn arc_through_point_hits_point() {
        let mut path = Path::new("p", Pose::default());
        path.push_arc_through(vec2(300., 200.), false);
        let end = path.end_pose();
        assert!(close(end.x, 300.) && close(end.y, 200.), "{end:?}");
    }

    #[test]
    fn hermite_endpoints_and_tangents() {
        let start = Pose { x: 0., y: 0., theta: 0. };
        let to = Pose { x: 500., y: 300., theta: PI / 2. };
        let h = Hermite::new(start, to, 400., 400., false);
        assert!(h.point(0.).distance(start.pos()) < 1e-9);
        assert!(h.point(1.).distance(to.pos()) < 1e-9);
        assert!(close(h.pose(0.).theta, 0.));
        assert!(close(h.pose(1.).theta, PI / 2.));
        // Chord is a lower bound on length.
        let len = Segment::hermite(to, 400., 400., false).length(start);
        assert!(len > start.pos().distance(to.pos()));
    }

    #[test]
    fn straight_discontinuity_and_auto_turn() {
        let mut path = Path::new("p", Pose::default());
        path.segments.push(Segment::straight(vec2(0., 100.), false));
        assert_eq!(path.discontinuities(), vec![0]);

        let mut path = Path::new("p", Pose::default());
        path.push_straight(vec2(0., 100.), false, true);
        assert_eq!(path.segments.len(), 2);
        assert!(path.discontinuities().is_empty());
        assert!(path.segments[0].is_in_place());

        // Move the straight's target: the auto turn follows.
        path.segments[1].kind = SegmentKind::Straight { to: vec2(-100., 0.) };
        assert_eq!(path.discontinuities(), vec![1]);
        path.fix_auto_turns();
        assert!(path.discontinuities().is_empty());
    }

    #[test]
    fn reverse_straight_needs_no_turn() {
        let mut path = Path::new("p", Pose::default());
        path.push_straight(vec2(-200., 0.), true, true);
        assert_eq!(path.segments.len(), 1);
        assert!(close(path.end_pose().theta, 0.));
    }

    #[test]
    fn sampling_is_monotonic_and_ends_at_end() {
        let mut path = Path::new("p", Pose::default());
        path.push_straight(vec2(500., 0.), false, true);
        path.push_arc_through(vec2(800., 300.), false);
        path.push_hermite(vec2(1200., 900.), false);
        path.push_straight(vec2(1200., 1500.), false, true);
        let samples = path.sample(10.);
        assert!(samples.windows(2).all(|w| w[1].s >= w[0].s - 1e-9));
        let last = samples.last().unwrap();
        assert!(last.pose.pos().distance(path.end_pose().pos()) < 1e-6);
        assert!(close(last.s, path.length()));
        // No sample is more than a step (plus slack) away from its predecessor.
        assert!(samples.windows(2).all(|w| w[0].pose.pos().distance(w[1].pose.pos()) <= 10.5));
    }

    #[test]
    fn serde_roundtrip() {
        let mut path = Path::new("p", Pose::default());
        path.push_straight(vec2(0., 100.), false, true);
        path.push_hermite(vec2(300., 300.), true);
        let json = serde_json::to_string(&path).unwrap();
        assert!(json.contains(r#""type":"turn""#) && json.contains(r#""auto":true"#));
        assert_eq!(serde_json::from_str::<Path>(&json).unwrap(), path);
    }
}
