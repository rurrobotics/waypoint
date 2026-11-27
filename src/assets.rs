//! Embedded assets: Lucide icons (ISC, see `assets/icons/LICENSE-lucide.txt`).

use std::borrow::Cow;

use anyhow::Result;
use gpui::{AssetSource, SharedString};

macro_rules! icons {
    ($($variant:ident => $file:literal),* $(,)?) => {
        /// An icon from the embedded Lucide set (not every icon is used yet).
        #[allow(dead_code)]
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Icon { $($variant),* }

        impl Icon {
            /// Asset path for `svg().path(..)`.
            pub fn path(self) -> &'static str {
                match self { $(Icon::$variant => concat!("icons/", $file, ".svg")),* }
            }
        }

        const ICON_FILES: &[(&str, &[u8])] = &[
            $((concat!("icons/", $file, ".svg"), include_bytes!(concat!("../assets/icons/", $file, ".svg")))),*
        ];
    };
}

icons! {
    Plus => "plus",
    Route => "route",
    Square => "square",
    Circle => "circle",
    Pentagon => "pentagon",
    SquareDashed => "square-dashed",
    Ruler => "ruler",
    FolderOpen => "folder-open",
    Save => "save",
    FilePlus => "file-plus",
    Download => "download",
    Undo => "undo-2",
    Redo => "redo-2",
    Fit => "scan",
    Trash => "trash-2",
    Eye => "eye",
    EyeOff => "eye-off",
    Magnet => "magnet",
    Reverse => "arrow-left-right",
    RotateCw => "rotate-cw",
    Spline => "spline",
    MoveRight => "move-right",
    CornerUpRight => "corner-up-right",
    Close => "x",
    Minus => "minus",
    Maximize => "maximize-2",
    Restore => "minimize-2",
    Grip => "grip-vertical",
    PanelLeft => "panel-left",
    PanelRight => "panel-right",
    ChevronDown => "chevron-down",
    ChevronRight => "chevron-right",
    Pointer => "mouse-pointer-2",
    Bot => "bot",
    Map => "map",
    Settings => "settings-2",
    Layers => "layers",
}

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(ICON_FILES.iter().find(|(p, _)| *p == path).map(|(_, bytes)| Cow::Borrowed(*bytes)))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(ICON_FILES.iter().filter(|(p, _)| p.starts_with(path)).map(|(p, _)| (*p).into()).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_loads() {
        for (path, _) in ICON_FILES {
            let bytes = Assets.load(path).unwrap().unwrap();
            assert!(std::str::from_utf8(&bytes).unwrap().contains("<svg"), "{path}");
        }
        assert!(Assets.load(Icon::Route.path()).unwrap().is_some());
    }
}
