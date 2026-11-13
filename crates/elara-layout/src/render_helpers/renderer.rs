// ELARA: stand-in for niri's renderer traits. See `render_helpers/mod.rs`.

use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::backend::renderer::Texture;

/// What niri's layout code requires of a renderer.
///
/// The only implementor is [`GlesRenderer`], which has no values, so no renderer ever exists.
pub trait NiriRenderer: AsGlesRenderer {
    type NiriTextureId: Texture + Clone + Send + 'static;
}

impl NiriRenderer for GlesRenderer {
    type NiriTextureId = GlesTexture;
}

/// Trait for getting the underlying `GlesRenderer`.
pub trait AsGlesRenderer {
    fn as_gles_renderer(&mut self) -> &mut GlesRenderer;
}

impl AsGlesRenderer for GlesRenderer {
    fn as_gles_renderer(&mut self) -> &mut GlesRenderer {
        self
    }
}
