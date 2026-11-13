// ELARA: stand-in for niri's shader registry. See `render_helpers/mod.rs`. Shaders are compiled
// by, and stored on, a GL renderer. There is none, so the registry has no values.

use glam::Mat3;
use smithay::backend::renderer::gles::Uniform;

use super::renderer::NiriRenderer;
use super::shader_element::ShaderProgram;

/// The shader programs niri compiles at startup. It has no values.
#[derive(Debug)]
pub enum Shaders {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramType {
    Border,
    Shadow,
    Resize,
    Close,
    Open,
}

impl Shaders {
    pub fn get(renderer: &mut impl NiriRenderer) -> &Self {
        match *renderer.as_gles_renderer() {}
    }

    pub fn program(&self, program: ProgramType) -> Option<ShaderProgram> {
        let _ = program;
        match *self {}
    }
}

pub fn mat3_uniform(name: &str, mat: Mat3) -> Uniform<'_> {
    Uniform::new(name, mat.to_cols_array())
}
