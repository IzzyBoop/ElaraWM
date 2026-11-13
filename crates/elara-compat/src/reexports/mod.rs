//! Mirrors `smithay::reexports`.

pub mod input;

/// Smithay re-exports `rustix`; niri reads the monotonic clock through this path.
pub use rustix;

/// Mirrors the few items of `wayland_server` that niri's layout code names.
pub mod wayland_server {
    /// Stands in for a Wayland client connection.
    ///
    /// On macOS the closest equivalent is the process that owns a window, so this wraps a
    /// process id.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Client(pub i32);

    pub mod protocol {
        pub mod wl_surface {
            /// Stands in for a Wayland surface. It has no values: macOS windows are not
            /// surfaces, and lookups by surface never match.
            #[derive(Debug, Clone, PartialEq, Eq, Hash)]
            pub enum WlSurface {}
        }
    }
}
