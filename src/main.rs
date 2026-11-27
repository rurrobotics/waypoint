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
    App, Application, Bounds, TitlebarOptions, WindowBackgroundAppearance, WindowBounds, WindowOptions, prelude::*,
    px, size,
};

use theme::Theme;
use ui::workspace::{self, Workspace};

fn main() {
    Application::new().with_assets(assets::Assets).run(|cx: &mut App| {
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
                // Transparent so the client-side shadow shows around the frame.
                window_background: WindowBackgroundAppearance::Transparent,
                app_id: Some("waypoint".into()),
                ..Default::default()
            },
            |window, cx| {
                Theme::sync(window, cx);
                cx.new(|cx| {
                    let mut workspace = Workspace::new(window, cx);
                    // `waypoint strategy.waypoint` opens a project.
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
