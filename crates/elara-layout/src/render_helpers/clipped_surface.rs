// ELARA: stand-in for niri's clipped surface element, which rounds the corners of a window's
// contents. See `render_helpers/mod.rs`. It wraps a Wayland surface element, which has no
// values, so neither does this. `RoundedCornerDamage` is niri's.

use std::fmt;

use niri_config::CornerRadius;
use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::backend::renderer::element::{Element, RenderElement};
use smithay::backend::renderer::gles::GlesTexProgram;
use smithay::utils::{Logical, Physical, Rectangle, Scale};

use super::damage::ExtraDamage;
use super::renderer::NiriRenderer;

pub struct ClippedSurfaceRenderElement<R: NiriRenderer> {
    inner: WaylandSurfaceRenderElement<R>,
}

#[derive(Debug, Default, Clone)]
pub struct RoundedCornerDamage {
    damage: ExtraDamage,
    corner_radius: CornerRadius,
}

impl<R: NiriRenderer> ClippedSurfaceRenderElement<R> {
    pub fn new(
        elem: WaylandSurfaceRenderElement<R>,
        scale: Scale<f64>,
        geometry: Rectangle<f64, Logical>,
        program: GlesTexProgram,
        corner_radius: CornerRadius,
    ) -> Self {
        let _ = (scale, geometry, program, corner_radius);
        Self { inner: elem }
    }

    pub fn shader(renderer: &mut R) -> Option<&GlesTexProgram> {
        match *renderer.as_gles_renderer() {}
    }

    pub fn will_clip(
        elem: &WaylandSurfaceRenderElement<R>,
        scale: Scale<f64>,
        geometry: Rectangle<f64, Logical>,
        corner_radius: CornerRadius,
    ) -> bool {
        // A surface element has no values, so this cannot be reached. Answer as niri does for an
        // element that lies entirely inside the geometry.
        let _ = (geometry, corner_radius);
        let _ = elem.geometry(scale);
        false
    }
}

impl<R: NiriRenderer> fmt::Debug for ClippedSurfaceRenderElement<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl<R: NiriRenderer> Element for ClippedSurfaceRenderElement<R> {
    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        self.inner.geometry(scale)
    }
}

impl<R: NiriRenderer> RenderElement<R> for ClippedSurfaceRenderElement<R> {}

impl RoundedCornerDamage {
    pub fn set_corner_radius(&mut self, corner_radius: CornerRadius) {
        if self.corner_radius == corner_radius {
            return;
        }

        // FIXME: make the damage granular.
        self.corner_radius = corner_radius;
        self.damage.damage_all();
    }

    pub fn render(&self, geometry: Rectangle<f64, Logical>) -> ExtraDamage {
        self.damage.render(geometry)
    }
}
