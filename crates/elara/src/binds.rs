//! Finding the bind for a key press.

use niri_config::{Bind, Config, ModKey, Modifiers, Trigger};

/// The modifier that `Mod` stands for in binds when the config does not say.
///
/// niri defaults to Super. On macOS that is Command, which nearly every application shortcut
/// uses, so ElaraWM defaults to Option, which the config calls Alt.
pub const DEFAULT_MOD_KEY: ModKey = ModKey::Alt;

/// Returns the modifier that `Mod` stands for under `config`.
pub fn mod_key(config: &Config) -> ModKey {
    config.input.mod_key.unwrap_or(DEFAULT_MOD_KEY)
}

/// Finds the configured bind for `trigger` pressed with exactly `modifiers` held.
///
/// This is niri's `find_configured_bind()`, taking the held modifiers as `Modifiers` instead of
/// Smithay's keyboard state. `Mod` in a bind and the actual mod key are interchangeable: a bind
/// on `Mod+H` matches Option+H when the mod key is Option, and so does a bind on `Alt+H`.
pub fn find_configured_bind<'a>(
    bindings: impl IntoIterator<Item = &'a Bind>,
    mod_key: ModKey,
    trigger: Trigger,
    mut modifiers: Modifiers,
) -> Option<Bind> {
    let mod_down = modifiers.contains(mod_key.to_modifiers());
    if mod_down {
        modifiers |= Modifiers::COMPOSITOR;
    }

    for bind in bindings {
        if bind.key.trigger != trigger {
            continue;
        }

        let mut bind_modifiers = bind.key.modifiers;
        if bind_modifiers.contains(Modifiers::COMPOSITOR) {
            bind_modifiers |= mod_key.to_modifiers();
        } else if bind_modifiers.contains(mod_key.to_modifiers()) {
            bind_modifiers |= Modifiers::COMPOSITOR;
        }

        if bind_modifiers == modifiers {
            return Some(bind.clone());
        }
    }

    None
}

/// Finds the bind in `config` for `trigger` pressed with exactly `modifiers` held.
pub fn find_bind(config: &Config, trigger: Trigger, modifiers: Modifiers) -> Option<Bind> {
    find_configured_bind(&config.binds.0, mod_key(config), trigger, modifiers)
}

#[cfg(test)]
mod tests {
    use niri_config::Action;
    use smithay::input::keyboard::Keysym;

    use super::*;

    fn config(text: &str) -> Config {
        Config::parse_mem(text).expect("config must parse")
    }

    fn action(config: &Config, key: Keysym, modifiers: Modifiers) -> Option<Action> {
        find_bind(config, Trigger::Keysym(key), modifiers).map(|bind| bind.action)
    }

    #[test]
    fn mod_is_option_unless_configured() {
        let config = config("binds {\n Mod+H { focus-column-left; }\n }");

        assert_eq!(mod_key(&config), ModKey::Alt);
        assert_eq!(
            action(&config, Keysym::h, Modifiers::ALT),
            Some(Action::FocusColumnLeft)
        );
        // Command is not the mod key, so Command+H stays with the application.
        assert_eq!(action(&config, Keysym::h, Modifiers::SUPER), None);
    }

    #[test]
    fn mod_key_can_be_configured() {
        let config = config(
            r#"
            input {
                mod-key "Super"
            }
            binds {
                Mod+H { focus-column-left; }
            }
            "#,
        );

        assert_eq!(
            action(&config, Keysym::h, Modifiers::SUPER),
            Some(Action::FocusColumnLeft)
        );
        assert_eq!(action(&config, Keysym::h, Modifiers::ALT), None);
    }

    #[test]
    fn modifiers_must_match_exactly() {
        let config = config(
            r#"
            binds {
                Mod+H { focus-column-left; }
                Mod+Shift+H { move-column-left; }
                Mod+Ctrl+H { focus-monitor-left; }
            }
            "#,
        );

        let alt = Modifiers::ALT;
        assert_eq!(
            action(&config, Keysym::h, alt),
            Some(Action::FocusColumnLeft)
        );
        assert_eq!(
            action(&config, Keysym::h, alt | Modifiers::SHIFT),
            Some(Action::MoveColumnLeft)
        );
        assert_eq!(
            action(&config, Keysym::h, alt | Modifiers::CTRL),
            Some(Action::FocusMonitorLeft)
        );
        assert_eq!(action(&config, Keysym::h, Modifiers::empty()), None);
        assert_eq!(
            action(&config, Keysym::h, alt | Modifiers::SHIFT | Modifiers::CTRL),
            None
        );
        assert_eq!(action(&config, Keysym::j, alt), None);
    }

    #[test]
    fn naming_the_mod_key_itself_is_the_same_as_mod() {
        let config = config("binds {\n Alt+H { focus-column-left; }\n }");

        assert_eq!(
            action(&config, Keysym::h, Modifiers::ALT),
            Some(Action::FocusColumnLeft)
        );
    }

    #[test]
    fn default_config_binds_resolve() {
        let config = Config::load_default();
        let alt = Modifiers::ALT;

        assert_eq!(
            action(&config, Keysym::Left, alt),
            Some(Action::FocusColumnLeft)
        );
        assert_eq!(
            action(&config, Keysym::Right, alt | Modifiers::CTRL),
            Some(Action::MoveColumnRight)
        );
        assert_eq!(
            action(&config, Keysym::f, alt),
            Some(Action::MaximizeColumn)
        );
        assert_eq!(
            action(&config, Keysym::Page_Down, alt),
            Some(Action::FocusWorkspaceDown)
        );
    }
}
