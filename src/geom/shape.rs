use serde::{Deserialize, Serialize};

use super::vec::{Vec2, vec2};

/// An obstacle or region outline in table space (mm).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Shape {
    Rect {
        center: Vec2,
        size: Vec2,
        /// Radians, counter-clockwise.
        #[serde(default)]
        rotation: f64,
    },
    Circle { center: Vec2, radius: f64 },
    /// Convex polygon; vertices in either winding.
    Polygon { points: Vec<Vec2> },
}

impl Shape {
    /// Rectangle spanning two opposite corners.
    pub fn rect_from_corners(a: Vec2, b: Vec2) -> Self {
        Shape::Rect {
            center: a.lerp(b, 0.5),
            size: vec2((a.x - b.x).abs(), (a.y - b.y).abs()),
            rotation: 0.,
        }
    }

    /// Vertices for polygonal shapes, `None` for circles.
    pub fn vertices(&self) -> Option<Vec<Vec2>> {
        match self {
            Shape::Rect { center, size, rotation } => Some(rect_corners(*center, *size, *rotation).to_vec()),
            Shape::Polygon { points } => Some(points.clone()),
            Shape::Circle { .. } => None,
        }
    }

    pub fn center(&self) -> Vec2 {
        match self {
            Shape::Rect { center, .. } | Shape::Circle { center, .. } => *center,
            Shape::Polygon { points } => {
                let sum = points.iter().fold(Vec2::ZERO, |a, &p| a + p);
                sum * (1. / points.len().max(1) as f64)
            }
        }
    }

    pub fn translate(&mut self, d: Vec2) {
        match self {
            Shape::Rect { center, .. } | Shape::Circle { center, .. } => *center += d,
            Shape::Polygon { points } => points.iter_mut().for_each(|p| *p += d),
        }
    }

    pub fn contains(&self, p: Vec2) -> bool {
        match self {
            Shape::Circle { center, radius } => p.distance(*center) <= *radius,
            _ => self.vertices().is_some_and(|v| polygon_contains(&v, p)),
        }
    }
}

pub fn rect_corners(center: Vec2, size: Vec2, rotation: f64) -> [Vec2; 4] {
    let h = size * 0.5;
    [vec2(-h.x, -h.y), vec2(h.x, -h.y), vec2(h.x, h.y), vec2(-h.x, h.y)].map(|c| center + c.rotated(rotation))
}

/// True for a convex, non-degenerate polygon (either winding).
pub fn is_convex(points: &[Vec2]) -> bool {
    if points.len() < 3 {
        return false;
    }
    let n = points.len();
    let mut sign = 0.;
    for i in 0..n {
        let a = points[i];
        let b = points[(i + 1) % n];
        let c = points[(i + 2) % n];
        let z = (b - a).cross(c - b);
        if z.abs() < 1e-9 {
            continue;
        }
        if sign == 0. {
            sign = z.signum();
        } else if z.signum() != sign {
            return false;
        }
    }
    sign != 0.
}

/// Point-in-polygon by ray casting (works for any simple polygon).
pub fn polygon_contains(poly: &[Vec2], p: Vec2) -> bool {
    let mut inside = false;
    let n = poly.len();
    for i in 0..n {
        let a = poly[i];
        let b = poly[(i + n - 1) % n];
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convexity() {
        let square = [vec2(0., 0.), vec2(1., 0.), vec2(1., 1.), vec2(0., 1.)];
        assert!(is_convex(&square));
        let mut cw = square;
        cw.reverse();
        assert!(is_convex(&cw));
        let dart = [vec2(0., 0.), vec2(2., 1.), vec2(0., 2.), vec2(1., 1.)];
        assert!(!is_convex(&dart));
    }

    #[test]
    fn rect_contains_rotated() {
        let r = Shape::Rect { center: vec2(0., 0.), size: vec2(100., 10.), rotation: std::f64::consts::FRAC_PI_2 };
        assert!(r.contains(vec2(0., 40.)));
        assert!(!r.contains(vec2(40., 0.)));
    }
}
