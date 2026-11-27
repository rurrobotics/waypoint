use serde::{Deserialize, Serialize};

use super::shape::rect_corners;
use super::vec::{Pose, Vec2, vec2};

/// Rectangular robot body. The pose of a path is the robot's rotation centre.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Robot {
    /// Size along the direction of travel (mm).
    pub length: f64,
    /// Size across the direction of travel (mm).
    pub width: f64,
    /// Body centre relative to the rotation centre, in the robot frame
    /// (x forward, y left).
    #[serde(default)]
    pub offset: Vec2,
    /// Clearance the path must keep from walls and obstacles (mm).
    #[serde(default)]
    pub margin: f64,
}

impl Default for Robot {
    fn default() -> Self {
        Self { length: 300., width: 250., offset: Vec2::ZERO, margin: 20. }
    }
}

impl Robot {
    /// Body corners at `pose`, counter-clockwise.
    pub fn corners(&self, pose: Pose) -> [Vec2; 4] {
        rect_corners(Vec2::ZERO, vec2(self.length, self.width), 0.).map(|c| pose.to_world(c + self.offset))
    }

    /// Radius of the circle swept by a turn in place.
    pub fn sweep_radius(&self) -> f64 {
        rect_corners(self.offset, vec2(self.length, self.width), 0.)
            .iter()
            .map(|c| c.length())
            .fold(0., f64::max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::FRAC_PI_2;

    #[test]
    fn corners_follow_heading() {
        let robot = Robot { length: 200., width: 100., offset: Vec2::ZERO, margin: 0. };
        let c = robot.corners(Pose { x: 1000., y: 500., theta: FRAC_PI_2 });
        let xs: Vec<f64> = c.iter().map(|p| p.x).collect();
        let ys: Vec<f64> = c.iter().map(|p| p.y).collect();
        // Facing +y, the long side runs along y.
        assert!((xs.iter().cloned().fold(f64::MIN, f64::max) - 1050.).abs() < 1e-9);
        assert!((ys.iter().cloned().fold(f64::MIN, f64::max) - 600.).abs() < 1e-9);
    }

    #[test]
    fn sweep_radius_with_offset() {
        let robot = Robot { length: 200., width: 100., offset: vec2(50., 0.), margin: 0. };
        assert!((robot.sweep_radius() - (150f64.hypot(50.))).abs() < 1e-9);
    }
}
