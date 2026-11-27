// Release builds on Windows are GUI apps: no console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod assets;
mod export;
mod geom;
mod model;
mod theme;
mod titlebar;
mod ui;

use std::borrow::Cow;

use gpui::{
    App, Application, Bounds, TitlebarOptions, WindowBackgroundAppearance, WindowBounds, WindowDecorations,
    WindowOptions, prelude::*,
    px, size,
};

use theme::Theme;
use ui::workspace::{self, Workspace};

fn main() {
    without_system_fonts(Application::new).with_assets(assets::Assets).run(|cx: &mut App| {
        cx.text_system()
            .add_fonts(theme::FONTS.iter().map(|f| Cow::Borrowed(*f)).collect())
            .expect("embedded fonts are valid");
        workspace::bind_keys(cx);

        let bounds = Bounds::centered(None, size(px(1440.), px(900.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions { title: Some("Waypoint".into()), ..Default::default() }),
                window_min_size: Some(size(px(960.), px(600.))),
                // Draw our own title bar (the toolbar) on Linux. gpui defaults to
                // server-side decorations, and on compositors without them
                // (GNOME/Mutter on Wayland) it still reports them as present, so
                // nobody would draw a title bar. No effect on macOS/Windows.
                window_decorations: Some(WindowDecorations::Client),
                // Transparent so the client-side shadow shows around the frame.
                window_background: WindowBackgroundAppearance::Transparent,
                app_id: Some("waypoint".into()),
                ..Default::default()
            },
            |window, cx| {
                Theme::sync(window, cx);
                cx.new(|cx| {
                    let mut workspace = Workspace::new(window, cx);
                    // `waypoint strategy.json` opens a project.
                    if let Some(path) = std::env::args_os().nth(1) {
                        workspace.load_project(path.into(), cx);
                    }
                    workspace
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}

/// Runs `f` (gpui's `Application::new`) with fontconfig pointed at an empty
/// config. On Linux gpui's text system loads every system font up front, which
/// takes seconds on a machine with thousands of fonts; the UI only uses the
/// embedded fonts (`theme::FONTS`). The variable is restored afterwards so
/// processes we spawn (`xdg-open`) see the user's fonts.
fn without_system_fonts<T>(f: impl FnOnce() -> T) -> T {
    if !cfg!(any(target_os = "linux", target_os = "freebsd")) {
        return f();
    }
    const VAR: &str = "FONTCONFIG_FILE";
    let previous = std::env::var_os(VAR);
    // SAFETY: called first thing in `main`, before gpui starts any threads.
    unsafe { std::env::set_var(VAR, "/dev/null") };
    let result = f();
    // SAFETY: gpui's worker threads exist now but are idle; nothing else in
    // the process reads the environment concurrently at this point.
    unsafe {
        match previous {
            Some(v) => std::env::set_var(VAR, v),
            None => std::env::remove_var(VAR),
        }
    }
    result
}
