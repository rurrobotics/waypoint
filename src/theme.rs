//! Neutral light/dark palette (zinc greys, one blue accent) and fonts.

use gpui::{App, Global, Hsla, Pixels, Rgba, Window, WindowAppearance, px, rgb};

pub const FONT_SANS: &str = "Inter";
pub const FONT_MONO: &str = "JetBrains Mono";

/// Corner radius for controls and panels.
pub const RADIUS: Pixels = px(6.);

/// Static TTFs embedded in the binary (OFL, see `assets/fonts`).
pub const FONTS: &[&[u8]] = &[
    include_bytes!("../assets/fonts/Inter-Regular.ttf"),
    include_bytes!("../assets/fonts/Inter-Medium.ttf"),
    include_bytes!("../assets/fonts/Inter-SemiBold.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf"),
];

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub dark: bool,
    /// Panels and toolbar.
    pub bg: Rgba,
    /// Inputs, hovered rows, pressed toggles.
    pub elevated: Rgba,
    pub border: Rgba,
    pub border_strong: Rgba,
    pub fg: Rgba,
    /// Secondary text, inactive paths, ghosts.
    pub muted: Rgba,
    pub accent: Rgba,
    pub accent_fg: Rgba,
    pub danger: Rgba,
    pub danger_fg: Rgba,
    pub warning: Rgba,
    pub success: Rgba,
    /// Canvas surround behind the table.
    pub canvas: Rgba,
    /// Empty table surface when there's no background image.
    pub table: Rgba,
}

impl Theme {
    pub fn light() -> Self {
        Self {
            dark: false,
            bg: rgb(0xffffff),
            elevated: rgb(0xf4f4f5),
            border: rgb(0xe4e4e7),
            border_strong: rgb(0xa1a1aa),
            fg: rgb(0x18181b),
            muted: rgb(0x71717a),
            accent: rgb(0x2563eb),
            accent_fg: rgb(0xffffff),
            danger: rgb(0xdc2626),
            danger_fg: rgb(0xffffff),
            warning: rgb(0xd97706),
            success: rgb(0x16a34a),
            canvas: rgb(0xf4f4f5),
            table: rgb(0xfafafa),
        }
    }

    pub fn dark() -> Self {
        Self {
            dark: true,
            bg: rgb(0x18181b),
            elevated: rgb(0x27272a),
            border: rgb(0x2e2e33),
            border_strong: rgb(0x71717a),
            fg: rgb(0xfafafa),
            muted: rgb(0xa1a1aa),
            accent: rgb(0x3b82f6),
            accent_fg: rgb(0xffffff),
            danger: rgb(0xef4444),
            danger_fg: rgb(0xffffff),
            warning: rgb(0xf59e0b),
            success: rgb(0x22c55e),
            canvas: rgb(0x09090b),
            table: rgb(0x1c1c20),
        }
    }

    pub fn for_appearance(appearance: WindowAppearance) -> Self {
        match appearance {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Self::dark(),
            WindowAppearance::Light | WindowAppearance::VibrantLight => Self::light(),
        }
    }

    pub fn get(cx: &App) -> Self {
        *cx.global::<Theme>()
    }

    /// Re-reads the window appearance; returns true when the theme changed.
    pub fn sync(window: &Window, cx: &mut App) -> bool {
        let theme = Self::for_appearance(window.appearance());
        let changed = !cx.has_global::<Theme>() || cx.global::<Theme>().dark != theme.dark;
        cx.set_global(theme);
        changed
    }
}

impl Global for Theme {}

/// A colour with its alpha scaled.
pub fn alpha(c: Rgba, a: f32) -> Hsla {
    let mut h: Hsla = c.into();
    h.a *= a;
    h
}
