//! Mirrors the part of `smithay::desktop` that niri's layout code uses.
//!
//! niri asks each output's layer map for the area left over once panels and docks have reserved
//! their edges, and tiles windows inside it. On macOS that is the display's visible frame: the
//! display minus the menu bar and the Dock. The macOS backend reports it with
//! [`LayerMap::set_non_exclusive_zone`], which Smithay does not have.

use std::sync::{Mutex, MutexGuard};

use crate::output::{Output, WeakOutput};
use crate::utils::{Logical, Point, Rectangle};

/// Per-output record of the area available to windows.
#[derive(Debug)]
pub struct LayerMap {
    output: WeakOutput,
    zone: Option<Rectangle<i32, Logical>>,
}

/// Returns the layer map of `output`, creating it on first use.
pub fn layer_map_for_output(output: &Output) -> MutexGuard<'_, LayerMap> {
    let user_data = output.user_data();
    user_data.insert_if_missing_threadsafe(|| {
        Mutex::new(LayerMap {
            output: output.downgrade(),
            zone: None,
        })
    });
    user_data.get::<Mutex<LayerMap>>().unwrap().lock().unwrap()
}

impl LayerMap {
    /// Returns the area of the output that windows may occupy, relative to the output's
    /// top-left corner.
    ///
    /// This is the whole output unless a smaller area was set.
    pub fn non_exclusive_zone(&self) -> Rectangle<i32, Logical> {
        self.zone.unwrap_or_else(|| self.output_geometry())
    }

    /// Sets the area of the output that windows may occupy, relative to the output's top-left
    /// corner. `None` restores the whole output.
    pub fn set_non_exclusive_zone(&mut self, zone: Option<Rectangle<i32, Logical>>) {
        self.zone = zone;
    }

    fn output_geometry(&self) -> Rectangle<i32, Logical> {
        let Some(output) = self.output.upgrade() else {
            return Rectangle::default();
        };
        let Some(mode) = output.current_mode() else {
            return Rectangle::default();
        };
        let size = output
            .current_transform()
            .transform_size(mode.size)
            .to_f64()
            .to_logical(output.current_scale().fractional_scale())
            .to_i32_round();
        Rectangle::new(Point::from((0, 0)), size)
    }
}
