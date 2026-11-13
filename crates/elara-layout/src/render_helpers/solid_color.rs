// ELARA: stand-in for niri's solid color buffer and element. See `render_helpers/mod.rs`.
//
// The buffer and the element's constructors and accessors are niri's. The drawing code is gone.

use smithay::backend::renderer::element::{Element, Id, Kind, RenderElement};
use smithay::backend::renderer::utils::CommitCounter;
use smithay::backend::renderer::Color32F;
use smithay::utils::{Logical, Physical, Point, Rectangle, Scale, Size};

/// Smithay's solid color buffer, but with fractional scale.
#[derive(Debug, Clone)]
pub struct SolidColorBuffer {
    id: Id,
    size: Size<f64, Logical>,
    commit: CommitCounter,
    color: Color32F,
}

/// Render element for a [`SolidColorBuffer`].
#[derive(Debug, Clone)]
pub struct SolidColorRenderElement {
    id: Id,
    geometry: Rectangle<f64, Logical>,
    commit: CommitCounter,
    color: Color32F,
    kind: Kind,
}

impl Default for SolidColorBuffer {
    fn default() -> Self {
        Self {
            id: Id::new(),
            size: Default::default(),
            commit: Default::default(),
            color: Default::default(),
        }
    }
}

impl SolidColorBuffer {
    pub fn new(size: impl Into<Size<f64, Logical>>, color: impl Into<Color32F>) -> Self {
        SolidColorBuffer {
            id: Id::new(),
            color: color.into(),
            commit: CommitCounter::default(),
            size: size.into(),
        }
    }

    pub fn resize(&mut self, size: impl Into<Size<f64, Logical>>) {
        let size = size.into();
        if size != self.size {
            self.size = size;
            self.commit.increment();
        }
    }

    pub fn set_color(&mut self, color: impl Into<Color32F>) {
        let color = color.into();
        if color != self.color {
            self.color = color;
            self.commit.increment();
        }
    }

    pub fn update(&mut self, size: impl Into<Size<f64, Logical>>, color: impl Into<Color32F>) {
        let size = size.into();
        let color = color.into();
        if size != self.size || color != self.color {
            self.size = size;
            self.color = color;
            self.commit.increment();
        }
    }

    pub fn color(&self) -> Color32F {
        self.color
    }

    pub fn size(&self) -> Size<f64, Logical> {
        self.size
    }
}

impl SolidColorRenderElement {
    pub fn from_buffer(
        buffer: &SolidColorBuffer,
        location: impl Into<Point<f64, Logical>>,
        alpha: f32,
        kind: Kind,
    ) -> Self {
        let geo = Rectangle::new(location.into(), buffer.size());
        let color = buffer.color * alpha;
        Self::new(buffer.id.clone(), geo, buffer.commit, color, kind)
    }

    pub fn new(
        id: Id,
        geometry: Rectangle<f64, Logical>,
        commit: CommitCounter,
        color: Color32F,
        kind: Kind,
    ) -> Self {
        SolidColorRenderElement {
            id,
            geometry,
            commit,
            color,
            kind,
        }
    }

    pub fn color(&self) -> Color32F {
        self.color
    }

    pub fn geo(&self) -> Rectangle<f64, Logical> {
        self.geometry
    }

    pub fn id(&self) -> &Id {
        &self.id
    }

    pub fn current_commit(&self) -> CommitCounter {
        self.commit
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }
}

impl Element for SolidColorRenderElement {
    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        self.geometry.to_physical_precise_round(scale)
    }
}

impl<R> RenderElement<R> for SolidColorRenderElement {}
