// Geometry is wired into the UI in a later commit.
#[allow(dead_code, unused_imports)]
mod geom;

use gpui::{App, Application, Bounds, WindowBounds, WindowOptions, div, prelude::*, px, size};

struct Waypoint;

impl Render for Waypoint {
    fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        div().size_full()
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1280.), px(800.)), cx);
        cx.open_window(
            WindowOptions { window_bounds: Some(WindowBounds::Windowed(bounds)), ..Default::default() },
            |_, cx| cx.new(|_| Waypoint),
        )
        .unwrap();
        cx.activate(true);
    });
}
