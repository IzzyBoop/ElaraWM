//! Mirrors `smithay::backend::renderer::gles`.

use std::borrow::Cow;

use super::Texture;

/// Stands in for Smithay's OpenGL ES renderer.
///
/// This type has no values. A function that takes a `GlesRenderer` therefore cannot be called,
/// which is how niri's GL-only code paths are kept compiled but unreachable on macOS.
#[derive(Debug)]
pub enum GlesRenderer {}

/// Stands in for an OpenGL ES texture. Like [`GlesRenderer`], it has no values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlesTexture {}

impl Texture for GlesTexture {
    fn width(&self) -> u32 {
        match *self {}
    }

    fn height(&self) -> u32 {
        match *self {}
    }
}

/// Stands in for a compiled texture shader. It has no values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlesTexProgram {}

/// A named shader parameter.
#[derive(Debug, Clone, PartialEq)]
pub struct Uniform<'a> {
    /// Name of the parameter in the shader source.
    pub name: Cow<'a, str>,
    /// Value to set it to.
    pub value: UniformValue,
}

impl<'a> Uniform<'a> {
    /// Creates a named shader parameter.
    pub fn new(name: impl Into<Cow<'a, str>>, value: impl Into<UniformValue>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }
}

/// Value of a [`Uniform`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UniformValue {
    Float(f32),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Vec4([f32; 4]),
    Mat3([f32; 9]),
}

impl From<f32> for UniformValue {
    fn from(value: f32) -> Self {
        Self::Float(value)
    }
}

impl From<[f32; 2]> for UniformValue {
    fn from(value: [f32; 2]) -> Self {
        Self::Vec2(value)
    }
}

impl From<(f32, f32)> for UniformValue {
    fn from(value: (f32, f32)) -> Self {
        Self::Vec2([value.0, value.1])
    }
}

impl From<[f32; 3]> for UniformValue {
    fn from(value: [f32; 3]) -> Self {
        Self::Vec3(value)
    }
}

impl From<[f32; 4]> for UniformValue {
    fn from(value: [f32; 4]) -> Self {
        Self::Vec4(value)
    }
}

impl From<[f32; 9]> for UniformValue {
    fn from(value: [f32; 9]) -> Self {
        Self::Mat3(value)
    }
}
