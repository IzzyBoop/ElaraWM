// ELARA: stand-in for niri's shadow element. See `render_helpers/mod.rs`. The constructor
// signatures are niri's. The element keeps the parameters niri computed.

use niri_config::{Color, CornerRadius};
use smithay::backend::renderer::element::{Element, RenderElement};
use smithay::backend::renderer::utils::CommitCounter;
use smithay::utils::{Logical, Physical, Point, Rectangle, Scale, Size};

use super::renderer::NiriRenderer;

/// Renders a rounded rectangle shadow.
#[derive(Debug, Clone)]
pub struct ShadowRenderElement {
    location: Point<f64, Logical>,
    params: Parameters,
    commit: CommitCounter,
}

/// What a [`ShadowRenderElement`] would draw.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Parameters {
    /// Size of the area this element covers.
    pub size: Size<f64, Logical>,
    /// Rectangle casting the shadow, relative to the element.
    pub geometry: Rectangle<f64, Logical>,
    /// Shadow color.
    pub color: Color,
    /// Blur radius.
    pub sigma: f32,
    /// Corner radius of the rectangle casting the shadow.
    pub corner_radius: CornerRadius,
    /// Scale of the output the shadow is on.
    pub scale: f32,
    /// Window rectangle to cut out of the shadow, relative to the element.
    pub window_geometry: Rectangle<f64, Logical>,
    /// Corner radius of the window rectangle.
    pub window_corner_radius: CornerRadius,
    /// Opacity.
    pub alpha: f32,
}

impl ShadowRenderElement {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        size: Size<f64, Logical>,
        geometry: Rectangle<f64, Logical>,
        color: Color,
        sigma: f32,
        corner_radius: CornerRadius,
        scale: f32,
        window_geometry: Rectangle<f64, Logical>,
        window_corner_radius: CornerRadius,
        alpha: f32,
    ) -> Self {
        Self {
            location: Point::default(),
            params: Parameters {
                size,
                geometry,
                color,
                sigma,
                corner_radius,
                scale,
                window_geometry,
                window_corner_radius,
                alpha,
            },
            commit: CommitCounter::default(),
        }
    }

    pub fn empty() -> Self {
        Self::new(
            Default::default(),
            Default::default(),
            Default::default(),
            0.,
            Default::default(),
            1.,
            Default::default(),
            Default::default(),
            1.,
        )
    }

    pub fn damage_all(&mut self) {
        self.commit.increment();
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        size: Size<f64, Logical>,
        geometry: Rectangle<f64, Logical>,
        color: Color,
        sigma: f32,
        corner_radius: CornerRadius,
        scale: f32,
        window_geometry: Rectangle<f64, Logical>,
        window_corner_radius: CornerRadius,
        alpha: f32,
    ) {
        let params = Parameters {
            size,
            geometry,
            color,
            sigma,
            corner_radius,
            scale,
            window_geometry,
            window_corner_radius,
            alpha,
        };
        if self.params == params {
            return;
        }

        self.params = params;
        self.commit.increment();
    }

    pub fn with_location(mut self, location: Point<f64, Logical>) -> Self {
        self.location = location;
        self
    }

    pub fn with_alpha(mut self, alpha: f32) -> Self {
        self.params.alpha = alpha;
        self
    }

    /// Where the element sits.
    pub fn location(&self) -> Point<f64, Logical> {
        self.location
    }

    /// What the element would draw.
    pub fn parameters(&self) -> &Parameters {
        &self.params
    }

    /// Whether the shadow shader is available. See `BorderRenderElement::has_shader`.
    pub fn has_shader(renderer: &mut impl NiriRenderer) -> bool {
        let _ = renderer;
        true
    }
}

impl Default for ShadowRenderElement {
    fn default() -> Self {
        Self::empty()
    }
}

impl Element for ShadowRenderElement {
    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        Rectangle::new(self.location, self.params.size).to_physical_precise_round(scale)
    }
}

impl<R> RenderElement<R> for ShadowRenderElement {}
