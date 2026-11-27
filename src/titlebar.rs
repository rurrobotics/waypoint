//! App header plus client-side window decorations for compositors that don't
//! draw them (e.g. GNOME/Mutter on Wayland). The header is always shown; the
//! window controls, border, shadow and resize edges only with client-side
//! decorations.

use std::{cell::Cell, rc::Rc};

use gpui::{
    AnyElement, Bounds, CursorStyle, Decorations, FontWeight, Hitbox, HitboxBehavior, Hsla, MouseButton, Pixels,
    Point, ResizeEdge, Rgba, Size, Window, canvas, div, point, prelude::*, px, transparent_black,
};

use crate::theme::{FONT_DISPLAY, FONT_MONO, Theme};

const TITLEBAR_HEIGHT: Pixels = px(44.);
const SHADOW_SIZE: Pixels = px(10.);

/// Per-window titlebar state. Keep one alive alongside your root view.
#[derive(Clone, Default)]
pub struct TitlebarState {
    /// Set on mouse-down in the titlebar; the move starts on the first drag so
    /// the compositor doesn't swallow double-clicks.
    should_move: Rc<Cell<bool>>,
}

/// Wraps `content` in the app frame: header (with `subtitle`) on top, and
/// border, shadow, and resize edges when the window uses client-side decorations.
pub fn window_frame(
    window: &mut Window,
    theme: &Theme,
    state: &TitlebarState,
    subtitle: impl IntoElement,
    actions: impl IntoElement,
    content: impl IntoElement,
) -> AnyElement {
    let decorations = window.window_decorations();
    let body = div()
        .size_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .bg(theme.background)
        .text_color(theme.foreground)
        .child(titlebar(window, theme, state, subtitle, actions))
        .child(div().flex_1().min_h_0().child(content));

    let Decorations::Client { tiling } = decorations else {
        window.set_client_inset(px(0.));
        return body.into_any_element();
    };

    window.set_client_inset(SHADOW_SIZE);

    div()
        .id("window-backdrop")
        .size_full()
        .bg(transparent_black())
        // Resize cursor + hitbox over the shadow area.
        .child(
            canvas(
                |_bounds, window, _cx| {
                    window.insert_hitbox(
                        Bounds::new(point(px(0.), px(0.)), window.window_bounds().get_bounds().size),
                        HitboxBehavior::Normal,
                    )
                },
                move |_bounds, hitbox: Hitbox, window, _cx| {
                    let size = window.window_bounds().get_bounds().size;
                    if let Some(edge) = resize_edge(window.mouse_position(), size) {
                        window.set_cursor_style(cursor_for_edge(edge), &hitbox);
                    }
                },
            )
            .size_full()
            .absolute(),
        )
        .when(!tiling.top, |d| d.pt(SHADOW_SIZE))
        .when(!tiling.bottom, |d| d.pb(SHADOW_SIZE))
        .when(!tiling.left, |d| d.pl(SHADOW_SIZE))
        .when(!tiling.right, |d| d.pr(SHADOW_SIZE))
        .on_mouse_move(|_, window, _| window.refresh())
        .on_mouse_down(MouseButton::Left, |e, window, _| {
            let size = window.window_bounds().get_bounds().size;
            if let Some(edge) = resize_edge(e.position, size) {
                window.start_window_resize(edge);
            }
        })
        .child(
            body.cursor(CursorStyle::Arrow)
                .border_color(theme.border)
                .when(!tiling.top, |d| d.border_t_2())
                .when(!tiling.bottom, |d| d.border_b_2())
                .when(!tiling.left, |d| d.border_l_2())
                .when(!tiling.right, |d| d.border_r_2())
                .when(!tiling.is_tiled(), |d| {
                    d.shadow(vec![gpui::BoxShadow {
                        color: Hsla { h: 0., s: 0., l: 0., a: 0.35 },
                        blur_radius: SHADOW_SIZE / 2.,
                        spread_radius: px(0.),
                        offset: point(px(0.), px(0.)),
                    }])
                })
                // Keep the resize cursor from leaking into the window body.
                .on_mouse_move(|_, _, cx| cx.stop_propagation()),
        )
        .into_any_element()
}

fn titlebar(
    window: &Window,
    theme: &Theme,
    state: &TitlebarState,
    subtitle: impl IntoElement,
    actions: impl IntoElement,
) -> impl IntoElement {
    let client = matches!(window.window_decorations(), Decorations::Client { .. });
    let controls = window.window_controls();
    let maximized = window.is_maximized();

    div()
        .id("titlebar")
        .flex()
        .flex_none()
        .items_center()
        .h(TITLEBAR_HEIGHT)
        .w_full()
        .pl_4()
        .bg(theme.background)
        .border_b_4()
        .border_color(theme.border)
        .when(client, |d| {
            d.on_mouse_down(MouseButton::Left, {
                let should_move = state.should_move.clone();
                move |_, _, _| should_move.set(true)
            })
            .on_mouse_up(MouseButton::Left, {
                let should_move = state.should_move.clone();
                move |_, _, _| should_move.set(false)
            })
            .on_mouse_up_out(MouseButton::Left, {
                let should_move = state.should_move.clone();
                move |_, _, _| should_move.set(false)
            })
            .on_mouse_move({
                let should_move = state.should_move.clone();
                move |_, window, _| {
                    if should_move.replace(false) {
                        window.start_window_move();
                    }
                }
            })
            .on_click(|e, window, _| {
                if e.click_count() == 2 {
                    window.zoom_window();
                } else if e.is_right_click() && window.window_controls().window_menu {
                    window.show_window_menu(e.position());
                }
            })
        })
        .child(
            div()
                .flex()
                .items_baseline()
                .gap_4()
                .min_w_0()
                .child(
                    div()
                        .flex()
                        .font_family(FONT_DISPLAY)
                        .font_weight(FontWeight::BOLD)
                        .text_size(px(20.))
                        .child("WAYPOINT")
                        .child(div().text_color(theme.primary).child(".")),
                )
                .child(
                    div()
                        .font_family(FONT_MONO)
                        .text_xs()
                        .text_color(theme.ink_muted)
                        .truncate()
                        .child(subtitle),
                ),
        )
        .child(div().flex_1())
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .pr_4()
                // Buttons, not a drag area.
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(actions),
        )
        .when(client, |d| {
            d.child(
                div()
                    .flex()
                    .h_full()
                    .font_family(FONT_MONO)
                    .text_sm()
                    .when(controls.minimize, |d| {
                        d.child(control_button("minimize", "–", theme.foreground, theme.background, |w| {
                            w.minimize_window()
                        }))
                    })
                    .when(controls.maximize, |d| {
                        d.child(control_button(
                            "maximize",
                            if maximized { "❐" } else { "□" },
                            theme.foreground,
                            theme.background,
                            |w| w.zoom_window(),
                        ))
                    })
                    .child(control_button("close", "✕", theme.primary, theme.primary_fg, |w| w.remove_window())),
            )
        })
}

fn control_button(
    id: &'static str,
    icon: &'static str,
    hover_bg: Rgba,
    hover_fg: Rgba,
    action: fn(&mut Window),
) -> impl IntoElement {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .w(px(48.))
        .h_full()
        .hover(move |s| s.bg(hover_bg).text_color(hover_fg))
        // Don't let the titlebar start a window move from a button.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            action(window);
        })
        .child(icon)
}

fn resize_edge(pos: Point<Pixels>, size: Size<Pixels>) -> Option<ResizeEdge> {
    let s = SHADOW_SIZE;
    let top = pos.y < s;
    let bottom = pos.y > size.height - s;
    let left = pos.x < s;
    let right = pos.x > size.width - s;
    Some(match (top, bottom, left, right) {
        (true, _, true, _) => ResizeEdge::TopLeft,
        (true, _, _, true) => ResizeEdge::TopRight,
        (true, _, _, _) => ResizeEdge::Top,
        (_, true, true, _) => ResizeEdge::BottomLeft,
        (_, true, _, true) => ResizeEdge::BottomRight,
        (_, true, _, _) => ResizeEdge::Bottom,
        (_, _, true, _) => ResizeEdge::Left,
        (_, _, _, true) => ResizeEdge::Right,
        _ => return None,
    })
}

fn cursor_for_edge(edge: ResizeEdge) -> CursorStyle {
    match edge {
        ResizeEdge::Top | ResizeEdge::Bottom => CursorStyle::ResizeUpDown,
        ResizeEdge::Left | ResizeEdge::Right => CursorStyle::ResizeLeftRight,
        ResizeEdge::TopLeft | ResizeEdge::BottomRight => CursorStyle::ResizeUpLeftDownRight,
        ResizeEdge::TopRight | ResizeEdge::BottomLeft => CursorStyle::ResizeUpRightDownLeft,
    }
}
