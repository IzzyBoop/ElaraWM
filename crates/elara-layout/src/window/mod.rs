use std::cmp::{max, min};

use niri_config::utils::MergeWith as _;
use niri_config::window_rule::{Match, WindowRule};
use niri_config::{
    BackgroundEffect, BlockOutFrom, BorderRule, CornerRadius, FloatingPosition, PresetSize,
    ResolvedPopupsRules, ShadowRule, TabIndicatorRule,
};
use niri_ipc::ColumnDisplay;
use smithay::utils::{Logical, Size};

// ELARA: niri's `mapped` and `unmapped` submodules wrap Wayland toplevel surfaces and are not
// imported. Upstream, `WindowRef` is an enum over those two types, and rule matching reads the
// title, app id and activation state from the surface's xdg-toplevel role. Here `WindowRef`
// carries those facts directly, filled in by whoever owns the window. The accessor methods keep
// their names so the matching code below is unchanged apart from where the values come from.

/// The facts about a window that window rules can match on.
#[derive(Debug, Clone, Copy, Default)]
pub struct WindowRef<'a> {
    /// Application identifier. On macOS this is the bundle identifier.
    pub app_id: Option<&'a str>,
    /// Window title.
    pub title: Option<&'a str>,
    /// Whether the window is the active one on its workspace.
    pub is_active: bool,
    /// Whether the window has keyboard focus.
    pub is_focused: bool,
    /// Whether the window asks for attention.
    pub is_urgent: bool,
    /// Whether the window is the active one in its column.
    pub is_active_in_column: bool,
    /// Whether the window is floating.
    pub is_floating: bool,
    /// Whether the window is being captured.
    pub is_window_cast_target: bool,
}

/// Rules fully resolved for a window.
#[derive(Debug, Default, PartialEq, Clone)]
pub struct ResolvedWindowRules {
    /// Default width for this window.
    ///
    /// - `None`: unset (global default should be used).
    /// - `Some(None)`: set to empty (window picks its own width).
    /// - `Some(Some(width))`: set to a particular width.
    pub default_width: Option<Option<PresetSize>>,

    /// Default height for this window.
    ///
    /// - `None`: unset (global default should be used).
    /// - `Some(None)`: set to empty (window picks its own height).
    /// - `Some(Some(height))`: set to a particular height.
    pub default_height: Option<Option<PresetSize>>,

    /// Default column display for this window.
    pub default_column_display: Option<ColumnDisplay>,

    /// Default floating position for this window.
    pub default_floating_position: Option<FloatingPosition>,

    /// Output to open this window on.
    pub open_on_output: Option<String>,

    /// Workspace to open this window on.
    pub open_on_workspace: Option<String>,

    /// Whether the window should open full-width.
    pub open_maximized: Option<bool>,

    /// Whether the window should open maximized to edges (true maximized).
    pub open_maximized_to_edges: Option<bool>,

    /// Whether the window should open fullscreen.
    pub open_fullscreen: Option<bool>,

    /// Whether the window should open floating.
    pub open_floating: Option<bool>,

    /// Whether the window should open focused.
    pub open_focused: Option<bool>,

    /// Extra bound on the minimum window width.
    pub min_width: Option<u16>,
    /// Extra bound on the minimum window height.
    pub min_height: Option<u16>,
    /// Extra bound on the maximum window width.
    pub max_width: Option<u16>,
    /// Extra bound on the maximum window height.
    pub max_height: Option<u16>,

    /// Focus ring overrides.
    pub focus_ring: BorderRule,
    /// Window border overrides.
    pub border: BorderRule,
    /// Shadow overrides.
    pub shadow: ShadowRule,
    /// Tab indicator overrides.
    pub tab_indicator: TabIndicatorRule,

    /// Whether or not to draw the border with a solid background.
    ///
    /// `None` means using the SSD heuristic.
    pub draw_border_with_background: Option<bool>,

    /// Extra opacity to draw this window with.
    pub opacity: Option<f32>,

    /// Corner radius to assume this window has.
    pub geometry_corner_radius: Option<CornerRadius>,

    /// Whether to clip this window to its geometry, including the corner radius.
    pub clip_to_geometry: Option<bool>,

    /// Whether to bob this window up and down.
    pub baba_is_float: Option<bool>,

    /// Whether to block out this window from certain render targets.
    pub block_out_from: Option<BlockOutFrom>,

    /// Whether to enable VRR on this window's primary output if it is on-demand.
    pub variable_refresh_rate: Option<bool>,

    /// Multiplier for all scroll events sent to this window.
    pub scroll_factor: Option<f64>,

    /// Override whether to set the Tiled xdg-toplevel state on the window.
    pub tiled_state: Option<bool>,

    /// Background effect configuration.
    pub background_effect: BackgroundEffect,

    /// Rules for this window's popups.
    pub popups: ResolvedPopupsRules,
}

impl WindowRef<'_> {
    pub fn is_focused(self) -> bool {
        self.is_focused
    }

    pub fn is_urgent(self) -> bool {
        self.is_urgent
    }

    pub fn is_active_in_column(self) -> bool {
        self.is_active_in_column
    }

    pub fn is_floating(self) -> bool {
        self.is_floating
    }

    pub fn is_window_cast_target(self) -> bool {
        self.is_window_cast_target
    }
}

impl ResolvedWindowRules {
    pub fn compute(rules: &[WindowRule], window: WindowRef, is_at_startup: bool) -> Self {
        let _span = tracy_client::span!("ResolvedWindowRules::compute");

        let mut resolved = ResolvedWindowRules::default();

        // ELARA: upstream runs this block inside `with_toplevel_role()` to reach the surface's
        // role data. The block is kept so the indentation of its contents matches upstream.
        {
            let mut open_on_output = None;
            let mut open_on_workspace = None;

            for rule in rules {
                let matches = |m: &Match| {
                    if let Some(at_startup) = m.at_startup {
                        if at_startup != is_at_startup {
                            return false;
                        }
                    }

                    window_matches(window, m)
                };

                if !(rule.matches.is_empty() || rule.matches.iter().any(matches)) {
                    continue;
                }

                if rule.excludes.iter().any(matches) {
                    continue;
                }

                if let Some(x) = rule.default_column_width {
                    resolved.default_width = Some(x.0);
                }

                if let Some(x) = rule.default_window_height {
                    resolved.default_height = Some(x.0);
                }

                if let Some(x) = rule.default_column_display {
                    resolved.default_column_display = Some(x);
                }

                if let Some(x) = rule.default_floating_position {
                    resolved.default_floating_position = Some(x);
                }

                if let Some(x) = rule.open_on_output.as_deref() {
                    open_on_output = Some(x);
                }

                if let Some(x) = rule.open_on_workspace.as_deref() {
                    open_on_workspace = Some(x);
                }

                if let Some(x) = rule.open_maximized {
                    resolved.open_maximized = Some(x);
                }

                if let Some(x) = rule.open_maximized_to_edges {
                    resolved.open_maximized_to_edges = Some(x);
                }

                if let Some(x) = rule.open_fullscreen {
                    resolved.open_fullscreen = Some(x);
                }

                if let Some(x) = rule.open_floating {
                    resolved.open_floating = Some(x);
                }

                if let Some(x) = rule.open_focused {
                    resolved.open_focused = Some(x);
                }

                if let Some(x) = rule.min_width {
                    resolved.min_width = Some(x);
                }
                if let Some(x) = rule.min_height {
                    resolved.min_height = Some(x);
                }
                if let Some(x) = rule.max_width {
                    resolved.max_width = Some(x);
                }
                if let Some(x) = rule.max_height {
                    resolved.max_height = Some(x);
                }

                resolved.focus_ring.merge_with(&rule.focus_ring);
                resolved.border.merge_with(&rule.border);
                resolved.shadow.merge_with(&rule.shadow);
                resolved.tab_indicator.merge_with(&rule.tab_indicator);

                if let Some(x) = rule.draw_border_with_background {
                    resolved.draw_border_with_background = Some(x);
                }
                if let Some(x) = rule.opacity {
                    resolved.opacity = Some(x);
                }
                if let Some(x) = rule.geometry_corner_radius {
                    resolved.geometry_corner_radius = Some(x);
                }
                if let Some(x) = rule.clip_to_geometry {
                    resolved.clip_to_geometry = Some(x);
                }
                if let Some(x) = rule.baba_is_float {
                    resolved.baba_is_float = Some(x);
                }
                if let Some(x) = rule.block_out_from {
                    resolved.block_out_from = Some(x);
                }
                if let Some(x) = rule.variable_refresh_rate {
                    resolved.variable_refresh_rate = Some(x);
                }
                if let Some(x) = rule.scroll_factor {
                    resolved.scroll_factor = Some(x.0);
                }
                if let Some(x) = rule.tiled_state {
                    resolved.tiled_state = Some(x);
                }

                resolved
                    .background_effect
                    .merge_with(&rule.background_effect);

                resolved.popups.merge_with(&rule.popups);
            }

            resolved.open_on_output = open_on_output.map(|x| x.to_owned());
            resolved.open_on_workspace = open_on_workspace.map(|x| x.to_owned());
        }

        resolved
    }

    pub fn apply_min_size(&self, min_size: Size<i32, Logical>) -> Size<i32, Logical> {
        let mut size = min_size;

        if let Some(x) = self.min_width {
            size.w = max(size.w, i32::from(x));
        }
        if let Some(x) = self.min_height {
            size.h = max(size.h, i32::from(x));
        }

        size
    }

    pub fn apply_max_size(&self, max_size: Size<i32, Logical>) -> Size<i32, Logical> {
        let mut size = max_size;

        if let Some(x) = self.max_width {
            if size.w == 0 {
                size.w = i32::from(x);
            } else if x > 0 {
                size.w = min(size.w, i32::from(x));
            }
        }
        if let Some(x) = self.max_height {
            if size.h == 0 {
                size.h = i32::from(x);
            } else if x > 0 {
                size.h = min(size.h, i32::from(x));
            }
        }

        size
    }

    pub fn apply_min_max_size(
        &self,
        min_size: Size<i32, Logical>,
        max_size: Size<i32, Logical>,
    ) -> (Size<i32, Logical>, Size<i32, Logical>) {
        let min_size = self.apply_min_size(min_size);
        let max_size = self.apply_max_size(max_size);
        (min_size, max_size)
    }

    // ELARA: upstream takes the toplevel surface and reads the parent and the size limits from
    // it. Here the caller passes them in.
    pub fn compute_open_floating(
        &self,
        has_parent: bool,
        min_size: Size<i32, Logical>,
        max_size: Size<i32, Logical>,
    ) -> bool {
        if let Some(res) = self.open_floating {
            return res;
        }

        // Windows with a parent (usually dialogs) open as floating by default.
        if has_parent {
            return true;
        }

        let (min_size, max_size) = self.apply_min_max_size(min_size, max_size);

        // We open fixed-height windows as floating.
        min_size.h > 0 && min_size.h == max_size.h
    }
}

// ELARA: upstream also takes the xdg-toplevel role and reads `is_active`, the app id and the
// title from it.
fn window_matches(window: WindowRef, m: &Match) -> bool {
    if let Some(is_focused) = m.is_focused {
        if window.is_focused() != is_focused {
            return false;
        }
    }

    if let Some(is_urgent) = m.is_urgent {
        if window.is_urgent() != is_urgent {
            return false;
        }
    }

    if let Some(is_active) = m.is_active {
        // Our "is-active" definition corresponds to the window having a pending Activated state.
        let pending_activated = window.is_active;
        if is_active != pending_activated {
            return false;
        }
    }

    if let Some(app_id_re) = &m.app_id {
        let Some(app_id) = window.app_id else {
            return false;
        };
        if !app_id_re.0.is_match(app_id) {
            return false;
        }
    }

    if let Some(title_re) = &m.title {
        let Some(title) = window.title else {
            return false;
        };
        if !title_re.0.is_match(title) {
            return false;
        }
    }

    if let Some(is_active_in_column) = m.is_active_in_column {
        if window.is_active_in_column() != is_active_in_column {
            return false;
        }
    }

    if let Some(is_floating) = m.is_floating {
        if window.is_floating() != is_floating {
            return false;
        }
    }

    if let Some(is_window_cast_target) = m.is_window_cast_target {
        if window.is_window_cast_target() != is_window_cast_target {
            return false;
        }
    }

    true
}
