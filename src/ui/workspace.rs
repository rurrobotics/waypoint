//! Root view: owns the document, tools, selection, undo history and view.

mod files;
mod inspector;
mod mouse;
#[cfg(test)]
mod tests;

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;

use gpui::{
    App, Bounds, Context, CursorStyle, Div, FocusHandle, Focusable, FontWeight, KeyBinding, MouseButton,
    ObjectFit, Pixels, SharedString, Stateful, Subscription, Window, actions, canvas, div, img, prelude::*, px,
};

use crate::geom::collision::{self, Report};
use crate::geom::{Path, Sample, Vec2, vec2};
use crate::model::{Doc, History};
use crate::theme::{FONT_MONO, FONT_SANS, RADIUS, Theme, alpha};
use crate::titlebar::{TitlebarState, window_frame};
use crate::ui::canvas::{self, Frame, Handle, Selection, View, Xform};
use crate::ui::fields::Edit;
use crate::assets::Icon;
use crate::ui::widgets::{badge, chip_icon, icon, icon_button, label, primary_button, section_title, tooltip};

/// Sampling step for drawing and collision checking (mm).
const ANALYSIS_STEP: f64 = 5.;

actions!(
    waypoint,
    [
        Undo,
        Redo,
        NewProject,
        OpenProject,
        SaveProject,
        SaveProjectAs,
        OpenMap,
        SaveMap,
        ExportJson,
        DeleteSelection,
        Cancel,
        FinishShape,
        FitView,
        NewPath,
        NewObstacle,
        NewRegion,
        StartMeasure,
        Quit,
    ]
);

pub fn bind_keys(cx: &mut App) {
    let ws = Some("Workspace");
    let canvas = Some("Canvas");
    cx.bind_keys([
        KeyBinding::new("secondary-z", Undo, ws),
        KeyBinding::new("secondary-shift-z", Redo, ws),
        KeyBinding::new("ctrl-y", Redo, ws),
        KeyBinding::new("secondary-n", NewProject, ws),
        KeyBinding::new("secondary-o", OpenProject, ws),
        KeyBinding::new("secondary-s", SaveProject, ws),
        KeyBinding::new("secondary-shift-s", SaveProjectAs, ws),
        KeyBinding::new("secondary-e", ExportJson, ws),
        KeyBinding::new("secondary-q", Quit, None),
        KeyBinding::new("p", NewPath, canvas),
        KeyBinding::new("o", NewObstacle, canvas),
        KeyBinding::new("r", NewRegion, canvas),
        KeyBinding::new("m", StartMeasure, canvas),
        KeyBinding::new("f", FitView, canvas),
        KeyBinding::new("delete", DeleteSelection, canvas),
        KeyBinding::new("backspace", DeleteSelection, canvas),
        KeyBinding::new("escape", Cancel, canvas),
        KeyBinding::new("enter", FinishShape, canvas),
    ]);
}

/// Which map layer a new shape goes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    Obstacle,
    Region,
}

/// A one-shot action started from the toolbar. Without one, the canvas
/// selects and drags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pending {
    /// Clicks add waypoints to the active path (a new path when there is none).
    DrawPath,
    /// The next drag (or polygon clicks) draws one shape.
    Shape { layer: Layer, kind: ShapeTool },
    /// The next drag measures a distance.
    Measure,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeTool {
    Rect,
    Circle,
    Polygon,
}

/// Segment appended by the waypoint tool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegTool {
    Straight,
    TurnInPlace,
    TurnArc,
    Hermite,
}

const SNAP_STEPS: [f64; 6] = [0., 1., 5., 10., 50., 100.];

enum Drag {
    None,
    Pan { last: Vec2 },
    Handle { path: usize, handle: Handle },
    Object { last: Vec2 },
    Create { start: Vec2, current: Vec2 },
    Measure,
    Scrub { field: crate::ui::fields::Field, start_x: f64, start: f64 },
}

struct Analysis {
    revision: u64,
    active: Option<usize>,
    samples: Vec<Vec<Sample>>,
    report: Option<Report>,
}

struct Status {
    text: String,
    error: bool,
}

pub struct Workspace {
    doc: Doc,
    history: History<Doc>,
    /// Document before the current drag, recorded on release if it changed.
    drag_snapshot: Option<Doc>,
    project_path: Option<PathBuf>,
    map_file: Option<PathBuf>,
    dirty: bool,
    revision: u64,
    analysis: Option<Analysis>,

    pending: Option<Pending>,
    seg_tool: SegTool,
    reverse: bool,
    auto_turn: bool,
    snap: f64,
    show_sweep: bool,
    /// Sample spacing for exports (mm).
    export_step: f64,

    active_path: Option<usize>,
    selection: Selection,
    drag: Drag,
    polygon: Vec<Vec2>,
    measure: Option<(Vec2, Vec2)>,
    hover: Option<Vec2>,
    modifiers: gpui::Modifiers,

    view: View,
    auto_fit: bool,
    last_canvas_size: Vec2,
    canvas_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,

    focus: FocusHandle,
    canvas_focus: FocusHandle,
    field_focus: FocusHandle,
    edit: Option<Edit>,
    status: Option<Status>,
    titlebar: TitlebarState,
    _subscriptions: Vec<Subscription>,
}

impl Focusable for Workspace {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.canvas_focus.clone()
    }
}

impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let field_focus = cx.focus_handle();
        let subscriptions = vec![
            cx.on_blur(&field_focus, window, |this, _, cx| this.commit_edit(cx)),
            cx.observe_window_appearance(window, |_, window, cx| {
                if Theme::sync(window, cx) {
                    cx.notify();
                }
            }),
        ];
        let canvas_focus = cx.focus_handle();
        window.focus(&canvas_focus);

        Self {
            doc: Doc::default(),
            history: History::new(200),
            drag_snapshot: None,
            project_path: None,
            map_file: None,
            dirty: false,
            revision: 0,
            analysis: None,
            pending: None,
            seg_tool: SegTool::Straight,
            reverse: false,
            auto_turn: true,
            snap: 10.,
            show_sweep: true,
            export_step: 10.,
            active_path: None,
            selection: Selection::None,
            drag: Drag::None,
            polygon: Vec::new(),
            measure: None,
            hover: None,
            modifiers: Default::default(),
            view: View::default(),
            auto_fit: true,
            last_canvas_size: Vec2::ZERO,
            canvas_bounds: Rc::new(Cell::new(None)),
            focus: cx.focus_handle(),
            canvas_focus,
            field_focus,
            edit: None,
            status: None,
            titlebar: TitlebarState::default(),
            _subscriptions: subscriptions,
        }
    }

    // ---- document bookkeeping ------------------------------------------------

    /// Call after every document change.
    fn changed(&mut self, cx: &mut Context<Self>) {
        for path in &mut self.doc.paths {
            path.fix_auto_turns();
        }
        self.clamp_selection();
        self.revision += 1;
        self.dirty = true;
        cx.notify();
    }

    /// Runs `f` on the document as one undoable step.
    fn edit_doc(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut Doc)) {
        let before = self.doc.clone();
        f(&mut self.doc);
        if self.doc != before {
            self.history.push(before);
            self.changed(cx);
        }
    }

    fn begin_drag_edit(&mut self) {
        self.drag_snapshot = Some(self.doc.clone());
    }

    /// Records the drag as one undo step; returns true if it changed anything.
    fn end_drag_edit(&mut self) -> bool {
        if let Some(before) = self.drag_snapshot.take()
            && before != self.doc
        {
            self.history.push(before);
            self.dirty = true;
            return true;
        }
        false
    }

    /// Re-derives everything from the document without marking it dirty
    /// (used while dragging; the undo step is recorded on release).
    fn refresh(&mut self, cx: &mut Context<Self>) {
        for path in &mut self.doc.paths {
            path.fix_auto_turns();
        }
        self.clamp_selection();
        self.revision += 1;
        cx.notify();
    }

    fn clamp_selection(&mut self) {
        let paths = self.doc.paths.len();
        if self.active_path.is_some_and(|i| i >= paths) {
            self.active_path = paths.checked_sub(1);
        }
        let valid = match self.selection {
            Selection::None => true,
            Selection::Waypoint { path, index } => {
                path < paths && index <= self.doc.paths[path].segments.len()
            }
            Selection::Obstacle(i) => i < self.doc.map.obstacles.len(),
            Selection::Region(i) => i < self.doc.map.regions.len(),
        };
        if !valid {
            self.selection = Selection::None;
        }
    }

    fn set_status(&mut self, text: impl Into<String>, error: bool) {
        self.status = Some(Status { text: text.into(), error });
    }

    fn ensure_analysis(&mut self) -> &Analysis {
        let stale = self
            .analysis
            .as_ref()
            .is_none_or(|a| a.revision != self.revision || a.active != self.active_path);
        if stale {
            let samples: Vec<Vec<Sample>> = self.doc.paths.iter().map(|p| p.sample(ANALYSIS_STEP)).collect();
            let obstacles: Vec<_> = self.doc.map.obstacles.iter().map(|o| &o.shape).collect();
            let report = self.active_path.and_then(|i| {
                Some(collision::check(
                    samples.get(i)?,
                    self.doc.paths[i].segments.len(),
                    &self.doc.robot,
                    self.doc.map.size(),
                    &obstacles,
                ))
            });
            self.analysis = Some(Analysis { revision: self.revision, active: self.active_path, samples, report });
        }
        self.analysis.as_ref().unwrap()
    }

    fn xform(&self) -> Option<Xform> {
        let b = self.canvas_bounds.get()?;
        (self.view.zoom > 0.).then(|| Xform::new(self.view, canvas::to_vec(b.origin), &self.doc))
    }

    fn active(&self) -> Option<&Path> {
        self.active_path.and_then(|i| self.doc.paths.get(i))
    }

    // ---- actions ---------------------------------------------------------------

    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(prev) = self.history.undo(self.doc.clone()) {
            self.doc = prev;
            self.changed(cx);
        }
    }

    fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(next) = self.history.redo(self.doc.clone()) {
            self.doc = next;
            self.changed(cx);
        }
    }

    /// Starts a one-shot action; starting the armed one again cancels it.
    fn start(&mut self, pending: Pending, cx: &mut Context<Self>) {
        self.polygon.clear();
        self.measure = None;
        self.drag = Drag::None;
        if self.pending == Some(pending) {
            self.pending = None;
        } else {
            self.pending = Some(pending);
            if pending == Pending::DrawPath {
                self.active_path = None;
                self.selection = Selection::None;
            }
        }
        cx.notify();
    }

    /// Adds waypoints to the end of an existing path.
    fn extend_path(&mut self, path: usize, cx: &mut Context<Self>) {
        self.polygon.clear();
        self.measure = None;
        self.active_path = Some(path);
        self.selection = Selection::Waypoint { path, index: self.doc.paths[path].segments.len() };
        self.pending = Some(Pending::DrawPath);
        cx.notify();
    }

    /// Ends the pending action, closing an open polygon.
    fn finish_pending(&mut self, cx: &mut Context<Self>) {
        if !self.polygon.is_empty() {
            self.finish_polygon();
        }
        self.pending = None;
        self.refresh(cx);
    }

    fn delete_selection(&mut self, _: &DeleteSelection, _: &mut Window, cx: &mut Context<Self>) {
        match self.selection {
            Selection::None => {}
            Selection::Waypoint { path, index: 0 } => {
                self.edit_doc(cx, |d| {
                    d.paths.remove(path);
                });
                self.active_path = None;
                self.selection = Selection::None;
                self.set_status("Path deleted", false);
            }
            Selection::Waypoint { path, index } => {
                let mut new_index = index - 1;
                self.edit_doc(cx, |d| {
                    let segs = &mut d.paths[path].segments;
                    let seg = segs.remove(index - 1);
                    // A straight takes its auto turn with it.
                    let prev_auto = index >= 2
                        && matches!(segs[index - 2].kind, crate::geom::SegmentKind::Turn { auto: true, .. });
                    if matches!(seg.kind, crate::geom::SegmentKind::Straight { .. }) && prev_auto {
                        segs.remove(index - 2);
                        new_index -= 1;
                    }
                });
                self.selection = Selection::Waypoint { path, index: new_index };
            }
            Selection::Obstacle(i) => {
                self.edit_doc(cx, |d| {
                    d.map.obstacles.remove(i);
                });
                self.selection = Selection::None;
            }
            Selection::Region(i) => {
                self.edit_doc(cx, |d| {
                    d.map.regions.remove(i);
                });
                self.selection = Selection::None;
            }
        }
        cx.notify();
    }

    fn cancel(&mut self, _: &Cancel, _: &mut Window, cx: &mut Context<Self>) {
        if self.pending.is_some() || !self.polygon.is_empty() || self.measure.is_some() || !matches!(self.drag, Drag::None) {
            self.pending = None;
            self.polygon.clear();
            self.measure = None;
            self.drag = Drag::None;
        } else {
            self.selection = Selection::None;
        }
        cx.notify();
    }

    fn fit_view(&mut self, _: &FitView, _: &mut Window, cx: &mut Context<Self>) {
        self.auto_fit = true;
        self.last_canvas_size = Vec2::ZERO;
        cx.notify();
    }

    // ---- rendering -------------------------------------------------------------

    fn render_toolbar(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let sep = || div().w(px(1.)).h(px(18.)).mx_1().flex_none().bg(t.border);
        let snap_label = if self.snap == 0. { "Off".to_string() } else { format!("{} mm", canvas::format_mm(self.snap)) };

        // "Obstacle □ ○ ⬠": one icon per shape, each starting a one-shot draw.
        let shape_group = |layer: Layer, name: &str, cx: &mut Context<Self>| {
            let mut group = div()
                .flex()
                .flex_none()
                .items_center()
                .h(px(30.))
                .pl_2()
                .gap_0p5()
                .rounded(RADIUS)
                .border_1()
                .border_color(t.border)
                .child(icon(Icon::Plus, 14., t.muted))
                .child(div().text_size(px(12.)).pr_1().child(name.to_string()));
            for (kind, i, tip) in [
                (ShapeTool::Rect, Icon::Square, "rectangle"),
                (ShapeTool::Circle, Icon::Circle, "circle"),
                (ShapeTool::Polygon, Icon::Pentagon, "polygon"),
            ] {
                let pending = Pending::Shape { layer, kind };
                group = group.child(
                    icon_button(
                        SharedString::from(format!("new-{name}-{tip}")),
                        i,
                        &format!("New {} {tip}", name.to_lowercase()),
                        self.pending == Some(pending),
                        t,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.start(pending, cx))),
                );
            }
            group
        };

        div()
            .flex()
            .flex_1()
            .min_w_0()
            .items_center()
            .gap_1()
            .overflow_hidden()
            .child(self.render_file_actions(t, cx))
            .child(sep())
            .child(
                chip_icon("new-path", Some(Icon::Route), "New path", self.pending == Some(Pending::DrawPath), t)
                    .h(px(30.))
                    .tooltip(tooltip("New path (P)"))
                    .on_click(cx.listener(|this, _, _, cx| this.start(Pending::DrawPath, cx))),
            )
            .child(shape_group(Layer::Obstacle, "Obstacle", cx))
            .child(shape_group(Layer::Region, "Region", cx))
            .child(
                icon_button("measure", Icon::Ruler, "Measure (M)", self.pending == Some(Pending::Measure), t)
                    .on_click(cx.listener(|this, _, _, cx| this.start(Pending::Measure, cx))),
            )
            .child(div().flex_1())
            .child(chip_icon("snap", Some(Icon::Magnet), &snap_label, self.snap > 0., t).tooltip(tooltip("Grid snap")).on_click(
                cx.listener(|this, _, _, cx| {
                    let i = SNAP_STEPS.iter().position(|&s| s == this.snap).unwrap_or(0);
                    this.snap = SNAP_STEPS[(i + 1) % SNAP_STEPS.len()];
                    cx.notify();
                }),
            ))
            .child(
                icon_button(
                    "sweep",
                    if self.show_sweep { Icon::Eye } else { Icon::EyeOff },
                    "Show robot sweep",
                    self.show_sweep,
                    t,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.show_sweep = !this.show_sweep;
                    cx.notify();
                })),
            )
            .child(icon_button("fit", Icon::Fit, "Fit table (F)", false, t).on_click(cx.listener(|this, _, w, cx| {
                this.fit_view(&FitView, w, cx)
            })))
            .child(
                primary_button("file-export", Some(Icon::Download), "Export", t)
                    .on_click(cx.listener(|this, _, w, cx| this.export_json(&ExportJson, w, cx))),
            )
    }

    /// Floating options while drawing a path.
    fn render_draw_bar(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let mut bar = div()
            .flex()
            .items_center()
            .gap_1()
            .p_1()
            .rounded(RADIUS)
            .bg(t.bg)
            .border_1()
            .border_color(t.border)
            .shadow_md()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        for (id, i, label, s) in [
            ("seg-straight", Icon::MoveRight, "Straight", SegTool::Straight),
            ("seg-turn", Icon::RotateCw, "Turn in place", SegTool::TurnInPlace),
            ("seg-arc", Icon::CornerUpRight, "Arc", SegTool::TurnArc),
            ("seg-hermite", Icon::Spline, "Hermite", SegTool::Hermite),
        ] {
            bar = bar.child(chip_icon(id, Some(i), label, self.seg_tool == s, t).on_click(cx.listener(
                move |this, _, _, cx| {
                    this.seg_tool = s;
                    cx.notify();
                },
            )));
        }
        bar.child(div().w(px(1.)).h(px(18.)).mx_1().bg(t.border))
            .child(chip_icon("reverse", Some(Icon::Reverse), "Reverse", self.reverse, t).on_click(cx.listener(
                |this, _, _, cx| {
                    this.reverse = !this.reverse;
                    cx.notify();
                },
            )))
            .child(
                chip_icon("auto-turn", None, "Auto turn", self.auto_turn, t)
                    .tooltip(tooltip("Turn in place before a straight that doesn't start aligned"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.auto_turn = !this.auto_turn;
                        cx.notify();
                    })),
            )
            .child(div().w(px(1.)).h(px(18.)).mx_1().bg(t.border))
            .child(primary_button("draw-done", None, "Done", t).on_click(cx.listener(|this, _, _, cx| this.finish_pending(cx))))
    }

    fn render_file_actions(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap_1()
            .child(icon_button("file-new", Icon::FilePlus, "New project (Ctrl+N)", false, t).on_click(cx.listener(|this, _, w, cx| {
                this.new_project(&NewProject, w, cx)
            })))
            .child(icon_button("file-open", Icon::FolderOpen, "Open project (Ctrl+O)", false, t).on_click(cx.listener(|this, _, w, cx| {
                this.open_project(&OpenProject, w, cx)
            })))
            .child(icon_button("file-save", Icon::Save, "Save project (Ctrl+S)", false, t).on_click(cx.listener(|this, _, w, cx| {
                this.save_project(&SaveProject, w, cx)
            })))
            .child(div().w(px(1.)).h(px(18.)).mx_1().bg(t.border))
            .child(icon_button("undo", Icon::Undo, "Undo (Ctrl+Z)", false, t).on_click(cx.listener(|this, _, w, cx| {
                this.undo(&Undo, w, cx)
            })))
            .child(icon_button("redo", Icon::Redo, "Redo (Ctrl+Shift+Z)", false, t).on_click(cx.listener(|this, _, w, cx| {
                this.redo(&Redo, w, cx)
            })))
    }

    fn render_sidebar(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let row = |id: (&'static str, usize), label: String, detail: String, active: bool| -> Stateful<Div> {
            let t = *t;
            div()
                .id(id)
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .px_2()
                .h(px(28.))
                .rounded(RADIUS)
                .cursor_pointer()
                .when(active, |d| d.bg(alpha(t.accent, 0.15)).text_color(t.fg))
                .when(!active, move |d| d.hover(move |s| s.bg(t.elevated)))
                .child(div().text_sm().font_weight(FontWeight::MEDIUM).truncate().child(label))
                .child(div().font_family(FONT_MONO).text_size(px(10.)).child(detail))
        };

        let mut paths = div().flex().flex_col().gap_1();
        for (i, path) in self.doc.paths.iter().enumerate() {
            let len = format!("{:.2} m", path.length() / 1000.);
            paths = paths.child(row(("path", i), path.name.clone(), len, self.active_path == Some(i)).on_click(
                cx.listener(move |this, _, _, cx| {
                    this.active_path = Some(i);
                    this.selection = Selection::Waypoint { path: i, index: 0 };
                    cx.notify();
                }),
            ));
        }
        if self.doc.paths.is_empty() {
            paths = paths.child(label(t, "No paths yet — use New path in the toolbar"));
        }

        let mut obstacles = div().flex().flex_col().gap_1();
        for (i, o) in self.doc.map.obstacles.iter().enumerate() {
            obstacles = obstacles.child(
                row(("obstacle", i), o.name.clone(), shape_kind(&o.shape).into(), self.selection == Selection::Obstacle(i))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.selection = Selection::Obstacle(i);
                        cx.notify();
                    })),
            );
        }
        let mut regions = div().flex().flex_col().gap_1();
        for (i, r) in self.doc.map.regions.iter().enumerate() {
            let swatch = canvas::region_color(t, r.color);
            regions = regions.child(
                row(("region", i), r.name.clone(), shape_kind(&r.shape).into(), self.selection == Selection::Region(i))
                    .child(div().size(px(10.)).bg(swatch))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.selection = Selection::Region(i);
                        cx.notify();
                    })),
            );
        }

        div()
            .id("sidebar")
            .flex()
            .flex_col()
            .flex_none()
            .w(px(240.))
            .h_full()
            .gap_5()
            .p_4()
            .border_r_1()
            .border_color(t.border)
            .overflow_y_scroll()
            .child(div().flex().flex_col().gap_2().child(section_title("Paths", t)).child(paths))
            .child(div().flex().flex_col().gap_2().child(section_title("Obstacles", t)).child(obstacles))
            .child(div().flex().flex_col().gap_2().child(section_title("Regions", t)).child(regions))
    }

    fn render_canvas(&mut self, t: &Theme, frame: Frame, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let draw_bar = self.render_draw_bar(t, cx);
        let bounds_cell = self.canvas_bounds.clone();
        let view = self.view;
        let size = self.doc.map.size() * view.zoom;
        let background = self.doc.map.background.clone().filter(|_| view.zoom > 0.);
        let cursor = match (&self.drag, self.pending) {
            (Drag::Pan { .. } | Drag::Handle { .. } | Drag::Object { .. }, _) => CursorStyle::ClosedHand,
            (_, None) => CursorStyle::Arrow,
            _ => CursorStyle::Crosshair,
        };

        div()
            .id("canvas")
            .flex_1()
            .h_full()
            .min_w_0()
            .relative()
            .overflow_hidden()
            .bg(t.canvas)
            .cursor(cursor)
            .track_focus(&self.canvas_focus)
            .key_context("Canvas")
            .on_mouse_down(MouseButton::Left, cx.listener(Self::canvas_mouse_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::canvas_mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::canvas_mouse_down))
            .on_scroll_wheel(cx.listener(Self::scroll))
            .when(view.zoom > 0., |d| {
                d.child(
                    div()
                        .absolute()
                        .left(px(view.pan.x as f32))
                        .top(px(view.pan.y as f32))
                        .w(px(size.x as f32))
                        .h(px(size.y as f32))
                        .bg(t.table),
                )
            })
            .when_some(background, |d, bg| {
                d.child(
                    img(bg)
                        .absolute()
                        .left(px(view.pan.x as f32))
                        .top(px(view.pan.y as f32))
                        .w(px(size.x as f32))
                        .h(px(size.y as f32))
                        .object_fit(ObjectFit::Fill),
                )
            })
            .child(
                canvas(
                    move |bounds, window, _| {
                        if bounds_cell.get() != Some(bounds) {
                            bounds_cell.set(Some(bounds));
                            window.refresh();
                        }
                    },
                    move |bounds, (), window, cx| canvas::paint(&frame, bounds, window, cx),
                )
                .absolute()
                .size_full(),
            )
            .when(self.pending == Some(Pending::DrawPath), |d| {
                d.child(div().absolute().top_3().left_0().right_0().flex().justify_center().child(draw_bar))
            })
    }

    fn render_status(&mut self, t: &Theme) -> impl IntoElement + use<> {
        let hover = self.hover.map(|p| format!("X {:>6}  Y {:>6}", canvas::format_mm(p.x), canvas::format_mm(p.y)));
        let hint = match self.pending {
            None => "Drag handles or shapes · wheel zooms · right-drag pans",
            Some(Pending::DrawPath) if self.active_path.is_none() => "Click to place the start of the path",
            Some(Pending::DrawPath) => "Click to add waypoints · Shift snaps 15° · Enter or double-click finishes",
            Some(Pending::Shape { kind: ShapeTool::Polygon, .. }) => {
                "Click vertices · Enter or double-click closes (convex only)"
            }
            Some(Pending::Shape { .. }) => "Drag to draw · Shift makes it square",
            Some(Pending::Measure) => "Drag to measure",
        };
        let zoom = format!("{:.0}%", self.view.zoom * 100.);
        let name = self
            .project_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map_or_else(|| "Untitled".to_string(), |n| n.to_string_lossy().into_owned());
        let project = format!("{name}{}", if self.dirty { " •" } else { "" });
        let active = self.active_path;
        let length = self.active().map(|p| format!("{:.3} m", p.length() / 1000.));
        let gaps = self.active().map(|p| p.discontinuities().len()).unwrap_or(0);
        let a = self.ensure_analysis();
        let (events, clearance) = match &a.report {
            Some(r) => (r.events, r.min_clearance.map(|(c, _)| c)),
            None => (0, None),
        };

        div()
            .flex()
            .flex_none()
            .items_center()
            .gap_3()
            .px_4()
            .h(px(30.))
            .border_t_1()
            .border_color(t.border)
            .text_size(px(12.))
            .child(div().font_weight(FontWeight::MEDIUM).child(project))
            .child(div().w(px(150.)).font_family(FONT_MONO).text_size(px(11.)).child(hover.unwrap_or_default()))
            .child(div().text_color(t.muted).truncate().child(hint))
            .child(div().flex_1())
            .when_some(self.status.as_ref(), |d, s| {
                d.child(div().truncate().text_color(if s.error { t.danger } else { t.fg }).child(s.text.clone()))
            })
            .when(active.is_some(), |d| {
                d.children(length)
                    .child(badge(
                        if events == 0 { "clear".into() } else { format!("{events} collision{}", if events == 1 { "" } else { "s" }) },
                        events > 0,
                        t.danger,
                        t.danger_fg,
                        t,
                    ))
                    .when_some(clearance, |d, c| d.child(format!("min clearance {} mm", canvas::format_mm(c))))
                    .when(gaps > 0, |d| {
                        d.child(badge(format!("{gaps} heading jump{}", if gaps == 1 { "" } else { "s" }), true, t.warning, t.accent_fg, t))
                    })
            })
            .child(div().text_color(t.muted).child(zoom))
    }
}

fn shape_kind(s: &crate::geom::Shape) -> &'static str {
    match s {
        crate::geom::Shape::Rect { .. } => "rect",
        crate::geom::Shape::Circle { .. } => "circle",
        crate::geom::Shape::Polygon { .. } => "polygon",
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = Theme::get(cx);

        if let Some(b) = self.canvas_bounds.get() {
            let size = vec2(f32::from(b.size.width) as f64, f32::from(b.size.height) as f64);
            if self.auto_fit && size != self.last_canvas_size && size.x > 0. && size.y > 0. {
                self.view = View::fit(self.doc.map.size(), size);
            }
            self.last_canvas_size = size;
        }

        let preview = self.preview();
        self.ensure_analysis();
        let analysis = self.analysis.as_ref().unwrap();
        let frame = Frame {
            theme: t,
            doc: self.doc.clone(),
            view: self.view,
            active_path: self.active_path,
            selection: self.selection,
            samples: analysis.samples.clone(),
            report: analysis.report.clone(),
            show_sweep: self.show_sweep,
            preview,
        };

        let toolbar = self.render_toolbar(&t, cx);
        let sidebar = self.render_sidebar(&t, cx);
        let canvas = self.render_canvas(&t, frame, cx);
        let inspector = self.render_inspector(&t, cx);
        let status = self.render_status(&t);
        let content = div()
            .size_full()
            .flex()
            .flex_col()
            .font_family(FONT_SANS)
            .child(div().flex_1().min_h_0().flex().flex_row().child(sidebar).child(canvas).child(inspector))
            .child(status);

        div()
            .size_full()
            .track_focus(&self.focus)
            .key_context("Workspace")
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(Self::new_project))
            .on_action(cx.listener(Self::open_project))
            .on_action(cx.listener(Self::save_project))
            .on_action(cx.listener(Self::save_project_as))
            .on_action(cx.listener(Self::open_map))
            .on_action(cx.listener(Self::save_map))
            .on_action(cx.listener(Self::export_json))
            .on_action(cx.listener(Self::delete_selection))
            .on_action(cx.listener(Self::cancel))
            .on_action(cx.listener(Self::finish_shape))
            .on_action(cx.listener(Self::fit_view))
            .on_action(cx.listener(|this, _: &NewPath, _, cx| this.start(Pending::DrawPath, cx)))
            .on_action(cx.listener(|this, _: &NewObstacle, _, cx| {
                this.start(Pending::Shape { layer: Layer::Obstacle, kind: ShapeTool::Rect }, cx)
            }))
            .on_action(cx.listener(|this, _: &NewRegion, _, cx| {
                this.start(Pending::Shape { layer: Layer::Region, kind: ShapeTool::Rect }, cx)
            }))
            .on_action(cx.listener(|this, _: &StartMeasure, _, cx| this.start(Pending::Measure, cx)))
            .on_action(|_: &Quit, _, cx| cx.quit())
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::mouse_up))
            .on_mouse_up(MouseButton::Right, cx.listener(Self::mouse_up))
            .on_modifiers_changed(cx.listener(|this, e: &gpui::ModifiersChangedEvent, _, cx| {
                this.modifiers = e.modifiers;
                cx.notify();
            }))
            .child(window_frame(window, &t, &self.titlebar, toolbar, content))
    }
}
