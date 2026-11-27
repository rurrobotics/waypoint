//! Dock layout: which panels sit in the left and right docks, in what order,
//! and how big they are. Persisted to the user's config directory.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const MIN_WIDTH: f32 = 200.;
pub const MAX_WIDTH: f32 = 640.;
/// Smallest share of a dock's height a panel can be resized to.
const MIN_SHARE: f32 = 0.12;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PanelId {
    Selected,
    Outline,
    Settings,
}

impl PanelId {
    pub const ALL: [PanelId; 3] = [PanelId::Selected, PanelId::Outline, PanelId::Settings];

    pub fn title(self) -> &'static str {
        match self {
            PanelId::Selected => "Selected",
            PanelId::Outline => "Outline",
            PanelId::Settings => "Settings",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Left,
    Right,
}

impl Side {
    pub fn other(self) -> Side {
        match self {
            Side::Left => Side::Right,
            Side::Right => Side::Left,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Dock {
    pub panels: Vec<PanelId>,
    /// Share of the dock's height per panel; sums to 1.
    pub shares: Vec<f32>,
    pub width: f32,
}

impl Dock {
    fn normalize(&mut self) {
        self.shares.resize(self.panels.len(), 1.);
        let sum: f32 = self.shares.iter().sum();
        if sum > 0. {
            self.shares.iter_mut().for_each(|s| *s /= sum);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub left: Dock,
    pub right: Dock,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            left: Dock { panels: vec![PanelId::Selected, PanelId::Outline], shares: vec![0.45, 0.55], width: 280. },
            right: Dock { panels: vec![PanelId::Settings], shares: vec![1.], width: 300. },
        }
    }
}

impl Layout {
    pub fn dock(&self, side: Side) -> &Dock {
        match side {
            Side::Left => &self.left,
            Side::Right => &self.right,
        }
    }

    pub fn dock_mut(&mut self, side: Side) -> &mut Dock {
        match side {
            Side::Left => &mut self.left,
            Side::Right => &mut self.right,
        }
    }

    #[cfg(test)]
    pub fn side_of(&self, panel: PanelId) -> Side {
        if self.left.panels.contains(&panel) { Side::Left } else { Side::Right }
    }

    /// Moves `panel` into the dock on `to`, before `before` (or at the end).
    pub fn move_panel(&mut self, panel: PanelId, to: Side, before: Option<PanelId>) {
        if before == Some(panel) {
            return;
        }
        for side in [Side::Left, Side::Right] {
            let dock = self.dock_mut(side);
            if let Some(i) = dock.panels.iter().position(|&p| p == panel) {
                dock.panels.remove(i);
                dock.shares.remove(i);
                dock.normalize();
            }
        }
        let dock = self.dock_mut(to);
        let i = before.and_then(|b| dock.panels.iter().position(|&p| p == b)).unwrap_or(dock.panels.len());
        // The newcomer gets an equal share.
        let share = 1. / (dock.panels.len() + 1) as f32;
        dock.shares.iter_mut().for_each(|s| *s *= 1. - share);
        dock.panels.insert(i, panel);
        dock.shares.insert(i, share);
        dock.normalize();
    }

    pub fn set_width(&mut self, side: Side, width: f32) {
        self.dock_mut(side).width = width.clamp(MIN_WIDTH, MAX_WIDTH);
    }

    /// Moves the boundary below panel `i` by `delta` (a share of the dock height).
    pub fn resize_split(&mut self, side: Side, i: usize, start: (f32, f32), delta: f32) {
        let dock = self.dock_mut(side);
        if i + 1 >= dock.shares.len() {
            return;
        }
        let total = start.0 + start.1;
        let a = (start.0 + delta).clamp(MIN_SHARE.min(total / 2.), total - MIN_SHARE.min(total / 2.));
        dock.shares[i] = a;
        dock.shares[i + 1] = total - a;
    }

    /// Repairs layouts from older versions or hand edits: every panel exactly once.
    fn sanitize(mut self) -> Self {
        let mut seen = Vec::new();
        for side in [Side::Left, Side::Right] {
            let dock = self.dock_mut(side);
            let mut panels = Vec::new();
            let mut shares = Vec::new();
            for (i, p) in dock.panels.iter().enumerate() {
                if !seen.contains(p) {
                    seen.push(*p);
                    panels.push(*p);
                    shares.push(dock.shares.get(i).copied().unwrap_or(1.).max(MIN_SHARE));
                }
            }
            dock.panels = panels;
            dock.shares = shares;
            dock.width = dock.width.clamp(MIN_WIDTH, MAX_WIDTH);
            dock.normalize();
        }
        for p in PanelId::ALL {
            if !seen.contains(&p) {
                self.move_panel(p, Side::Right, None);
            }
        }
        self
    }

    fn file() -> Option<PathBuf> {
        Some(crate::model::config_dir()?.join("layout.json"))
    }

    /// The saved layout, or the default. Tests always get the default.
    pub fn load() -> Self {
        if cfg!(test) {
            return Self::default();
        }
        Self::file()
            .and_then(|f| std::fs::read_to_string(f).ok())
            .and_then(|s| serde_json::from_str::<Layout>(&s).ok())
            .map(Layout::sanitize)
            .unwrap_or_default()
    }

    /// Best effort: a layout that can't be saved isn't worth an error.
    pub fn save(&self) {
        if cfg!(test) {
            return;
        }
        if let Some(file) = Self::file()
            && let Some(dir) = file.parent()
            && std::fs::create_dir_all(dir).is_ok()
            && let Ok(json) = serde_json::to_string_pretty(self)
        {
            let _ = std::fs::write(file, json);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sum(d: &Dock) -> f32 {
        d.shares.iter().sum()
    }

    #[test]
    fn move_between_docks_keeps_shares_normalized() {
        let mut l = Layout::default();
        l.move_panel(PanelId::Outline, Side::Right, Some(PanelId::Settings));
        assert_eq!(l.left.panels, [PanelId::Selected]);
        assert_eq!(l.right.panels, [PanelId::Outline, PanelId::Settings]);
        assert!((sum(&l.left) - 1.).abs() < 1e-6 && (sum(&l.right) - 1.).abs() < 1e-6);
        assert_eq!(l.side_of(PanelId::Outline), Side::Right);

        l.move_panel(PanelId::Selected, Side::Right, None);
        assert!(l.left.panels.is_empty());
        assert_eq!(l.right.panels.len(), 3);
    }

    #[test]
    fn reorder_within_a_dock() {
        let mut l = Layout::default();
        l.move_panel(PanelId::Outline, Side::Left, Some(PanelId::Selected));
        assert_eq!(l.left.panels, [PanelId::Outline, PanelId::Selected]);
    }

    #[test]
    fn resize_is_clamped() {
        let mut l = Layout::default();
        l.set_width(Side::Left, 10.);
        assert_eq!(l.left.width, MIN_WIDTH);
        l.resize_split(Side::Left, 0, (0.45, 0.55), 0.9);
        assert!((l.left.shares[1] - MIN_SHARE).abs() < 1e-6);
        assert!((sum(&l.left) - 1.).abs() < 1e-6);
    }

    #[test]
    fn sanitize_restores_missing_and_duplicate_panels() {
        let broken = Layout {
            left: Dock { panels: vec![PanelId::Outline, PanelId::Outline], shares: vec![1.], width: 9999. },
            right: Dock { panels: vec![], shares: vec![], width: 300. },
        }
        .sanitize();
        assert_eq!(broken.left.panels, [PanelId::Outline]);
        assert_eq!(broken.left.width, MAX_WIDTH);
        let mut all: Vec<_> = broken.left.panels.iter().chain(&broken.right.panels).collect();
        all.sort_by_key(|p| p.title());
        assert_eq!(all.len(), 3);
    }
}
