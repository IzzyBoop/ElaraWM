//! Dragging the strip of columns and the stack of workspaces with two fingers.

use std::thread::sleep;
use std::time::Duration;

use elara::config::default_config;
use elara::State;
use elara_macos::{MockBackend, Rect, WindowBackend, WindowId};
use niri_config::Action;

const SCREEN: Rect = Rect::new(0., 0., 1800., 1169.);
const GAP: f64 = 16.;

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

fn is_parked(state: &State<MockBackend>, id: WindowId) -> bool {
    let frame = frame(state, id);
    frame.x == SCREEN.max_x() - 1. && frame.y == SCREEN.max_y() - 1.
}

/// Three half-width columns with the first focused: `a` and `b` in view, `c` out of view on
/// the right.
fn three_columns() -> (State<MockBackend>, [WindowId; 3]) {
    let mut state = State::new(MockBackend::with_laptop_display(), default_config());
    let a = open(&mut state, "a");
    let b = open(&mut state, "b");
    let c = open(&mut state, "c");
    state.do_action(Action::FocusColumnFirst);
    assert_eq!(frame(&state, a).x, GAP);
    assert!(is_parked(&state, c));
    (state, [a, b, c])
}

/// Moves the fingers in steps, a few milliseconds apart as a trackpad reports them.
fn drag(state: &mut State<MockBackend>, delta_x: f64, delta_y: f64, steps: u32) {
    for _ in 0..steps {
        state.gesture_update(delta_x / f64::from(steps), delta_y / f64::from(steps));
        state.tick();
        sleep(Duration::from_millis(4));
    }
}

#[test]
fn dragging_left_brings_columns_in_from_the_right() {
    let (mut state, [a, b, c]) = three_columns();
    let b_before = frame(&state, b).x;

    // Content moves left with the fingers.
    drag(&mut state, -300., 0., 10);

    // Mid-gesture the windows sit wherever the fingers put them, not on a column boundary.
    assert!(frame(&state, b).x < b_before, "{:?}", frame(&state, b));
    assert!(!is_parked(&state, c));
    assert!(frame(&state, c).x < SCREEN.max_x());
    // Columns stay in order with their gap.
    assert_eq!(frame(&state, c).x, frame(&state, b).max_x() + GAP);

    // Keep going until `a` has left the screen entirely.
    drag(&mut state, -700., 0., 10);
    assert!(is_parked(&state, a));

    // Wait out the fingers' speed so that lifting them does not fling the strip further.
    sleep(Duration::from_millis(200));
    state.gesture_end();
    state.tick();

    // The strip settled with a column edge aligned, and the layout picked a column in view.
    let focused = state.backend().focused_window().unwrap();
    assert!([b, c].contains(&focused));
    assert_eq!(frame(&state, c).max_x(), SCREEN.max_x() - GAP);
    assert_eq!(frame(&state, b).x, GAP);
    assert!(is_parked(&state, a));
}

#[test]
fn dragging_right_goes_back() {
    let (mut state, [a, b, c]) = three_columns();
    state.do_action(Action::FocusColumnLast);
    assert!(is_parked(&state, a));

    drag(&mut state, 900., 0., 10);
    sleep(Duration::from_millis(200));
    state.gesture_end();
    state.tick();

    assert_eq!(frame(&state, a).x, GAP);
    assert!(!is_parked(&state, b));
    assert!(is_parked(&state, c));
}

#[test]
fn dragging_up_switches_to_the_next_workspace() {
    let (mut state, [a, b, _c]) = three_columns();
    state.do_action(Action::FocusWorkspaceDown);
    let d = open(&mut state, "d");
    state.do_action(Action::FocusWorkspaceUp);
    assert!(is_parked(&state, d));
    assert!(!is_parked(&state, a));

    // Content moves up with the fingers, revealing the workspace below.
    drag(&mut state, 0., -700., 10);
    sleep(Duration::from_millis(200));
    state.gesture_end();
    state.tick();

    assert!(!is_parked(&state, d));
    assert!(is_parked(&state, a) && is_parked(&state, b));
    assert_eq!(state.backend().focused_window(), Some(d));

    // And down again.
    drag(&mut state, 0., 700., 10);
    sleep(Duration::from_millis(200));
    state.gesture_end();
    state.tick();
    assert!(!is_parked(&state, a));
    assert!(is_parked(&state, d));
}

#[test]
fn a_small_movement_changes_nothing_once_released() {
    let (mut state, [a, b, c]) = three_columns();
    let before = [frame(&state, a), frame(&state, b), frame(&state, c)];

    // Slowly: niri carries the fingers' speed past the point of release, so a quick flick of
    // the same length would go on to the next column.
    for _ in 0..4 {
        state.gesture_update(-10., 1.);
        state.tick();
        sleep(Duration::from_millis(60));
    }
    state.gesture_end();
    state.tick();

    assert_eq!(
        [frame(&state, a), frame(&state, b), frame(&state, c)],
        before
    );
}

#[test]
fn ending_without_a_gesture_is_harmless() {
    let (mut state, [a, _b, _c]) = three_columns();
    let before = frame(&state, a);

    state.gesture_end();
    state.tick();

    assert_eq!(frame(&state, a), before);
}
