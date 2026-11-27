//! Inline editing of inspector fields. Only one field edits at a time, so the
//! buffer lives here instead of in per-field entities.

use gpui::Keystroke;

/// Every editable value in the inspector.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Field {
    PoseX,
    PoseY,
    PoseTheta,
    TurnAngle,
    TurnRadius,
    HermiteT0,
    HermiteT1,
    PathName,
    ItemName,
    ShapeCx,
    ShapeCy,
    ShapeW,
    ShapeH,
    ShapeRotation,
    ShapeRadius,
    RobotLength,
    RobotWidth,
    RobotOffsetX,
    RobotOffsetY,
    RobotMargin,
    MapName,
    MapYear,
    MapWidth,
    MapHeight,
    SampleStep,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Mm,
    Deg,
    Plain,
    Text,
}

impl Field {
    pub fn label(self) -> &'static str {
        use Field::*;
        match self {
            PoseX | ShapeCx => "X",
            PoseY | ShapeCy => "Y",
            PoseTheta => "HEADING",
            TurnAngle => "ANGLE",
            TurnRadius => "RADIUS",
            HermiteT0 => "TANGENT IN",
            HermiteT1 => "TANGENT OUT",
            PathName | ItemName | MapName => "NAME",
            ShapeW | MapWidth | RobotWidth => "WIDTH",
            ShapeH | MapHeight => "HEIGHT",
            ShapeRotation => "ROTATION",
            ShapeRadius => "RADIUS",
            RobotLength => "LENGTH",
            RobotOffsetX => "OFFSET X",
            RobotOffsetY => "OFFSET Y",
            RobotMargin => "MARGIN",
            MapYear => "YEAR",
            SampleStep => "STEP",
        }
    }

    pub fn unit(self) -> Unit {
        use Field::*;
        match self {
            PoseTheta | TurnAngle | ShapeRotation => Unit::Deg,
            PathName | ItemName | MapName => Unit::Text,
            MapYear => Unit::Plain,
            _ => Unit::Mm,
        }
    }

    /// Change per arrow-key press / per pixel of scrubbing.
    pub fn step(self) -> f64 {
        match self.unit() {
            Unit::Deg => 1.,
            Unit::Plain => 1.,
            _ => 5.,
        }
    }
}

impl Unit {
    pub fn suffix(self) -> &'static str {
        match self {
            Unit::Mm => "MM",
            Unit::Deg => "°",
            Unit::Plain | Unit::Text => "",
        }
    }
}

pub struct Edit {
    pub field: Field,
    pub text: String,
    /// Byte offset, always on a char boundary.
    pub cursor: usize,
    /// The whole text is selected; typing replaces it.
    pub select_all: bool,
}

pub enum Outcome {
    Continue,
    Commit,
    Cancel,
    /// Arrow up/down: step the value by this many steps.
    Nudge(f64),
}

impl Edit {
    pub fn new(field: Field, text: String) -> Self {
        Self { field, cursor: text.len(), text, select_all: true }
    }

    pub fn key(&mut self, ks: &Keystroke) -> Outcome {
        let m = &ks.modifiers;
        let command = m.control || m.platform;
        match ks.key.as_str() {
            "enter" | "tab" => return Outcome::Commit,
            "escape" => return Outcome::Cancel,
            "up" => return Outcome::Nudge(if m.shift { 10. } else { 1. }),
            "down" => return Outcome::Nudge(if m.shift { -10. } else { -1. }),
            "backspace" => {
                if self.select_all {
                    self.clear();
                } else if let Some((i, _)) = self.text[..self.cursor].char_indices().next_back() {
                    self.text.remove(i);
                    self.cursor = i;
                }
            }
            "delete" => {
                if self.select_all {
                    self.clear();
                } else if self.cursor < self.text.len() {
                    self.text.remove(self.cursor);
                }
            }
            "left" => {
                self.select_all = false;
                if let Some((i, _)) = self.text[..self.cursor].char_indices().next_back() {
                    self.cursor = i;
                }
            }
            "right" => {
                self.select_all = false;
                if let Some(c) = self.text[self.cursor..].chars().next() {
                    self.cursor += c.len_utf8();
                }
            }
            "home" => {
                self.select_all = false;
                self.cursor = 0;
            }
            "end" => {
                self.select_all = false;
                self.cursor = self.text.len();
            }
            "a" if command => self.select_all = true,
            _ if command || m.alt => {}
            _ => {
                if let Some(ch) = ks.key_char.as_deref().filter(|c| !c.chars().any(char::is_control)) {
                    if self.select_all {
                        self.clear();
                    }
                    self.text.insert_str(self.cursor, ch);
                    self.cursor += ch.len();
                }
            }
        }
        Outcome::Continue
    }

    fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
        self.select_all = false;
    }
}

/// Parses a number, accepting a decimal comma.
pub fn parse_number(s: &str) -> Option<f64> {
    s.trim().replace(',', ".").parse::<f64>().ok().filter(|v| v.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(k: &str, ch: Option<&str>) -> Keystroke {
        Keystroke { key: k.into(), key_char: ch.map(Into::into), ..Default::default() }
    }

    #[test]
    fn typing_replaces_selection_then_edits() {
        let mut e = Edit::new(Field::PoseX, "120".into());
        e.key(&key("4", Some("4")));
        e.key(&key("2", Some("2")));
        assert_eq!(e.text, "42");
        e.key(&key("left", None));
        e.key(&key("backspace", None));
        assert_eq!(e.text, "2");
        e.key(&key("1", Some("1")));
        assert_eq!(e.text, "12");
        assert!(matches!(e.key(&key("enter", None)), Outcome::Commit));
    }

    #[test]
    fn numbers() {
        assert_eq!(parse_number(" 12,5 "), Some(12.5));
        assert_eq!(parse_number("abc"), None);
    }
}
