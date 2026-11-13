//! ElaraWM: niri's scrollable-tiling layout, applied to macOS windows.
//!
//! niri is a Wayland compositor: it owns the windows, decides where they go and draws them.
//! ElaraWM keeps the deciding and delegates the rest. [`State`] holds niri's
//! [`Layout`](elara_layout::layout::Layout), feeds it what the system reports about real windows,
//! and asks the system to put each window where the layout says.

#[macro_use]
extern crate tracing;

mod actions;
pub mod binds;
mod state;
mod window;

pub use state::State;
pub use window::Window;
