//! Mirrors `smithay::backend::renderer::element`.
//!
//! In Smithay a render element knows how to draw itself. Here an element only reports where it
//! would have been drawn.

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::utils::{Physical, Rectangle, Scale};

/// Identity of a render element across frames.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Id(usize);

impl Id {
    /// Creates an id distinct from every other id.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

/// What an element is for, as a hint to the display hardware.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Kind {
    Cursor,
    ScanoutCandidate,
    #[default]
    Unspecified,
}

/// Opaque record of which outputs showed an element. Nothing is displayed here, so it is empty.
#[derive(Debug, Clone, Default)]
pub struct RenderElementStates;

/// Something with a position on an output.
pub trait Element {
    /// Area the element covers, in physical pixels at the given scale.
    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical>;
}

impl<E: Element + ?Sized> Element for &E {
    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        (**self).geometry(scale)
    }
}

/// An [`Element`] that renderer `R` could draw. A marker here, since nothing draws.
pub trait RenderElement<R>: Element {}

impl<R, E: RenderElement<R> + ?Sized> RenderElement<R> for &E {}

/// Mirrors `smithay::backend::renderer::element::surface`.
pub mod surface {
    use std::convert::Infallible;
    use std::fmt;
    use std::marker::PhantomData;

    use super::{Element, RenderElement};
    use crate::utils::{Physical, Rectangle, Scale};

    /// Stands in for the element that draws a Wayland surface. It has no values: on macOS the
    /// window server draws window contents.
    pub struct WaylandSurfaceRenderElement<R> {
        never: Infallible,
        _renderer: PhantomData<R>,
    }

    impl<R> fmt::Debug for WaylandSurfaceRenderElement<R> {
        fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self.never {}
        }
    }

    impl<R> Element for WaylandSurfaceRenderElement<R> {
        fn geometry(&self, _scale: Scale<f64>) -> Rectangle<i32, Physical> {
            match self.never {}
        }
    }

    impl<R> RenderElement<R> for WaylandSurfaceRenderElement<R> {}
}

/// Mirrors `smithay::backend::renderer::element::utils`.
pub mod utils {
    use super::{Element, RenderElement};
    use crate::utils::{Physical, Point, Rectangle, Scale};

    /// How a [`RelocateRenderElement`] applies its location.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Relocate {
        /// Replace the element's location.
        Absolute,
        /// Offset the element's location.
        Relative,
    }

    /// An element moved to a different location.
    #[derive(Debug)]
    pub struct RelocateRenderElement<E> {
        element: E,
        relocate: Relocate,
        location: Point<i32, Physical>,
    }

    impl<E: Element> RelocateRenderElement<E> {
        /// Moves `element` to, or by, `location`.
        pub fn from_element(
            element: E,
            location: impl Into<Point<i32, Physical>>,
            relocate: Relocate,
        ) -> Self {
            Self {
                element,
                relocate,
                location: location.into(),
            }
        }
    }

    impl<E: Element> Element for RelocateRenderElement<E> {
        fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
            let mut geo = self.element.geometry(scale);
            match self.relocate {
                Relocate::Absolute => geo.loc = self.location,
                Relocate::Relative => geo.loc += self.location,
            }
            geo
        }
    }

    impl<R, E: RenderElement<R>> RenderElement<R> for RelocateRenderElement<E> {}

    /// An element scaled around an origin.
    #[derive(Debug)]
    pub struct RescaleRenderElement<E> {
        element: E,
        origin: Point<i32, Physical>,
        scale: Scale<f64>,
    }

    impl<E: Element> RescaleRenderElement<E> {
        /// Scales `element` by `scale` around `origin`.
        pub fn from_element(
            element: E,
            origin: Point<i32, Physical>,
            scale: impl Into<Scale<f64>>,
        ) -> Self {
            Self {
                element,
                origin,
                scale: scale.into(),
            }
        }
    }

    impl<E: Element> Element for RescaleRenderElement<E> {
        fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
            let mut geo = self.element.geometry(scale);
            geo.loc -= self.origin;
            geo = geo.to_f64().upscale(self.scale).to_i32_round();
            geo.loc += self.origin;
            geo
        }
    }

    impl<R, E: RenderElement<R>> RenderElement<R> for RescaleRenderElement<E> {}

    /// An element cut down to a rectangle.
    #[derive(Debug)]
    pub struct CropRenderElement<E> {
        element: E,
        crop_rect: Rectangle<i32, Physical>,
    }

    impl<E: Element> CropRenderElement<E> {
        /// Cuts `element` down to `crop_rect`. Returns `None` if nothing of it remains.
        pub fn from_element(
            element: E,
            scale: impl Into<Scale<f64>>,
            crop_rect: Rectangle<i32, Physical>,
        ) -> Option<Self> {
            let geo = element.geometry(scale.into());
            geo.intersection(crop_rect)?;
            Some(Self { element, crop_rect })
        }
    }

    impl<E: Element> Element for CropRenderElement<E> {
        fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
            let geo = self.element.geometry(scale);
            geo.intersection(self.crop_rect).unwrap_or_default()
        }
    }

    impl<R, E: RenderElement<R>> RenderElement<R> for CropRenderElement<E> {}
}
