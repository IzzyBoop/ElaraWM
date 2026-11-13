//! Mirrors `smithay::backend::renderer::sync`.

/// Marks the point at which queued rendering has finished. Nothing renders here, so it carries
/// no state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncPoint;
