//! niri's layout engine, for use outside a Wayland compositor.
//!
//! The module tree mirrors niri's `src/` so that imported files keep their `crate::` paths:
//!
//! - [`layout`], [`animation`], [`rubber_band`] and [`input::swipe_tracker`] are niri's code.
//! - [`utils`] and [`window`] are niri's code with the Wayland-specific parts removed.
//! - [`render_helpers`] is not niri's code. niri draws through OpenGL; macOS draws application
//!   windows itself. The module keeps the names and signatures the layout uses, with render
//!   elements reduced to plain data and GL objects replaced by types that have no values.
//!
//! Every change to an imported file is marked with an `ELARA:` comment. `UPSTREAM.md` at the
//! repository root lists the files and where they came from.

#[macro_use]
extern crate tracing;

pub mod animation;
pub mod input;
// The renderer and texture stand-ins have no values, so the compiler can tell that niri's drawing
// code never runs. It reports the code after each such point as unreachable and the values
// feeding it as unused. Those lints are silenced here, for the imported module only, instead of
// editing the drawing code out of niri's files.
#[allow(unreachable_code, unused_variables, unused_mut, unused_imports)]
pub mod layout;
pub mod render_helpers;
pub mod rubber_band;
pub mod utils;
pub mod window;
