//! Table background images. Raster images go straight to gpui's `img`; SVGs
//! are rendered here at a bounded size, because gpui rasterises an SVG at its
//! intrinsic size × 2, and a table drawn in millimetres (3000 × 2000 mm) would
//! become a ~22 000 px wide bitmap, far beyond GPU texture limits.

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context as _, Result, anyhow};
use gpui::RenderImage;
use image::{Frame, ImageBuffer};
use resvg::{tiny_skia, usvg};

/// Longest side of a rendered SVG background, in pixels: sharp when zoomed
/// in on a 3 m table (~1.4 px/mm) and well within GPU texture limits.
pub const MAX_SIDE: u32 = 4096;

pub fn is_svg(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg"))
}

/// Text is set with the app's own embedded fonts: scanning every system font
/// takes seconds, and this way a table renders the same on every machine.
fn options() -> usvg::Options<'static> {
    let mut options = usvg::Options { font_family: crate::theme::FONT_SANS.into(), ..Default::default() };
    let db = options.fontdb_mut();
    for font in crate::theme::FONTS {
        db.load_font_data(font.to_vec());
    }
    db.set_sans_serif_family(crate::theme::FONT_SANS);
    db.set_serif_family(crate::theme::FONT_SANS);
    db.set_monospace_family(crate::theme::FONT_MONO);
    options
}

/// Renders SVG bytes so the longer side is `max_side` pixels.
pub fn render_svg(bytes: &[u8], max_side: u32) -> Result<Arc<RenderImage>> {
    let tree = usvg::Tree::from_data(bytes, &options()).context("parsing SVG")?;

    let size = tree.size();
    let scale = max_side as f32 / size.width().max(size.height());
    let (w, h) = ((size.width() * scale).round() as u32, (size.height() * scale).round() as u32);
    let mut pixmap = tiny_skia::Pixmap::new(w.max(1), h.max(1)).ok_or_else(|| anyhow!("SVG has no size"))?;
    resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());

    // tiny-skia gives premultiplied RGBA; gpui wants straight BGRA.
    let mut data = pixmap.take();
    for px in data.chunks_exact_mut(4) {
        px.swap(0, 2);
        let a = px[3];
        if a > 0 && a < 255 {
            for c in &mut px[..3] {
                *c = ((*c as u16 * 255 + a as u16 / 2) / a as u16).min(255) as u8;
            }
        }
    }
    let buffer = ImageBuffer::from_raw(w.max(1), h.max(1), data).ok_or_else(|| anyhow!("bad pixel buffer"))?;
    Ok(Arc::new(RenderImage::new(vec![Frame::new(buffer)])))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_millimetre_svg_at_bounded_size() {
        // Same shape as a real table file: 3000 × 2000 mm.
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="3000mm" height="2000mm" viewBox="0 0 3000 2000">
            <rect width="3000" height="2000" fill="#2b5f3a"/>
            <rect x="0" y="0" width="1500" height="2000" fill="#ff0000" fill-opacity="0.5"/>
        </svg>"##;
        let image = render_svg(svg, 600).unwrap();
        let size = image.size(0);
        assert_eq!((size.width.0, size.height.0), (600, 400));

        // Opaque green on the right, as BGRA.
        let bytes = image.as_bytes(0).unwrap();
        let px = |x: usize, y: usize| &bytes[(y * 600 + x) * 4..][..4];
        assert_eq!(px(500, 200), [0x3a, 0x5f, 0x2b, 255]);
        // Half-transparent red over green on the left blends to an opaque colour.
        assert_eq!(px(100, 200)[3], 255);
        assert!(px(100, 200)[2] > 0x80, "red channel raised: {:?}", px(100, 200));
    }

    #[test]
    fn detects_svg_paths() {
        assert!(is_svg(Path::new("a/table.SVG")));
        assert!(!is_svg(Path::new("table.png")));
    }

    #[test]
    fn shipped_table_renders() {
        let bytes = crate::assets::get("templates/table.svg").expect("table.svg is shipped");
        let image = render_svg(bytes, 300).unwrap();
        assert_eq!(image.size(0).width.0, 300);
    }
}
