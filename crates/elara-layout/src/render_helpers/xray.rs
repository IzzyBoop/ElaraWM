// ELARA: stand-in for niri's xray effect, which shows the wallpaper through translucent windows.
// See `render_helpers/mod.rs`. `XrayPos` and the fields of `Xray` are niri's. The buffers that
// `Xray` holds are empty here: there is no backdrop to sample.

use std::cell::RefCell;
use std::rc::Rc;

use smithay::backend::renderer::Color32F;
use smithay::utils::{Logical, Point, Rectangle};

use super::effect_buffer::EffectBuffer;
use super::RenderTarget;

#[derive(Debug, Default)]
pub struct Xray {
    // The buffers are per-render-target to avoid constant rerendering when screencasting.
    pub background: [Rc<RefCell<EffectBuffer>>; RenderTarget::COUNT],
    pub backdrop: [Rc<RefCell<EffectBuffer>>; RenderTarget::COUNT],
    pub backdrop_color: Color32F,
    pub workspaces: Vec<(Rectangle<f64, Logical>, Color32F)>,
}

/// Position for drawing xray background.
#[derive(Debug, Clone, Copy)]
pub struct XrayPos {
    /// Position of geometry relative to the backdrop in zoomed coordinates.
    ///
    /// Should be upscaled by `zoom` to get position in backdrop coordinates.
    pub pos_in_backdrop: Point<f64, Logical>,

    /// Zoom factor between backdrop coordinates and geometry.
    pub zoom: f64,
}

impl XrayPos {
    pub fn new(pos_in_backdrop: Point<f64, Logical>, zoom: f64) -> Self {
        Self {
            pos_in_backdrop: pos_in_backdrop.downscale(zoom),
            zoom,
        }
    }

    pub fn offset(mut self, offset: Point<f64, Logical>) -> Self {
        self.pos_in_backdrop += offset;
        self
    }
}

impl Default for XrayPos {
    fn default() -> Self {
        Self {
            pos_in_backdrop: Point::new(0., 0.),
            zoom: 1.,
        }
    }
}

impl Xray {
    pub fn new() -> Self {
        Self::default()
    }
}
