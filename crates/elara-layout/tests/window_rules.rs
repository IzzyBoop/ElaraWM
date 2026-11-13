//! Window rules resolved against plain window facts instead of a Wayland surface.

use elara_layout::window::{ResolvedWindowRules, WindowRef};
use niri_config::Config;
use smithay::utils::Size;

fn rules(kdl: &str) -> Config {
    Config::parse_mem(kdl).expect("config must parse")
}

fn window<'a>(app_id: &'a str, title: &'a str) -> WindowRef<'a> {
    WindowRef {
        app_id: Some(app_id),
        title: Some(title),
        ..Default::default()
    }
}

#[test]
fn matches_on_app_id_and_title() {
    let config = rules(
        r#"
        window-rule {
            match app-id="^com\\.apple\\.Terminal$"
            open-floating true
        }
        window-rule {
            match app-id="Safari" title="Downloads"
            open-maximized true
        }
        "#,
    );
    let rules = &config.window_rules;

    let terminal = window("com.apple.Terminal", "zsh");
    let resolved = ResolvedWindowRules::compute(rules, terminal, false);
    assert_eq!(resolved.open_floating, Some(true));
    assert_eq!(resolved.open_maximized, None);

    let downloads = window("com.apple.Safari", "Downloads");
    let resolved = ResolvedWindowRules::compute(rules, downloads, false);
    assert_eq!(resolved.open_floating, None);
    assert_eq!(resolved.open_maximized, Some(true));

    // Both matchers of one `match` must hold.
    let page = window("com.apple.Safari", "Start Page");
    let resolved = ResolvedWindowRules::compute(rules, page, false);
    assert_eq!(resolved, ResolvedWindowRules::default());
}

#[test]
fn window_without_app_id_or_title_does_not_match_them() {
    let config = rules(
        r#"
        window-rule {
            match app-id="."
            open-floating true
        }
        window-rule {
            match title="."
            open-maximized true
        }
        "#,
    );

    let resolved = ResolvedWindowRules::compute(&config.window_rules, WindowRef::default(), false);
    assert_eq!(resolved, ResolvedWindowRules::default());
}

#[test]
fn excludes_and_later_rules_override() {
    let config = rules(
        r#"
        window-rule {
            match app-id="Code"
            exclude title="Settings"
            opacity 0.5
        }
        window-rule {
            opacity 0.9
        }
        window-rule {
            match is-focused=true
            opacity 1.0
        }
        "#,
    );
    let rules = &config.window_rules;

    // A rule without matchers applies to every window and overrides the earlier one.
    let editor = window("com.microsoft.VSCode", "main.rs");
    assert_eq!(
        ResolvedWindowRules::compute(rules, editor, false).opacity,
        Some(0.9)
    );

    let focused = WindowRef {
        is_focused: true,
        ..editor
    };
    assert_eq!(
        ResolvedWindowRules::compute(rules, focused, false).opacity,
        Some(1.0)
    );
}

#[test]
fn state_matchers_read_the_window_facts() {
    let config = rules(
        r#"
        window-rule {
            match is-active=true is-floating=false
            draw-border-with-background false
        }
        window-rule {
            match at-startup=true
            open-on-workspace "chat"
        }
        "#,
    );
    let rules = &config.window_rules;

    let inactive = window("a", "b");
    let active = WindowRef {
        is_active: true,
        ..inactive
    };
    let active_floating = WindowRef {
        is_floating: true,
        ..active
    };

    let resolved = ResolvedWindowRules::compute(rules, inactive, false);
    assert_eq!(resolved.draw_border_with_background, None);
    let resolved = ResolvedWindowRules::compute(rules, active, false);
    assert_eq!(resolved.draw_border_with_background, Some(false));
    let resolved = ResolvedWindowRules::compute(rules, active_floating, false);
    assert_eq!(resolved.draw_border_with_background, None);

    let resolved = ResolvedWindowRules::compute(rules, inactive, false);
    assert_eq!(resolved.open_on_workspace, None);
    let resolved = ResolvedWindowRules::compute(rules, inactive, true);
    assert_eq!(resolved.open_on_workspace.as_deref(), Some("chat"));
}

#[test]
fn open_floating_default_follows_parent_and_fixed_height() {
    let rules = ResolvedWindowRules::default();
    let none = Size::from((0, 0));

    assert!(!rules.compute_open_floating(false, none, none));
    // Dialogs have a parent.
    assert!(rules.compute_open_floating(true, none, none));
    // Fixed-height windows float.
    assert!(rules.compute_open_floating(false, Size::from((0, 300)), Size::from((0, 300))));
    assert!(!rules.compute_open_floating(false, Size::from((0, 300)), Size::from((0, 600))));

    // An explicit rule wins over both.
    let rules = ResolvedWindowRules {
        open_floating: Some(false),
        ..Default::default()
    };
    assert!(!rules.compute_open_floating(true, Size::from((0, 300)), Size::from((0, 300))));
}
