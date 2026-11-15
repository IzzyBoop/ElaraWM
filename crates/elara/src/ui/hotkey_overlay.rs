// ELARA: the imports for drawing with Pango and Smithay's renderer are gone; see `content()`.
use niri_config::{Action, Bind, Config, Key, ModKey, Modifiers, Trigger};
use smithay::input::keyboard::xkb::keysym_get_name;

use crate::ui::markup;

// ELARA: the measures and colors niri draws with moved to `elara-macos`, which does the
// drawing.
const TITLE: &str = "Important Hotkeys";

// ELARA: holds neither the config, which is passed to `content()`, nor rendered buffers.
pub struct HotkeyOverlay {
    is_open: bool,
    mod_key: ModKey,
}

impl HotkeyOverlay {
    pub fn new(mod_key: ModKey) -> Self {
        Self {
            is_open: false,
            mod_key,
        }
    }

    pub fn show(&mut self) -> bool {
        if !self.is_open {
            self.is_open = true;
            true
        } else {
            false
        }
    }

    pub fn hide(&mut self) -> bool {
        if self.is_open {
            self.is_open = false;
            true
        } else {
            false
        }
    }

    pub fn is_open(&self) -> bool {
        self.is_open
    }

    pub fn on_hotkey_config_updated(&mut self, mod_key: ModKey) {
        self.mod_key = mod_key;
    }

    // ELARA: stands in for `render()`. niri draws the overlay into a texture with Pango and
    // shows that on every output. Here the lines to draw are handed to the backend, which
    // draws them with AppKit; the first half of niri's free function `render()` is what
    // collects them. `a11y_text()`, which reads the overlay to a screen reader, is left out.
    pub fn content(&self, config: &Config) -> Option<elara_macos::HotkeyOverlay> {
        if !self.is_open {
            return None;
        }

        let rows = collect_actions(config)
            .into_iter()
            .filter_map(|action| format_bind(&config.binds.0, action))
            .map(|(key, action)| {
                let key = key.map(|key| key_name(false, self.mod_key, &key));
                let key = key.as_deref().unwrap_or("(not bound)").to_owned();

                let action = match markup::parse(&action) {
                    Ok(spans) => spans,
                    Err(err) => {
                        warn!("error parsing markup for key {key}: {err}");
                        vec![elara_macos::TextSpan {
                            text: action,
                            ..Default::default()
                        }]
                    }
                };

                elara_macos::HotkeyOverlayRow { key, action }
            })
            .collect();

        Some(elara_macos::HotkeyOverlay {
            title: String::from(TITLE),
            rows,
        })
    }
}

fn format_bind(binds: &[Bind], action: &Action) -> Option<(Option<Key>, String)> {
    let mut bind_with_non_null = None;
    let mut bind_with_custom_title = None;
    let mut found_null_title = false;

    for bind in binds {
        if bind.action != *action {
            continue;
        }

        match &bind.hotkey_overlay_title {
            Some(Some(_)) => {
                bind_with_custom_title.get_or_insert(bind);
            }
            Some(None) => {
                found_null_title = true;
            }
            None => {
                bind_with_non_null.get_or_insert(bind);
            }
        }
    }

    if bind_with_custom_title.is_none() && found_null_title {
        return None;
    }

    let mut title = None;
    let key = if let Some(bind) = bind_with_custom_title.or(bind_with_non_null) {
        if let Some(Some(custom)) = &bind.hotkey_overlay_title {
            title = Some(custom.clone());
        }

        Some(bind.key)
    } else {
        None
    };
    let title = title.unwrap_or_else(|| action_name(action));

    Some((key, title))
}

fn collect_actions(config: &Config) -> Vec<&Action> {
    let binds = &config.binds.0;

    // Collect actions that we want to show.
    let mut actions = vec![&Action::ShowHotkeyOverlay];

    // Prefer Quit(false) if found, otherwise try Quit(true), and if there's neither, fall back to
    // Quit(false).
    if binds.iter().any(|bind| bind.action == Action::Quit(false)) {
        actions.push(&Action::Quit(false));
    } else if binds.iter().any(|bind| bind.action == Action::Quit(true)) {
        actions.push(&Action::Quit(true));
    } else {
        actions.push(&Action::Quit(false));
    }

    actions.extend(&[
        &Action::CloseWindow,
        &Action::FocusColumnLeft,
        &Action::FocusColumnRight,
        &Action::MoveColumnLeft,
        &Action::MoveColumnRight,
        &Action::FocusWorkspaceDown,
        &Action::FocusWorkspaceUp,
    ]);

    // Prefer move-column-to-workspace-down, but fall back to move-window-to-workspace-down.
    if let Some(bind) = binds
        .iter()
        .find(|bind| matches!(bind.action, Action::MoveColumnToWorkspaceDown(_)))
    {
        actions.push(&bind.action);
    } else if binds
        .iter()
        .any(|bind| matches!(bind.action, Action::MoveWindowToWorkspaceDown(_)))
    {
        actions.push(&Action::MoveWindowToWorkspaceDown(true));
    } else {
        actions.push(&Action::MoveColumnToWorkspaceDown(true));
    }

    // Same for -up.
    if let Some(bind) = binds
        .iter()
        .find(|bind| matches!(bind.action, Action::MoveColumnToWorkspaceUp(_)))
    {
        actions.push(&bind.action);
    } else if binds
        .iter()
        .any(|bind| matches!(bind.action, Action::MoveWindowToWorkspaceUp(_)))
    {
        actions.push(&Action::MoveWindowToWorkspaceUp(true));
    } else {
        actions.push(&Action::MoveColumnToWorkspaceUp(true));
    }

    actions.extend(&[
        &Action::SwitchPresetColumnWidth,
        &Action::MaximizeColumn,
        &Action::ConsumeOrExpelWindowLeft,
        &Action::ConsumeOrExpelWindowRight,
        &Action::ToggleWindowFloating,
        &Action::SwitchFocusBetweenFloatingAndTiling,
        &Action::ToggleOverview,
    ]);

    // Screenshot is not as important, can omit if not bound.
    if let Some(bind) = binds
        .iter()
        .find(|bind| matches!(bind.action, Action::Screenshot(_, _)))
    {
        actions.push(&bind.action);
    }

    // Add actions with a custom hotkey-overlay-title.
    for bind in binds {
        if matches!(bind.hotkey_overlay_title, Some(Some(_))) {
            // Avoid duplicate actions.
            if !actions.contains(&&bind.action) {
                actions.push(&bind.action);
            }
        }
    }

    // Add the spawn actions.
    for bind in binds.iter().filter(|bind| {
        matches!(bind.action, Action::Spawn(_) | Action::SpawnSh(_))
            // Only show binds with Mod or Super to filter out stuff like volume up/down.
            && (bind.key.modifiers.contains(Modifiers::COMPOSITOR)
                || bind.key.modifiers.contains(Modifiers::SUPER))
            // Also filter out wheel and touchpad scroll binds.
            && matches!(bind.key.trigger, Trigger::Keysym(_))
    }) {
        let action = &bind.action;

        // We only show one bind for each action, so we need to deduplicate the Spawn actions.
        if !actions.contains(&action) {
            actions.push(action);
        }
    }

    if config.hotkey_overlay.hide_not_bound {
        // Only keep actions that have been bound
        actions.retain(|&action| binds.iter().any(|bind| bind.action == *action))
    }

    actions
}

fn action_name(action: &Action) -> String {
    match action {
        // ELARA: "Exit niri" upstream.
        Action::Quit(_) => String::from("Exit ElaraWM"),
        Action::ShowHotkeyOverlay => String::from("Show Important Hotkeys"),
        Action::CloseWindow => String::from("Close Focused Window"),
        Action::FocusColumnLeft => String::from("Focus Column to the Left"),
        Action::FocusColumnRight => String::from("Focus Column to the Right"),
        Action::MoveColumnLeft => String::from("Move Column Left"),
        Action::MoveColumnRight => String::from("Move Column Right"),
        Action::FocusWorkspaceDown => String::from("Switch Workspace Down"),
        Action::FocusWorkspaceUp => String::from("Switch Workspace Up"),
        Action::MoveColumnToWorkspaceDown(_) => String::from("Move Column to Workspace Down"),
        Action::MoveColumnToWorkspaceUp(_) => String::from("Move Column to Workspace Up"),
        Action::MoveWindowToWorkspaceDown(_) => String::from("Move Window to Workspace Down"),
        Action::MoveWindowToWorkspaceUp(_) => String::from("Move Window to Workspace Up"),
        Action::SwitchPresetColumnWidth => String::from("Switch Preset Column Widths"),
        Action::MaximizeColumn => String::from("Maximize Column"),
        Action::ConsumeOrExpelWindowLeft => String::from("Consume or Expel Window Left"),
        Action::ConsumeOrExpelWindowRight => String::from("Consume or Expel Window Right"),
        Action::ToggleWindowFloating => String::from("Move Window Between Floating and Tiling"),
        Action::SwitchFocusBetweenFloatingAndTiling => {
            String::from("Switch Focus Between Floating and Tiling")
        }
        Action::ToggleOverview => String::from("Open the Overview"),
        Action::Screenshot(_, _) => String::from("Take a Screenshot"),
        Action::Spawn(args) => format!(
            "Spawn <span face='monospace' bgcolor='#000000'>{}</span>",
            args.first().unwrap_or(&String::new())
        ),
        Action::SpawnSh(command) => format!(
            "Spawn <span face='monospace' bgcolor='#000000'>{}</span>",
            // Fairly crude but should get the job done in most cases.
            command.split_ascii_whitespace().next().unwrap_or("")
        ),
        _ => String::from("FIXME: Unknown"),
    }
}

fn key_name(screen_reader: bool, mod_key: ModKey, key: &Key) -> String {
    let mut name = String::new();

    let has_comp_mod = key.modifiers.contains(Modifiers::COMPOSITOR);

    // ELARA: the modifiers are called what a Mac keyboard calls them: Command where niri
    // says Super, Option for Alt and Control for Ctrl.
    // Compositor mod goes first.
    if has_comp_mod {
        match mod_key {
            ModKey::Super => {
                name.push_str("Command + ");
            }
            ModKey::Alt => {
                name.push_str("Option + ");
            }
            ModKey::Shift => {
                name.push_str("Shift + ");
            }
            ModKey::Ctrl => {
                name.push_str("Control + ");
            }
            ModKey::IsoLevel3Shift => {
                name.push_str("Mod5 + ");
            }
            ModKey::IsoLevel5Shift => {
                name.push_str("Mod3 + ");
            }
        }
    }

    if key.modifiers.contains(Modifiers::SUPER) && !(has_comp_mod && mod_key == ModKey::Super) {
        name.push_str("Command + ");
    }
    if key.modifiers.contains(Modifiers::CTRL) && !(has_comp_mod && mod_key == ModKey::Ctrl) {
        name.push_str("Control + ");
    }
    if key.modifiers.contains(Modifiers::SHIFT) && !(has_comp_mod && mod_key == ModKey::Shift) {
        name.push_str("Shift + ");
    }
    if key.modifiers.contains(Modifiers::ALT) && !(has_comp_mod && mod_key == ModKey::Alt) {
        name.push_str("Option + ");
    }
    if key.modifiers.contains(Modifiers::ISO_LEVEL3_SHIFT)
        && !(has_comp_mod && mod_key == ModKey::IsoLevel3Shift)
    {
        name.push_str("Mod5 + ");
    }
    if key.modifiers.contains(Modifiers::ISO_LEVEL5_SHIFT)
        && !(has_comp_mod && mod_key == ModKey::IsoLevel5Shift)
    {
        name.push_str("Mod3 + ");
    }

    let pretty = match key.trigger {
        Trigger::Keysym(keysym) => prettify_keysym_name(screen_reader, &keysym_get_name(keysym)),
        Trigger::MouseLeft => String::from("Mouse Left"),
        Trigger::MouseRight => String::from("Mouse Right"),
        Trigger::MouseMiddle => String::from("Mouse Middle"),
        Trigger::MouseBack => String::from("Mouse Back"),
        Trigger::MouseForward => String::from("Mouse Forward"),
        Trigger::WheelScrollDown => String::from("Wheel Scroll Down"),
        Trigger::WheelScrollUp => String::from("Wheel Scroll Up"),
        Trigger::WheelScrollLeft => String::from("Wheel Scroll Left"),
        Trigger::WheelScrollRight => String::from("Wheel Scroll Right"),
        Trigger::TouchpadScrollDown => String::from("Touchpad Scroll Down"),
        Trigger::TouchpadScrollUp => String::from("Touchpad Scroll Up"),
        Trigger::TouchpadScrollLeft => String::from("Touchpad Scroll Left"),
        Trigger::TouchpadScrollRight => String::from("Touchpad Scroll Right"),
    };
    name.push_str(&pretty);

    name
}

fn prettify_keysym_name(screen_reader: bool, name: &str) -> String {
    let name = if screen_reader {
        name
    } else {
        match name {
            "slash" => "/",
            "comma" => ",",
            "period" => ".",
            "minus" => "-",
            "equal" => "=",
            "grave" => "`",
            "bracketleft" => "[",
            "bracketright" => "]",
            "adiaeresis" => "Ä",
            "ediaeresis" => "Ë",
            "idiaeresis" => "Ï",
            "odiaeresis" => "Ö",
            "udiaeresis" => "Ü",
            "ydiaeresis" => "Ÿ",
            "wdiaeresis" => "Ẅ",
            _ => name,
        }
    };

    let name = match name {
        "Next" => "Page Down",
        "Prior" => "Page Up",
        "Print" => "PrtSc",
        "Return" => "Enter",
        "space" => "Space",
        _ => name,
    };

    if name.len() == 1 && name.is_ascii() {
        name.to_ascii_uppercase()
    } else {
        name.into()
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use super::*;

    #[track_caller]
    fn check(config: &str, action: Action) -> String {
        let config = Config::parse_mem(config).unwrap();
        if let Some((key, title)) = format_bind(&config.binds.0, &action) {
            let key = key.map(|key| key_name(false, ModKey::Super, &key));
            let key = key.as_deref().unwrap_or("(not bound)");
            format!(" {key} : {title}")
        } else {
            String::from("None")
        }
    }

    // ELARA: the snapshots say "Command" where niri's say "Super".
    #[test]
    fn test_format_bind() {
        // Not bound.
        assert_snapshot!(check("", Action::Screenshot(true, None)), @" (not bound) : Take a Screenshot");

        // Bound with a default title.
        assert_snapshot!(
            check(
                r#"binds {
                    Mod+P { screenshot; }
                }"#,
                Action::Screenshot(true, None),
            ),
            @" Command + P : Take a Screenshot"
        );

        // Custom title.
        assert_snapshot!(
            check(
                r#"binds {
                    Mod+P hotkey-overlay-title="Hello" { screenshot; }
                }"#,
                Action::Screenshot(true, None),
            ),
            @" Command + P : Hello"
        );

        // Prefer first bind.
        assert_snapshot!(
            check(
                r#"binds {
                    Mod+P { screenshot; }
                    Print { screenshot; }
                }"#,
                Action::Screenshot(true, None),
            ),
            @" Command + P : Take a Screenshot"
        );

        // Prefer bind with custom title.
        assert_snapshot!(
            check(
                r#"binds {
                    Mod+P { screenshot; }
                    Print hotkey-overlay-title="My Cool Bind" { screenshot; }
                }"#,
                Action::Screenshot(true, None),
            ),
            @" PrtSc : My Cool Bind"
        );

        // Prefer first bind with custom title.
        assert_snapshot!(
            check(
                r#"binds {
                    Mod+P hotkey-overlay-title="First" { screenshot; }
                    Print hotkey-overlay-title="My Cool Bind" { screenshot; }
                }"#,
                Action::Screenshot(true, None),
            ),
            @" Command + P : First"
        );

        // Any bind with null title hides it.
        assert_snapshot!(
            check(
                r#"binds {
                    Mod+P { screenshot; }
                    Print hotkey-overlay-title=null { screenshot; }
                }"#,
                Action::Screenshot(true, None),
            ),
            @"None"
        );

        // Custom title takes preference over null.
        assert_snapshot!(
            check(
                r#"binds {
                    Mod+P hotkey-overlay-title="Hello" { screenshot; }
                    Print hotkey-overlay-title=null { screenshot; }
                }"#,
                Action::Screenshot(true, None),
            ),
            @" Command + P : Hello"
        );
    }
}
