//! Mirrors `smithay::backend::renderer::utils`.

/// Counts changes to a render element, for damage tracking.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct CommitCounter(usize);

impl CommitCounter {
    /// Records that the element changed.
    pub fn increment(&mut self) {
        self.0 = self.0.wrapping_add(1);
    }
}
