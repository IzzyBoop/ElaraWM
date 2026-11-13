// ELARA: stand-in for niri's offscreen buffer, which renders elements into a texture so they can
// be drawn as one. See `render_helpers/mod.rs`. Rendering needs a GL renderer, so `render()`
// cannot be called and an `OffscreenRenderElement` never exists.

use smithay::backend::renderer::element::{Element, Id, RenderElement, RenderElementStates};
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::backend::renderer::sync::SyncPoint;
use smithay::utils::{Logical, Physical, Point, Rectangle, Scale, Size};

/// Buffer for offscreen rendering.
#[derive(Debug, Default)]
pub struct OffscreenBuffer {}

/// Render element for an [`OffscreenBuffer`].
#[derive(Debug, Clone)]
pub struct OffscreenRenderElement {
    texture: GlesTexture,
    offset: Point<f64, Logical>,
    size: Size<f64, Logical>,
    alpha: f32,
}

/// Offscreen data that can be stored to update the offscreened elements' states.
#[derive(Debug)]
pub struct OffscreenData {
    /// Id of the offscreen element.
    pub id: Id,
    /// States for the render into the offscreen buffer.
    pub states: RenderElementStates,
}

impl OffscreenBuffer {
    pub fn render(
        &self,
        renderer: &mut GlesRenderer,
        _scale: Scale<f64>,
        _elements: &[impl RenderElement<GlesRenderer>],
    ) -> anyhow::Result<(OffscreenRenderElement, SyncPoint, OffscreenData)> {
        match *renderer {}
    }
}

impl OffscreenRenderElement {
    pub fn texture(&self) -> &GlesTexture {
        &self.texture
    }

    pub fn offset(&self) -> Point<f64, Logical> {
        self.offset
    }

    pub fn with_alpha(mut self, alpha: f32) -> Self {
        self.alpha = alpha;
        self
    }

    pub fn with_offset(mut self, offset: Point<f64, Logical>) -> Self {
        self.offset = offset;
        self
    }

    pub fn logical_size(&self) -> Size<f64, Logical> {
        self.size
    }
}

impl Element for OffscreenRenderElement {
    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        Rectangle::new(self.offset, self.size).to_physical_precise_round(scale)
    }
}

impl<R> RenderElement<R> for OffscreenRenderElement {}
