//! Table view: world↔screen transform, hit-testing and painting.
//!
//! Screen-space math uses [`Vec2`] in logical pixels relative to the window,
//! converted to gpui points only when painting.

use std::f64::consts::{PI, TAU};

use gpui::{
    App, Bounds, Font, FontWeight, Hsla, PathBuilder, Pixels, Point, TextRun, Window, font, point, px, size,
};

use crate::geom::collision::Report;
use crate::geom::path::{Arc, Hermite};
use crate::geom::{Path, Sample, SegmentKind, Shape, Vec2, vec2};
use crate::model::{Doc, Origin, RegionColor};
use crate::theme::{FONT_MONO, Theme, alpha};

/// Handle hit radius (px).
pub const HIT_RADIUS: f64 = 9.;
const HEADING_KNOB: f64 = 52.;
const TURN_KNOB: f64 = 38.;

/// Pan/zoom state. `pan` is the screen offset of the table's top-left corner
/// from the canvas origin; `zoom` is pixels per millimetre.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct View {
    pub pan: Vec2,
    pub zoom: f64,
}

impl View {
    pub fn fit(table: Vec2, canvas: Vec2) -> Self {
        let pad = 48.;
        let zoom = ((canvas.x - 2. * pad) / table.x).min((canvas.y - 2. * pad) / table.y).max(0.01);
        let pan = (canvas - table * zoom) * 0.5;
        Self { pan, zoom }
    }

    /// Zooms by `factor` keeping the canvas-relative point `at` fixed.
    pub fn zoom_at(&mut self, factor: f64, at: Vec2) {
        let new = (self.zoom * factor).clamp(0.02, 20.);
        self.pan = at - (at - self.pan) * (new / self.zoom);
        self.zoom = new;
    }
}

/// Resolved transform for one frame.
#[derive(Clone, Copy, Debug)]
pub struct Xform {
    /// Screen position of the table's top-left corner.
    pub corner: Vec2,
    pub zoom: f64,
    pub height: f64,
    pub y_down: bool,
}

impl Xform {
    pub fn new(view: View, canvas_origin: Vec2, doc: &Doc) -> Self {
        Self {
            corner: canvas_origin + view.pan,
            zoom: view.zoom,
            height: doc.map.height,
            y_down: doc.map.origin == Origin::TopLeft,
        }
    }

    pub fn to_screen(self, p: Vec2) -> Vec2 {
        let y = if self.y_down { p.y } else { self.height - p.y };
        self.corner + vec2(p.x, y) * self.zoom
    }

    pub fn to_world(self, s: Vec2) -> Vec2 {
        let l = (s - self.corner) * (1. / self.zoom);
        vec2(l.x, if self.y_down { l.y } else { self.height - l.y })
    }

    /// Screen direction of a world angle.
    pub fn dir(self, angle: f64) -> Vec2 {
        let d = Vec2::from_angle(angle);
        if self.y_down { d } else { vec2(d.x, -d.y) }
    }
}

pub fn to_vec(p: Point<Pixels>) -> Vec2 {
    vec2(f32::from(p.x) as f64, f32::from(p.y) as f64)
}

fn pt(v: Vec2) -> Point<Pixels> {
    point(px(v.x as f32), px(v.y as f32))
}

/// Something on the active path that can be dragged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    /// Waypoint position: 0 is the start, `i + 1` the end of segment `i`.
    Pose(usize),
    StartHeading,
    /// Angle of turn in place `seg`.
    TurnKnob(usize),
    /// End of an arc turn: drags its angle.
    ArcEnd(usize),
    /// Middle of an arc turn: drags its radius.
    ArcRadius(usize),
    Tangent0(usize),
    Tangent1(usize),
}

/// Screen positions of all handles on `path`, highest priority first, each
/// with the anchor its guide line is drawn from.
pub fn handles(path: &Path, x: &Xform) -> Vec<(Handle, Vec2, Option<Vec2>)> {
    let poses = path.poses();
    let mut knobs = Vec::new();
    let mut points = Vec::new();

    let start = x.to_screen(path.start.pos());
    knobs.push((Handle::StartHeading, start + x.dir(path.start.theta) * HEADING_KNOB, Some(start)));
    points.push((Handle::Pose(0), start, None));

    for (i, seg) in path.segments.iter().enumerate() {
        let (from, to) = (poses[i], poses[i + 1]);
        let end = x.to_screen(to.pos());
        match seg.kind {
            SegmentKind::Turn { .. } if seg.is_in_place() => {
                knobs.push((Handle::TurnKnob(i), end + x.dir(to.theta) * TURN_KNOB, Some(end)));
            }
            SegmentKind::Turn { angle, radius, .. } => {
                let arc = Arc::new(from, angle, radius, seg.reverse);
                knobs.push((Handle::ArcRadius(i), x.to_screen(arc.pose_at(angle / 2.).pos()), None));
                points.push((Handle::ArcEnd(i), end, None));
            }
            SegmentKind::Hermite { to: target, t0, t1 } => {
                let h = Hermite::new(from, target, t0, t1, seg.reverse);
                let (c0, c1) = h.bezier_controls();
                knobs.push((Handle::Tangent0(i), x.to_screen(c0), Some(x.to_screen(h.p0))));
                knobs.push((Handle::Tangent1(i), x.to_screen(c1), Some(end)));
                points.push((Handle::Pose(i + 1), end, None));
            }
            SegmentKind::Straight { .. } => points.push((Handle::Pose(i + 1), end, None)),
        }
    }
    knobs.extend(points);
    knobs
}

pub fn hit_handle(path: &Path, x: &Xform, mouse: Vec2) -> Option<Handle> {
    handles(path, x).into_iter().find(|(_, p, _)| p.distance(mouse) <= HIT_RADIUS).map(|(h, _, _)| h)
}

/// Waypoint index a handle belongs to (for selection).
pub fn handle_waypoint(h: Handle) -> usize {
    match h {
        Handle::Pose(i) => i,
        Handle::StartHeading => 0,
        Handle::TurnKnob(s) | Handle::ArcEnd(s) | Handle::ArcRadius(s) | Handle::Tangent0(s) | Handle::Tangent1(s) => {
            s + 1
        }
    }
}

/// Distance (px) from `mouse` to a path's drawn line.
pub fn path_distance(samples: &[Sample], x: &Xform, mouse: Vec2) -> f64 {
    samples
        .windows(2)
        .map(|w| {
            crate::geom::collision::segment_point_distance(
                x.to_screen(w[0].pose.pos()),
                x.to_screen(w[1].pose.pos()),
                mouse,
            )
        })
        .fold(f64::INFINITY, f64::min)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Selection {
    #[default]
    None,
    Waypoint {
        path: usize,
        index: usize,
    },
    Obstacle(usize),
    Region(usize),
}

/// Transient overlay drawn above the document.
#[derive(Clone, Debug, Default)]
pub enum Preview {
    #[default]
    None,
    Shape { shape: Shape, region: Option<RegionColor> },
    Polygon { points: Vec<Vec2>, cursor: Option<Vec2>, region: Option<RegionColor> },
    Measure { from: Vec2, to: Vec2 },
    /// Segments that clicking would append to the active path.
    Ghost { path: Path, from: usize },
}

/// Everything the paint pass needs, cloned out of the workspace.
pub struct Frame {
    pub theme: Theme,
    pub doc: Doc,
    pub view: View,
    pub active_path: Option<usize>,
    pub selection: Selection,
    pub samples: Vec<Vec<Sample>>,
    pub report: Option<Report>,
    pub show_sweep: bool,
    pub preview: Preview,
}

pub fn region_color(theme: &Theme, c: RegionColor) -> gpui::Rgba {
    match c {
        RegionColor::Primary => theme.primary,
        RegionColor::Secondary => theme.secondary,
        RegionColor::Accent => theme.accent,
        RegionColor::Tertiary => theme.tertiary,
        RegionColor::Muted => theme.muted,
    }
}

struct Painter<'a> {
    window: &'a mut Window,
    cx: &'a mut App,
    x: Xform,
}

impl Painter<'_> {
    fn stroke(&mut self, pts: &[Vec2], width: f64, color: impl Into<Hsla>) {
        if pts.len() < 2 {
            return;
        }
        let mut b = PathBuilder::stroke(px(width as f32));
        b.move_to(pt(pts[0]));
        for &p in &pts[1..] {
            b.line_to(pt(p));
        }
        if let Ok(path) = b.build() {
            self.window.paint_path(path, color.into());
        }
    }

    fn stroke_closed(&mut self, pts: &[Vec2], width: f64, color: impl Into<Hsla>) {
        let mut closed = pts.to_vec();
        if let Some(&first) = pts.first() {
            closed.push(first);
        }
        self.stroke(&closed, width, color);
    }

    fn dashed(&mut self, pts: &[Vec2], width: f64, dash: f64, color: impl Into<Hsla> + Copy) {
        let mut on = true;
        let mut left = dash;
        let mut run = vec![];
        for w in pts.windows(2) {
            let (mut a, b) = (w[0], w[1]);
            if on && run.is_empty() {
                run.push(a);
            }
            let mut seg = b.distance(a);
            while seg > left {
                let p = a.lerp(b, left / seg);
                if on {
                    run.push(p);
                    self.stroke(&run, width, color);
                    run.clear();
                } else {
                    run.push(p);
                }
                on = !on;
                seg -= left;
                a = p;
                left = dash;
            }
            left -= seg;
            if on {
                run.push(b);
            }
        }
        if on {
            self.stroke(&run, width, color);
        }
    }

    fn fill(&mut self, pts: &[Vec2], color: impl Into<Hsla>) {
        if pts.len() < 3 {
            return;
        }
        let mut b = PathBuilder::fill();
        b.move_to(pt(pts[0]));
        for &p in &pts[1..] {
            b.line_to(pt(p));
        }
        b.close();
        if let Ok(path) = b.build() {
            self.window.paint_path(path, color.into());
        }
    }

    fn square(&mut self, c: Vec2, half: f64, fill: impl Into<Hsla>, border: impl Into<Hsla>, border_w: f64) {
        let bounds = Bounds::new(pt(c - vec2(half, half)), size(px(2. * half as f32), px(2. * half as f32)));
        self.window.paint_quad(gpui::quad(
            bounds,
            px(0.),
            fill.into(),
            px(border_w as f32),
            border.into(),
            gpui::BorderStyle::Solid,
        ));
    }

    fn text(&mut self, s: &str, at: Vec2, size_px: f64, color: impl Into<Hsla>, font: Font, center: bool) {
        if s.is_empty() {
            return;
        }
        let run = TextRun {
            len: s.len(),
            font,
            color: color.into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let line = self.window.text_system().shape_line(s.to_string().into(), px(size_px as f32), &[run], None);
        let w = f32::from(line.width) as f64;
        let origin = if center { at - vec2(w / 2., size_px * 0.62) } else { at - vec2(0., size_px * 0.62) };
        line.paint(pt(origin), px((size_px * 1.25) as f32), self.window, self.cx).ok();
    }

    fn shape_points(&self, shape: &Shape) -> Vec<Vec2> {
        match shape {
            Shape::Circle { center, radius } => circle_points(*center, *radius, 48),
            _ => shape.vertices().unwrap_or_default(),
        }
        .into_iter()
        .map(|p| self.x.to_screen(p))
        .collect()
    }
}

fn mono(weight: FontWeight) -> Font {
    Font { weight, ..font(FONT_MONO) }
}

fn circle_points(c: Vec2, r: f64, n: usize) -> Vec<Vec2> {
    (0..n).map(|i| c + Vec2::from_angle(TAU * i as f64 / n as f64) * r).collect()
}

/// Screen polyline for every segment of `samples` (turns in place are skipped).
fn segment_polylines(samples: &[Sample], segments: usize, x: &Xform) -> Vec<Vec<Vec2>> {
    let mut lines = vec![Vec::new(); segments];
    for w in samples.windows(2) {
        let (a, b) = (w[0], w[1]);
        if b.in_place {
            continue;
        }
        let line = &mut lines[b.segment];
        if line.is_empty() {
            line.push(x.to_screen(a.pose.pos()));
        }
        line.push(x.to_screen(b.pose.pos()));
    }
    lines
}

pub fn format_mm(v: f64) -> String {
    if (v - v.round()).abs() < 0.05 { format!("{}", v.round() as i64) } else { format!("{v:.1}") }
}

pub fn paint(frame: &Frame, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
    if frame.view.zoom <= 0. {
        return;
    }
    let t = frame.theme;
    let x = Xform::new(frame.view, to_vec(bounds.origin), &frame.doc);
    let map = &frame.doc.map;
    let table = [vec2(0., 0.), vec2(map.width, 0.), vec2(map.width, map.height), vec2(0., map.height)];

    window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
        let mut p = Painter { window, cx, x };

        // Grid: 100 mm minor, 500 mm major lines.
        let minor = alpha(t.foreground, if map.background.is_some() { 0.10 } else { 0.07 });
        let major = alpha(t.foreground, 0.18);
        for (step, color) in [(100., minor), (500., major)] {
            if step * x.zoom < 6. {
                continue;
            }
            let mut v = step;
            while v < map.width - 1e-6 {
                p.stroke(&[x.to_screen(vec2(v, 0.)), x.to_screen(vec2(v, map.height))], 1., color);
                v += step;
            }
            let mut v = step;
            while v < map.height - 1e-6 {
                p.stroke(&[x.to_screen(vec2(0., v)), x.to_screen(vec2(map.width, v))], 1., color);
                v += step;
            }
        }

        // Ruler labels every 500 mm along the outside of the table.
        let label = mono(FontWeight::NORMAL);
        let mut v = 0.;
        while v <= map.width + 1e-6 {
            let s = x.to_screen(vec2(v, if x.y_down { 0. } else { map.height }));
            p.text(&format_mm(v), s - vec2(0., 16.), 10., t.ink_muted, label.clone(), true);
            v += 500.;
        }
        let mut v = 0.;
        while v <= map.height + 1e-6 {
            let s = x.to_screen(vec2(0., v));
            p.text(&format_mm(v), s - vec2(34., 0.), 10., t.ink_muted, label.clone(), false);
            v += 500.;
        }

        // Regions.
        for (i, region) in map.regions.iter().enumerate() {
            let c = region_color(&t, region.color);
            let pts = p.shape_points(&region.shape);
            p.fill(&pts, alpha(c, 0.28));
            let selected = frame.selection == Selection::Region(i);
            p.stroke_closed(&pts, if selected { 4. } else { 2. }, if selected { t.foreground.into() } else { alpha(c, 1.) });
            let center = x.to_screen(region.shape.center());
            p.text(&region.name.to_uppercase(), center, 11., t.foreground, mono(FontWeight::BOLD), true);
        }
        // Swept footprint and collisions of the active path.
        if let (Some(ai), true) = (frame.active_path, frame.show_sweep)
            && let (Some(path), Some(samples)) = (frame.doc.paths.get(ai), frame.samples.get(ai))
        {
            let robot = &frame.doc.robot;
            let colliding = frame.report.as_ref().map(|r| r.colliding.as_slice()).unwrap_or(&[]);
            let ghost = alpha(t.ink_muted, 0.35);
            let mut last: Option<Sample> = None;
            for (k, s) in samples.iter().enumerate() {
                let hit = colliding.get(k).copied().unwrap_or(false);
                let due = last.is_none_or(|l| {
                    (s.s - l.s) * x.zoom >= 14. || (s.in_place && (s.pose.theta - l.pose.theta).abs() >= PI / 12.)
                });
                if !due {
                    continue;
                }
                let body: Vec<Vec2> = robot.corners(s.pose).iter().map(|&c| x.to_screen(c)).collect();
                if hit {
                    p.fill(&body, alpha(t.primary, 0.10));
                    p.stroke_closed(&body, 1.5, alpha(t.primary, 0.85));
                } else {
                    p.stroke_closed(&body, 1., ghost);
                }
                last = Some(*s);
            }
            // Turn-in-place envelopes.
            let poses = path.poses();
            for (i, seg) in path.segments.iter().enumerate() {
                if seg.is_in_place() {
                    let c = circle_points(poses[i].pos(), robot.sweep_radius(), 64);
                    let mut pts: Vec<Vec2> = c.iter().map(|&q| x.to_screen(q)).collect();
                    pts.push(pts[0]);
                    p.dashed(&pts, 1., 5., alpha(t.tertiary_ink, 0.7));
                }
            }
        }

        // Obstacles above the sweep so collisions never hide them.
        for (i, obstacle) in map.obstacles.iter().enumerate() {
            let pts = p.shape_points(&obstacle.shape);
            p.fill(&pts, alpha(t.foreground, 0.22));
            let selected = frame.selection == Selection::Obstacle(i);
            p.stroke_closed(&pts, if selected { 4. } else { 2. }, if selected { t.primary } else { t.foreground });
            let center = x.to_screen(obstacle.shape.center());
            p.text(&obstacle.name.to_uppercase(), center, 11., t.foreground, mono(FontWeight::BOLD), true);
        }

        // Paths: inactive first, active on top.
        let order: Vec<usize> = (0..frame.doc.paths.len())
            .filter(|&i| Some(i) != frame.active_path)
            .chain(frame.active_path)
            .collect();
        for i in order {
            let (Some(path), Some(samples)) = (frame.doc.paths.get(i), frame.samples.get(i)) else {
                continue;
            };
            let active = Some(i) == frame.active_path;
            let lines = segment_polylines(samples, path.segments.len(), &x);
            let selected_seg = match frame.selection {
                Selection::Waypoint { path: sp, index } if sp == i && index > 0 => Some(index - 1),
                _ => None,
            };
            for (si, (line, seg)) in lines.iter().zip(&path.segments).enumerate() {
                let (w, color): (f64, Hsla) = if !active {
                    (2., alpha(t.muted, 0.9))
                } else if Some(si) == selected_seg {
                    (4., t.primary.into())
                } else {
                    (3., t.foreground.into())
                };
                if seg.reverse {
                    p.dashed(line, w, 9., color);
                } else {
                    p.stroke(line, w, color);
                }
            }
            if !active {
                continue;
            }

            let poses = path.poses();
            // Heading discontinuities.
            for si in path.discontinuities() {
                let c = x.to_screen(poses[si].pos());
                p.stroke(&[c - vec2(9., 9.), c + vec2(9., 9.)], 3., t.primary);
                p.stroke(&[c - vec2(9., -9.), c + vec2(9., -9.)], 3., t.primary);
            }
            // Turn-in-place glyphs.
            for (si, seg) in path.segments.iter().enumerate() {
                if let SegmentKind::Turn { angle, .. } = seg.kind
                    && seg.is_in_place()
                {
                    let c = x.to_screen(poses[si].pos());
                    let from = poses[si].theta;
                    let n = ((angle.abs() / (PI / 24.)).ceil() as usize).max(2);
                    let r = 24.;
                    let arc: Vec<Vec2> = (0..=n).map(|k| c + x.dir(from + angle * k as f64 / n as f64) * r).collect();
                    let color = if Some(si) == selected_seg { t.primary } else { t.tertiary_ink };
                    p.stroke(&arc, 2.5, color);
                    // Arrow head at the end of the sweep.
                    let tip = *arc.last().unwrap();
                    let tangent = x.dir(from + angle + angle.signum() * PI / 2.);
                    let back = tip - tangent * 8.;
                    let side = tangent.perp() * 4.;
                    p.fill(&[tip, back + side, back - side], color);
                    let mid = c + x.dir(from + angle / 2.) * (r + 14.);
                    p.text(&format!("{:.0}°", angle.to_degrees()), mid, 10., color, mono(FontWeight::BOLD), true);
                }
            }
            // Body heading ticks at each waypoint.
            for pose in &poses {
                let c = x.to_screen(pose.pos());
                p.stroke(&[c, c + x.dir(pose.theta) * 18.], 2., t.foreground);
            }
            // Robot outline at the selected waypoint.
            if let Selection::Waypoint { path: sp, index } = frame.selection
                && sp == i
                && let Some(pose) = poses.get(index)
            {
                let body: Vec<Vec2> = frame.doc.robot.corners(*pose).iter().map(|&c| x.to_screen(c)).collect();
                p.stroke_closed(&body, 2.5, t.tertiary_ink);
                let front = x.to_screen(pose.to_world(vec2(frame.doc.robot.length / 2. + frame.doc.robot.offset.x, 0.)));
                p.stroke(&[x.to_screen(pose.pos()), front], 2.5, t.tertiary_ink);
            }

            // Handles.
            let selected_wp = match frame.selection {
                Selection::Waypoint { path: sp, index } if sp == i => Some(index),
                _ => None,
            };
            for (h, at, anchor) in handles(path, &x) {
                if let Some(a) = anchor {
                    p.stroke(&[a, at], 1.5, t.ink_muted);
                }
                match h {
                    Handle::Pose(_) | Handle::ArcEnd(_) => {
                        let idx = handle_waypoint(h);
                        let selected = selected_wp == Some(idx);
                        let fill: Hsla = if selected { t.primary.into() } else { t.background.into() };
                        p.square(at, 7., fill, t.foreground, 2.);
                        let num = format!("{idx}");
                        p.text(&num, at + vec2(14., -12.), 10., t.foreground, mono(FontWeight::BOLD), false);
                    }
                    _ => {
                        p.square(at, 4.5, t.secondary, t.foreground, 1.5);
                    }
                }
            }
        }

        // Transient preview.
        let preview_color = |region: &Option<RegionColor>| -> Hsla {
            region.map_or(t.foreground.into(), |c| alpha(region_color(&t, c), 1.))
        };
        match &frame.preview {
            Preview::None => {}
            Preview::Shape { shape, region } => {
                let mut pts = p.shape_points(shape);
                if let Some(&f) = pts.first() {
                    pts.push(f);
                }
                p.dashed(&pts, 2., 6., preview_color(region));
            }
            Preview::Polygon { points, cursor, region } => {
                let mut pts: Vec<Vec2> = points.iter().map(|&q| x.to_screen(q)).collect();
                if let Some(c) = cursor {
                    pts.push(x.to_screen(*c));
                }
                let color = preview_color(region);
                p.dashed(&pts, 2., 6., color);
                for q in points {
                    p.square(x.to_screen(*q), 4., t.background, color, 2.);
                }
            }
            Preview::Measure { from, to } => {
                let (a, b) = (x.to_screen(*from), x.to_screen(*to));
                p.stroke(&[a, b], 2., t.primary);
                p.square(a, 3., t.primary, t.primary, 0.);
                p.square(b, 3., t.primary, t.primary, 0.);
                let d = *to - *from;
                let label = format!("{} MM  {:.1}°", format_mm(d.length()), d.angle().to_degrees());
                p.text(&label, b + vec2(12., 14.), 11., t.primary, mono(FontWeight::BOLD), false);
            }
            Preview::Ghost { path, from } => {
                let samples = path.sample(4. / x.zoom);
                let lines = segment_polylines(&samples, path.segments.len(), &x);
                for line in lines.iter().skip(*from) {
                    p.dashed(line, 2., 6., alpha(t.foreground, 0.55));
                }
                if let Some(end) = path.poses().last() {
                    p.square(x.to_screen(end.pos()), 5., alpha(t.background, 0.6), alpha(t.foreground, 0.6), 2.);
                }
            }
        }

        // Table border.
        let mut border: Vec<Vec2> = table.iter().map(|&c| x.to_screen(c)).collect();
        border.push(border[0]);
        p.stroke(&border, 4., t.border);

        // Origin axes.
        let o = x.to_screen(Vec2::ZERO);
        p.stroke(&[o, o + x.dir(0.) * 56.], 3., t.primary);
        p.stroke(&[o, o + x.dir(PI / 2.) * 56.], 3., t.tertiary_ink);
        p.text("X", o + x.dir(0.) * 66., 10., t.primary, mono(FontWeight::BOLD), true);
        p.text("Y", o + x.dir(PI / 2.) * 66., 10., t.tertiary_ink, mono(FontWeight::BOLD), true);
    });
}
