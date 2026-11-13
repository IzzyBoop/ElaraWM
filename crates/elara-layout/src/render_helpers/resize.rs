// ELARA: stand-in for niri's resize element, which crossfades between a window's old and new
// contents while it resizes. See `render_helpers/mod.rs`. It needs two textures, and the
// texture type has no values, so `new()` cannot be called.

use niri_config::CornerRadius;
use smithay::backend::renderer::element::{Element, Id, RenderElement};
use smithay::backend::renderer::gles::GlesTexture;
use smithay::utils::{Logical, Physical, Rectangle, Scale, Size};

use super::renderer::NiriRenderer;
use super::shader_element::ShaderRenderElement;

#[derive(Debug)]
pub struct ResizeRenderElement(ShaderRenderElement);

impl ResizeRenderElement {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        area: Rectangle<f64, Logical>,
        scale: Scale<f64>,
        texture_prev: (GlesTexture, Rectangle<i32, Physical>),
        size_prev: Size<f64, Logical>,
        texture_next: (GlesTexture, Rectangle<i32, Physical>),
        size_next: Size<f64, Logical>,
        progress: f32,
        clamped_progress: f32,
        corner_radius: CornerRadius,
        clip_to_geometry: bool,
        result_alpha: f32,
    ) -> Self {
        let _ = (area, scale, size_prev, texture_next, size_next);
        let _ = (
            progress,
            clamped_progress,
            corner_radius,
            clip_to_geometry,
            result_alpha,
        );
        match texture_prev.0 {}
    }

    pub fn id(&self) -> &Id {
        self.0.id()
    }

    /// Whether the resize shader is available. It is not: without textures of the window's
    /// contents there is nothing to crossfade.
    pub fn has_shader(renderer: &mut impl NiriRenderer) -> bool {
        let _ = renderer;
        false
    }
}

impl Element for ResizeRenderElement {
    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        self.0.geometry(scale)
    }
}

impl<R> RenderElement<R> for ResizeRenderElement {}
