//! Mirrors `smithay::backend::allocator`.

/// Pixel formats named by niri's layout code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Fourcc {
    Abgr8888,
    Argb8888,
    Xbgr8888,
    Xrgb8888,
}
