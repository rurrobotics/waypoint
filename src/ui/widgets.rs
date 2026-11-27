//! Small building blocks: section labels, toggles, buttons and badges.

use gpui::{Div, ElementId, FontWeight, Rgba, Stateful, div, prelude::*, px};

use crate::theme::{FONT_MONO, RADIUS, Theme};

/// Small semibold section heading.
pub fn section_title(title: &str, t: &Theme) -> Div {
    div().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(t.fg).child(title.to_string())
}

/// A panel section separated from the next by a hairline.
pub fn section(t: &Theme) -> Div {
    div().flex().flex_col().gap_2().py_3().border_b_1().border_color(t.border)
}

/// Secondary text label.
pub fn label(t: &Theme, text: impl Into<gpui::SharedString>) -> Div {
    div().text_size(px(12.)).text_color(t.muted).child(text.into())
}

/// Toggle: subtle outline, filled with the accent when active.
pub fn chip(id: impl Into<ElementId>, text: &str, active: bool, t: &Theme) -> Stateful<Div> {
    let t = *t;
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_1()
        .h(px(26.))
        .px_2()
        .rounded(RADIUS)
        .border_1()
        .text_size(px(12.))
        .cursor_pointer()
        .when(active, |d| d.bg(t.accent).border_color(t.accent).text_color(t.accent_fg))
        .when(!active, move |d| d.border_color(t.border).hover(move |s| s.bg(t.elevated)))
        .child(text.to_string())
}

/// Outline button; `danger` tints the hover red.
pub fn button(id: impl Into<ElementId>, text: &str, danger: bool, t: &Theme) -> Stateful<Div> {
    let t = *t;
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .gap_1()
        .h(px(26.))
        .px_2()
        .rounded(RADIUS)
        .border_1()
        .border_color(t.border)
        .text_size(px(12.))
        .cursor_pointer()
        .hover(move |s| if danger { s.bg(t.danger).border_color(t.danger).text_color(t.danger_fg) } else { s.bg(t.elevated) })
        .child(text.to_string())
}

/// Filled primary button.
pub fn primary_button(id: impl Into<ElementId>, text: &str, t: &Theme) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .gap_1()
        .h(px(26.))
        .px_3()
        .rounded(RADIUS)
        .bg(t.accent)
        .text_color(t.accent_fg)
        .text_size(px(12.))
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .hover(|s| s.opacity(0.9))
        .child(text.to_string())
}

/// Status pill: neutral, or filled with `color` when `alarm`.
pub fn badge(text: String, alarm: bool, color: Rgba, color_fg: Rgba, t: &Theme) -> Div {
    div()
        .flex()
        .items_center()
        .px_2()
        .h(px(18.))
        .rounded_full()
        .when(alarm, |d| d.bg(color).text_color(color_fg))
        .when(!alarm, |d| d.bg(t.elevated).text_color(t.muted))
        .font_family(FONT_MONO)
        .text_size(px(10.))
        .child(text)
}
