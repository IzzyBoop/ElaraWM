// ELARA: this module replaces niri's `render_helpers`, which draws with OpenGL through Smithay.
//
// On macOS the window server draws application windows, so nothing here renders. The module
// keeps the names, types and signatures that niri's layout code uses, so that code is imported
// almost unchanged:
//
// - Items that only carry data (`RenderCtx`, `RenderTarget`, `BakedBuffer`, solid colors, borders,
//   shadows, damage) keep that data. A border or shadow element records the parameters niri
//   computed for it, which is what a later overlay-window implementation needs.
// - Items that need a GL renderer or texture take the uninhabited stand-ins from the `smithay`
//   compat crate. Their bodies are `match *renderer {}`: they type-check, and the compiler proves
//   they cannot run.
//
// Nothing here is copied from niri except where a comment says so.

use niri_config::BlockOutFrom;
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::{Element, Kind, RenderElement};
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::backend::renderer::sync::SyncPoint;
use smithay::utils::{Logical, Physical, Point, Rectangle, Scale, Size, Transform};

use self::primary_gpu_texture::PrimaryGpuTextureRenderElement;
use self::renderer::AsGlesRenderer;
use self::solid_color::{SolidColorBuffer, SolidColorRenderElement};
use self::texture::{TextureBuffer, TextureRenderElement};
use self::xray::Xray;

pub mod background_effect;
pub mod border;
pub mod clipped_surface;
pub mod damage;
pub mod effect_buffer;
pub mod offscreen;
pub mod primary_gpu_texture;
pub mod render_elements;
pub mod renderer;
pub mod resize;
pub mod shader_element;
pub mod shaders;
pub mod shadow;
pub mod snapshot;
pub mod solid_color;
pub mod texture;
pub mod xray;

// The items from here to `encompassing_geo` are niri's, unchanged.

/// A rendering context.
///
/// Bundles together things needed by most rendering code.
pub struct RenderCtx<'a, R> {
    pub renderer: &'a mut R,
    pub target: RenderTarget,
    pub xray: Option<&'a Xray>,
}

impl<'a, R> RenderCtx<'a, R> {
    /// Reborrows this context with a smaller lifetime.
    #[inline]
    pub fn r<'b>(&'b mut self) -> RenderCtx<'b, R> {
        RenderCtx {
            renderer: self.renderer,
            target: self.target,
            xray: self.xray,
        }
    }
}

impl<'a, R: AsGlesRenderer> RenderCtx<'a, R> {
    pub fn as_gles<'b>(&'b mut self) -> RenderCtx<'b, GlesRenderer> {
        RenderCtx {
            renderer: self.renderer.as_gles_renderer(),
            target: self.target,
            xray: self.xray,
        }
    }
}

/// What we're rendering for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderTarget {
    /// Rendering to display on screen.
    Output = 0,
    /// Rendering for a screencast.
    Screencast,
    /// Rendering for any other screen capture.
    ScreenCapture,
}

/// Buffer with location, src and dst.
#[derive(Debug)]
pub struct BakedBuffer<B> {
    pub buffer: B,
    pub location: Point<f64, Logical>,
    pub src: Option<Rectangle<f64, Logical>>,
    pub dst: Option<Size<i32, Logical>>,
}

pub trait ToRenderElement {
    type RenderElement;

    fn to_render_element(
        &self,
        location: Point<f64, Logical>,
        scale: Scale<f64>,
        alpha: f32,
        kind: Kind,
    ) -> Self::RenderElement;
}

impl RenderTarget {
    pub const COUNT: usize = 3;

    pub fn should_block_out(self, block_out_from: Option<BlockOutFrom>) -> bool {
        match block_out_from {
            None => false,
            Some(BlockOutFrom::Screencast) => self == RenderTarget::Screencast,
            Some(BlockOutFrom::ScreenCapture) => self != RenderTarget::Output,
        }
    }
}

impl ToRenderElement for BakedBuffer<TextureBuffer<GlesTexture>> {
    type RenderElement = PrimaryGpuTextureRenderElement;

    fn to_render_element(
        &self,
        location: Point<f64, Logical>,
        _scale: Scale<f64>,
        alpha: f32,
        kind: Kind,
    ) -> Self::RenderElement {
        let elem = TextureRenderElement::from_texture_buffer(
            self.buffer.clone(),
            location + self.location,
            alpha,
            self.src,
            self.dst.map(|dst| dst.to_f64()),
            kind,
        );
        PrimaryGpuTextureRenderElement(elem)
    }
}

impl ToRenderElement for BakedBuffer<SolidColorBuffer> {
    type RenderElement = SolidColorRenderElement;

    fn to_render_element(
        &self,
        location: Point<f64, Logical>,
        _scale: Scale<f64>,
        alpha: f32,
        kind: Kind,
    ) -> Self::RenderElement {
        SolidColorRenderElement::from_buffer(&self.buffer, location + self.location, alpha, kind)
    }
}

pub fn encompassing_geo(
    scale: Scale<f64>,
    elements: impl Iterator<Item = impl Element>,
) -> Rectangle<i32, Physical> {
    elements
        .map(|ele| ele.geometry(scale))
        .reduce(|a, b| a.merge(b))
        .unwrap_or_default()
}

/// Renders elements into a new texture just large enough to hold them. Needs a GL renderer, so
/// it cannot be called.
pub fn render_to_encompassing_texture(
    renderer: &mut GlesRenderer,
    _scale: Scale<f64>,
    _transform: Transform,
    _fourcc: Fourcc,
    _elements: &[impl RenderElement<GlesRenderer>],
) -> anyhow::Result<(GlesTexture, SyncPoint, Rectangle<i32, Physical>)> {
    match *renderer {}
}
