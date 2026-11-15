//! The hotkey overlay: the list of important key bindings shown over everything else.

use elara::config::default_config;
use elara::ui::hides_hotkey_overlay;
use elara::State;
use elara_macos::{
    HotkeyOverlay, InputEvent, KeyEvent, KeyModifiers, MockBackend, MouseButton, MouseEvent,
    MouseEventKind, Point, Rect, ScrollEvent, ScrollPhase,
};
use niri_config::{Action, Config};

fn config(text: &str) -> Config {
    Config::parse_mem(text).expect("config must parse")
}

fn state(config: Config) -> State<MockBackend> {
    State::new(MockBackend::with_laptop_display(), config)
}

fn shown(state: &State<MockBackend>) -> &HotkeyOverlay {
    state
        .backend()
        .hotkey_overlay()
        .expect("the hotkey overlay is showing")
}

/// The line for a binding, as `keys: what they do`.
fn lines(overlay: &HotkeyOverlay) -> Vec<String> {
    overlay
        .rows
        .iter()
        .map(|row| {
            let action: String = row.action.iter().map(|span| span.text.as_str()).collect();
            format!("{}: {action}", row.key)
        })
        .collect()
}

#[test]
fn it_greets_the_user_unless_the_config_says_not_to() {
    let state = state(default_config());
    assert!(state.hotkey_overlay_open());
    assert_eq!(shown(&state).title, "Important Hotkeys");

    let state = self::state(config("hotkey-overlay {\n skip-at-startup\n }"));
    assert!(!state.hotkey_overlay_open());
    assert_eq!(state.backend().hotkey_overlay(), None);
}

#[test]
fn the_action_shows_it_and_input_takes_it_away() {
    let mut state = state(config("hotkey-overlay {\n skip-at-startup\n }"));

    assert!(state.do_action(Action::ShowHotkeyOverlay));
    assert!(state.hotkey_overlay_open());
    assert!(state.backend().hotkey_overlay().is_some());

    // Showing it again changes nothing.
    state.do_action(Action::ShowHotkeyOverlay);
    assert!(state.hotkey_overlay_open());

    state.hide_hotkey_overlay();
    assert!(!state.hotkey_overlay_open());
    assert_eq!(state.backend().hotkey_overlay(), None);

    // Neither does taking away what is not there.
    state.hide_hotkey_overlay();
    assert_eq!(state.backend().hotkey_overlay(), None);
}

#[test]
fn it_stays_up_through_other_actions() {
    let mut state = state(default_config());
    state
        .backend_mut()
        .open_window("a", "a", Rect::new(300., 200., 640., 480.));
    state.tick();

    state.do_action(Action::FocusColumnLeft);
    state.do_action(Action::ToggleOverview);
    assert!(state.backend().hotkey_overlay().is_some());
}

#[test]
fn it_lists_the_binds_of_the_default_config() {
    let state = state(default_config());
    let lines = lines(shown(&state));

    // Mod is Option here, and the modifiers have the names a Mac keyboard gives them.
    assert_eq!(lines[0], "Option + Shift + /: Show Important Hotkeys");
    assert_eq!(lines[1], "Option + Shift + E: Exit ElaraWM");
    assert!(lines.contains(&"Option + Q: Close Focused Window".to_owned()));
    assert!(lines.contains(&"Option + Control + Left: Move Column Left".to_owned()));
    assert!(lines.contains(&"Option + Page Down: Switch Workspace Down".to_owned()));
    assert!(lines.contains(&"Option + O: Open the Overview".to_owned()));
    // A bind with a title of its own.
    assert!(lines.contains(&"Option + T: Open a Terminal".to_owned()));
}

#[test]
fn it_names_keys_by_the_mod_key_in_use() {
    let state = state(config(
        r#"
        input {
            mod-key "Super"
        }
        binds {
            Mod+H { focus-column-left; }
            Mod+Alt+L { focus-column-right; }
        }
        "#,
    ));
    let lines = lines(shown(&state));

    assert!(lines.contains(&"Command + H: Focus Column to the Left".to_owned()));
    assert!(lines.contains(&"Command + Option + L: Focus Column to the Right".to_owned()));
    // Actions niri always lists are there without a bind.
    assert!(lines.contains(&"(not bound): Close Focused Window".to_owned()));
}

#[test]
fn it_follows_the_config() {
    let mut state = state(config("binds {\n Mod+H { focus-column-left; }\n }"));
    assert!(lines(shown(&state)).contains(&"Option + H: Focus Column to the Left".to_owned()));

    state.set_config(config(
        r#"
        hotkey-overlay {
            hide-not-bound
        }
        binds {
            Mod+J { focus-column-left; }
        }
        "#,
    ));
    assert_eq!(
        lines(shown(&state)),
        ["Option + J: Focus Column to the Left"]
    );
}

#[test]
fn spawn_binds_show_the_program_in_a_font_of_its_own() {
    let state = state(config(
        r#"
        binds {
            Mod+T { spawn "open" "-a" "Terminal"; }
            Mod+B hotkey-overlay-title="Open <b>the</b> Browser" { spawn "open" "-a" "Safari"; }
            Mod+N hotkey-overlay-title="Fish & Chips" { spawn "true"; }
        }
        "#,
    ));
    let overlay = shown(&state);
    let row = |key: &str| {
        let row = overlay.rows.iter().find(|row| row.key == key);
        &row.expect("the bind is listed").action
    };

    let spawn = row("Option + T");
    assert_eq!(spawn.len(), 2);
    assert_eq!(
        (spawn[0].text.as_str(), spawn[0].monospace),
        ("Spawn ", false)
    );
    assert_eq!((spawn[1].text.as_str(), spawn[1].monospace), ("open", true));
    assert_eq!(spawn[1].background, Some([0., 0., 0., 1.]));

    let titled = row("Option + B");
    let bold: Vec<_> = titled
        .iter()
        .map(|span| (span.text.as_str(), span.bold))
        .collect();
    assert_eq!(bold, [("Open ", false), ("the", true), (" Browser", false)]);

    // Markup that cannot be read is shown as it was written.
    let broken = row("Option + N");
    assert_eq!(broken.len(), 1);
    assert_eq!(broken[0].text, "Fish & Chips");
}

#[test]
fn presses_and_scrolling_take_it_away_and_releases_do_not() {
    let key = |pressed| {
        InputEvent::Key(KeyEvent {
            keycode: 0,
            character: Some('a'),
            modifiers: KeyModifiers::default(),
            pressed,
            is_repeat: false,
        })
    };
    let mouse = |kind| {
        InputEvent::Mouse(MouseEvent {
            kind,
            position: Point { x: 0., y: 0. },
            modifiers: KeyModifiers::default(),
        })
    };
    let scroll = InputEvent::Scroll(ScrollEvent {
        delta_x: 0.,
        delta_y: 1.,
        continuous: false,
        phase: ScrollPhase::None,
        momentum: false,
        modifiers: KeyModifiers::default(),
    });

    assert!(hides_hotkey_overlay(&key(true)));
    assert!(!hides_hotkey_overlay(&key(false)));
    assert!(hides_hotkey_overlay(&mouse(MouseEventKind::Pressed(
        MouseButton::Left
    ))));
    assert!(!hides_hotkey_overlay(&mouse(MouseEventKind::Released(
        MouseButton::Left
    ))));
    assert!(!hides_hotkey_overlay(&mouse(MouseEventKind::Dragged)));
    assert!(hides_hotkey_overlay(&scroll));
}

#[test]
fn it_goes_when_elara_stops() {
    let mut state = state(default_config());
    assert!(state.backend().hotkey_overlay().is_some());

    state.release_windows();
    assert_eq!(state.backend().hotkey_overlay(), None);
}

#[test]
fn the_line_around_it_is_the_focus_rings() {
    // niri's default ring: 4 wide, light blue, around corners as round as a window's.
    let state = state(default_config());
    let frame = shown(&state).frame;
    assert_eq!(frame.width, 4.);
    assert_eq!(frame.corner_radius, 16.);
    assert_eq!(frame.color, [127. / 255., 200. / 255., 1., 1.]);
    assert_eq!(frame.gradient, None);

    let state = self::state(config(
        r##"layout { focus-ring { width 6; active-gradient from="#ff0000" to="#0000ff" angle=45; }; }"##,
    ));
    let frame = shown(&state).frame;
    assert_eq!(frame.width, 6.);
    assert_eq!(frame.color, [1., 0., 0., 1.]);
    assert_eq!(frame.gradient.expect("a gradient").to, [0., 0., 1., 1.]);
}

#[test]
fn a_ring_that_is_off_still_leaves_the_list_an_edge() {
    let state = state(config("layout { focus-ring { off; }; }"));
    assert_eq!(shown(&state).frame.width, 4.);
}
