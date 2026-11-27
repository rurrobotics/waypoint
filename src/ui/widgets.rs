//! Small building blocks: section labels, toggles, buttons and badges.

use gpui::{
    AnyView, App, Context, Div, ElementId, FontWeight, Hsla, MouseButton, Rgba, SharedString, Stateful, Svg, Window, div, prelude::*, px,
    svg,
};

use crate::assets::Icon;
use crate::theme::{FONT_MONO, FONT_SANS, RADIUS, Theme};

/// An icon; svgs don't inherit the text colour, so it's passed explicitly.
/// Buttons sit in the toolbar, which is also the window's drag area: keep
/// mouse-downs on them from starting a window move.
fn pressable(d: Stateful<Div>) -> Stateful<Div> {
    d.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
}

pub fn icon(icon: Icon, size: f32, color: impl Into<Hsla>) -> Svg {
    svg().path(icon.path()).size(px(size)).flex_none().text_color(color.into())
}

/// Hover tooltip content.
pub struct Tooltip(SharedString);

impl Render for Tooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = Theme::get(cx);
        // A tooltip is its own view: it doesn't inherit the workspace font.
        div().font_family(FONT_SANS).px_2().py_1().rounded(RADIUS).bg(t.fg).text_color(t.bg).text_size(px(12.)).child(self.0.clone())
    }
}

pub fn tooltip(text: impl Into<SharedString>) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let text = text.into();
    move |_, cx| cx.new(|_| Tooltip(text.clone())).into()
}

/// Square icon-only button with a tooltip; `active` fills it like a pressed toggle.
pub fn icon_button(id: impl Into<ElementId>, i: Icon, tip: &str, active: bool, t: &Theme) -> Stateful<Div> {
    let t = *t;
    pressable(div().id(id))
        .flex()
        .items_center()
        .justify_center()
        .size(px(28.))
        .flex_none()
        .rounded(RADIUS)
        .cursor_pointer()
        .text_color(if active { t.accent } else { t.fg })
        .when(active, |d| d.bg(crate::theme::alpha(t.accent, 0.15)))
        .when(!active, move |d| d.hover(move |s| s.bg(t.elevated)))
        .tooltip(tooltip(tip.to_string()))
        .child(icon(i, 16., if active { t.accent } else { t.fg }))
}

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
    chip_icon(id, None, text, active, t)
}

pub fn chip_icon(id: impl Into<ElementId>, i: Option<Icon>, text: &str, active: bool, t: &Theme) -> Stateful<Div> {
    let t = *t;
    pressable(div().id(id))
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
        .when_some(i, |d, i| d.child(icon(i, 14., if active { t.accent_fg } else { t.fg })))
        .child(text.to_string())
}

/// Outline button with an optional leading icon; `danger` tints the hover red.
pub fn button_icon(id: impl Into<ElementId>, i: Option<Icon>, text: &str, danger: bool, t: &Theme) -> Stateful<Div> {
    let t = *t;
    pressable(div().id(id))
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
        .group("button")
        .hover(move |s| if danger { s.bg(t.danger).border_color(t.danger).text_color(t.danger_fg) } else { s.bg(t.elevated) })
        .when_some(i, |d, i| {
            d.child(icon(i, 14., t.fg).when(danger, |s| s.group_hover("button", move |s| s.text_color(t.danger_fg))))
        })
        .child(text.to_string())
}

/// Filled primary button.
pub fn primary_button(id: impl Into<ElementId>, i: Option<Icon>, text: &str, t: &Theme) -> Stateful<Div> {
    pressable(div().id(id))
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
        .when_some(i, |d, i| d.child(icon(i, 14., t.accent_fg)))
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
