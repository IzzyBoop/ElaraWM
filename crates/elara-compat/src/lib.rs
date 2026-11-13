//! Stand-ins for the parts of [Smithay] that niri's layout and config code depend on.
//!
//! niri is a Wayland compositor built on Smithay. ElaraWM reuses niri's code on macOS, where
//! Smithay itself cannot be built. This crate mirrors Smithay's module paths for the handful of
//! pure-data types niri needs. The library target is named `smithay`, so imported niri files keep
//! their `use smithay::...` lines.
//!
//! Files copied from Smithay are byte-identical to upstream unless marked with `// ELARA:`. They
//! are listed in `UPSTREAM.md` at the repository root.
//!
//! [Smithay]: https://github.com/Smithay/smithay

pub mod backend;
pub mod desktop;
pub mod input;
// ELARA: copied Smithay files are excluded from rustfmt and clippy so they stay verbatim.
#[rustfmt::skip]
#[allow(clippy::all)]
pub mod output;
pub mod reexports;
pub mod utils;
pub mod wayland;
