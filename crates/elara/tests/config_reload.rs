//! Switching configs while windows are open, and the built-in default config.

use elara::config::default_config;
use elara::keys::KeyHandler;
use elara::State;
use elara_macos::{KeyEvent, KeyModifiers, MockBackend, Rect, WindowId};
use niri_config::{Action, Config};
use niri_ipc::SizeChange;

const USABLE: Rect = Rect::new(0., 39., 1800., 1130.);

/// Parses a config. Sections that are left out take niri's built-in defaults, which for the
/// layout are the same as in the default config file.
fn config(text: &str) -> Config {
    Config::parse_mem(text).expect("config must parse")
}

fn open(state: &mut State<MockBackend>, app: &str) -> WindowId {
    let id = state
        .backend_mut()
        .open_window(app, app, Rect::new(300., 200., 640., 480.));
    state.tick();
    id
}

fn frame(state: &State<MockBackend>, id: WindowId) -> Rect {
    state.backend().frame(id)
}

#[test]
fn new_layout_options_retile_open_windows() {
    let mut state = State::new(MockBackend::with_laptop_display(), default_config());
    let a = open(&mut state, "a");
    let b = open(&mut state, "b");
    assert_eq!(frame(&state, a), Rect::new(16., 55., 876., 1098.));

    state.set_config(config("layout {\n gaps 40\n}"));

    let gap = 40.;
    let width = (USABLE.w - gap) * 0.5 - gap;
    for id in [a, b] {
        let frame = frame(&state, id);
        assert_eq!(frame.y, USABLE.y + gap);
        assert_eq!(frame.w, width);
        assert_eq!(frame.h, USABLE.h - gap * 2.);
    }
    assert_eq!(frame(&state, b).x, frame(&state, a).max_x() + gap);
    // Where the strip sits horizontally is up to the view, which niri does not move when the
    // config changes; it only has to keep the focused column on screen.
    assert!(frame(&state, b).x >= 0. && frame(&state, b).max_x() <= USABLE.max_x());
}

#[test]
fn new_window_rules_reach_open_windows() {
    let mut state = State::new(MockBackend::with_laptop_display(), default_config());
    let a = open(&mut state, "com.example.editor");
    let b = open(&mut state, "com.example.other");
    assert_eq!(frame(&state, a).w, 876.);

    state.set_config(config(
        r#"
        window-rule {
            match app-id="editor"
            min-width 1000
        }
        "#,
    ));

    assert_eq!(frame(&state, a).w, 1000.);
    assert_eq!(frame(&state, b).w, 876.);

    // Removing the rule lets the window be made narrower again.
    state.set_config(default_config());
    state.do_action(Action::FocusColumnFirst);
    state.do_action(Action::SetColumnWidth(SizeChange::SetFixed(700)));
    assert_eq!(frame(&state, a).w, 700.);
}

#[test]
fn new_binds_take_effect() {
    let mut state = State::new(MockBackend::with_laptop_display(), default_config());
    let mut handler = KeyHandler::new();
    // Option+G on an ANSI keyboard.
    let key = KeyEvent {
        keycode: 0x05,
        character: Some('g'),
        modifiers: KeyModifiers {
            option: true,
            ..Default::default()
        },
        pressed: true,
        is_repeat: false,
    };
    assert_eq!(handler.handle(state.config(), &key).action, None);

    state.set_config(config("binds {\n Mod+G { center-column; }\n}"));

    assert_eq!(
        handler.handle(state.config(), &key).action,
        Some(Action::CenterColumn)
    );
}

#[test]
fn every_bind_in_the_default_config_is_supported() {
    let mut state = State::new(MockBackend::with_laptop_display(), default_config());
    for app in ["a", "b", "c"] {
        open(&mut state, app);
    }

    let binds = default_config().binds.0;
    assert!(binds.len() > 50, "the default config should bind many keys");
    for bind in binds {
        match &bind.action {
            // Starting real programs is not something a test should do. These are the lines
            // the generator rewrote, so check that they name a macOS program.
            Action::Spawn(command) => assert_eq!(command[0], "open", "{command:?}"),
            Action::SpawnSh(command) => panic!("unexpected shell command bound: {command}"),
            action => assert!(
                state.do_action(action.clone()),
                "the default config binds {:?} to unsupported action {action:?}",
                bind.key
            ),
        }
        state.tick();
    }
}
