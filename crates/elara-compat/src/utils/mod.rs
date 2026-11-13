//! Mirrors `smithay::utils`.

// ELARA: copied Smithay files are excluded from rustfmt and clippy so they stay verbatim.
#[rustfmt::skip]
#[allow(clippy::all)]
mod geometry;
#[rustfmt::skip]
#[allow(clippy::all)]
mod serial;
#[rustfmt::skip]
#[allow(clippy::all)]
pub mod user_data;

pub use self::geometry::{
    Buffer, Coordinate, Logical, Physical, Point, Raw, Rectangle, Scale, Size, Transform,
};
pub use self::serial::*;
