use gpui::{Context, Div, FontWeight, KeyDownEvent, MouseButton, MouseDownEvent, Window, div, prelude::*, px};

use super::Workspace;
use crate::geom::path::{body_heading, travel_heading};
use crate::geom::{Path, Pose, Segment, SegmentKind, Shape, Vec2, vec2};
use crate::model::{Origin, RegionColor};
use crate::theme::{FONT_MONO, Theme};
use crate::ui::canvas::{self, Selection};
use crate::ui::fields::{Edit, Field, Outcome, Unit, parse_number};
use crate::ui::widgets::{button, chip, mono_label, section, section_title};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Straight,
    Turn,
    Hermite,
}

impl Workspace {
    // ---- field values ----------------------------------------------------------

    /// The selected waypoint's path, index and (for index > 0) segment.
    fn selected_waypoint(&self) -> Option<(usize, usize)> {
        match self.selection {
            Selection::Waypoint { path, index } if path < self.doc.paths.len() => Some((path, index)),
            _ => None,
        }
    }

    fn selected_shape(&self) -> Option<&Shape> {
        match self.selection {
            Selection::Obstacle(i) => self.doc.map.obstacles.get(i).map(|o| &o.shape),
            Selection::Region(i) => self.doc.map.regions.get(i).map(|r| &r.shape),
            _ => None,
        }
    }

    fn selected_shape_mut(&mut self) -> Option<&mut Shape> {
        match self.selection {
            Selection::Obstacle(i) => self.doc.map.obstacles.get_mut(i).map(|o| &mut o.shape),
            Selection::Region(i) => self.doc.map.regions.get_mut(i).map(|r| &mut r.shape),
            _ => None,
        }
    }

    /// Current value of a numeric field, in display units (degrees for angles).
    pub(super) fn number(&self, f: Field) -> Option<f64> {
        let d = &self.doc;
        let deg = f64::to_degrees;
        Some(match f {
            Field::PoseX | Field::PoseY | Field::PoseTheta => {
                let (pi, idx) = self.selected_waypoint()?;
                let path = &d.paths[pi];
                let pose = if idx == 0 {
                    path.start
                } else {
                    match path.segments.get(idx - 1)?.kind {
                        SegmentKind::Straight { to } => Pose::new(to, 0.),
                        SegmentKind::Hermite { to, .. } => to,
                        SegmentKind::Turn { .. } => return None,
                    }
                };
                match f {
                    Field::PoseX => pose.x,
                    Field::PoseY => pose.y,
                    _ => deg(pose.theta),
                }
            }
            Field::TurnAngle | Field::TurnRadius => {
                let (pi, idx) = self.selected_waypoint()?;
                let SegmentKind::Turn { angle, radius, .. } = d.paths[pi].segments.get(idx.checked_sub(1)?)?.kind
                else {
                    return None;
                };
                if f == Field::TurnAngle { deg(angle) } else { radius }
            }
            Field::HermiteT0 | Field::HermiteT1 => {
                let (pi, idx) = self.selected_waypoint()?;
                let SegmentKind::Hermite { t0, t1, .. } = d.paths[pi].segments.get(idx.checked_sub(1)?)?.kind else {
                    return None;
                };
                if f == Field::HermiteT0 { t0 } else { t1 }
            }
            Field::ShapeCx | Field::ShapeCy => {
                let c = self.selected_shape()?.center();
                if f == Field::ShapeCx { c.x } else { c.y }
            }
            Field::ShapeW | Field::ShapeH | Field::ShapeRotation => match self.selected_shape()? {
                Shape::Rect { size, rotation, .. } => match f {
                    Field::ShapeW => size.x,
                    Field::ShapeH => size.y,
                    _ => deg(*rotation),
                },
                _ => return None,
            },
            Field::ShapeRadius => match self.selected_shape()? {
                Shape::Circle { radius, .. } => *radius,
                _ => return None,
            },
            Field::RobotLength => d.robot.length,
            Field::RobotWidth => d.robot.width,
            Field::RobotOffsetX => d.robot.offset.x,
            Field::RobotOffsetY => d.robot.offset.y,
            Field::RobotMargin => d.robot.margin,
            Field::MapYear => d.map.year as f64,
            Field::MapWidth => d.map.width,
            Field::MapHeight => d.map.height,
            Field::SampleStep => self.export_step,
            Field::PathName | Field::ItemName | Field::MapName => return None,
        })
    }

    pub(super) fn set_number(&mut self, f: Field, v: f64) {
        let rad = v.to_radians();
        let positive = v.max(1.);
        if let Some((pi, idx)) = self.selected_waypoint() {
            let path = &mut self.doc.paths[pi];
            match (f, idx) {
                (Field::PoseX, 0) => path.start.x = v,
                (Field::PoseY, 0) => path.start.y = v,
                (Field::PoseTheta, 0) => path.start.theta = rad,
                _ => {}
            }
            if let Some(seg) = idx.checked_sub(1).and_then(|i| path.segments.get_mut(i)) {
                match (&mut seg.kind, f) {
                    (SegmentKind::Straight { to }, Field::PoseX) => to.x = v,
                    (SegmentKind::Straight { to }, Field::PoseY) => to.y = v,
                    (SegmentKind::Hermite { to, .. }, Field::PoseX) => to.x = v,
                    (SegmentKind::Hermite { to, .. }, Field::PoseY) => to.y = v,
                    (SegmentKind::Hermite { to, .. }, Field::PoseTheta) => to.theta = rad,
                    (SegmentKind::Hermite { t0, .. }, Field::HermiteT0) => *t0 = v.max(0.),
                    (SegmentKind::Hermite { t1, .. }, Field::HermiteT1) => *t1 = v.max(0.),
                    (SegmentKind::Turn { angle, auto, .. }, Field::TurnAngle) => {
                        *angle = rad;
                        *auto = false;
                    }
                    (SegmentKind::Turn { radius, .. }, Field::TurnRadius) => *radius = v.max(0.),
                    _ => {}
                }
            }
        }
        let snap_center = |shape: &mut Shape, c: Vec2| {
            let d = c - shape.center();
            shape.translate(d);
        };
        if let Some(shape) = self.selected_shape_mut() {
            let center = shape.center();
            match (f, &mut *shape) {
                (Field::ShapeCx, s) => snap_center(s, vec2(v, center.y)),
                (Field::ShapeCy, s) => snap_center(s, vec2(center.x, v)),
                (Field::ShapeW, Shape::Rect { size, .. }) => size.x = positive,
                (Field::ShapeH, Shape::Rect { size, .. }) => size.y = positive,
                (Field::ShapeRotation, Shape::Rect { rotation, .. }) => *rotation = rad,
                (Field::ShapeRadius, Shape::Circle { radius, .. }) => *radius = positive,
                _ => {}
            }
        }
        let d = &mut self.doc;
        match f {
            Field::RobotLength => d.robot.length = positive,
            Field::RobotWidth => d.robot.width = positive,
            Field::RobotOffsetX => d.robot.offset.x = v,
            Field::RobotOffsetY => d.robot.offset.y = v,
            Field::RobotMargin => d.robot.margin = v.max(0.),
            Field::MapYear => d.map.year = v.clamp(0., 9999.) as u32,
            Field::MapWidth => d.map.width = v.max(100.),
            Field::MapHeight => d.map.height = v.max(100.),
            Field::SampleStep => self.export_step = v.max(1.),
            _ => {}
        }
    }

    fn text(&self, f: Field) -> Option<String> {
        match f {
            Field::PathName => self.active().map(|p| p.name.clone()),
            Field::ItemName => match self.selection {
                Selection::Obstacle(i) => self.doc.map.obstacles.get(i).map(|o| o.name.clone()),
                Selection::Region(i) => self.doc.map.regions.get(i).map(|r| r.name.clone()),
                _ => None,
            },
            Field::MapName => Some(self.doc.map.name.clone()),
            _ => self.number(f).map(canvas::format_mm),
        }
    }

    fn set_text(&mut self, f: Field, s: String) {
        match f {
            Field::PathName => {
                if let Some(i) = self.active_path {
                    self.doc.paths[i].name = s;
                }
            }
            Field::ItemName => match self.selection {
                Selection::Obstacle(i) => self.doc.map.obstacles[i].name = s,
                Selection::Region(i) => self.doc.map.regions[i].name = s,
                _ => {}
            },
            Field::MapName => self.doc.map.name = s,
            _ => {}
        }
    }

    // ---- editing ---------------------------------------------------------------

    pub(super) fn start_edit(&mut self, f: Field, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_edit(cx);
        if let Some(text) = self.text(f) {
            self.edit = Some(Edit::new(f, text));
            window.focus(&self.field_focus);
            cx.notify();
        }
    }

    pub(super) fn commit_edit(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.edit.take() else { return };
        let before = self.doc.clone();
        let export_step = self.export_step;
        if edit.field.unit() == Unit::Text {
            let s = edit.text.trim().to_string();
            if !s.is_empty() {
                self.set_text(edit.field, s);
            }
        } else if let Some(v) = parse_number(&edit.text) {
            self.set_number(edit.field, v);
        } else {
            self.set_status(format!("“{}” is not a number", edit.text), true);
        }
        if self.doc != before {
            self.history.push(before);
            self.changed(cx);
        } else if self.export_step != export_step {
            cx.notify();
        }
        cx.notify();
    }

    fn edit_key(&mut self, e: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = self.edit.as_mut() else { return };
        cx.stop_propagation();
        match edit.key(&e.keystroke) {
            Outcome::Continue => {}
            Outcome::Commit => {
                self.commit_edit(cx);
                window.focus(&self.canvas_focus);
            }
            Outcome::Cancel => {
                self.edit = None;
                window.focus(&self.canvas_focus);
            }
            Outcome::Nudge(n) => {
                let f = edit.field;
                if f.unit() != Unit::Text
                    && let Some(v) = parse_number(&edit.text).or_else(|| self.number(f))
                {
                    let before = self.doc.clone();
                    self.set_number(f, v + n * f.step());
                    if self.doc != before {
                        self.history.push(before);
                        self.changed(cx);
                    }
                    if let (Some(text), Some(edit)) = (self.text(f), self.edit.as_mut()) {
                        *edit = Edit::new(f, text);
                    }
                }
            }
        }
        cx.notify();
    }

    // ---- segment kind changes --------------------------------------------------

    fn set_kind(&mut self, pi: usize, si: usize, kind: Kind, cx: &mut Context<Self>) {
        self.edit_doc(cx, |d| {
            let path = &mut d.paths[pi];
            let start = path.poses()[si];
            let seg = &path.segments[si];
            let end = seg.end_pose(start);
            let reverse = seg.reverse;
            // Where the segment ended, or a point ahead for turns in place.
            let ahead = Vec2::from_angle(travel_heading(start.theta, reverse)) * 300.;
            let target = if end.pos().distance(start.pos()) < 1. { start.pos() + ahead } else { end.pos() };
            let mut tmp = Path::new("", start);
            match kind {
                Kind::Straight => tmp.segments.push(Segment::straight(target, reverse)),
                Kind::Turn => {
                    tmp.push_arc_through(target, reverse);
                    if !matches!(tmp.segments.first().map(|s| &s.kind), Some(SegmentKind::Turn { .. })) {
                        tmp.segments = vec![Segment::turn(std::f64::consts::FRAC_PI_2, 0., reverse)];
                    }
                }
                Kind::Hermite => {
                    let len = target.distance(start.pos());
                    let theta = if end.pos().distance(start.pos()) < 1. {
                        body_heading((target - start.pos()).angle(), reverse)
                    } else {
                        end.theta
                    };
                    tmp.segments.push(Segment::hermite(Pose::new(target, theta), len, len, reverse));
                }
            }
            path.segments[si] = tmp.segments.remove(0);
        });
    }

    // ---- rendering -------------------------------------------------------------

    fn field_row(&self, f: Field, t: &Theme, cx: &mut Context<Self>) -> Option<Div> {
        let value = self.text(f)?;
        let unit = f.unit();
        let editing = self.edit.as_ref().filter(|e| e.field == f);
        let t = *t;

        let label = div()
            .id(("field-label", f as usize))
            .w(px(96.))
            .flex_none()
            .font_family(FONT_MONO)
            .font_weight(FontWeight::BOLD)
            .text_size(px(11.))
            .text_color(t.ink_muted)
            .when(unit != Unit::Text, |d| {
                d.cursor(gpui::CursorStyle::ResizeLeftRight).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        this.start_scrub(f, canvas::to_vec(e.position).x);
                    }),
                )
            })
            .child(f.label());

        let mut boxed = div()
            .id(("field", f as usize))
            .flex()
            .items_center()
            .flex_1()
            .min_w_0()
            .h(px(28.))
            .px_2()
            .border_2()
            .border_color(t.border)
            .font_family(FONT_MONO)
            .text_size(px(12.))
            .cursor(gpui::CursorStyle::IBeam);

        if let Some(e) = editing {
            boxed = boxed
                .border_color(t.primary)
                .track_focus(&self.field_focus)
                .key_context("Field")
                .on_key_down(cx.listener(Self::edit_key));
            boxed = if e.select_all {
                boxed.child(div().bg(t.primary).text_color(t.primary_fg).child(e.text.clone()))
            } else {
                boxed
                    .child(e.text[..e.cursor].to_string())
                    .child(div().w(px(2.)).h(px(16.)).bg(t.foreground))
                    .child(e.text[e.cursor..].to_string())
            };
        } else {
            boxed = boxed
                .hover(move |s| s.border_color(t.secondary))
                .on_click(cx.listener(move |this, _, window, cx| this.start_edit(f, window, cx)))
                .child(div().truncate().child(value));
        }
        let boxed = boxed
            .child(div().flex_1())
            .child(div().text_color(t.ink_muted).text_size(px(10.)).child(unit.suffix()));

        Some(div().flex().items_center().gap_2().child(label).child(boxed))
    }

    fn fields(&self, fields: &[Field], t: &Theme, cx: &mut Context<Self>) -> Div {
        div().flex().flex_col().gap_1().children(fields.iter().filter_map(|&f| self.field_row(f, t, cx)))
    }

    fn info(t: &Theme, label: &str, value: String) -> Div {
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(mono_label(t, label.to_string()).w(px(96.)).flex_none())
            .child(div().font_family(FONT_MONO).text_size(px(12.)).child(value))
    }

    fn render_waypoint(&self, t: &Theme, cx: &mut Context<Self>) -> Option<Div> {
        let (pi, idx) = self.selected_waypoint()?;
        let path = &self.doc.paths[pi];
        let poses = path.poses();
        let mut s = section(t, t.primary).child(section_title(
            &if idx == 0 { "Start".to_string() } else { format!("Waypoint {idx}") },
            t.primary,
        ));

        if idx == 0 {
            return Some(s.child(self.fields(&[Field::PoseX, Field::PoseY, Field::PoseTheta], t, cx)));
        }
        let si = idx - 1;
        let seg = path.segments.get(si)?;
        let kind = match seg.kind {
            SegmentKind::Straight { .. } => Kind::Straight,
            SegmentKind::Turn { .. } => Kind::Turn,
            SegmentKind::Hermite { .. } => Kind::Hermite,
        };

        let mut kinds = div().flex().flex_wrap().gap_1();
        for (label, k) in [("Straight", Kind::Straight), ("Turn", Kind::Turn), ("Hermite", Kind::Hermite)] {
            kinds = kinds.child(
                chip(("kind", k as usize), label, kind == k, t.foreground, t.background, t)
                    .on_click(cx.listener(move |this, _, _, cx| this.set_kind(pi, si, k, cx))),
            );
        }
        let reverse = seg.reverse;
        kinds = kinds.child(
            chip("seg-reverse", "Reverse", reverse, t.secondary, t.secondary_fg, t).on_click(cx.listener(
                move |this, _, _, cx| {
                    this.edit_doc(cx, |d| {
                        let seg = &mut d.paths[pi].segments[si];
                        seg.reverse = !seg.reverse;
                        // Keep a Hermite's end facing the same way of travel.
                        if let SegmentKind::Hermite { to, .. } = &mut seg.kind {
                            to.theta = crate::geom::vec::wrap_angle(to.theta + std::f64::consts::PI);
                        }
                    })
                },
            )),
        );
        s = s.child(kinds);

        let fields: &[Field] = match seg.kind {
            SegmentKind::Straight { .. } => &[Field::PoseX, Field::PoseY],
            SegmentKind::Turn { .. } => &[Field::TurnAngle, Field::TurnRadius],
            SegmentKind::Hermite { .. } => {
                &[Field::PoseX, Field::PoseY, Field::PoseTheta, Field::HermiteT0, Field::HermiteT1]
            }
        };
        s = s.child(self.fields(fields, t, cx));

        if let SegmentKind::Turn { radius, auto, .. } = seg.kind {
            let in_place = seg.is_in_place();
            s = s.child(
                div()
                    .flex()
                    .gap_1()
                    .child(chip("in-place", "In place", in_place, t.tertiary, t.tertiary_fg, t).on_click(
                        cx.listener(move |this, _, _, cx| {
                            this.edit_doc(cx, |d| {
                                if let SegmentKind::Turn { radius, auto, .. } = &mut d.paths[pi].segments[si].kind {
                                    *radius = if in_place { 300. } else { 0. };
                                    *auto = false;
                                }
                            })
                        }),
                    ))
                    .when(auto && radius == 0., |d| {
                        d.child(chip("auto", "Auto", true, t.secondary, t.secondary_fg, t).on_click(cx.listener(
                            move |this, _, _, cx| {
                                this.edit_doc(cx, |d| {
                                    if let SegmentKind::Turn { auto, .. } = &mut d.paths[pi].segments[si].kind {
                                        *auto = false;
                                    }
                                })
                            },
                        )))
                    }),
            );
        }

        let end = poses[idx];
        let start = poses[si];
        let clearance = self
            .analysis
            .as_ref()
            .filter(|a| a.active == Some(pi))
            .and_then(|a| a.report.as_ref())
            .and_then(|r| r.segment_clearance.get(si).copied())
            .filter(|c| c.is_finite());
        s = s
            .child(Self::info(
                t,
                "END",
                format!("{}, {}  {:.1}°", canvas::format_mm(end.x), canvas::format_mm(end.y), end.theta.to_degrees()),
            ))
            .child(Self::info(t, "LENGTH", format!("{} MM", canvas::format_mm(seg.length(start)))))
            .when_some(clearance, |d, c| {
                let bad = c < self.doc.robot.margin || c <= 0.;
                d.child(
                    Self::info(t, "CLEARANCE", format!("{} MM", canvas::format_mm(c)))
                        .when(bad, |d| d.text_color(t.primary)),
                )
            })
            .when(seg.is_discontinuous(start), |d| {
                d.child(mono_label(t, "HEADING JUMP AT START — ADD A TURN").text_color(t.primary))
            })
            .child(
                button("delete-waypoint", "Delete waypoint", t.primary, t.primary_fg, t)
                    .on_click(cx.listener(|this, _, w, cx| this.delete_selection(&super::DeleteSelection, w, cx))),
            );
        Some(s)
    }

    fn render_path(&self, t: &Theme, cx: &mut Context<Self>) -> Option<Div> {
        let pi = self.active_path?;
        let path = self.doc.paths.get(pi)?;
        Some(
            section(t, t.foreground)
                .child(section_title("Path", t.primary))
                .child(self.fields(&[Field::PathName], t, cx))
                .child(Self::info(t, "LENGTH", format!("{:.3} M", path.length() / 1000.)))
                .child(Self::info(t, "SEGMENTS", path.segments.len().to_string()))
                .child(
                    button("delete-path", "Delete path", t.primary, t.primary_fg, t).on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.edit_doc(cx, |d| {
                                d.paths.remove(pi);
                            });
                            this.active_path = None;
                            this.selection = Selection::None;
                            cx.notify();
                        },
                    )),
                ),
        )
    }

    fn render_shape(&self, t: &Theme, cx: &mut Context<Self>) -> Option<Div> {
        let shape = self.selected_shape()?;
        let region = match self.selection {
            Selection::Region(i) => Some((i, self.doc.map.regions[i].color)),
            _ => None,
        };
        let strip = region.map_or(t.foreground, |(_, c)| canvas::region_color(t, c));
        let mut s = section(t, strip).child(section_title(if region.is_some() { "Region" } else { "Obstacle" }, strip));
        let fields: &[Field] = match shape {
            Shape::Rect { .. } => {
                &[Field::ItemName, Field::ShapeCx, Field::ShapeCy, Field::ShapeW, Field::ShapeH, Field::ShapeRotation]
            }
            Shape::Circle { .. } => &[Field::ItemName, Field::ShapeCx, Field::ShapeCy, Field::ShapeRadius],
            Shape::Polygon { .. } => &[Field::ItemName, Field::ShapeCx, Field::ShapeCy],
        };
        s = s.child(self.fields(fields, t, cx));
        if let Some((ri, current)) = region {
            let mut colors = div().flex().gap_1();
            for (k, c) in RegionColor::ALL.into_iter().enumerate() {
                let color = canvas::region_color(t, c);
                colors = colors.child(
                    div()
                        .id(("inspector-color", k))
                        .size(px(24.))
                        .bg(color)
                        .border_2()
                        .border_color(if c == current { t.foreground } else { color })
                        .when(c == current, |d| d.border_4())
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.edit_doc(cx, |d| d.map.regions[ri].color = c);
                        })),
                );
            }
            s = s.child(colors);
        }
        if let Shape::Polygon { points } = shape {
            s = s.child(Self::info(t, "VERTICES", points.len().to_string()));
        }
        Some(s.child(
            button("delete-shape", "Delete", t.primary, t.primary_fg, t)
                .on_click(cx.listener(|this, _, w, cx| this.delete_selection(&super::DeleteSelection, w, cx))),
        ))
    }

    pub(super) fn render_inspector(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let origin = self.doc.map.origin;
        let background = self
            .doc
            .map
            .background
            .as_ref()
            .and_then(|p| p.file_name())
            .map_or("NONE".to_string(), |n| n.to_string_lossy().to_uppercase());

        let robot = section(t, t.tertiary).child(section_title("Robot", t.tertiary_ink)).child(self.fields(
            &[Field::RobotLength, Field::RobotWidth, Field::RobotOffsetX, Field::RobotOffsetY, Field::RobotMargin],
            t,
            cx,
        ));

        let map = section(t, t.secondary)
            .child(section_title("Map", t.secondary))
            .child(self.fields(&[Field::MapName, Field::MapYear, Field::MapWidth, Field::MapHeight], t, cx))
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(
                        chip("origin-bl", "Origin bottom-left", origin == Origin::BottomLeft, t.foreground, t.background, t)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.edit_doc(cx, |d| d.map.origin = Origin::BottomLeft)
                            })),
                    )
                    .child(chip("origin-tl", "Top-left", origin == Origin::TopLeft, t.foreground, t.background, t).on_click(
                        cx.listener(|this, _, _, cx| this.edit_doc(cx, |d| d.map.origin = Origin::TopLeft)),
                    )),
            )
            .child(Self::info(t, "BACKGROUND", background))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .child(button("bg-set", "Set image", t.foreground, t.background, t).on_click(cx.listener(
                        |this, _, w, cx| this.pick_background(w, cx),
                    )))
                    .child(button("bg-clear", "Clear", t.foreground, t.background, t).on_click(cx.listener(
                        |this, _, _, cx| this.edit_doc(cx, |d| d.map.background = None),
                    )))
                    .child(button("map-open", "Open map", t.foreground, t.background, t).on_click(cx.listener(
                        |this, _, w, cx| this.open_map(&super::OpenMap, w, cx),
                    )))
                    .child(button("map-save", "Save map", t.foreground, t.background, t).on_click(cx.listener(
                        |this, _, w, cx| this.save_map(&super::SaveMap, w, cx),
                    ))),
            );

        let export = section(t, t.accent)
            .child(section_title("Export", t.accent))
            .child(self.fields(&[Field::SampleStep], t, cx))
            .child(mono_label(t, "JSON INCLUDES SEGMENTS + SAMPLES EVERY STEP"));

        div()
            .id("inspector")
            .flex()
            .flex_col()
            .flex_none()
            .w(px(300.))
            .h_full()
            .gap_4()
            .px_4()
            .pb_4()
            .border_l_2()
            .border_color(t.border)
            .overflow_y_scroll()
            .children(self.render_waypoint(t, cx))
            .children(self.render_shape(t, cx))
            .children(self.render_path(t, cx))
            .child(robot)
            .child(map)
            .child(export)
    }
}
