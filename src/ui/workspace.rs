//! Root view: owns the document, tools, selection, undo history and view.

mod files;
mod inspector;
mod mouse;
mod panels;
#[cfg(test)]
mod tests;

use anyhow::Context as _;
use std::cell::Cell;
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    App, Bounds, Context, CursorStyle, FocusHandle, Focusable, FontWeight, KeyBinding, MouseButton,
    ImageSource, ObjectFit, Pixels, RenderImage, SharedString, Subscription, Task, Window, actions, canvas, div, img, prelude::*, px,
};

use crate::geom::collision::{self, Report};
use crate::geom::{Path, Sample, Shape, Vec2, vec2};
use crate::model::{Doc, History};
use crate::theme::{FONT_MONO, FONT_SANS, RADIUS, Theme};
use crate::titlebar::{TitlebarState, window_frame};
use crate::ui::background;
use crate::ui::canvas::{self, Frame, Handle, Selection, ShapeHandle, View, Xform};
use crate::ui::dock::{Layout, Side};
use crate::ui::fields::Edit;
use crate::assets::Icon;
use crate::ui::widgets::{badge, chip_icon, icon, icon_button, primary_button, tooltip};

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
        ExportJson,
        DeleteSelection,
        Cancel,
        FinishShape,
        FitView,
        NewPath,
        NewObstacle,
        NewRegion,
        StartMeasure,
        ToggleLeftDock,
        ToggleRightDock,
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
        KeyBinding::new("secondary-b", ToggleLeftDock, ws),
        KeyBinding::new("secondary-alt-b", ToggleRightDock, ws),
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
    /// Resizing or rotating the selected shape; `start` is the shape when
    /// the drag began and `angle` the pointer's angle around its centre then.
    ShapeHandle { handle: ShapeHandle, start: Shape, angle: f64 },
    Create { start: Vec2, current: Vec2 },
    Measure,
    Scrub { field: crate::ui::fields::Field, start_x: f64, start: f64 },
    DockWidth { side: Side, start_x: f64, start: f32 },
    DockSplit { side: Side, index: usize, start_y: f64, start: (f32, f32) },
}

struct Analysis {
    revision: u64,
    active: Option<usize>,
    samples: Vec<Vec<Sample>>,
    report: Option<Report>,
}

/// An SVG background rendered off the UI thread (see `ui::background`).
struct SvgBackground {
    /// The `Map::background` value it was rendered from.
    source: PathBuf,
    /// `None` while rendering (or when rendering failed).
    image: Option<Arc<RenderImage>>,
    _task: Task<()>,
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
    dirty: bool,
    revision: u64,
    analysis: Option<Analysis>,

    pending: Option<Pending>,
    seg_tool: SegTool,
    reverse: bool,
    auto_turn: bool,
    snap: f64,
    show_sweep: bool,

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
    settings_tab: panels::SettingsTab,
    layout: Layout,
    svg_background: Option<SvgBackground>,
    /// The New ▾ template menu is open.
    new_menu: bool,
    /// Where user templates live (`None` in tests, so they never touch the user's files).
    templates_dir: Option<PathBuf>,
    collapsed: HashSet<panels::Group>,
    expanded_paths: HashSet<usize>,
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
            doc: crate::model::template::default().doc,
            history: History::new(200),
            drag_snapshot: None,
            project_path: None,
            dirty: false,
            revision: 0,
            analysis: None,
            pending: None,
            seg_tool: SegTool::Straight,
            reverse: false,
            auto_turn: true,
            snap: 10.,
            show_sweep: true,
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
            settings_tab: Default::default(),
            layout: Layout::load(),
            svg_background: None,
            new_menu: false,
            templates_dir: if cfg!(test) { None } else { crate::model::template::dir() },
            collapsed: HashSet::new(),
            expanded_paths: HashSet::new(),
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
            Selection::Path(path) => path < paths,
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

    /// Starts rendering the map's background when it is an SVG that hasn't
    /// been rendered yet. Raster images are drawn by gpui directly.
    fn ensure_svg_background(&mut self, cx: &mut Context<Self>) {
        let source = self.doc.map.background.clone().filter(|bg| background::is_svg(bg));
        let Some(source) = source else {
            self.svg_background = None;
            return;
        };
        if self.svg_background.as_ref().is_some_and(|b| b.source == source) {
            return;
        }
        let asset = self.doc.map.background_asset().map(str::to_owned);
        let file = source.clone();
        let task = cx.spawn(async move |this, cx| {
            let rendered = cx
                .background_executor()
                .spawn(async move {
                    let bytes = match asset {
                        Some(asset) => crate::assets::get(&asset)
                            .map(<[u8]>::to_vec)
                            .ok_or_else(|| anyhow::anyhow!("{asset} is not shipped with the app"))?,
                        None => std::fs::read(&file).with_context(|| format!("reading {}", file.display()))?,
                    };
                    background::render_svg(&bytes, background::MAX_SIDE)
                })
                .await;
            this.update(cx, |this, cx| {
                let Some(bg) = this.svg_background.as_mut() else { return };
                match rendered {
                    Ok(image) => bg.image = Some(image),
                    Err(e) => this.set_status(format!("Background: {e:#}"), true),
                }
                cx.notify();
            })
            .ok();
        });
        self.svg_background = Some(SvgBackground { source, image: None, _task: task });
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
        self.delete(self.selection, cx);
    }

    /// Deletes a path, waypoint (with its segment) or shape as one undo step.
    fn delete(&mut self, what: Selection, cx: &mut Context<Self>) {
        match what {
            Selection::None => {}
            Selection::Path(path) | Selection::Waypoint { path, index: 0 } => {
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

    /// The visibility flag of a path (waypoints share their path's), obstacle or region.
    fn hidden_flag(doc: &mut Doc, what: Selection) -> Option<&mut bool> {
        match what {
            Selection::None => None,
            Selection::Path(i) | Selection::Waypoint { path: i, .. } => doc.paths.get_mut(i).map(|p| &mut p.hidden),
            Selection::Obstacle(i) => doc.map.obstacles.get_mut(i).map(|o| &mut o.hidden),
            Selection::Region(i) => doc.map.regions.get_mut(i).map(|r| &mut r.hidden),
        }
    }

    /// True if the item isn't drawn on the table.
    pub(super) fn is_hidden(&self, what: Selection) -> bool {
        match what {
            Selection::None => false,
            Selection::Path(i) | Selection::Waypoint { path: i, .. } => self.doc.paths.get(i).is_some_and(|p| p.hidden),
            Selection::Obstacle(i) => self.doc.map.obstacles.get(i).is_some_and(|o| o.hidden),
            Selection::Region(i) => self.doc.map.regions.get(i).is_some_and(|r| r.hidden),
        }
    }

    /// Hides or shows one path, obstacle or region as one undo step.
    pub(super) fn toggle_hidden(&mut self, what: Selection, cx: &mut Context<Self>) {
        self.edit_doc(cx, |d| {
            if let Some(h) = Self::hidden_flag(d, what) {
                *h = !*h;
            }
        });
        cx.notify();
    }

    /// Hides every item of an outline group, or shows them all if all are hidden.
    pub(super) fn toggle_group_hidden(&mut self, group: panels::Group, cx: &mut Context<Self>) {
        self.edit_doc(cx, |d| {
            let mut flags: Vec<&mut bool> = match group {
                panels::Group::Paths => d.paths.iter_mut().map(|p| &mut p.hidden).collect(),
                panels::Group::Obstacles => d.map.obstacles.iter_mut().map(|o| &mut o.hidden).collect(),
                panels::Group::Regions => d.map.regions.iter_mut().map(|r| &mut r.hidden).collect(),
            };
            let hide = flags.iter().any(|h| !**h);
            flags.iter_mut().for_each(|h| **h = hide);
        });
        cx.notify();
    }

    fn cancel(&mut self, _: &Cancel, _: &mut Window, cx: &mut Context<Self>) {
        if self.new_menu {
            self.new_menu = false;
            cx.notify();
            return;
        }
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

    /// Collapses or reopens a side dock; the panels stay where they are.
    pub(super) fn toggle_dock(&mut self, side: Side, cx: &mut Context<Self>) {
        self.layout.toggle_collapsed(side);
        self.layout.save();
        cx.notify();
    }

    fn toggle_left_dock(&mut self, _: &ToggleLeftDock, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_dock(Side::Left, cx);
    }

    fn toggle_right_dock(&mut self, _: &ToggleRightDock, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_dock(Side::Right, cx);
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
            .child(sep())
            .child(
                icon_button("toggle-left-dock", Icon::PanelLeft, "Toggle left panels (Ctrl+B)", self.layout.is_open(Side::Left), t)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_dock(Side::Left, cx))),
            )
            .child(
                icon_button(
                    "toggle-right-dock",
                    Icon::PanelRight,
                    "Toggle right panels (Ctrl+Alt+B)",
                    self.layout.is_open(Side::Right),
                    t,
                )
                .on_click(cx.listener(|this, _, _, cx| this.toggle_dock(Side::Right, cx))),
            )
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
            .child(
                div()
                    .child(
                        icon_button("file-new", Icon::FilePlus, "New project from a template (Ctrl+N)", self.new_menu, t)
                            .on_click(cx.listener(|this, _, w, cx| this.new_project(&NewProject, w, cx))),
                    )
                    .when(self.new_menu, |d| d.child(self.render_new_menu(t, cx))),
            )
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

    fn render_canvas(&mut self, t: &Theme, frame: Frame, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let draw_bar = self.render_draw_bar(t, cx);
        let bounds_cell = self.canvas_bounds.clone();
        let view = self.view;
        let size = self.doc.map.size() * view.zoom;
        // SVGs are pre-rendered (see `ensure_svg_background`); other images
        // are loaded by gpui, from the app (shipped templates) or from disk.
        let svg = self.svg_background.as_ref();
        let rendering_svg = svg.is_some_and(|b| b.image.is_none());
        let background = self.doc.map.background.clone().filter(|_| view.zoom > 0.).and_then(|bg| match svg {
            Some(b) => b.image.clone().map(ImageSource::from),
            None => Some(match self.doc.map.background_asset() {
                Some(asset) => ImageSource::from(asset.to_string()),
                None => ImageSource::from(bg),
            }),
        });
        let cursor = match (&self.drag, self.pending) {
            (Drag::Pan { .. } | Drag::Handle { .. } | Drag::Object { .. } | Drag::ShapeHandle { .. }, _) => {
                CursorStyle::ClosedHand
            }
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
            .when(rendering_svg, |d| {
                d.child(div().absolute().bottom_3().left_0().right_0().flex().justify_center().child(badge(
                    "Rendering table…".into(),
                    false,
                    t.accent,
                    t.accent_fg,
                    t,
                )))
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
            Some(Pending::Measure) if self.measure.is_some() => "Click the end point · Shift keeps it horizontal, vertical or diagonal",
            Some(Pending::Measure) => "Click the start point (or drag)",
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

        self.ensure_svg_background(cx);
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
        let left = self.render_dock(Side::Left, &t, cx);
        let right = self.render_dock(Side::Right, &t, cx);
        let left_handle = self.width_handle(Side::Left, &t, cx);
        let right_handle = self.width_handle(Side::Right, &t, cx);
        let canvas = self.render_canvas(&t, frame, cx);
        let status = self.render_status(&t);
        let content = div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_row()
                    .children(left)
                    .children(left_handle)
                    .child(canvas)
                    .children(right_handle)
                    .children(right),
            )
            .child(status);

        // No system fonts are loaded (see `main.rs`): set the font at the root
        // so nothing falls back to gpui's default `.SystemUIFont`.
        div()
            .size_full()
            .font_family(FONT_SANS)
            .track_focus(&self.focus)
            .key_context("Workspace")
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::toggle_left_dock))
            .on_action(cx.listener(Self::toggle_right_dock))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(Self::new_project))
            .on_action(cx.listener(Self::open_project))
            .on_action(cx.listener(Self::save_project))
            .on_action(cx.listener(Self::save_project_as))
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
