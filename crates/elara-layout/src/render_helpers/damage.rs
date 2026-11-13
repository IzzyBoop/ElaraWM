// ELARA: stand-in for niri's extra damage element. See `render_helpers/mod.rs`. The type and
// its methods are niri's; only the drawing traits are reduced.

use smithay::backend::renderer::element::{Element, Id, RenderElement};
use smithay::backend::renderer::utils::CommitCounter;
use smithay::utils::{Logical, Physical, Rectangle, Scale};

#[derive(Debug, Clone)]
pub struct ExtraDamage {
    #[allow(dead_code)]
    id: Id,
    commit: CommitCounter,
    geometry: Rectangle<f64, Logical>,
}

impl ExtraDamage {
    pub fn new() -> Self {
        Self {
            id: Id::new(),
            commit: Default::default(),
            geometry: Default::default(),
        }
    }

    pub fn damage_all(&mut self) {
        self.commit.increment();
    }

    pub fn render(&self, geometry: Rectangle<f64, Logical>) -> Self {
        let mut this = self.clone();
        this.geometry = geometry;
        this
    }
}

impl Default for ExtraDamage {
    fn default() -> Self {
        Self::new()
    }
}

impl Element for ExtraDamage {
    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        self.geometry.to_physical_precise_round(scale)
    }
}

impl<R> RenderElement<R> for ExtraDamage {}
