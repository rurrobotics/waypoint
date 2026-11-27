//! Pure geometry: no UI types. Units are millimetres and radians, with the
//! table origin in a corner, x along the long side and angles counter-clockwise.

pub mod collision;
pub mod footprint;
pub mod path;
pub mod shape;
pub mod vec;

pub use footprint::Robot;
pub use path::{Path, Sample, Segment, SegmentKind};
pub use shape::Shape;
pub use vec::{Pose, Vec2, vec2};
