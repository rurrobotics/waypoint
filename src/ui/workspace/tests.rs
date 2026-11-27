//! Drives the real workspace through gpui's headless test platform.

use gpui::{Entity, Modifiers, MouseButton, Point, TestAppContext, VisualTestContext, point, px};

use super::{Pending, SegTool, Workspace};
use crate::geom::{SegmentKind, Shape, vec2};
use crate::theme::Theme;
use crate::ui::canvas::Selection;
use crate::ui::dock::{PanelId, Side};
use crate::ui::fields::Field;

fn setup(cx: &mut TestAppContext) -> (Entity<Workspace>, &mut VisualTestContext) {
    let (ws, cx) = cx.add_window_view(|window, cx| {
        Theme::sync(window, cx);
        super::bind_keys(cx);
        Workspace::new(window, cx)
    });
    // The first frame records the canvas bounds, the next one fits the table.
    for _ in 0..2 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
    (ws, cx)
}

/// Screen position of a table point (mm).
fn at(ws: &Entity<Workspace>, cx: &mut VisualTestContext, x: f64, y: f64) -> Point<gpui::Pixels> {
    ws.read_with(cx, |w, _| {
        let s = w.xform().expect("view is fitted").to_screen(vec2(x, y));
        point(px(s.x as f32), px(s.y as f32))
    })
}

fn click(ws: &Entity<Workspace>, cx: &mut VisualTestContext, x: f64, y: f64) {
    let p = at(ws, cx, x, y);
    cx.simulate_mouse_move(p, None, Modifiers::none());
    cx.simulate_click(p, Modifiers::none());
    cx.run_until_parked();
}

fn drag(ws: &Entity<Workspace>, cx: &mut VisualTestContext, from: (f64, f64), to: (f64, f64)) {
    drag_with(ws, cx, from, to, Modifiers::none());
}

fn drag_with(ws: &Entity<Workspace>, cx: &mut VisualTestContext, from: (f64, f64), to: (f64, f64), m: Modifiers) {
    let a = at(ws, cx, from.0, from.1);
    let b = at(ws, cx, to.0, to.1);
    cx.simulate_mouse_move(a, None, m);
    cx.simulate_mouse_down(a, MouseButton::Left, m);
    for i in 1..=8 {
        let t = i as f32 / 8.;
        let p = point(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
        cx.simulate_mouse_move(p, MouseButton::Left, m);
    }
    cx.simulate_mouse_up(b, MouseButton::Left, m);
    cx.run_until_parked();
}

fn set_seg_tool(ws: &Entity<Workspace>, cx: &mut VisualTestContext, tool: SegTool) {
    ws.update(cx, |w, cx| {
        w.seg_tool = tool;
        cx.notify();
    });
}

#[gpui::test]
fn draws_path_with_every_segment_kind(cx: &mut TestAppContext) {
    let (ws, cx) = setup(cx);

    cx.simulate_keystrokes("p");
    assert_eq!(ws.read_with(cx, |w, _| w.pending), Some(Pending::DrawPath));
    click(&ws, cx, 300., 300.); // start
    click(&ws, cx, 1200., 300.); // straight ahead
    click(&ws, cx, 1200., 1000.); // straight to the left: auto turn first
    set_seg_tool(&ws, cx, SegTool::TurnArc);
    click(&ws, cx, 1700., 1400.);
    set_seg_tool(&ws, cx, SegTool::Hermite);
    click(&ws, cx, 2500., 1000.);
    set_seg_tool(&ws, cx, SegTool::TurnInPlace);
    click(&ws, cx, 2500., 300.);

    ws.read_with(cx, |w, _| {
        let path = &w.doc.paths[0];
        let kinds: Vec<&str> = path
            .segments
            .iter()
            .map(|s| match s.kind {
                SegmentKind::Straight { .. } => "straight",
                SegmentKind::Turn { .. } if s.is_in_place() => "spin",
                SegmentKind::Turn { .. } => "arc",
                SegmentKind::Hermite { .. } => "hermite",
            })
            .collect();
        assert_eq!(kinds, ["straight", "spin", "straight", "arc", "hermite", "spin"]);
        assert!(path.discontinuities().is_empty());
        let end = path.end_pose();
        assert!((end.x - 2500.).abs() < 1e-6 && (end.y - 1000.).abs() < 1e-6);
        assert!((end.theta.to_degrees() + 90.).abs() < 1e-6, "faces down towards the last click");
        assert_eq!(w.selection, Selection::Waypoint { path: 0, index: 6 });
    });

    // Enter ends the session; further clicks select instead of adding.
    cx.simulate_keystrokes("enter");
    click(&ws, cx, 2000., 1800.);
    ws.read_with(cx, |w, _| {
        assert_eq!(w.pending, None);
        assert_eq!(w.doc.paths[0].segments.len(), 6);
    });
}

#[gpui::test]
fn dragging_a_waypoint_is_one_undo_step(cx: &mut TestAppContext) {
    let (ws, cx) = setup(cx);
    cx.simulate_keystrokes("p");
    click(&ws, cx, 300., 300.);
    click(&ws, cx, 1200., 300.);
    click(&ws, cx, 1200., 1000.);
    cx.simulate_keystrokes("enter");

    drag(&ws, cx, (1200., 1000.), (1500., 1200.));
    ws.read_with(cx, |w, _| {
        let SegmentKind::Straight { to } = w.doc.paths[0].segments[2].kind else { panic!() };
        assert!(to.distance(vec2(1500., 1200.)) < 10., "{to:?}");
        // The auto turn followed so the straight stays aligned.
        assert!(w.doc.paths[0].discontinuities().is_empty());
    });

    cx.simulate_keystrokes("ctrl-z");
    cx.run_until_parked();
    ws.read_with(cx, |w, _| {
        let SegmentKind::Straight { to } = w.doc.paths[0].segments[2].kind else { panic!() };
        assert_eq!(to, vec2(1200., 1000.));
    });

    cx.simulate_keystrokes("ctrl-shift-z");
    cx.run_until_parked();
    ws.read_with(cx, |w, _| {
        let SegmentKind::Straight { to } = w.doc.paths[0].segments[2].kind else { panic!() };
        assert!(to.distance(vec2(1500., 1200.)) < 10.);
    });
}

#[gpui::test]
fn obstacle_on_the_path_is_reported(cx: &mut TestAppContext) {
    let (ws, cx) = setup(cx);
    cx.simulate_keystrokes("p");
    click(&ws, cx, 300., 1000.);
    click(&ws, cx, 2700., 1000.);
    cx.simulate_keystrokes("enter");

    // New obstacle is a one-shot: one drag draws it, then back to selecting.
    cx.simulate_keystrokes("o");
    drag(&ws, cx, (1400., 900.), (1600., 1100.));

    ws.update(cx, |w, _| {
        assert_eq!(w.pending, None);
        assert_eq!(w.doc.map.obstacles.len(), 1);
        let report = w.ensure_analysis().report.clone().expect("active path is analysed");
        assert_eq!(report.events, 1);
        assert_eq!(report.min_clearance.unwrap().0, 0.);
    });

    // The new obstacle is selected; deleting it clears the path again.
    assert_eq!(ws.read_with(cx, |w, _| w.selection), Selection::Obstacle(0));
    cx.simulate_keystrokes("delete");
    cx.run_until_parked();
    ws.update(cx, |w, _| {
        assert!(w.doc.map.obstacles.is_empty());
        assert_eq!(w.ensure_analysis().report.as_ref().unwrap().events, 0);
    });
}

#[gpui::test]
fn editing_a_field_by_typing(cx: &mut TestAppContext) {
    let (ws, cx) = setup(cx);
    ws.update_in(cx, |w, window, cx| w.start_edit(Field::RobotLength, window, cx));
    cx.run_until_parked();
    cx.simulate_keystrokes("4 2 0 enter");
    cx.run_until_parked();
    ws.read_with(cx, |w, _| {
        assert_eq!(w.doc.robot.length, 420.);
        assert!(w.edit.is_none());
    });
    cx.simulate_keystrokes("ctrl-z");
    cx.run_until_parked();
    ws.read_with(cx, |w, _| assert_eq!(w.doc.robot.length, 300.));
}

#[gpui::test]
fn shift_drag_draws_a_square_region(cx: &mut TestAppContext) {
    let (ws, cx) = setup(cx);
    cx.simulate_keystrokes("r");
    drag_with(&ws, cx, (500., 500.), (900., 700.), Modifiers::shift());
    cx.simulate_keystrokes("r");
    drag(&ws, cx, (1500., 500.), (1900., 700.));

    ws.read_with(cx, |w, _| {
        let sizes: Vec<_> = w
            .doc
            .map
            .regions
            .iter()
            .map(|r| match r.shape {
                Shape::Rect { size, .. } => size,
                _ => panic!("expected rectangles"),
            })
            .collect();
        assert_eq!(sizes, [vec2(400., 400.), vec2(400., 200.)]);
        assert_eq!(w.pending, None);
    });
}

#[gpui::test]
fn docks_resize_and_panels_move(cx: &mut TestAppContext) {
    let (ws, cx) = setup(cx);
    let width = ws.read_with(cx, |w, _| w.layout.left.width);

    // The handle sits just right of the left dock.
    let y = px(400.);
    let x = px(width + 2.);
    cx.simulate_mouse_move(point(x, y), None, Modifiers::none());
    cx.simulate_mouse_down(point(x, y), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(point(x + px(100.), y), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(point(x + px(100.), y), MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    ws.read_with(cx, |w, _| assert!((w.layout.left.width - (width + 100.)).abs() < 0.5, "{}", w.layout.left.width));

    // Move every panel to the right: the left dock disappears and the canvas grows.
    let canvas_before = ws.read_with(cx, |w, _| w.canvas_bounds.get().unwrap().size.width);
    ws.update(cx, |w, cx| {
        w.move_panel(PanelId::Selected, Side::Right, None, cx);
        w.move_panel(PanelId::Outline, Side::Right, Some(PanelId::Settings), cx);
    });
    for _ in 0..2 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
    ws.read_with(cx, |w, _| {
        assert!(w.layout.left.panels.is_empty());
        assert_eq!(w.layout.right.panels, [PanelId::Outline, PanelId::Settings, PanelId::Selected]);
        assert!(w.canvas_bounds.get().unwrap().size.width > canvas_before);
    });
}

#[gpui::test]
fn project_saves_and_loads_as_plain_json(cx: &mut TestAppContext) {
    let (ws, cx) = setup(cx);
    cx.simulate_keystrokes("p");
    click(&ws, cx, 300., 300.);
    click(&ws, cx, 1500., 300.);
    cx.simulate_keystrokes("enter");

    let file = std::env::temp_dir().join(format!("waypoint-ui-{}.json", std::process::id()));
    let saved = ws.update(cx, |w, cx| {
        w.write_project(file.clone(), cx);
        assert!(!w.dirty);
        w.doc.clone()
    });
    let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(json["paths"][0]["segments"][0]["type"], "straight");

    ws.update(cx, |w, cx| {
        w.doc.paths.clear();
        w.load_project(file.clone(), cx);
        assert_eq!(w.doc, saved);
        assert_eq!(w.active_path, Some(0));
    });
    std::fs::remove_file(file).ok();
}

#[gpui::test]
fn new_project_from_a_saved_template(cx: &mut TestAppContext) {
    let (ws, cx) = setup(cx);
    let dir = std::env::temp_dir().join(format!("waypoint-ui-templates-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();

    // A table with one obstacle and a path, saved as a template.
    cx.simulate_keystrokes("o");
    drag(&ws, cx, (1400., 900.), (1600., 1100.));
    cx.simulate_keystrokes("p");
    click(&ws, cx, 300., 300.);
    click(&ws, cx, 900., 300.);
    cx.simulate_keystrokes("enter");
    ws.update(cx, |w, cx| {
        w.templates_dir = Some(dir.clone());
        w.save_template(cx);
        w.dirty = false; // skip the discard prompt
    });

    cx.simulate_keystrokes("ctrl-n");
    let template = ws.read_with(cx, |w, _| {
        assert!(w.new_menu);
        let all = crate::model::template::list(w.templates_dir.as_deref());
        assert_eq!(all.len(), 2);
        all[1].clone()
    });
    ws.update_in(cx, |w, window, cx| w.new_from_template(template, window, cx));
    cx.run_until_parked();

    ws.read_with(cx, |w, _| {
        assert!(!w.new_menu);
        assert_eq!(w.doc.map.obstacles.len(), 1);
        assert!(w.doc.paths.is_empty(), "templates carry no paths");
        assert!(w.project_path.is_none());
    });
    std::fs::remove_dir_all(&dir).ok();
}
