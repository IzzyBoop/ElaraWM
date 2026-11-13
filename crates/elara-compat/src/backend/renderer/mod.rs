//! Mirrors `smithay::backend::renderer`.
//!
//! macOS draws application windows itself, so nothing here renders. [`Color32F`] is Smithay's
//! own type. Everything else is an inert stand-in that lets niri's layout code compile:
//!
//! - GL objects ([`gles::GlesRenderer`], [`gles::GlesTexture`]) are uninhabited. Code that needs
//!   one can be compiled but never run, which is checked by the compiler rather than by convention.
//! - Render elements are plain data describing what niri would have drawn.

// ELARA: copied Smithay files are excluded from rustfmt and clippy so they stay verbatim.
#[rustfmt::skip]
#[allow(clippy::all)]
mod color;
pub mod element;
pub mod gles;
pub mod sync;
pub mod utils;

use std::fmt;

pub use self::color::Color32F;
use crate::utils::{Buffer, Size};

/// A texture that a renderer can draw.
pub trait Texture: fmt::Debug {
    /// Width of the texture in pixels.
    fn width(&self) -> u32;
    /// Height of the texture in pixels.
    fn height(&self) -> u32;
    /// Size of the texture in pixels.
    fn size(&self) -> Size<i32, Buffer> {
        Size::from((self.width() as i32, self.height() as i32))
    }
}
