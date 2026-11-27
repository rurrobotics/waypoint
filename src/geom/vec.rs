use std::f64::consts::{PI, TAU};
use std::ops::{Add, AddAssign, Mul, Neg, Sub, SubAssign};

use serde::{Deserialize, Serialize};

/// A 2D vector in table space (millimetres). Serialised as `[x, y]`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(from = "[f64; 2]", into = "[f64; 2]")]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl From<[f64; 2]> for Vec2 {
    fn from([x, y]: [f64; 2]) -> Self {
        Self { x, y }
    }
}

impl From<Vec2> for [f64; 2] {
    fn from(v: Vec2) -> Self {
        [v.x, v.y]
    }
}

pub const fn vec2(x: f64, y: f64) -> Vec2 {
    Vec2 { x, y }
}

impl Vec2 {
    pub const ZERO: Vec2 = vec2(0., 0.);

    /// Unit vector pointing at `angle` radians (counter-clockwise from +x).
    pub fn from_angle(angle: f64) -> Self {
        vec2(angle.cos(), angle.sin())
    }

    pub fn dot(self, o: Self) -> f64 {
        self.x * o.x + self.y * o.y
    }

    /// Z component of the 3D cross product; positive when `o` is to the left.
    pub fn cross(self, o: Self) -> f64 {
        self.x * o.y - self.y * o.x
    }

    pub fn length(self) -> f64 {
        self.x.hypot(self.y)
    }

    pub fn distance(self, o: Self) -> f64 {
        (self - o).length()
    }

    pub fn angle(self) -> f64 {
        self.y.atan2(self.x)
    }

    pub fn normalized(self) -> Self {
        let len = self.length();
        if len < 1e-12 { Self::ZERO } else { self * (1. / len) }
    }

    /// Rotated 90° counter-clockwise.
    pub fn perp(self) -> Self {
        vec2(-self.y, self.x)
    }

    pub fn rotated(self, angle: f64) -> Self {
        let (s, c) = angle.sin_cos();
        vec2(self.x * c - self.y * s, self.x * s + self.y * c)
    }

    pub fn lerp(self, o: Self, t: f64) -> Self {
        self + (o - self) * t
    }
}

impl Add for Vec2 {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        vec2(self.x + o.x, self.y + o.y)
    }
}

impl AddAssign for Vec2 {
    fn add_assign(&mut self, o: Self) {
        *self = *self + o;
    }
}

impl Sub for Vec2 {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        vec2(self.x - o.x, self.y - o.y)
    }
}

impl SubAssign for Vec2 {
    fn sub_assign(&mut self, o: Self) {
        *self = *self - o;
    }
}

impl Mul<f64> for Vec2 {
    type Output = Self;
    fn mul(self, k: f64) -> Self {
        vec2(self.x * k, self.y * k)
    }
}

impl Neg for Vec2 {
    type Output = Self;
    fn neg(self) -> Self {
        vec2(-self.x, -self.y)
    }
}

/// Position plus body heading (radians, counter-clockwise from +x).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Pose {
    pub x: f64,
    pub y: f64,
    pub theta: f64,
}

impl Pose {
    pub fn new(pos: Vec2, theta: f64) -> Self {
        Self { x: pos.x, y: pos.y, theta }
    }

    pub fn pos(&self) -> Vec2 {
        vec2(self.x, self.y)
    }

    pub fn set_pos(&mut self, p: Vec2) {
        self.x = p.x;
        self.y = p.y;
    }

    /// Transforms a point from the robot frame (x forward, y left) to table space.
    pub fn to_world(self, local: Vec2) -> Vec2 {
        self.pos() + local.rotated(self.theta)
    }
}

/// Wraps an angle into `(-π, π]`.
pub fn wrap_angle(a: f64) -> f64 {
    let mut a = a % TAU;
    if a <= -PI {
        a += TAU;
    } else if a > PI {
        a -= TAU;
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_angle_range() {
        assert!((wrap_angle(3. * PI) - PI).abs() < 1e-12);
        assert!((wrap_angle(-PI) - PI).abs() < 1e-12);
        assert!((wrap_angle(PI / 2. + TAU) - PI / 2.).abs() < 1e-12);
    }

    #[test]
    fn rotate_and_cross() {
        let v = vec2(1., 0.).rotated(PI / 2.);
        assert!((v.x).abs() < 1e-12 && (v.y - 1.).abs() < 1e-12);
        assert!(vec2(1., 0.).cross(vec2(0., 1.)) > 0.);
    }

    #[test]
    fn serde_as_array() {
        let s = serde_json::to_string(&vec2(1.5, -2.)).unwrap();
        assert_eq!(s, "[1.5,-2.0]");
    }
}
