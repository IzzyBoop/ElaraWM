// ELARA: stand-in for niri's primary GPU texture element. See `render_helpers/mod.rs`.

use smithay::backend::renderer::element::{Element, RenderElement};
use smithay::backend::renderer::gles::GlesTexture;
use smithay::utils::{Physical, Rectangle, Scale};

use super::texture::TextureRenderElement;

/// Wrapper for a texture from the primary GPU for rendering with the primary GPU.
#[derive(Debug)]
pub struct PrimaryGpuTextureRenderElement(pub TextureRenderElement<GlesTexture>);

impl Element for PrimaryGpuTextureRenderElement {
    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        self.0.geometry(scale)
    }
}

impl<R> RenderElement<R> for PrimaryGpuTextureRenderElement {}
