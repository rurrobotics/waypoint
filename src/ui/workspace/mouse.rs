use std::f64::consts::{PI, TAU};

use gpui::{Context, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ScrollWheelEvent, Window, px};

use super::{Drag, FinishShape, Layer, Pending, SegTool, ShapeTool, Workspace};
use crate::geom::path::{Arc, body_heading, travel_heading};
use crate::geom::shape::is_convex;
use crate::geom::{Path, Pose, SegmentKind, Shape, Vec2, vec2};
use crate::model::{Obstacle, Region, RegionColor};
use crate::ui::canvas::{self, HIT_RADIUS, Handle, Preview, Selection, Xform};
use crate::ui::dock::Side;
use crate::ui::fields::Unit;

const ANGLE_SNAP: f64 = PI / 12.;

fn snap_angle(a: f64) -> f64 {
    (a / ANGLE_SNAP).round() * ANGLE_SNAP
}

impl Workspace {
    /// Grid snap (Alt disables).
    fn snap_point(&self, p: Vec2, m: &Modifiers) -> Vec2 {
        if self.snap <= 0. || m.alt {
            return p;
        }
        vec2((p.x / self.snap).round() * self.snap, (p.y / self.snap).round() * self.snap)
    }

    /// Where the waypoint tool would place a point: Shift snaps the direction
    /// from the current end to 15° steps, otherwise grid snap.
    fn waypoint_target(&self, world: Vec2, m: &Modifiers) -> Vec2 {
        if m.shift
            && let Some(path) = self.active()
        {
            let end = path.end_pose().pos();
            let d = world - end;
            return end + Vec2::from_angle(snap_angle(d.angle())) * d.length();
        }
        self.snap_point(world, m)
    }

    /// End of a measurement: Shift keeps it horizontal, vertical or diagonal
    /// (45° steps) by projecting the pointer onto the nearest such line;
    /// otherwise grid snap.
    fn measure_end(&self, from: Vec2, world: Vec2, m: &Modifiers) -> Vec2 {
        if !m.shift {
            return self.snap_point(world, m);
        }
        let d = world - from;
        let step = PI / 4.;
        let dir = Vec2::from_angle((d.angle() / step).round() * step);
        from + dir * d.dot(dir)
    }

    fn append(&self, path: &mut Path, target: Vec2) {
        match self.seg_tool {
            SegTool::Straight => path.push_straight(target, self.reverse, self.auto_turn),
            SegTool::TurnInPlace => path.push_turn_towards(target, self.reverse),
            SegTool::TurnArc => path.push_arc_through(target, self.reverse),
            SegTool::Hermite => path.push_hermite(target, self.reverse),
        }
    }

    pub(super) fn canvas_mouse_down(&mut self, e: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.canvas_focus);
        self.status = None;
        let Some(x) = self.xform() else { return };
        let mouse = canvas::to_vec(e.position);
        let world = x.to_world(mouse);

        if matches!(e.button, MouseButton::Middle | MouseButton::Right) {
            self.drag = Drag::Pan { last: mouse };
            return;
        }
        if e.button != MouseButton::Left {
            return;
        }
        self.measure = None;

        // Handles on the active path win unless a shape or measurement is armed.
        if matches!(self.pending, None | Some(Pending::DrawPath))
            && let Some(ai) = self.active_path
            && let Some(h) = canvas::hit_handle(&self.doc.paths[ai], &x, mouse)
        {
            self.selection = Selection::Waypoint { path: ai, index: canvas::handle_waypoint(h) };
            self.begin_drag_edit();
            self.drag = Drag::Handle { path: ai, handle: h };
            cx.notify();
            return;
        }

        match self.pending {
            None => self.select_at(world, mouse, &x),
            // The first click of a double-click already added the last waypoint.
            Some(Pending::DrawPath) if e.click_count >= 2 => self.pending = None,
            Some(Pending::DrawPath) => self.place_waypoint(world, &e.modifiers),
            Some(Pending::Shape { kind, .. }) => {
                let p = self.snap_point(world, &e.modifiers);
                match kind {
                    ShapeTool::Polygon => self.polygon_click(p, mouse, e.click_count, &x),
                    _ => self.drag = Drag::Create { start: p, current: p },
                }
            }
            Some(Pending::Measure) => {
                let p = self.snap_point(world, &e.modifiers);
                self.measure = Some((p, p));
                self.drag = Drag::Measure;
            }
        }
        self.refresh(cx);
    }

    fn select_at(&mut self, world: Vec2, mouse: Vec2, x: &Xform) {
        // Waypoints of other paths, then any path line.
        for (i, path) in self.doc.paths.iter().enumerate() {
            if let Some(h) = canvas::hit_handle(path, x, mouse)
                && matches!(h, Handle::Pose(_) | Handle::ArcEnd(_))
            {
                self.active_path = Some(i);
                self.selection = Selection::Waypoint { path: i, index: canvas::handle_waypoint(h) };
                return;
            }
        }
        let samples = &self.ensure_analysis().samples;
        let near = samples
            .iter()
            .enumerate()
            .map(|(i, s)| (i, canvas::path_distance(s, x, mouse)))
            .filter(|&(_, d)| d < 6.)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((i, _)) = near {
            self.active_path = Some(i);
            self.selection = Selection::Path(i);
            return;
        }

        let picked = if let Some(i) = self.doc.map.obstacles.iter().rposition(|o| o.shape.contains(world)) {
            Selection::Obstacle(i)
        } else if let Some(i) = self.doc.map.regions.iter().rposition(|r| r.shape.contains(world)) {
            Selection::Region(i)
        } else {
            Selection::None
        };
        self.selection = picked;
        if picked != Selection::None {
            self.begin_drag_edit();
            self.drag = Drag::Object { last: self.snap_point(world, &self.modifiers) };
        }
    }

    fn place_waypoint(&mut self, world: Vec2, m: &Modifiers) {
        let target = self.waypoint_target(world, m);
        let before = self.doc.clone();
        match self.active_path {
            None => {
                let n = self.doc.paths.len() + 1;
                self.doc.paths.push(Path::new(format!("Path {n}"), Pose::new(target, 0.)));
                let i = self.doc.paths.len() - 1;
                self.active_path = Some(i);
                self.selection = Selection::Waypoint { path: i, index: 0 };
            }
            Some(ai) => {
                let mut path = self.doc.paths[ai].clone();
                self.append(&mut path, target);
                let index = path.segments.len();
                self.doc.paths[ai] = path;
                self.selection = Selection::Waypoint { path: ai, index };
            }
        }
        if self.doc != before {
            self.history.push(before);
            self.dirty = true;
        }
    }

    fn polygon_click(&mut self, p: Vec2, mouse: Vec2, clicks: usize, x: &Xform) {
        let closes = self.polygon.len() >= 3
            && (clicks >= 2 || x.to_screen(self.polygon[0]).distance(mouse) <= HIT_RADIUS);
        if closes {
            self.finish_polygon();
            self.pending = None;
        } else if self.polygon.last() != Some(&p) {
            self.polygon.push(p);
        }
    }

    pub(super) fn finish_shape(&mut self, _: &FinishShape, _: &mut Window, cx: &mut Context<Self>) {
        self.finish_pending(cx);
    }

    pub(super) fn finish_polygon(&mut self) {
        let points = std::mem::take(&mut self.polygon);
        if points.len() < 3 {
            return;
        }
        if !is_convex(&points) {
            self.set_status("Polygon must be convex — split it into convex parts", true);
            return;
        }
        self.add_shape(Shape::Polygon { points });
    }

    /// Colour for the next region: cycles through the palette.
    fn next_region_color(&self) -> RegionColor {
        RegionColor::ALL[self.doc.map.regions.len() % RegionColor::ALL.len()]
    }

    fn pending_layer(&self) -> Option<Layer> {
        match self.pending {
            Some(Pending::Shape { layer, .. }) => Some(layer),
            _ => None,
        }
    }

    fn add_shape(&mut self, shape: Shape) {
        let before = self.doc.clone();
        let color = self.next_region_color();
        let region = self.pending_layer() == Some(Layer::Region);
        let map = &mut self.doc.map;
        if region {
            let n = map.regions.len() + 1;
            map.regions.push(Region { name: format!("Region {n}"), color, shape });
            self.selection = Selection::Region(map.regions.len() - 1);
        } else {
            let n = map.obstacles.len() + 1;
            map.obstacles.push(Obstacle { name: format!("Obstacle {n}"), shape });
            self.selection = Selection::Obstacle(map.obstacles.len() - 1);
        }
        self.history.push(before);
        self.dirty = true;
    }

    fn created_shape(&self, start: Vec2, current: Vec2) -> Option<Shape> {
        let Some(Pending::Shape { kind, .. }) = self.pending else { return None };
        match kind {
            ShapeTool::Rect => {
                let mut d = current - start;
                // Shift makes a square with the larger side.
                if self.modifiers.shift {
                    let side = d.x.abs().max(d.y.abs());
                    d = vec2(side.copysign(d.x), side.copysign(d.y));
                }
                (d.x.abs() > 1. && d.y.abs() > 1.).then(|| Shape::rect_from_corners(start, start + d))
            }
            ShapeTool::Circle => {
                let r = start.distance(current);
                (r > 1.).then_some(Shape::Circle { center: start, radius: r })
            }
            ShapeTool::Polygon => None,
        }
    }

    pub(super) fn mouse_move(&mut self, e: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.modifiers = e.modifiers;
        let mouse = canvas::to_vec(e.position);
        let x = self.xform();
        let over_canvas = self.canvas_bounds.get().is_some_and(|b| b.contains(&e.position));
        self.hover = x.filter(|_| over_canvas).map(|x| x.to_world(mouse));

        // Released outside the window: finish whatever was going on.
        if e.pressed_button.is_none() && !matches!(self.drag, Drag::None) {
            self.finish_drag(cx);
            cx.notify();
            return;
        }

        match self.drag {
            Drag::None => {}
            Drag::Pan { last } => {
                self.view.pan += mouse - last;
                self.auto_fit = false;
                self.drag = Drag::Pan { last: mouse };
            }
            Drag::Handle { path, handle } => {
                if let Some(x) = x {
                    self.drag_handle(path, handle, x.to_world(mouse), &e.modifiers);
                    self.refresh(cx);
                }
            }
            Drag::Object { last } => {
                if let Some(x) = x {
                    let p = self.snap_point(x.to_world(mouse), &e.modifiers);
                    let d = p - last;
                    if d != Vec2::ZERO {
                        match self.selection {
                            Selection::Obstacle(i) => self.doc.map.obstacles[i].shape.translate(d),
                            Selection::Region(i) => self.doc.map.regions[i].shape.translate(d),
                            _ => {}
                        }
                        self.drag = Drag::Object { last: p };
                        self.refresh(cx);
                    }
                }
            }
            Drag::Create { start, .. } => {
                if let Some(x) = x {
                    let current = self.snap_point(x.to_world(mouse), &e.modifiers);
                    self.drag = Drag::Create { start, current };
                }
            }
            Drag::Measure => {
                if let (Some(x), Some((from, _))) = (x, self.measure) {
                    self.measure = Some((from, self.measure_end(from, x.to_world(mouse), &e.modifiers)));
                }
            }
            Drag::DockWidth { side, start_x, start } => {
                let dx = (mouse.x - start_x) as f32;
                self.layout.set_width(side, if side == Side::Left { start + dx } else { start - dx });
            }
            Drag::DockSplit { side, index, start_y, start } => {
                let height = self.canvas_bounds.get().map_or(600., |b| f32::from(b.size.height));
                self.layout.resize_split(side, index, start, (mouse.y - start_y) as f32 / height.max(1.));
            }
            Drag::Scrub { field, start_x, start } => {
                let mut k = field.step();
                if e.modifiers.shift {
                    k *= 10.;
                }
                if e.modifiers.alt {
                    k /= 10.;
                }
                let v = start + ((mouse.x - start_x) / 2.).round() * k;
                self.set_number(field, v);
                self.refresh(cx);
            }
        }
        cx.notify();
    }

    pub(super) fn mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.finish_drag(cx);
        cx.notify();
    }

    fn finish_drag(&mut self, cx: &mut Context<Self>) {
        match std::mem::replace(&mut self.drag, Drag::None) {
            Drag::Create { start, current } => {
                if let Some(shape) = self.created_shape(start, current) {
                    self.add_shape(shape);
                    self.pending = None;
                    self.refresh(cx);
                }
            }
            Drag::Measure => self.pending = None,
            Drag::Handle { .. } | Drag::Object { .. } | Drag::Scrub { .. } => {
                self.end_drag_edit();
            }
            Drag::DockWidth { .. } | Drag::DockSplit { .. } => self.layout.save(),
            Drag::None | Drag::Pan { .. } => {}
        }
    }

    pub(super) fn start_scrub(&mut self, field: crate::ui::fields::Field, x: f64) {
        if field.unit() == Unit::Text {
            return;
        }
        if let Some(start) = self.number(field) {
            self.begin_drag_edit();
            self.drag = Drag::Scrub { field, start_x: x, start };
        }
    }

    pub(super) fn scroll(&mut self, e: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(b) = self.canvas_bounds.get() else { return };
        let dy = f32::from(e.delta.pixel_delta(px(20.)).y) as f64;
        if dy == 0. {
            return;
        }
        let at = canvas::to_vec(e.position) - canvas::to_vec(b.origin);
        self.view.zoom_at((dy * 0.004).exp(), at);
        self.auto_fit = false;
        cx.notify();
    }

    fn drag_handle(&mut self, ai: usize, h: Handle, world: Vec2, m: &Modifiers) {
        let snapped = self.snap_point(world, m);
        let path = &mut self.doc.paths[ai];
        let poses = path.poses();
        let heading = |a: f64| if m.shift { snap_angle(a) } else { a };
        match h {
            Handle::Pose(0) => path.start.set_pos(snapped),
            Handle::Pose(i) => match &mut path.segments[i - 1].kind {
                SegmentKind::Straight { to } => *to = snapped,
                SegmentKind::Hermite { to, .. } => to.set_pos(snapped),
                SegmentKind::Turn { .. } => {}
            },
            Handle::StartHeading => {
                path.start.theta = heading((world - path.start.pos()).angle());
            }
            Handle::TurnKnob(s) => {
                let from = poses[s];
                if let SegmentKind::Turn { angle, auto, .. } = &mut path.segments[s].kind {
                    let target = heading((world - from.pos()).angle());
                    *angle = crate::geom::vec::wrap_angle(target - from.theta);
                    *auto = false;
                }
            }
            Handle::ArcEnd(s) => {
                let reverse = path.segments[s].reverse;
                if let SegmentKind::Turn { angle, radius, .. } = &mut path.segments[s].kind {
                    let arc = Arc::new(poses[s], *angle, *radius, reverse);
                    let v0 = poses[s].pos() - arc.center;
                    let v1 = world - arc.center;
                    let delta = v0.cross(v1).atan2(v0.dot(v1));
                    let mut a = if *angle >= 0. { delta.rem_euclid(TAU) } else { -(-delta).rem_euclid(TAU) };
                    if m.shift {
                        a = snap_angle(a);
                    }
                    if a.abs() > 1e-3 {
                        *angle = a;
                    }
                }
            }
            Handle::ArcRadius(s) => {
                if let SegmentKind::Turn { angle, radius, .. } = &mut path.segments[s].kind {
                    let q = (*angle).abs() / 4.;
                    if q.sin() > 1e-3 {
                        let mut r = world.distance(poses[s].pos()) / (2. * q.sin());
                        if self.snap > 0. && !m.alt {
                            r = (r / self.snap).round().max(1.) * self.snap;
                        }
                        *radius = r.max(1.);
                    }
                }
            }
            Handle::Tangent0(s) => {
                let reverse = path.segments[s].reverse;
                if let SegmentKind::Hermite { t0, .. } = &mut path.segments[s].kind {
                    let dir = Vec2::from_angle(travel_heading(poses[s].theta, reverse));
                    *t0 = ((world - poses[s].pos()).dot(dir) * 3.).max(0.);
                }
            }
            Handle::Tangent1(s) => {
                let reverse = path.segments[s].reverse;
                if let SegmentKind::Hermite { to, t1, .. } = &mut path.segments[s].kind {
                    let v = to.pos() - world;
                    *t1 = v.length() * 3.;
                    to.theta = body_heading(heading(v.angle()), reverse);
                }
            }
        }
    }

    /// Overlay for the current tool and hover position.
    pub(super) fn preview(&self) -> Preview {
        if let Drag::Create { start, current } = self.drag {
            return self
                .created_shape(start, current)
                .map_or(Preview::None, |shape| Preview::Shape { shape, region: self.region_preview() });
        }
        if let Some((from, to)) = self.measure {
            return Preview::Measure { from, to };
        }
        if !self.polygon.is_empty() {
            return Preview::Polygon {
                points: self.polygon.clone(),
                cursor: self.hover.map(|h| self.snap_point(h, &self.modifiers)),
                region: self.region_preview(),
            };
        }
        if self.pending == Some(Pending::DrawPath)
            && matches!(self.drag, Drag::None)
            && let (Some(hover), Some(path)) = (self.hover, self.active())
        {
            let mut ghost = path.clone();
            let from = ghost.segments.len();
            self.append(&mut ghost, self.waypoint_target(hover, &self.modifiers));
            if ghost.segments.len() > from {
                return Preview::Ghost { path: ghost, from };
            }
        }
        Preview::None
    }

    fn region_preview(&self) -> Option<RegionColor> {
        (self.pending_layer() == Some(Layer::Region)).then(|| self.next_region_color())
    }
}
