//! RUR Robotics brand tokens (mirrors `src/styles/global.css` of the website).

use gpui::{App, Global, Hsla, Rgba, Window, WindowAppearance, rgb};

pub const FONT_DISPLAY: &str = "Oswald";
pub const FONT_SANS: &str = "Golos Text";
pub const FONT_MONO: &str = "JetBrains Mono";

/// Static TTFs embedded in the binary (OFL, see `assets/fonts`).
pub const FONTS: &[&[u8]] = &[
    include_bytes!("../assets/fonts/Oswald-Medium.ttf"),
    include_bytes!("../assets/fonts/Oswald-Bold.ttf"),
    include_bytes!("../assets/fonts/GolosText-Regular.ttf"),
    include_bytes!("../assets/fonts/GolosText-Medium.ttf"),
    include_bytes!("../assets/fonts/GolosText-SemiBold.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf"),
];

/// The full brand palette; not every token is used yet.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub dark: bool,
    pub background: Rgba,
    pub foreground: Rgba,
    pub primary: Rgba,
    pub primary_fg: Rgba,
    pub secondary: Rgba,
    pub secondary_fg: Rgba,
    pub accent: Rgba,
    pub accent_fg: Rgba,
    pub tertiary: Rgba,
    pub tertiary_fg: Rgba,
    /// Tertiary as a text colour (needs more contrast than the fill).
    pub tertiary_ink: Rgba,
    pub muted: Rgba,
    pub muted_fg: Rgba,
    pub ink_muted: Rgba,
    pub border: Rgba,
    /// Canvas surround behind the table.
    pub canvas: Rgba,
    /// Empty table surface when there's no background image.
    pub table: Rgba,
}

impl Theme {
    pub fn light() -> Self {
        Self {
            dark: false,
            background: rgb(0xffffff),
            foreground: rgb(0x111111),
            primary: rgb(0xee4036),
            primary_fg: rgb(0xffffff),
            secondary: rgb(0xffa112),
            secondary_fg: rgb(0x111111),
            accent: rgb(0xff7431),
            accent_fg: rgb(0x111111),
            tertiary: rgb(0x001d7d),
            tertiary_fg: rgb(0xffffff),
            tertiary_ink: rgb(0x001d7d),
            muted: rgb(0x737373),
            muted_fg: rgb(0xffffff),
            ink_muted: rgb(0x55503f),
            border: rgb(0x111111),
            canvas: rgb(0xece8e0),
            table: rgb(0xfaf8f4),
        }
    }

    /// Warm near-black and paper, as on the website.
    pub fn dark() -> Self {
        Self {
            dark: true,
            background: rgb(0x16130f),
            foreground: rgb(0xece5d8),
            muted: rgb(0x9c9384),
            muted_fg: rgb(0x16130f),
            ink_muted: rgb(0xaaa08b),
            border: rgb(0xece5d8),
            tertiary: rgb(0x1a3399),
            tertiary_ink: rgb(0x93a1d9),
            canvas: rgb(0x0c0a08),
            table: rgb(0x221e18),
            ..Self::light()
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
