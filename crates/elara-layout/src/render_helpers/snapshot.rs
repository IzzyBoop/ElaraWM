// ELARA: stand-in for niri's render snapshot. See `render_helpers/mod.rs`. The struct is niri's.
// Turning a snapshot into a texture needs a GL renderer, so `texture()` cannot be called.

use std::cell::OnceCell;

use niri_config::BlockOutFrom;
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::utils::{Logical, Physical, Rectangle, Scale, Size};

use super::RenderCtx;

/// Snapshot of a render.
#[derive(Debug)]
pub struct RenderSnapshot<C, B> {
    /// Contents for a normal render.
    ///
    /// Relative to the geometry.
    pub contents: Vec<C>,

    /// Contents that are not blocked out, but the background is blocked out.
    ///
    /// If `None` then the background doesn't have any blocked-out surfaces, and normal `contents`
    /// can be used instead.
    pub contents_with_blocked_out_bg: Option<Vec<C>>,

    /// Blocked-out contents.
    ///
    /// Relative to the geometry.
    pub blocked_out_contents: Vec<B>,

    /// Where the contents were blocked out from at the time of the snapshot.
    pub block_out_from: Option<BlockOutFrom>,

    /// Visual size of the element at the point of the snapshot.
    pub size: Size<f64, Logical>,

    /// Contents rendered into a texture (lazily).
    pub texture: OnceCell<Option<(GlesTexture, Rectangle<i32, Physical>)>>,

    /// Contents with blocked-out bg rendered into a texture (lazily).
    pub texture_with_blocked_out_bg: OnceCell<Option<(GlesTexture, Rectangle<i32, Physical>)>>,

    /// Blocked-out contents rendered into a texture (lazily).
    pub blocked_out_texture: OnceCell<Option<(GlesTexture, Rectangle<i32, Physical>)>>,
}

impl<C, B> RenderSnapshot<C, B> {
    pub fn texture(
        &self,
        ctx: RenderCtx<GlesRenderer>,
        _scale: Scale<f64>,
    ) -> Option<&(GlesTexture, Rectangle<i32, Physical>)> {
        match *ctx.renderer {}
    }
}
