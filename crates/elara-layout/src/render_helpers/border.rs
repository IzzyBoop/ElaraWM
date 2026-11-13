// ELARA: stand-in for niri's border element, which draws focus rings and borders with a shader.
// See `render_helpers/mod.rs`. The constructor signatures are niri's. The element keeps the
// parameters niri computed, so they can be drawn another way later.

use niri_config::{Color, CornerRadius, GradientInterpolation};
use smithay::backend::renderer::element::{Element, RenderElement};
use smithay::backend::renderer::utils::CommitCounter;
use smithay::utils::{Logical, Physical, Point, Rectangle, Scale, Size};

use super::renderer::NiriRenderer;

/// Renders a wide variety of borders and border parts.
///
/// This render element can render parts of a border, so that it can be used with border widths
/// greater than the corner radius, or with borders made of multiple rectangles.
#[derive(Debug, Clone)]
pub struct BorderRenderElement {
    location: Point<f64, Logical>,
    params: Parameters,
    commit: CommitCounter,
}

/// What a [`BorderRenderElement`] would draw.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Parameters {
    /// Size of the area this element covers.
    pub size: Size<f64, Logical>,
    /// Area that the gradient spans, relative to the element.
    pub gradient_area: Rectangle<f64, Logical>,
    /// Color space the gradient is interpolated in.
    pub gradient_format: GradientInterpolation,
    /// Color at the start of the gradient.
    pub color_from: Color,
    /// Color at the end of the gradient.
    pub color_to: Color,
    /// Angle of the gradient in radians.
    pub angle: f32,
    /// Outer rectangle of the whole border, relative to the element.
    pub geometry: Rectangle<f64, Logical>,
    /// Width of the border line.
    pub border_width: f32,
    /// Corner radius of the outer rectangle.
    pub corner_radius: CornerRadius,
    /// Scale of the output the border is on.
    pub scale: f32,
    /// Opacity.
    pub alpha: f32,
}

impl BorderRenderElement {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        size: Size<f64, Logical>,
        gradient_area: Rectangle<f64, Logical>,
        gradient_format: GradientInterpolation,
        color_from: Color,
        color_to: Color,
        angle: f32,
        geometry: Rectangle<f64, Logical>,
        border_width: f32,
        corner_radius: CornerRadius,
        scale: f32,
        alpha: f32,
    ) -> Self {
        Self {
            location: Point::default(),
            params: Parameters {
                size,
                gradient_area,
                gradient_format,
                color_from,
                color_to,
                angle,
                geometry,
                border_width,
                corner_radius,
                scale,
                alpha,
            },
            commit: CommitCounter::default(),
        }
    }

    pub fn empty() -> Self {
        Self::new(
            Default::default(),
            Default::default(),
            GradientInterpolation::default(),
            Default::default(),
            Default::default(),
            0.,
            Default::default(),
            0.,
            Default::default(),
            1.,
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
        gradient_area: Rectangle<f64, Logical>,
        gradient_format: GradientInterpolation,
        color_from: Color,
        color_to: Color,
        angle: f32,
        geometry: Rectangle<f64, Logical>,
        border_width: f32,
        corner_radius: CornerRadius,
        scale: f32,
        alpha: f32,
    ) {
        let params = Parameters {
            size,
            gradient_area,
            gradient_format,
            color_from,
            color_to,
            angle,
            geometry,
            border_width,
            corner_radius,
            scale,
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

    pub fn alpha(&self) -> f32 {
        self.params.alpha
    }

    /// Where the element sits.
    pub fn location(&self) -> Point<f64, Logical> {
        self.location
    }

    /// What the element would draw.
    pub fn parameters(&self) -> &Parameters {
        &self.params
    }

    /// Whether the border shader is available. Without it niri falls back to plain rectangles
    /// with square corners; the recorded parameters describe the full border, so report it as
    /// available.
    pub fn has_shader(renderer: &mut impl NiriRenderer) -> bool {
        let _ = renderer;
        true
    }
}

impl Default for BorderRenderElement {
    fn default() -> Self {
        Self::empty()
    }
}

impl Element for BorderRenderElement {
    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        Rectangle::new(self.location, self.params.size).to_physical_precise_round(scale)
    }
}

impl<R> RenderElement<R> for BorderRenderElement {}
