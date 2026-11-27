//! The side panels: what's selected, the outline of everything on the table,
//! and the general settings (robot, map, project) in tabs.

use gpui::{Context, Div, FontWeight, MouseButton, SharedString, Stateful, div, prelude::*, px};

use super::{ExportJson, OpenProject, SaveProject, SaveProjectAs, Workspace};
use crate::assets::Icon;
use crate::geom::{SegmentKind, Shape};
use crate::model::Origin;
use crate::theme::{FONT_MONO, RADIUS, Theme, alpha};
use crate::ui::canvas::{self, Selection};
use crate::ui::fields::Field;
use crate::ui::widgets::{button_icon, chip, icon, label, primary_button, section, section_title};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SettingsTab {
    #[default]
    Robot,
    Map,
    Project,
}

/// Outline groups that can be collapsed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Group {
    Paths,
    Obstacles,
    Regions,
}

/// Panel chrome: a header with icon and title above a scrolling body.
pub(super) fn panel(id: &'static str, title: &str, i: Icon, t: &Theme, body: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_col()
        .min_h_0()
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap_2()
                .h(px(32.))
                .px_3()
                .border_b_1()
                .border_color(t.border)
                .child(icon(i, 14., t.muted))
                .child(div().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).child(title.to_string())),
        )
        .child(div().id(id).flex_1().min_h_0().overflow_y_scroll().px_3().pb_3().child(body))
}

fn shape_icon(s: &Shape) -> Icon {
    match s {
        Shape::Rect { .. } => Icon::Square,
        Shape::Circle { .. } => Icon::Circle,
        Shape::Polygon { .. } => Icon::Pentagon,
    }
}

/// Short description of the segment ending at waypoint `index`.
fn waypoint_label(path: &crate::geom::Path, index: usize) -> String {
    let Some(seg) = index.checked_sub(1).and_then(|i| path.segments.get(i)) else {
        return "Start".into();
    };
    let rev = if seg.reverse { " ←" } else { "" };
    match seg.kind {
        SegmentKind::Straight { .. } => format!("Straight{rev}"),
        SegmentKind::Turn { angle, .. } if seg.is_in_place() => format!("Turn {:.0}°", angle.to_degrees()),
        SegmentKind::Turn { angle, radius, .. } => {
            format!("Arc {:.0}° r{}{rev}", angle.to_degrees(), canvas::format_mm(radius))
        }
        SegmentKind::Hermite { .. } => format!("Hermite{rev}"),
    }
}

/// What an outline row shows and selects.
struct Row {
    id: SharedString,
    lead: Div,
    name: String,
    detail: String,
    selects: Selection,
    depth: usize,
}

impl Workspace {
    /// One outline row: selectable, with a delete button revealed on hover.
    fn outline_row(&self, row: Row, t: &Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        let Row { id, lead, name, detail, selects: selected, depth } = row;
        let t = *t;
        let active = self.selection == selected;
        let trash_id = SharedString::from(format!("{id}-delete"));
        div()
            .id(id)
            .group("outline-row")
            .flex()
            .items_center()
            .gap_2()
            .h(px(28.))
            .pl(px(8. + 16. * depth as f32))
            .pr_1()
            .rounded(RADIUS)
            .cursor_pointer()
            .when(active, |d| d.bg(alpha(t.accent, 0.15)))
            .when(!active, move |d| d.hover(move |s| s.bg(t.elevated)))
            .child(lead)
            .child(div().flex_1().min_w_0().text_size(px(13.)).truncate().child(name))
            .child(div().font_family(FONT_MONO).text_size(px(10.)).text_color(t.muted).child(detail))
            .child(
                div()
                    .id(trash_id)
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(22.))
                    .rounded(RADIUS)
                    .opacity(0.)
                    .group_hover("outline-row", |s| s.opacity(1.))
                    .hover(move |s| s.bg(alpha(t.danger, 0.15)))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.delete(selected, cx);
                    }))
                    .child(icon(Icon::Trash, 13., t.danger)),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                if let Selection::Path(p) | Selection::Waypoint { path: p, .. } = selected {
                    this.active_path = Some(p);
                }
                this.selection = selected;
                cx.notify();
            }))
    }

    fn group_header(&self, g: Group, title: &str, count: usize, t: &Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        let collapsed = self.collapsed.contains(&g);
        let t = *t;
        div()
            .id(SharedString::from(format!("group-{title}")))
            .flex()
            .items_center()
            .gap_1()
            .h(px(28.))
            .mt_2()
            .cursor_pointer()
            .child(icon(if collapsed { Icon::ChevronRight } else { Icon::ChevronDown }, 14., t.muted))
            .child(section_title(title, &t))
            .child(div().text_size(px(11.)).text_color(t.muted).child(count.to_string()))
            .on_click(cx.listener(move |this, _, _, cx| {
                if !this.collapsed.remove(&g) {
                    this.collapsed.insert(g);
                }
                cx.notify();
            }))
    }

    pub(super) fn render_outline(&self, t: &Theme, cx: &mut Context<Self>) -> Div {
        let mut list = div().flex().flex_col();

        list = list.child(self.group_header(Group::Paths, "Paths", self.doc.paths.len(), t, cx));
        if !self.collapsed.contains(&Group::Paths) {
            if self.doc.paths.is_empty() {
                list = list.child(label(t, "No paths yet — use New path in the toolbar").pl_2().py_1());
            }
            for (i, path) in self.doc.paths.iter().enumerate() {
                let expanded = self.expanded_paths.contains(&i);
                let chevron = div()
                    .id(("expand-path", i))
                    .flex()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        if !this.expanded_paths.remove(&i) {
                            this.expanded_paths.insert(i);
                        }
                        cx.notify();
                    }))
                    .child(icon(if expanded { Icon::ChevronDown } else { Icon::ChevronRight }, 14., t.muted));
                let lead = div().flex().items_center().gap_1().child(chevron).child(icon(Icon::Route, 14., t.fg));
                list = list.child(self.outline_row(Row { id: format!("path-{i}").into(), lead, name: path.name.clone(), detail: format!("{:.2} m", path.length() / 1000.), selects: Selection::Path(i), depth: 0 }, t, cx));
                if expanded {
                    for index in 0..=path.segments.len() {
                        let lead = div()
                            .w(px(18.))
                            .font_family(FONT_MONO)
                            .text_size(px(10.))
                            .text_color(t.muted)
                            .child(index.to_string());
                        list = list.child(self.outline_row(Row { id: format!("waypoint-{i}-{index}").into(), lead, name: waypoint_label(path, index), detail: String::new(), selects: Selection::Waypoint { path: i, index }, depth: 1 }, t, cx));
                    }
                }
            }
        }

        list = list.child(self.group_header(Group::Obstacles, "Obstacles", self.doc.map.obstacles.len(), t, cx));
        if !self.collapsed.contains(&Group::Obstacles) {
            for (i, o) in self.doc.map.obstacles.iter().enumerate() {
                let lead = div().flex().child(icon(shape_icon(&o.shape), 14., t.fg));
                list = list.child(self.outline_row(Row { id: format!("obstacle-{i}").into(), lead, name: o.name.clone(), detail: String::new(), selects: Selection::Obstacle(i), depth: 0 }, t, cx));
            }
        }

        list = list.child(self.group_header(Group::Regions, "Regions", self.doc.map.regions.len(), t, cx));
        if !self.collapsed.contains(&Group::Regions) {
            for (i, r) in self.doc.map.regions.iter().enumerate() {
                let lead = div().size(px(10.)).rounded_full().bg(canvas::region_color(t, r.color));
                list = list.child(self.outline_row(Row { id: format!("region-{i}").into(), lead, name: r.name.clone(), detail: String::new(), selects: Selection::Region(i), depth: 0 }, t, cx));
            }
        }
        list
    }

    pub(super) fn render_selected(&self, t: &Theme, cx: &mut Context<Self>) -> Div {
        let body = match self.selection {
            Selection::None => None,
            Selection::Path(_) => self.render_path(t, cx),
            Selection::Waypoint { path, .. } => self.render_waypoint(t, cx).map(|d| {
                let name = self.doc.paths[path].name.clone();
                d.child(
                    button_icon("select-parent-path", Some(Icon::Route), &format!("Path: {name}"), false, t)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.selection = Selection::Path(path);
                            cx.notify();
                        })),
                )
            }),
            Selection::Obstacle(_) | Selection::Region(_) => self.render_shape(t, cx),
        };
        body.unwrap_or_else(|| {
            div()
                .flex()
                .flex_col()
                .gap_1()
                .py_3()
                .child(div().text_size(px(13.)).child("Nothing selected"))
                .child(label(t, "Click a path, waypoint or shape on the table or in the outline."))
        })
    }

    pub(super) fn render_settings(&self, t: &Theme, cx: &mut Context<Self>) -> Div {
        let tab = self.settings_tab;
        let mut tabs = div().flex().gap_1().py_2();
        for (id, i, name, which) in [
            ("tab-robot", Icon::Bot, "Robot", SettingsTab::Robot),
            ("tab-map", Icon::Map, "Map", SettingsTab::Map),
            ("tab-project", Icon::Settings, "Project", SettingsTab::Project),
        ] {
            let active = tab == which;
            let t2 = *t;
            tabs = tabs.child(
                div()
                    .id(id)
                    .flex()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .gap_1()
                    .h(px(28.))
                    .rounded(RADIUS)
                    .text_size(px(12.))
                    .cursor_pointer()
                    .when(active, |d| d.bg(t2.elevated).font_weight(FontWeight::MEDIUM))
                    .when(!active, move |d| d.text_color(t2.muted).hover(move |s| s.text_color(t2.fg)))
                    .child(icon(i, 14., if active { t.fg } else { t.muted }))
                    .child(name)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.settings_tab = which;
                        cx.notify();
                    })),
            );
        }

        let body = match tab {
            SettingsTab::Robot => section(t)
                .child(self.fields(
                    &[Field::RobotLength, Field::RobotWidth, Field::RobotOffsetX, Field::RobotOffsetY, Field::RobotMargin],
                    t,
                    cx,
                ))
                .child(label(t, "The offset moves the body relative to the rotation centre (x forward, y left).")),
            SettingsTab::Map => {
                let origin = self.doc.map.origin;
                let background = self
                    .doc
                    .map
                    .background
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map_or("None".to_string(), |n| n.to_string_lossy().into_owned());
                section(t)
                    .child(self.fields(&[Field::MapName, Field::MapYear, Field::MapWidth, Field::MapHeight], t, cx))
                    .child(label(t, "Origin"))
                    .child(
                        div()
                            .flex()
                            .gap_1()
                            .child(chip("origin-bl", "Bottom-left", origin == Origin::BottomLeft, t).on_click(cx.listener(
                                |this, _, _, cx| this.edit_doc(cx, |d| d.map.origin = Origin::BottomLeft),
                            )))
                            .child(chip("origin-tl", "Top-left", origin == Origin::TopLeft, t).on_click(cx.listener(
                                |this, _, _, cx| this.edit_doc(cx, |d| d.map.origin = Origin::TopLeft),
                            ))),
                    )
                    .child(Self::info(t, "Background", background))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_1()
                            .child(button_icon("bg-set", None, "Set image", false, t).on_click(cx.listener(
                                |this, _, w, cx| this.pick_background(w, cx),
                            )))
                            .child(button_icon("bg-clear", None, "Clear", false, t).on_click(cx.listener(
                                |this, _, _, cx| this.edit_doc(cx, |d| d.map.background = None),
                            )))
                            .child(button_icon("map-open", Some(Icon::FolderOpen), "Open map", false, t).on_click(
                                cx.listener(|this, _, w, cx| this.open_map(&super::OpenMap, w, cx)),
                            ))
                            .child(button_icon("map-save", Some(Icon::Save), "Save map", false, t).on_click(
                                cx.listener(|this, _, w, cx| this.save_map(&super::SaveMap, w, cx)),
                            )),
                    )
            }
            SettingsTab::Project => {
                let file = self
                    .project_path
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map_or("Not saved yet".to_string(), |n| n.to_string_lossy().into_owned());
                section(t)
                    .child(Self::info(t, "File", file))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_1()
                            .child(button_icon("project-open", Some(Icon::FolderOpen), "Open", false, t).on_click(
                                cx.listener(|this, _, w, cx| this.open_project(&OpenProject, w, cx)),
                            ))
                            .child(button_icon("project-save", Some(Icon::Save), "Save", false, t).on_click(
                                cx.listener(|this, _, w, cx| this.save_project(&SaveProject, w, cx)),
                            ))
                            .child(button_icon("project-save-as", None, "Save as…", false, t).on_click(cx.listener(
                                |this, _, w, cx| this.save_project_as(&SaveProjectAs, w, cx),
                            ))),
                    )
                    .child(section_title("Export", t))
                    .child(self.fields(&[Field::SampleStep], t, cx))
                    .child(label(t, "JSON includes segments and samples every step."))
                    .child(
                        primary_button("project-export", Some(Icon::Download), "Export JSON", t)
                            .on_click(cx.listener(|this, _, w, cx| this.export_json(&ExportJson, w, cx))),
                    )
            }
        };
        div().flex().flex_col().child(tabs).child(body)
    }
}
