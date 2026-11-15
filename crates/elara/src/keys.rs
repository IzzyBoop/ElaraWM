//! Turning macOS key events into niri binds.
//!
//! niri identifies the key of a bind by its X11 keysym, taken from the first level of the
//! keyboard layout: the symbol the key produces with no modifiers. macOS identifies keys by
//! virtual key code. This module bridges the two.

use std::collections::HashSet;

use elara_macos::{KeyEvent, KeyModifiers};
use niri_config::{Action, Config, Modifiers, Trigger};
use smithay::input::keyboard::Keysym;

use crate::binds::find_bind;

/// Returns the keysym for a key that does not type a character, by its macOS virtual key code.
fn keysym_for_keycode(keycode: u16) -> Option<Keysym> {
    let keysym = match keycode {
        0x24 => Keysym::Return,
        0x30 => Keysym::Tab,
        0x31 => Keysym::space,
        0x33 => Keysym::BackSpace,
        0x35 => Keysym::Escape,
        0x72 => Keysym::Insert,
        0x73 => Keysym::Home,
        0x74 => Keysym::Prior,
        0x75 => Keysym::Delete,
        0x77 => Keysym::End,
        0x79 => Keysym::Next,
        0x7B => Keysym::Left,
        0x7C => Keysym::Right,
        0x7D => Keysym::Down,
        0x7E => Keysym::Up,

        0x7A => Keysym::F1,
        0x78 => Keysym::F2,
        0x63 => Keysym::F3,
        0x76 => Keysym::F4,
        0x60 => Keysym::F5,
        0x61 => Keysym::F6,
        0x62 => Keysym::F7,
        0x64 => Keysym::F8,
        0x65 => Keysym::F9,
        0x6D => Keysym::F10,
        0x67 => Keysym::F11,
        0x6F => Keysym::F12,
        0x69 => Keysym::F13,
        0x6B => Keysym::F14,
        0x71 => Keysym::F15,
        0x6A => Keysym::F16,
        0x40 => Keysym::F17,
        0x4F => Keysym::F18,
        0x50 => Keysym::F19,
        0x5A => Keysym::F20,

        0x52 => Keysym::KP_0,
        0x53 => Keysym::KP_1,
        0x54 => Keysym::KP_2,
        0x55 => Keysym::KP_3,
        0x56 => Keysym::KP_4,
        0x57 => Keysym::KP_5,
        0x58 => Keysym::KP_6,
        0x59 => Keysym::KP_7,
        0x5B => Keysym::KP_8,
        0x5C => Keysym::KP_9,
        0x41 => Keysym::KP_Decimal,
        0x43 => Keysym::KP_Multiply,
        0x45 => Keysym::KP_Add,
        0x47 => Keysym::Clear,
        0x4B => Keysym::KP_Divide,
        0x4C => Keysym::KP_Enter,
        0x4E => Keysym::KP_Subtract,
        0x51 => Keysym::KP_Equal,

        _ => return None,
    };
    Some(keysym)
}

/// Returns the keysym for a character.
///
/// Keysyms for printable ASCII and Latin-1 equal the character's code point; `a` is the keysym
/// `a`, `/` is `slash`. Any other character maps to the keysym range that encodes Unicode
/// directly.
fn keysym_for_character(character: char) -> Keysym {
    // Binds are matched on the unshifted symbol, which for letters is the lowercase one.
    let character = character.to_lowercase().next().unwrap_or(character);
    let code = u32::from(character);
    if (0x20..0x100).contains(&code) {
        Keysym::new(code)
    } else {
        Keysym::new(0x0100_0000 | code)
    }
}

/// Returns the keysym niri would match binds against for this key.
pub fn keysym(event: &KeyEvent) -> Option<Keysym> {
    keysym_for_keycode(event.keycode).or_else(|| event.character.map(keysym_for_character))
}

/// Converts held modifier keys to the modifiers binds are written with.
///
/// Command is Super, the key in the same place on a PC keyboard, and Option is Alt.
pub fn modifiers(held: KeyModifiers) -> Modifiers {
    let mut modifiers = Modifiers::empty();
    if held.control {
        modifiers |= Modifiers::CTRL;
    }
    if held.shift {
        modifiers |= Modifiers::SHIFT;
    }
    if held.option {
        modifiers |= Modifiers::ALT;
    }
    if held.command {
        modifiers |= Modifiers::SUPER;
    }
    modifiers
}

/// The keys niri has built in for the overview, with the action and whether holding the key
/// repeats it.
fn hardcoded_overview_bind(keysym: Keysym, held: Modifiers) -> Option<(Action, bool)> {
    if !held.is_empty() {
        return None;
    }
    let bind = match keysym {
        Keysym::Escape | Keysym::Return => (Action::ToggleOverview, false),
        Keysym::Left => (Action::FocusColumnLeft, true),
        Keysym::Right => (Action::FocusColumnRight, true),
        Keysym::Up => (Action::FocusWindowOrWorkspaceUp, true),
        Keysym::Down => (Action::FocusWindowOrWorkspaceDown, true),
        _ => return None,
    };
    Some(bind)
}

/// What to do with a key event.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyResponse {
    /// Whether to keep the event from reaching any application.
    pub swallow: bool,
    /// The action to carry out, if the event triggered a bind.
    pub action: Option<Action>,
}

impl KeyResponse {
    const PASS: Self = Self {
        swallow: false,
        action: None,
    };
    const SWALLOW: Self = Self {
        swallow: true,
        action: None,
    };
}

/// Decides, key event by key event, which ones are binds.
#[derive(Debug, Default)]
pub struct KeyHandler {
    /// Keys whose press triggered a bind and whose release has not been seen yet.
    suppressed: HashSet<u16>,
}

impl KeyHandler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Handles one key event against the binds in `config`.
    ///
    /// As in niri, a key that triggers a bind is hidden from applications for its whole press:
    /// the press, any repeats while it is held, and the release. Holding the key repeats the
    /// action only if the bind allows it.
    pub fn handle(&mut self, config: &Config, event: &KeyEvent) -> KeyResponse {
        self.handle_with_overview(config, event, false)
    }

    /// Like [`handle()`](Self::handle), and says whether the overview is open. While it is,
    /// niri gives the keyboard to the overview and not to any window. The binds work as
    /// ever, Escape and Return close the overview, and the arrow keys move around in it.
    /// Other keys go nowhere, except with Command or Control held, which is how the
    /// system's own shortcuts are typed.
    pub fn handle_with_overview(
        &mut self,
        config: &Config,
        event: &KeyEvent,
        overview_open: bool,
    ) -> KeyResponse {
        if !event.pressed {
            return if self.suppressed.remove(&event.keycode) {
                KeyResponse::SWALLOW
            } else {
                KeyResponse::PASS
            };
        }

        let held = modifiers(event.modifiers);
        let bind = keysym(event).and_then(|keysym| {
            find_bind(config, Trigger::Keysym(keysym), held)
                .map(|bind| (bind.action, bind.repeat))
                // If we didn't find any bind, try other hardcoded keys.
                .or_else(|| {
                    overview_open
                        .then(|| hardcoded_overview_bind(keysym, held))
                        .flatten()
                })
        });
        let Some((action, repeat)) = bind else {
            // If the modifiers changed while a bound key is held, its repeats no longer match
            // a bind, but the application never saw the press and should not see repeats.
            if event.is_repeat && self.suppressed.contains(&event.keycode) {
                return KeyResponse::SWALLOW;
            }
            let for_the_system = event.modifiers.command || event.modifiers.control;
            if overview_open && !for_the_system {
                self.suppressed.insert(event.keycode);
                return KeyResponse::SWALLOW;
            }
            return KeyResponse::PASS;
        };

        self.suppressed.insert(event.keycode);
        if event.is_repeat && !repeat {
            return KeyResponse::SWALLOW;
        }
        KeyResponse {
            swallow: true,
            action: Some(action),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPTION: KeyModifiers = KeyModifiers {
        control: false,
        shift: false,
        option: true,
        command: false,
    };

    fn press(keycode: u16, character: Option<char>, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            keycode,
            character,
            modifiers,
            pressed: true,
            is_repeat: false,
        }
    }

    fn repeat(event: KeyEvent) -> KeyEvent {
        KeyEvent {
            is_repeat: true,
            ..event
        }
    }

    fn release(event: KeyEvent) -> KeyEvent {
        KeyEvent {
            pressed: false,
            is_repeat: false,
            ..event
        }
    }

    // Virtual key codes on an ANSI keyboard.
    const KEY_H: u16 = 0x04;
    const KEY_Q: u16 = 0x0C;
    const KEY_1: u16 = 0x12;
    const KEY_SLASH: u16 = 0x2C;
    const KEY_LEFT: u16 = 0x7B;

    fn config(binds: &str) -> Config {
        Config::parse_mem(&format!("binds {{\n{binds}\n}}")).expect("config must parse")
    }

    #[test]
    fn character_keys_use_the_unshifted_symbol() {
        assert_eq!(keysym(&press(KEY_H, Some('h'), OPTION)), Some(Keysym::h));
        // Even if the layout reports an uppercase letter.
        assert_eq!(keysym(&press(KEY_H, Some('H'), OPTION)), Some(Keysym::h));
        assert_eq!(keysym(&press(KEY_1, Some('1'), OPTION)), Some(Keysym::_1));
        assert_eq!(
            keysym(&press(KEY_SLASH, Some('/'), OPTION)),
            Some(Keysym::slash)
        );
        assert_eq!(
            keysym(&press(0x21, Some('['), OPTION)),
            Some(Keysym::bracketleft)
        );
        assert_eq!(keysym(&press(0x1B, Some('-'), OPTION)), Some(Keysym::minus));
        // Latin-1 and beyond.
        assert_eq!(
            keysym(&press(0x27, Some('ä'), OPTION)),
            Some(Keysym::adiaeresis)
        );
        assert_eq!(
            keysym(&press(0x04, Some('р'), OPTION)).map(|k| k.raw()),
            Some(0x0100_0440)
        );
    }

    #[test]
    fn keys_without_characters_are_named_by_key_code() {
        assert_eq!(keysym(&press(KEY_LEFT, None, OPTION)), Some(Keysym::Left));
        assert_eq!(keysym(&press(0x24, None, OPTION)), Some(Keysym::Return));
        assert_eq!(keysym(&press(0x79, None, OPTION)), Some(Keysym::Next));
        assert_eq!(keysym(&press(0x7A, None, OPTION)), Some(Keysym::F1));
        // The key code wins over whatever private-use character the system reports for it.
        assert_eq!(
            keysym(&press(KEY_LEFT, Some('\u{F702}'), OPTION)),
            Some(Keysym::Left)
        );
        // A key that is neither known nor types anything has no keysym.
        assert_eq!(keysym(&press(0x3F, None, OPTION)), None);
    }

    #[test]
    fn modifiers_map_command_to_super_and_option_to_alt() {
        let all = KeyModifiers {
            control: true,
            shift: true,
            option: true,
            command: true,
        };
        assert_eq!(
            modifiers(all),
            Modifiers::CTRL | Modifiers::SHIFT | Modifiers::ALT | Modifiers::SUPER
        );
        assert_eq!(modifiers(KeyModifiers::default()), Modifiers::empty());
        assert_eq!(modifiers(OPTION), Modifiers::ALT);
    }

    #[test]
    fn a_bound_key_is_swallowed_from_press_to_release() {
        let config = config("Mod+H { focus-column-left; }");
        let mut handler = KeyHandler::new();
        let key = press(KEY_H, Some('h'), OPTION);

        assert_eq!(
            handler.handle(&config, &key),
            KeyResponse {
                swallow: true,
                action: Some(Action::FocusColumnLeft),
            }
        );
        // Releasing Option first must not leak the release of H to the application.
        let late_release = release(press(KEY_H, Some('h'), KeyModifiers::default()));
        assert_eq!(handler.handle(&config, &late_release), KeyResponse::SWALLOW);
        // That release was consumed; the next one belongs to the application.
        assert_eq!(handler.handle(&config, &late_release), KeyResponse::PASS);
    }

    #[test]
    fn unbound_keys_pass_through() {
        let config = config("Mod+H { focus-column-left; }");
        let mut handler = KeyHandler::new();

        let plain = press(KEY_H, Some('h'), KeyModifiers::default());
        assert_eq!(handler.handle(&config, &plain), KeyResponse::PASS);
        assert_eq!(handler.handle(&config, &release(plain)), KeyResponse::PASS);

        let other = press(KEY_Q, Some('q'), OPTION);
        assert_eq!(handler.handle(&config, &other), KeyResponse::PASS);
    }

    #[test]
    fn holding_a_key_repeats_only_if_the_bind_allows() {
        let config = config("Mod+H { focus-column-left; }\nMod+Q repeat=false { close-window; }");
        let mut handler = KeyHandler::new();

        let h = press(KEY_H, Some('h'), OPTION);
        handler.handle(&config, &h);
        assert_eq!(
            handler.handle(&config, &repeat(h)).action,
            Some(Action::FocusColumnLeft)
        );

        let q = press(KEY_Q, Some('q'), OPTION);
        assert_eq!(
            handler.handle(&config, &q).action,
            Some(Action::CloseWindow)
        );
        // Repeats do nothing, but still do not reach the application.
        assert_eq!(handler.handle(&config, &repeat(q)), KeyResponse::SWALLOW);
    }

    #[test]
    fn repeats_stay_hidden_after_the_modifier_is_released() {
        let config = config("Mod+H { focus-column-left; }");
        let mut handler = KeyHandler::new();

        handler.handle(&config, &press(KEY_H, Some('h'), OPTION));
        let unmodified_repeat = repeat(press(KEY_H, Some('h'), KeyModifiers::default()));
        assert_eq!(
            handler.handle(&config, &unmodified_repeat),
            KeyResponse::SWALLOW
        );
    }

    #[test]
    fn default_config_binds_work_from_key_events() {
        let config = Config::load_default();
        let mut handler = KeyHandler::new();

        let left = press(KEY_LEFT, None, OPTION);
        assert_eq!(
            handler.handle(&config, &left).action,
            Some(Action::FocusColumnLeft)
        );

        let shift_slash = press(
            KEY_SLASH,
            Some('/'),
            KeyModifiers {
                shift: true,
                ..OPTION
            },
        );
        assert_eq!(
            handler.handle(&config, &shift_slash).action,
            Some(Action::ShowHotkeyOverlay)
        );
    }
}
