use std::cmp::{max, min};
use std::fmt::Display;
use std::time::Duration;
use std::{f64, fmt};

use bitflags::bitflags;
use niri_config::OutputName;
use smithay::output::Output;
use smithay::reexports::rustix::time::{clock_gettime, ClockId};
use smithay::utils::{Coordinate, Logical, Point, Rectangle, Size, Transform};

// ELARA: this file keeps only the helpers that the layout, animation and window-rule code use.
// Removed, along with their imports: everything that touches Wayland surfaces, clients or
// toplevel roles, the version string, screenshot paths and notifications, and the `region`,
// `scale`, `signals`, `spawning`, `vblank_throttle`, `watcher` and `xwayland` submodules.

pub mod id;
pub mod transaction;

use id::IdCounter;

/// Unique ID for a screencast session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CastSessionId(u64);

impl CastSessionId {
    pub fn next() -> Self {
        static COUNTER: IdCounter = IdCounter::new();
        Self(COUNTER.next())
    }

    pub fn get(self) -> u64 {
        self.0
    }
}

impl Display for CastSessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u64> for CastSessionId {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

/// Unique ID for a screencast stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CastStreamId(u64);

impl CastStreamId {
    pub fn next() -> Self {
        static COUNTER: IdCounter = IdCounter::new();
        Self(COUNTER.next())
    }

    pub fn get(self) -> u64 {
        self.0
    }
}

impl Display for CastStreamId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct ResizeEdge: u32 {
        const TOP          = 0b0001;
        const BOTTOM       = 0b0010;
        const LEFT         = 0b0100;
        const RIGHT        = 0b1000;

        const TOP_LEFT     = Self::TOP.bits() | Self::LEFT.bits();
        const BOTTOM_LEFT  = Self::BOTTOM.bits() | Self::LEFT.bits();

        const TOP_RIGHT    = Self::TOP.bits() | Self::RIGHT.bits();
        const BOTTOM_RIGHT = Self::BOTTOM.bits() | Self::RIGHT.bits();

        const LEFT_RIGHT   = Self::LEFT.bits() | Self::RIGHT.bits();
        const TOP_BOTTOM   = Self::TOP.bits() | Self::BOTTOM.bits();
    }
}

pub fn get_monotonic_time() -> Duration {
    let ts = clock_gettime(ClockId::Monotonic);
    Duration::new(ts.tv_sec as u64, ts.tv_nsec as u32)
}

pub fn center(rect: Rectangle<i32, Logical>) -> Point<i32, Logical> {
    rect.loc + rect.size.downscale(2).to_point()
}

pub fn center_f64(rect: Rectangle<f64, Logical>) -> Point<f64, Logical> {
    rect.loc + rect.size.downscale(2.0).to_point()
}

/// Convert logical pixels to physical, rounding to physical pixels.
pub fn to_physical_precise_round<N: Coordinate>(scale: f64, logical: impl Coordinate) -> N {
    N::from_f64((logical.to_f64() * scale).round())
}

pub fn round_logical_in_physical(scale: f64, logical: f64) -> f64 {
    (logical * scale).round() / scale
}

pub fn round_logical_in_physical_max1(scale: f64, logical: f64) -> f64 {
    if logical == 0. {
        return 0.;
    }

    (logical * scale).max(1.).round() / scale
}

pub fn floor_logical_in_physical_max1(scale: f64, logical: f64) -> f64 {
    if logical == 0. {
        return 0.;
    }

    (logical * scale).max(1.).floor() / scale
}

pub fn output_size(output: &Output) -> Size<f64, Logical> {
    let output_scale = output.current_scale().fractional_scale();
    let output_transform = output.current_transform();
    let output_mode = output.current_mode().unwrap();
    let logical_size = output_mode.size.to_f64().to_logical(output_scale);
    output_transform.transform_size(logical_size)
}

pub fn logical_output(output: &Output) -> niri_ipc::LogicalOutput {
    let loc = output.current_location();
    let size = output_size(output);
    let transform = match output.current_transform() {
        Transform::Normal => niri_ipc::Transform::Normal,
        Transform::_90 => niri_ipc::Transform::_90,
        Transform::_180 => niri_ipc::Transform::_180,
        Transform::_270 => niri_ipc::Transform::_270,
        Transform::Flipped => niri_ipc::Transform::Flipped,
        Transform::Flipped90 => niri_ipc::Transform::Flipped90,
        Transform::Flipped180 => niri_ipc::Transform::Flipped180,
        Transform::Flipped270 => niri_ipc::Transform::Flipped270,
    };
    niri_ipc::LogicalOutput {
        x: loc.x,
        y: loc.y,
        width: size.w as u32,
        height: size.h as u32,
        scale: output.current_scale().fractional_scale(),
        transform,
    }
}

pub struct PanelOrientation(pub Transform);
pub fn panel_orientation(output: &Output) -> Transform {
    output
        .user_data()
        .get::<PanelOrientation>()
        .map(|x| x.0)
        .unwrap_or(Transform::Normal)
}

pub fn ipc_transform_to_smithay(transform: niri_ipc::Transform) -> Transform {
    match transform {
        niri_ipc::Transform::Normal => Transform::Normal,
        niri_ipc::Transform::_90 => Transform::_90,
        niri_ipc::Transform::_180 => Transform::_180,
        niri_ipc::Transform::_270 => Transform::_270,
        niri_ipc::Transform::Flipped => Transform::Flipped,
        niri_ipc::Transform::Flipped90 => Transform::Flipped90,
        niri_ipc::Transform::Flipped180 => Transform::Flipped180,
        niri_ipc::Transform::Flipped270 => Transform::Flipped270,
    }
}

pub fn output_matches_name(output: &Output, target: &str) -> bool {
    let name = output.user_data().get::<OutputName>().unwrap();
    name.matches(target)
}

pub fn is_laptop_panel(connector: &str) -> bool {
    matches!(connector.get(..4), Some("eDP-" | "LVDS" | "DSI-"))
}

pub fn ensure_min_max_size(mut x: i32, min_size: i32, max_size: i32) -> i32 {
    if max_size > 0 {
        x = min(x, max_size);
    }
    if min_size > 0 {
        x = max(x, min_size);
    }
    x
}

pub fn ensure_min_max_size_maybe_zero(x: i32, min_size: i32, max_size: i32) -> i32 {
    if x != 0 {
        ensure_min_max_size(x, min_size, max_size)
    } else if min_size > 0 && min_size == max_size {
        min_size
    } else {
        0
    }
}

pub fn clamp_preferring_top_left_in_area(
    area: Rectangle<f64, Logical>,
    rect: &mut Rectangle<f64, Logical>,
) {
    rect.loc.x = f64::min(rect.loc.x, area.loc.x + area.size.w - rect.size.w);
    rect.loc.y = f64::min(rect.loc.y, area.loc.y + area.size.h - rect.size.h);

    // Clamp by top and left last so it takes precedence.
    rect.loc.x = f64::max(rect.loc.x, area.loc.x);
    rect.loc.y = f64::max(rect.loc.y, area.loc.y);
}

pub fn center_preferring_top_left_in_area(
    area: Rectangle<f64, Logical>,
    size: Size<f64, Logical>,
) -> Point<f64, Logical> {
    let area_size = area.size.to_point();
    let size = size.to_point();
    let mut offset = (area_size - size).downscale(2.);
    offset.x = f64::max(offset.x, 0.);
    offset.y = f64::max(offset.y, 0.);
    area.loc + offset
}

pub fn baba_is_float_offset(now: Duration, view_height: f64) -> f64 {
    let now = now.as_secs_f64();
    let amplitude = view_height / 96.;
    amplitude * ((f64::consts::TAU * now / 3.6).sin() - 1.)
}

#[inline(never)]
pub fn cause_panic() {
    let a = Duration::from_secs(1);
    let b = Duration::from_secs(2);
    let _ = a - b;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clamp_preferring_top_left() {
        fn check(
            (ax, ay, aw, ah): (i32, i32, i32, i32),
            (rx, ry, rw, rh): (i32, i32, i32, i32),
            (ex, ey): (i32, i32),
        ) {
            let area = Rectangle::new(Point::from((ax, ay)), Size::from((aw, ah))).to_f64();
            let mut rect = Rectangle::new(Point::from((rx, ry)), Size::from((rw, rh))).to_f64();
            clamp_preferring_top_left_in_area(area, &mut rect);
            assert_eq!(rect.loc, Point::from((ex, ey)).to_f64());
        }

        check((0, 0, 10, 20), (2, 3, 4, 5), (2, 3));
        check((0, 0, 10, 20), (-2, 3, 4, 5), (0, 3));
        check((0, 0, 10, 20), (2, -3, 4, 5), (2, 0));
        check((0, 0, 10, 20), (-2, -3, 4, 5), (0, 0));

        check((1, 1, 10, 20), (2, 3, 4, 5), (2, 3));
        check((1, 1, 10, 20), (-2, 3, 4, 5), (1, 3));
        check((1, 1, 10, 20), (2, -3, 4, 5), (2, 1));
        check((1, 1, 10, 20), (-2, -3, 4, 5), (1, 1));

        check((0, 0, 10, 20), (20, 3, 4, 5), (6, 3));
        check((0, 0, 10, 20), (2, 30, 4, 5), (2, 15));
        check((0, 0, 10, 20), (20, 30, 4, 5), (6, 15));

        check((0, 0, 10, 20), (20, 30, 40, 5), (0, 15));
        check((0, 0, 10, 20), (20, 30, 4, 50), (6, 0));
        check((0, 0, 10, 20), (20, 30, 40, 50), (0, 0));
    }
}
