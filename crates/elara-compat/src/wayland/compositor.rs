//! Mirrors `smithay::wayland::compositor`.

/// State of a [`Blocker`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockerState {
    /// The thing being waited on has not happened yet.
    Pending,
    /// The blocker no longer holds anything back.
    Released,
    /// The blocker was cancelled.
    Cancelled,
}

/// Something that can hold back applying new window state until it is released.
///
/// In Smithay this delays a Wayland surface commit. niri's transactions implement it so that
/// several windows change size in the same frame.
pub trait Blocker {
    /// Returns the current state of the blocker.
    fn state(&self) -> BlockerState;
}
