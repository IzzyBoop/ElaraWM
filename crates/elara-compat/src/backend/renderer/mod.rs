//! Mirrors `smithay::backend::renderer`. Only the color type exists: macOS draws the windows.

// ELARA: copied Smithay files are excluded from rustfmt and clippy so they stay verbatim.
#[rustfmt::skip]
#[allow(clippy::all)]
mod color;

pub use self::color::Color32F;
