//! Window frame: the app's toolbar doubles as the title bar. With client-side
//! decorations (e.g. GNOME/Mutter on Wayland) it is also the drag area and
//! gets window controls, a border, a shadow and resize edges.

use std::{cell::Cell, rc::Rc};

use gpui::{
    AnyElement, Bounds, CursorStyle, Decorations, Hitbox, HitboxBehavior, Hsla, MouseButton, Pixels,
    Point, ResizeEdge, Rgba, Size, Window, canvas, div, point, prelude::*, px, transparent_black,
};

use crate::assets::Icon;
use crate::theme::Theme;

const TITLEBAR_HEIGHT: Pixels = px(44.);
const SHADOW_SIZE: Pixels = px(10.);

/// Per-window titlebar state. Keep one alive alongside your root view.
#[derive(Clone, Default)]
pub struct TitlebarState {
    /// Set on mouse-down in the titlebar; the move starts on the first drag so
    /// the compositor doesn't swallow double-clicks.
    should_move: Rc<Cell<bool>>,
}

/// Wraps `content` in the app frame: `toolbar` on top, plus border, shadow and
/// resize edges when the window uses client-side decorations.
pub fn window_frame(
    window: &mut Window,
    theme: &Theme,
    state: &TitlebarState,
    toolbar: impl IntoElement,
    content: impl IntoElement,
) -> AnyElement {
    let decorations = window.window_decorations();
    let body = div()
        .size_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .bg(theme.bg)
        .text_color(theme.fg)
        .child(titlebar(window, theme, state, toolbar))
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
        // Redraw for the resize cursor only on the edges. Mouse moves over the
        // body must keep propagating: the app handles its drags (panning,
        // measuring, handles…) in listeners outside this frame.
        .on_mouse_move(|e, window, _| {
            if resize_edge(e.position, window.window_bounds().get_bounds().size).is_some() {
                window.refresh();
            }
        })
        .on_mouse_down(MouseButton::Left, |e, window, _| {
            let size = window.window_bounds().get_bounds().size;
            if let Some(edge) = resize_edge(e.position, size) {
                window.start_window_resize(edge);
            }
        })
        .child(
            body.cursor(CursorStyle::Arrow)
                .border_color(theme.border)
                .when(!tiling.top, |d| d.border_t_1())
                .when(!tiling.bottom, |d| d.border_b_1())
                .when(!tiling.left, |d| d.border_l_1())
                .when(!tiling.right, |d| d.border_r_1())
                .when(!tiling.is_tiled(), |d| {
                    d.shadow(vec![gpui::BoxShadow {
                        color: Hsla { h: 0., s: 0., l: 0., a: 0.35 },
                        blur_radius: SHADOW_SIZE / 2.,
                        spread_radius: px(0.),
                        offset: point(px(0.), px(0.)),
                    }])
                }),
        )
        .into_any_element()
}

/// The toolbar row, doubling as the window's drag area. Interactive children
/// must stop mouse-down propagation so pressing them doesn't drag the window.
fn titlebar(window: &Window, theme: &Theme, state: &TitlebarState, toolbar: impl IntoElement) -> impl IntoElement {
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
        .pl_2()
        .bg(theme.bg)
        .border_b_1()
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
        .child(div().flex().items_center().flex_1().min_w_0().h_full().child(toolbar))
        .when(client, |d| {
            d.child(
                div()
                    .flex()
                    .h_full()
                    .text_sm()
                    .when(controls.minimize, |d| {
                        d.child(control_button("minimize", Icon::Minus, theme.fg, theme.elevated, theme.fg, |w| {
                            w.minimize_window()
                        }))
                    })
                    .when(controls.maximize, |d| {
                        d.child(control_button(
                            "maximize",
                            if maximized { Icon::Restore } else { Icon::Maximize },
                            theme.fg,
                            theme.elevated,
                            theme.fg,
                            |w| w.zoom_window(),
                        ))
                    })
                    .child(control_button("close", Icon::Close, theme.fg, theme.danger, theme.danger_fg, |w| w.remove_window())),
            )
        })
}

fn control_button(
    id: &'static str,
    icon: Icon,
    fg: Rgba,
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
        .group("window-control")
        .child(
            crate::ui::widgets::icon(icon, 14., fg)
                .group_hover("window-control", move |s| s.text_color(hover_fg)),
        )
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
