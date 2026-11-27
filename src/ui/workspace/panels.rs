//! The side panels: what's selected, the outline of everything on the table,
//! and the general settings (robot, map, project) in tabs.

use gpui::{
    Context, CursorStyle, Div, FontWeight, MouseButton, MouseDownEvent, SharedString, Stateful, Window, anchored,
    deferred, div, point, prelude::*, px, relative,
};

use super::{Drag, ExportJson, OpenProject, SaveProject, SaveProjectAs, Workspace};
use crate::assets::Icon;
use crate::geom::{SegmentKind, Shape};
use crate::model::{Origin, template};
use crate::theme::{FONT_MONO, RADIUS, Theme, alpha};
use crate::ui::canvas::{self, Selection};
use crate::ui::dock::{PanelId, Side};
use crate::ui::fields::Field;
use crate::ui::widgets::{button_icon, chip, icon, icon_button, label, primary_button, section, section_title};

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

/// Payload while a panel header is being dragged.
#[derive(Clone, Copy)]
pub struct DraggedPanel(PanelId);

/// Floating chip that follows the cursor while dragging a panel.
pub struct DragPreview(SharedString);

impl Render for DragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = Theme::get(cx);
        div()
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .h(px(28.))
            .rounded(RADIUS)
            .bg(t.bg)
            .border_1()
            .border_color(t.accent)
            .shadow_md()
            .text_size(px(12.))
            .child(icon(Icon::Grip, 14., t.muted))
            .child(self.0.clone())
    }
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
    /// Popover under the New button listing the templates.
    pub(super) fn render_new_menu(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let tt = *t;
        let mut menu = div()
            .id("new-menu")
            .flex()
            .flex_col()
            .w(px(260.))
            .p_1()
            .rounded(RADIUS)
            .bg(t.bg)
            .border_1()
            .border_color(t.border)
            .shadow_lg()
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.new_menu = false;
                cx.notify();
            }))
            .child(label(t, "New project from template").px_2().py_1());
        for (k, template) in template::list(self.templates_dir.as_deref()).into_iter().enumerate() {
            let detail = format!("{} × {}", canvas::format_mm(template.doc.map.width), canvas::format_mm(template.doc.map.height));
            let shapes = template.doc.map.obstacles.len() + template.doc.map.regions.len();
            menu = menu.child(
                div()
                    .id(("template", k))
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .h(px(30.))
                    .rounded(RADIUS)
                    .cursor_pointer()
                    .hover(move |s| s.bg(tt.elevated))
                    .child(icon(if template.file.is_some() { Icon::Map } else { Icon::FilePlus }, 14., t.muted))
                    .child(div().flex_1().min_w_0().truncate().text_size(px(13.)).child(template.name.clone()))
                    .child(
                        div()
                            .font_family(FONT_MONO)
                            .text_size(px(10.))
                            .text_color(t.muted)
                            .child(if shapes > 0 { format!("{shapes} shapes") } else { detail }),
                    )
                    .on_click(cx.listener(move |this, _, w, cx| this.new_from_template(template.clone(), w, cx))),
            );
        }
        if self.templates_dir.is_some() {
            menu = menu.child(div().h(px(1.)).my_1().bg(t.border)).child(
                div()
                    .id("open-templates-folder")
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .h(px(30.))
                    .rounded(RADIUS)
                    .cursor_pointer()
                    .text_size(px(12.))
                    .text_color(t.muted)
                    .hover(move |s| s.bg(tt.elevated))
                    .child(icon(Icon::FolderOpen, 14., t.muted))
                    .child("Open templates folder")
                    .on_click(cx.listener(|this, _, _, cx| this.open_templates_folder(cx))),
            );
        }
        deferred(anchored().offset(point(px(0.), px(32.))).snap_to_window_with_margin(px(8.)).child(menu))
            .with_priority(1)
    }

    pub(super) fn move_panel(&mut self, panel: PanelId, to: Side, before: Option<PanelId>, cx: &mut Context<Self>) {
        self.layout.move_panel(panel, to, before);
        self.layout.save();
        cx.notify();
    }

    /// A dock column, or a drop zone while a panel is dragged and the dock is empty.
    pub(super) fn render_dock(&self, side: Side, t: &Theme, cx: &mut Context<Self>) -> Option<Div> {
        let dock = self.layout.dock(side);
        let accent = t.accent;
        let on_drop = cx.listener(move |this, p: &DraggedPanel, _, cx| this.move_panel(p.0, side, None, cx));
        if dock.panels.is_empty() {
            return cx.has_active_drag().then(|| {
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .w(px(56.))
                    .h_full()
                    .border_1()
                    .border_color(t.border_strong)
                    .bg(t.bg)
                    .drag_over::<DraggedPanel>(move |s, _, _, _| s.bg(alpha(accent, 0.15)))
                    .on_drop(on_drop)
                    .child(icon(if side == Side::Left { Icon::PanelLeft } else { Icon::PanelRight }, 16., t.muted))
            });
        }
        let mut col = div()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(dock.width))
            .h_full()
            .bg(t.bg)
            .drag_over::<DraggedPanel>(move |s, _, _, _| s.bg(alpha(accent, 0.06)))
            .on_drop(on_drop);
        for (k, (&panel, &share)) in dock.panels.iter().zip(&dock.shares).enumerate() {
            if k > 0 {
                col = col.child(self.split_handle(side, k - 1, t, cx));
            }
            col = col.child(self.render_panel(panel, side, share, t, cx));
        }
        Some(col)
    }

    fn render_panel(&self, id: PanelId, side: Side, share: f32, t: &Theme, cx: &mut Context<Self>) -> Div {
        let (i, body) = match id {
            PanelId::Selected => (Icon::Pointer, self.render_selected(t, cx)),
            PanelId::Outline => (Icon::Layers, self.render_outline(t, cx)),
            PanelId::Settings => (Icon::Settings, self.render_settings(t, cx)),
        };
        let accent = t.accent;
        let other = side.other();
        let header = div()
            .id(("panel-header", id as usize))
            .flex()
            .flex_none()
            .items_center()
            .gap_2()
            .h(px(32.))
            .pl_2()
            .pr_1()
            .border_b_1()
            .border_color(t.border)
            .cursor(CursorStyle::OpenHand)
            .on_drag(DraggedPanel(id), |p, _, _, cx| cx.new(|_| DragPreview(p.0.title().into())))
            .drag_over::<DraggedPanel>(move |s, _, _, _| s.bg(alpha(accent, 0.15)))
            .on_drop(cx.listener(move |this, p: &DraggedPanel, _, cx| {
                cx.stop_propagation();
                this.move_panel(p.0, side, Some(id), cx);
            }))
            .child(icon(Icon::Grip, 14., t.muted))
            .child(icon(i, 14., t.muted))
            .child(div().flex_1().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).child(id.title()))
            .child(
                icon_button(
                    ("panel-move", id as usize),
                    if other == Side::Left { Icon::PanelLeft } else { Icon::PanelRight },
                    if other == Side::Left { "Move to the left dock" } else { "Move to the right dock" },
                    false,
                    t,
                )
                .on_click(cx.listener(move |this, _, _, cx| this.move_panel(id, other, None, cx))),
            );
        div()
            .flex()
            .flex_col()
            .flex_basis(relative(share))
            .flex_shrink()
            .min_h_0()
            .child(header)
            .child(
                div()
                    .id(("panel-body", id as usize))
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px_3()
                    .pb_3()
                    .child(body),
            )
    }

    /// Drag handle between two stacked panels.
    fn split_handle(&self, side: Side, index: usize, t: &Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        let accent = t.accent;
        div()
            .id(("split", side as usize * 16 + index))
            .flex_none()
            .h(px(5.))
            .w_full()
            .border_t_1()
            .border_color(t.border)
            .cursor(CursorStyle::ResizeUpDown)
            .hover(move |s| s.bg(alpha(accent, 0.4)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let shares = &this.layout.dock(side).shares;
                    let start = (shares[index], shares[index + 1]);
                    this.drag = Drag::DockSplit { side, index, start_y: canvas::to_vec(e.position).y, start };
                }),
            )
    }

    /// Drag handle between a dock and the canvas.
    pub(super) fn width_handle(&self, side: Side, t: &Theme, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        if self.layout.dock(side).panels.is_empty() {
            return None;
        }
        let accent = t.accent;
        Some(
            div()
                .id(("dock-width", side as usize))
                .flex_none()
                .flex()
                .justify_center()
                .w(px(5.))
                .h_full()
                .cursor(CursorStyle::ResizeLeftRight)
                .hover(move |s| s.bg(alpha(accent, 0.6)))
                .child(div().w(px(1.)).h_full().bg(t.border))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        let start = this.layout.dock(side).width;
                        this.drag = Drag::DockWidth { side, start_x: canvas::to_vec(e.position).x, start };
                    }),
                ),
        )
    }

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
                            ))),
                    )
                    .child(section_title("Template", t))
                    .child(label(t, "Save this table, its obstacles, regions and the robot to start new projects from."))
                    .child(
                        button_icon("save-template", Some(Icon::Save), "Save as template", false, t)
                            .on_click(cx.listener(|this, _, _, cx| this.save_template(cx))),
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
                    .child(label(t, "JSON with the start pose and every segment's parameters, as edited here."))
                    .child(
                        primary_button("project-export", Some(Icon::Download), "Export JSON", t)
                            .on_click(cx.listener(|this, _, w, cx| this.export_json(&ExportJson, w, cx))),
                    )
            }
        };
        div().flex().flex_col().child(tabs).child(body)
    }
}
