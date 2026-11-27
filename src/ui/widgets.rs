//! Small building blocks in the RUR Robotics style: square corners, ink
//! borders, mono uppercase labels, display titles ending in a coloured dot.

use gpui::{Div, ElementId, FontWeight, Rgba, Stateful, div, prelude::*, px};

use crate::theme::{FONT_DISPLAY, FONT_MONO, Theme};

/// "PATHS." — Oswald bold uppercase with a coloured full stop.
pub fn section_title(title: &str, dot: Rgba) -> Div {
    div()
        .flex()
        .font_family(FONT_DISPLAY)
        .font_weight(FontWeight::BOLD)
        .text_size(px(18.))
        .line_height(px(22.))
        .child(title.to_uppercase())
        .child(div().text_color(dot).child("."))
}

/// A panel section: coloured strip on top, padded content below.
pub fn section(t: &Theme, strip: Rgba) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .pb_4()
        .border_b_2()
        .border_color(t.border)
        .child(div().h(px(6.)).bg(strip).mx(px(-16.)).mb_2())
}

pub fn mono_label(t: &Theme, text: impl Into<gpui::SharedString>) -> Div {
    div()
        .font_family(FONT_MONO)
        .font_weight(FontWeight::BOLD)
        .text_size(px(11.))
        .text_color(t.ink_muted)
        .child(text.into())
}

/// Toggle chip (FilterChip on the website): 2px border, filled when active.
pub fn chip(id: impl Into<ElementId>, label: &str, active: bool, color: Rgba, color_fg: Rgba, t: &Theme) -> Stateful<Div> {
    let t = *t;
    div()
        .id(id)
        .flex()
        .items_center()
        .h(px(26.))
        .px_2()
        .border_2()
        .border_color(if active { color } else { t.border })
        .font_family(FONT_MONO)
        .font_weight(FontWeight::BOLD)
        .text_size(px(11.))
        .cursor_pointer()
        .when(active, |d| d.bg(color).text_color(color_fg))
        .when(!active, move |d| d.hover(move |s| s.bg(t.foreground).text_color(t.background)))
        .child(label.to_uppercase())
}

/// Push button with a filled hover in `accent` (the website's wipe, without the sweep).
pub fn button(id: impl Into<ElementId>, label: &str, accent: Rgba, accent_fg: Rgba, t: &Theme) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .h(px(26.))
        .px_2()
        .border_2()
        .border_color(t.border)
        .font_family(FONT_MONO)
        .font_weight(FontWeight::BOLD)
        .text_size(px(11.))
        .cursor_pointer()
        .hover(move |s| s.bg(accent).text_color(accent_fg))
        .child(label.to_uppercase())
}

/// Solid call-to-action button (the navbar's "Join us").
pub fn solid_button(id: impl Into<ElementId>, label: &str, bg: Rgba, fg: Rgba, t: &Theme) -> Stateful<Div> {
    let t = *t;
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .h(px(26.))
        .px_3()
        .bg(bg)
        .text_color(fg)
        .border_2()
        .border_color(bg)
        .font_family(FONT_MONO)
        .font_weight(FontWeight::BOLD)
        .text_size(px(11.))
        .cursor_pointer()
        .hover(move |s| s.bg(t.foreground).border_color(t.foreground).text_color(t.background))
        .child(label.to_uppercase())
}

/// Status chip: outlined, or solid when `alarm`.
pub fn badge(label: String, alarm: bool, color: Rgba, color_fg: Rgba, t: &Theme) -> Div {
    div()
        .flex()
        .items_center()
        .px_2()
        .h(px(18.))
        .border_2()
        .border_color(if alarm { color } else { t.border })
        .when(alarm, |d| d.bg(color).text_color(color_fg))
        .font_family(FONT_MONO)
        .font_weight(FontWeight::BOLD)
        .text_size(px(10.))
        .child(label)
}
