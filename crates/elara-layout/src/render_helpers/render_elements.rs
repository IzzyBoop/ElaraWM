// ELARA: stand-in for niri's `niri_render_elements!`. See `render_helpers/mod.rs`.
//
// niri's macro declares an enum over several render element types and forwards Smithay's
// drawing traits to the variants. This one takes the same input and declares the same enum and
// `From` impls, forwarding the reduced `Element` trait of the compat crate.

#[macro_export]
macro_rules! niri_render_elements {
    ($name:ident<R> => { $($variant:ident = $type:ty),+ $(,)? }) => {
        $crate::niri_render_elements!(@impl $name () ($name<R>) => { $($variant = $type),+ });

        $(impl<R: $crate::render_helpers::renderer::NiriRenderer> From<$type> for $name<R> {
            fn from(x: $type) -> Self {
                Self::$variant(x)
            }
        })+
    };

    ($name:ident => { $($variant:ident = $type:ty),+ $(,)? }) => {
        $crate::niri_render_elements!(@impl $name ($name) () => { $($variant = $type),+ });

        $(impl From<$type> for $name {
            fn from(x: $type) -> Self {
                Self::$variant(x)
            }
        })+
    };

    (@impl $name:ident ($($name_no_R:ident)?) ($($name_R:ident<$R:ident>)?) => { $($variant:ident = $type:ty),+ }) => {
        #[allow(clippy::large_enum_variant)]
        #[derive(Debug)]
        pub enum $name$(<$R: $crate::render_helpers::renderer::NiriRenderer>)? {
            $($variant($type)),+
        }

        impl$(<$R: $crate::render_helpers::renderer::NiriRenderer>)? smithay::backend::renderer::element::Element for $name$(<$R>)? {
            fn geometry(&self, scale: smithay::utils::Scale<f64>) -> smithay::utils::Rectangle<i32, smithay::utils::Physical> {
                match self {
                    $($name::$variant(elem) => smithay::backend::renderer::element::Element::geometry(elem, scale)),+
                }
            }
        }

        impl$(<$R: $crate::render_helpers::renderer::NiriRenderer>)? smithay::backend::renderer::element::RenderElement<smithay::backend::renderer::gles::GlesRenderer> for $name$(<$R>)? {}
    };
}
