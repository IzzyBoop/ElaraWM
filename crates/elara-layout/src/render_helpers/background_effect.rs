// ELARA: stand-in for niri's background effects (blur and xray behind translucent windows). See
// `render_helpers/mod.rs`. macOS composites windows itself, so there is no backdrop to process
// and the element has no values.

use smithay::backend::renderer::element::{Element, RenderElement};
use smithay::utils::{Physical, Rectangle, Scale};

/// Element that would draw a window's background effect. It has no values.
#[derive(Debug)]
pub enum BackgroundEffectElement {}

impl Element for BackgroundEffectElement {
    fn geometry(&self, _scale: Scale<f64>) -> Rectangle<i32, Physical> {
        match *self {}
    }
}

impl<R> RenderElement<R> for BackgroundEffectElement {}
