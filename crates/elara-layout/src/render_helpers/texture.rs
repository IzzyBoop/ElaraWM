// ELARA: stand-in for niri's texture buffer and element. See `render_helpers/mod.rs`. A value
// of either type needs a texture, and the only texture type has no values.

use smithay::backend::renderer::element::{Element, Kind, RenderElement};
use smithay::backend::renderer::Texture;
use smithay::utils::{Buffer, Logical, Physical, Point, Rectangle, Scale, Size, Transform};

/// Smithay's texture buffer, but with fractional scale.
#[derive(Debug, Clone)]
pub struct TextureBuffer<T: Texture> {
    texture: T,
    scale: Scale<f64>,
    transform: Transform,
}

/// Render element for a [`TextureBuffer`].
#[derive(Debug, Clone)]
pub struct TextureRenderElement<T: Texture> {
    buffer: TextureBuffer<T>,
    location: Point<f64, Logical>,
    alpha: f32,
    src: Option<Rectangle<f64, Logical>>,
    size: Option<Size<f64, Logical>>,
    kind: Kind,
}

impl<T: Texture> TextureBuffer<T> {
    pub fn from_texture<R>(
        renderer: &R,
        texture: T,
        scale: impl Into<Scale<f64>>,
        transform: Transform,
        opaque_regions: Vec<Rectangle<i32, Buffer>>,
    ) -> Self {
        let _ = (renderer, opaque_regions);
        TextureBuffer {
            texture,
            scale: scale.into(),
            transform,
        }
    }

    pub fn texture(&self) -> &T {
        &self.texture
    }

    pub fn texture_scale(&self) -> Scale<f64> {
        self.scale
    }

    pub fn set_texture_scale(&mut self, scale: impl Into<Scale<f64>>) {
        self.scale = scale.into();
    }

    pub fn texture_transform(&self) -> Transform {
        self.transform
    }

    pub fn set_texture_transform(&mut self, transform: Transform) {
        self.transform = transform;
    }

    pub fn logical_size(&self) -> Size<f64, Logical> {
        self.texture
            .size()
            .to_f64()
            .to_logical(self.scale, self.transform)
    }
}

impl<T: Texture> TextureRenderElement<T> {
    pub fn from_texture_buffer(
        buffer: TextureBuffer<T>,
        location: impl Into<Point<f64, Logical>>,
        alpha: f32,
        src: Option<Rectangle<f64, Logical>>,
        size: Option<Size<f64, Logical>>,
        kind: Kind,
    ) -> Self {
        TextureRenderElement {
            buffer,
            location: location.into(),
            alpha,
            src,
            size,
            kind,
        }
    }

    pub fn buffer(&self) -> &TextureBuffer<T> {
        &self.buffer
    }

    pub fn logical_size(&self) -> Size<f64, Logical> {
        self.size
            .or_else(|| self.src.map(|src| src.size))
            .unwrap_or_else(|| self.buffer.logical_size())
    }

    pub fn alpha(&self) -> f32 {
        self.alpha
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }
}

impl<T: Texture> Element for TextureRenderElement<T> {
    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        Rectangle::new(self.location, self.logical_size()).to_physical_precise_round(scale)
    }
}

impl<R, T: Texture> RenderElement<R> for TextureRenderElement<T> {}
