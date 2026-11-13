//! Mirrors the libinput enums that `niri-config` converts its settings into.
//!
//! macOS has no libinput. These exist so the config code compiles unchanged; nothing consumes
//! them.

/// Mirrors `input::ClickMethod`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickMethod {
    ButtonAreas,
    Clickfinger,
}

/// Mirrors `input::AccelProfile`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccelProfile {
    Flat,
    Adaptive,
}

/// Mirrors `input::ScrollMethod`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollMethod {
    NoScroll,
    TwoFinger,
    Edge,
    OnButtonDown,
}

/// Mirrors `input::TapButtonMap`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TapButtonMap {
    LeftRightMiddle,
    LeftMiddleRight,
}
