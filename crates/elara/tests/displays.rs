//! More than one display, and displays changing while windows are open.

use elara::config::default_config;
use elara::State;
use elara_macos::{DisplayId, DisplayInfo, MockBackend, Rect, WindowBackend, WindowId};
use niri_config::Action;

const LAPTOP: Rect = Rect::new(0., 0., 1800., 1169.);
const GAP: f64 = 16.;

fn external(frame: Rect) -> DisplayInfo {
    DisplayInfo {
        id: DisplayId(2),
        name: "External".to_owned(),
        frame,
        visible_frame: Rect::new(frame.x, frame.y + 25., frame.w, frame.h - 25.),
        scale: 1.,
        refresh_mhz: Some(60_000),
        is_primary: false,
    }
}

fn start_with_external(frame: Rect) -> State<MockBackend> {
    let mut backend = MockBackend::with_laptop_display();
    backend.add_display(external(frame));
    State::new(backend, default_config())
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

/// Whether the window shows on the laptop display beyond a single pixel.
fn shows_on_laptop(state: &State<MockBackend>, id: WindowId) -> bool {
    let f = frame(state, id);
    let inset = Rect::new(LAPTOP.x + 1., LAPTOP.y + 1., LAPTOP.w - 2., LAPTOP.h - 2.);
    f.intersects(&inset)
}

#[test]
fn hidden_windows_never_show_on_a_display_to_the_right() {
    let right = Rect::new(1800., 0., 2560., 1440.);
    let mut state = start_with_external(right);
    let a = open(&mut state, "a");
    let b = open(&mut state, "b");
    let c = open(&mut state, "c");

    // Three half-width columns on the laptop: one is out of view.
    assert!(!shows_on_laptop(&state, a));
    assert!(shows_on_laptop(&state, b) && shows_on_laptop(&state, c));
    for id in [a, b, c] {
        assert!(
            !frame(&state, id).intersects(&right),
            "window {id:?} at {:?} shows on the external display",
            frame(&state, id)
        );
    }

    // Other workspaces too.
    state.do_action(Action::FocusWorkspaceDown);
    for id in [a, b, c] {
        assert!(!shows_on_laptop(&state, id));
        assert!(!frame(&state, id).intersects(&right));
    }
}

#[test]
fn a_column_peeking_past_a_shared_edge_is_hidden_instead() {
    // On a single display, niri lets the next column peek in at the edge.
    let mut alone = State::new(MockBackend::with_laptop_display(), default_config());
    let a = open(&mut alone, "a");
    let b = open(&mut alone, "b");
    let _c = open(&mut alone, "c");
    alone.do_action(Action::FocusColumnFirst);
    alone.do_action(Action::SetColumnWidth(niri_ipc::SizeChange::SetFixed(1000)));
    assert_eq!(frame(&alone, a).x, GAP);
    assert!(frame(&alone, b).x < LAPTOP.max_x() && frame(&alone, b).max_x() > LAPTOP.max_x());

    // With a display on that side, the part past the edge would be drawn there.
    let right = Rect::new(1800., 0., 2560., 1440.);
    let mut state = start_with_external(right);
    let a = open(&mut state, "a");
    let b = open(&mut state, "b");
    let _c = open(&mut state, "c");
    state.do_action(Action::FocusColumnFirst);
    state.do_action(Action::SetColumnWidth(niri_ipc::SizeChange::SetFixed(1000)));
    assert_eq!(frame(&state, a).x, GAP);
    assert!(!frame(&state, b).intersects(&right));
    assert!(!shows_on_laptop(&state, b));
}

#[test]
fn parking_avoids_a_display_below_and_to_the_right() {
    // The external display sits diagonally below-right, where the default parking spot hangs.
    let diagonal = Rect::new(1800., 1169., 2560., 1440.);
    let mut state = start_with_external(diagonal);
    let a = open(&mut state, "a");
    let _b = open(&mut state, "b");
    let _c = open(&mut state, "c");

    assert!(!shows_on_laptop(&state, a));
    assert!(!frame(&state, a).intersects(&diagonal));
}

#[test]
fn floating_windows_may_straddle_displays() {
    let right = Rect::new(1800., 0., 2560., 1440.);
    let mut state = start_with_external(right);
    let a = open(&mut state, "a");
    state.do_action(Action::ToggleWindowFloating);

    let straddling = Rect::new(1500., 300., 600., 400.);
    state.backend_mut().user_set_frame(a, straddling);
    state.tick();
    state.tick();

    assert_eq!(frame(&state, a), straddling);
}

#[test]
fn unplugging_a_display_moves_its_windows_to_the_remaining_one() {
    let right = Rect::new(1800., 0., 2560., 1440.);
    let mut backend = MockBackend::with_laptop_display();
    backend.add_display(external(right));
    let on_laptop = backend.open_window("laptop", "l", Rect::new(100., 100., 500., 400.));
    let on_external = backend.open_window("external", "e", Rect::new(2500., 300., 500., 400.));
    let mut state = State::new(backend, default_config());
    assert!(frame(&state, on_external).x >= 1800.);

    state.backend_mut().remove_display(DisplayId(2));
    state.tick();

    // niri moves the workspaces of a removed output to another output.
    assert_eq!(state.layout().windows().count(), 2);
    // Each window either shows on the laptop display or is parked at its corner.
    for id in [on_laptop, on_external] {
        let f = frame(&state, id);
        let parked = f.x == LAPTOP.max_x() - 1. && f.y == LAPTOP.max_y() - 1.;
        assert!(shows_on_laptop(&state, id) || parked, "{f:?}");
    }
    // The laptop's own workspace is still the one showing.
    assert!(shows_on_laptop(&state, on_laptop));

    // And plugging it back in gives the layout a second output again.
    state.backend_mut().add_display(external(right));
    state.tick();
    assert_eq!(state.layout().outputs().count(), 2);
}

#[test]
fn the_dock_appearing_is_noticed_without_an_event() {
    let mut state = State::new(MockBackend::with_laptop_display(), default_config());
    let a = open(&mut state, "a");
    assert_eq!(frame(&state, a), Rect::new(16., 55., 876., 1098.));

    // A Dock 80 points tall appears at the bottom.
    state
        .backend_mut()
        .set_visible_frame(DisplayId(1), Rect::new(0., 39., 1800., 1050.));
    state.tick();

    assert_eq!(frame(&state, a), Rect::new(16., 55., 876., 1050. - 32.));
}

// macOS Spaces.

#[test]
fn a_window_on_another_space_is_left_alone() {
    let mut backend = MockBackend::with_laptop_display();
    let here = backend.open_window("here", "here", Rect::new(300., 200., 640., 480.));
    let elsewhere_frame = Rect::new(100., 100., 700., 500.);
    let elsewhere = backend.open_window_with("elsewhere", "elsewhere", elsewhere_frame, |info| {
        info.on_other_space = true;
    });
    let mut state = State::new(backend, default_config());

    // Only the window on this Space is tiled; the other keeps its frame and takes no room.
    assert_eq!(state.layout().windows().count(), 1);
    assert_eq!(state.backend().frame(elsewhere), elsewhere_frame);
    let alone = state.backend().frame(here);

    // A window that opens over there is not tiled either.
    let later = state
        .backend_mut()
        .open_window_with("later", "later", elsewhere_frame, |info| {
            info.on_other_space = true
        });
    state.tick();
    assert_eq!(state.layout().windows().count(), 1);
    assert_eq!(state.backend().frame(later), elsewhere_frame);
    assert_eq!(state.backend().frame(here), alone);
}

#[test]
fn a_window_joins_when_it_is_brought_to_this_space_and_leaves_when_it_is_taken_away() {
    let mut backend = MockBackend::with_laptop_display();
    let here = backend.open_window("here", "here", Rect::new(300., 200., 640., 480.));
    let mover = backend.open_window_with(
        "mover",
        "mover",
        Rect::new(100., 100., 700., 500.),
        |info| info.on_other_space = true,
    );
    let mut state = State::new(backend, default_config());
    let alone = state.backend().frame(here);

    state.backend_mut().set_on_other_space(mover, false);
    state.tick();
    assert_eq!(state.layout().windows().count(), 2);
    // It became a column next to the one that was there.
    assert_eq!(
        state.backend().frame(mover).x,
        state.backend().frame(here).max_x() + 16.
    );

    state.backend_mut().set_on_other_space(mover, true);
    state.tick();
    assert_eq!(state.layout().windows().count(), 1);
    assert_eq!(state.backend().frame(here), alone);
}

#[test]
fn nothing_is_focused_while_another_space_is_showing() {
    let mut backend = MockBackend::with_laptop_display();
    let a = backend.open_window("a", "a", Rect::new(300., 200., 640., 480.));
    let b = backend.open_window("b", "b", Rect::new(300., 200., 640., 480.));
    let mut state = State::new(backend, default_config());
    state.do_action(Action::FocusColumnFirst);
    assert_eq!(state.backend().focused_window(), Some(a));
    assert!(!state.is_dormant());

    // The user goes to another Space and clicks a window there.
    state.backend_mut().set_home_space_active(false);
    let other = state.backend_mut().open_window_with(
        "other",
        "other",
        Rect::new(100., 100., 700., 500.),
        |info| info.on_other_space = true,
    );
    state.backend_mut().user_focus(other);
    state.tick();
    assert!(state.is_dormant());

    // Whatever the layout does meanwhile, it must not take focus back: macOS would switch
    // Spaces to follow it.
    state.do_action(Action::FocusColumnRight);
    assert_eq!(state.backend().focused_window(), Some(other));

    // Back on the managed Space, the keys work again.
    state.backend_mut().set_home_space_active(true);
    state.backend_mut().user_focus(a);
    state.tick();
    state.do_action(Action::FocusColumnRight);
    assert_eq!(state.backend().focused_window(), Some(b));
}
