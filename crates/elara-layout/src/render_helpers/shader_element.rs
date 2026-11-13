// ELARA: stand-in for niri's shader element, which draws a rectangle with a custom shader. See
// `render_helpers/mod.rs`. The constructor signatures are niri's. A shader program needs a GL
// renderer to compile, so `ShaderProgram` has no values.

use std::collections::HashMap;
use std::rc::Rc;

use smithay::backend::renderer::element::{Element, Id, Kind, RenderElement};
use smithay::backend::renderer::gles::{GlesTexture, Uniform};
use smithay::utils::{Logical, Physical, Point, Rectangle, Scale, Size};

use super::shaders::ProgramType;

/// Renders a shader with optional texture input, on the primary GPU.
#[derive(Debug, Clone)]
pub struct ShaderRenderElement {
    id: Id,
    program: ProgramType,
    location: Point<f64, Logical>,
    size: Size<f64, Logical>,
    scale: f32,
    alpha: f32,
    additional_uniforms: Rc<[Uniform<'static>]>,
    textures: HashMap<String, GlesTexture>,
    kind: Kind,
}

/// A compiled shader program. It has no values.
#[derive(Debug, Clone, PartialEq)]
pub enum ShaderProgram {}

impl ShaderRenderElement {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        program: ProgramType,
        size: Size<f64, Logical>,
        opaque_regions: Option<Vec<Rectangle<f64, Logical>>>,
        scale: f32,
        alpha: f32,
        additional_uniforms: Rc<[Uniform<'static>]>,
        textures: HashMap<String, GlesTexture>,
        kind: Kind,
    ) -> Self {
        let _ = opaque_regions;
        Self {
            id: Id::new(),
            program,
            location: Point::default(),
            size,
            scale,
            alpha,
            additional_uniforms,
            textures,
            kind,
        }
    }

    pub fn empty(program: ProgramType, kind: Kind) -> Self {
        Self {
            id: Id::new(),
            program,
            location: Point::default(),
            size: Size::default(),
            scale: 1.,
            alpha: 1.,
            additional_uniforms: Rc::new([]),
            textures: HashMap::new(),
            kind,
        }
    }

    pub fn damage_all(&mut self) {}

    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        size: Size<f64, Logical>,
        opaque_regions: Option<Vec<Rectangle<f64, Logical>>>,
        scale: f32,
        alpha: f32,
        uniforms: Rc<[Uniform<'static>]>,
        textures: HashMap<String, GlesTexture>,
    ) {
        let _ = opaque_regions;
        self.size = size;
        self.scale = scale;
        self.alpha = alpha;
        self.additional_uniforms = uniforms;
        self.textures = textures;
    }

    pub fn with_location(mut self, location: Point<f64, Logical>) -> Self {
        self.location = location;
        self
    }

    pub fn with_alpha(mut self, alpha: f32) -> Self {
        self.alpha = alpha;
        self
    }

    pub fn id(&self) -> &Id {
        &self.id
    }

    /// Which of niri's shader programs the element uses.
    pub fn program(&self) -> ProgramType {
        self.program
    }

    /// Parameters the shader would receive.
    pub fn uniforms(&self) -> &[Uniform<'static>] {
        &self.additional_uniforms
    }

    pub fn alpha(&self) -> f32 {
        self.alpha
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }
}

impl Element for ShaderRenderElement {
    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        Rectangle::new(self.location, self.size).to_physical_precise_round(scale)
    }
}

impl<R> RenderElement<R> for ShaderRenderElement {}
