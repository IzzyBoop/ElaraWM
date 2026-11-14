//! Moving and resizing windows by dragging them with the mod key held.

use std::thread::sleep;
use std::time::Duration;

use elara::config::default_config;
use elara::pointer::{PointerHandler, PointerInput};
use elara::State;
use elara_macos::{
    KeyModifiers, MockBackend, MouseButton, MouseEvent, MouseEventKind, Point, Rect, WindowBackend,
    WindowId,
};
use niri_config::Action;

const SCREEN: Rect = Rect::new(0., 0., 1800., 1169.);
const GAP: f64 = 16.;

const MOD: KeyModifiers = KeyModifiers {
    control: false,
    shift: false,
    option: true,
    command: false,
};
const NONE: KeyModifiers = KeyModifiers {
    control: false,
    shift: false,
    option: false,
    command: false,
};

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

fn point(x: f64, y: f64) -> Point {
    Point { x, y }
}

fn center(frame: Rect) -> Point {
    point(frame.x + frame.w / 2., frame.y + frame.h / 2.)
}

/// Two half-width columns side by side, the first focused.
fn two_columns() -> (State<MockBackend>, [WindowId; 2]) {
    let mut state = State::new(MockBackend::with_laptop_display(), default_config());
    let a = open(&mut state, "a");
    let b = open(&mut state, "b");
    state.do_action(Action::FocusColumnFirst);
    assert_eq!(frame(&state, a).x, GAP);
    assert_eq!(frame(&state, b).x, frame(&state, a).max_x() + GAP);
    (state, [a, b])
}

fn press(state: &mut State<MockBackend>, button: MouseButton, position: Point) {
    state.pointer_input(PointerInput::Press {
        button,
        position,
        mod_down: true,
    });
    state.tick();
}

/// Moves the pointer from `from` to `to` in steps, as a mouse reports a drag.
fn drag(state: &mut State<MockBackend>, from: Point, to: Point) {
    const STEPS: u32 = 20;
    for step in 1..=STEPS {
        let t = f64::from(step) / f64::from(STEPS);
        let position = point(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t);
        state.pointer_input(PointerInput::Motion { position });
        state.tick();
    }
}

fn release(state: &mut State<MockBackend>, button: MouseButton) {
    state.pointer_input(PointerInput::Release { button });
    state.tick();
}

fn mouse(kind: MouseEventKind, position: Point, modifiers: KeyModifiers) -> MouseEvent {
    MouseEvent {
        kind,
        position,
        modifiers,
    }
}

// What the tap lets through.

#[test]
fn a_press_without_the_mod_key_is_left_alone() {
    let config = default_config();
    let mut handler = PointerHandler::new();
    let at = point(100., 100.);

    let press = mouse(MouseEventKind::Pressed(MouseButton::Left), at, NONE);
    let response = handler.handle(&config, &press, |_, _, mod_down| mod_down);
    assert!(!response.swallow);
    assert!(response.inputs.is_empty());

    // Nor is the drag and release that follow it.
    let drag = mouse(MouseEventKind::Dragged, at, NONE);
    assert!(
        !handler
            .handle(&config, &drag, |_, _, mod_down| mod_down)
            .swallow
    );
    let release = mouse(MouseEventKind::Released(MouseButton::Left), at, NONE);
    assert!(
        !handler
            .handle(&config, &release, |_, _, mod_down| mod_down)
            .swallow
    );
}

#[test]
fn a_mod_press_away_from_any_window_is_left_alone() {
    let config = default_config();
    let mut handler = PointerHandler::new();
    let press = mouse(
        MouseEventKind::Pressed(MouseButton::Left),
        point(5., 5.),
        MOD,
    );
    let response = handler.handle(&config, &press, |_, _, _| false);
    assert!(!response.swallow);
    assert!(response.inputs.is_empty());
}

#[test]
fn a_mod_press_on_a_window_takes_everything_until_the_release() {
    let config = default_config();
    let mut handler = PointerHandler::new();
    let at = point(100., 100.);
    let there = point(300., 100.);

    let press = mouse(MouseEventKind::Pressed(MouseButton::Left), at, MOD);
    let response = handler.handle(&config, &press, |_, _, mod_down| mod_down);
    assert!(response.swallow);
    assert_eq!(
        response.inputs,
        [PointerInput::Press {
            button: MouseButton::Left,
            position: at,
            mod_down: true
        }]
    );

    // The mod key may be let go mid-drag: the application never saw the button go down, so
    // it must not see the rest either.
    let drag = mouse(MouseEventKind::Dragged, there, NONE);
    let response = handler.handle(&config, &drag, |_, _, _| panic!("not a press"));
    assert!(response.swallow);
    assert_eq!(response.inputs, [PointerInput::Motion { position: there }]);

    // The other button, pressed and released during the drag.
    let other = mouse(MouseEventKind::Pressed(MouseButton::Right), there, NONE);
    let response = handler.handle(&config, &other, |_, _, _| panic!("already grabbed"));
    assert!(response.swallow);
    assert_eq!(
        response.inputs,
        [PointerInput::Press {
            button: MouseButton::Right,
            position: there,
            mod_down: false
        }]
    );
    let other = mouse(MouseEventKind::Released(MouseButton::Right), there, NONE);
    let response = handler.handle(&config, &other, |_, _, mod_down| mod_down);
    assert!(response.swallow);
    assert!(response.inputs.is_empty());

    let release = mouse(MouseEventKind::Released(MouseButton::Left), there, NONE);
    let response = handler.handle(&config, &release, |_, _, mod_down| mod_down);
    assert!(response.swallow);
    assert_eq!(
        response.inputs,
        [PointerInput::Release {
            button: MouseButton::Left
        }]
    );

    // And then the mouse is the applications' again.
    let press = mouse(MouseEventKind::Pressed(MouseButton::Left), there, NONE);
    assert!(
        !handler
            .handle(&config, &press, |_, _, mod_down| mod_down)
            .swallow
    );
}

#[test]
fn a_press_whose_release_went_missing_starts_over() {
    let config = default_config();
    let mut handler = PointerHandler::new();
    let at = point(100., 100.);

    let press = mouse(MouseEventKind::Pressed(MouseButton::Left), at, MOD);
    handler.handle(&config, &press, |_, _, mod_down| mod_down);

    let response = handler.handle(&config, &press, |_, _, mod_down| mod_down);
    assert!(response.swallow);
    assert_eq!(
        response.inputs,
        [
            PointerInput::Release {
                button: MouseButton::Left
            },
            PointerInput::Press {
                button: MouseButton::Left,
                position: at,
                mod_down: true
            }
        ]
    );
}

#[test]
fn the_layout_says_what_can_be_grabbed() {
    let (state, [a, _b]) = two_columns();
    let on_a = center(frame(&state, a));

    assert!(state.can_grab(MouseButton::Left, on_a));
    // As in niri, the middle of a window is no edge to resize by: that press goes to the
    // application. A corner is.
    assert!(!state.can_grab(MouseButton::Right, on_a));
    let corner = point(frame(&state, a).x + 10., frame(&state, a).y + 10.);
    assert!(state.can_grab(MouseButton::Right, corner));
    // The menu bar.
    assert!(!state.can_grab(MouseButton::Left, point(900., 10.)));
    // Off every display.
    assert!(!state.can_grab(MouseButton::Left, point(-50., 300.)));
}

// Moving.

#[test]
fn a_mod_click_focuses_the_window_and_moves_nothing() {
    let (mut state, [a, b]) = two_columns();
    let before = [frame(&state, a), frame(&state, b)];
    assert_eq!(state.backend().focused_window(), Some(a));

    let on_b = center(frame(&state, b));
    press(&mut state, MouseButton::Left, on_b);
    release(&mut state, MouseButton::Left);

    assert_eq!(state.backend().focused_window(), Some(b));
    assert_eq!([frame(&state, a), frame(&state, b)], before);
}

#[test]
fn a_short_drag_tugs_a_tiled_window_and_lets_it_spring_back() {
    let (mut state, [a, b]) = two_columns();
    let before = [frame(&state, a), frame(&state, b)];

    let from = center(frame(&state, a));
    let to = point(from.x + 100., from.y);
    press(&mut state, MouseButton::Left, from);
    drag(&mut state, from, to);

    // The window gives a little, less than the pointer moved, and stays in its column.
    let tugged = frame(&state, a).x - before[0].x;
    assert!(tugged > 0. && tugged < 100., "{tugged}");
    assert_eq!(frame(&state, b), before[1]);

    release(&mut state, MouseButton::Left);
    assert_eq!([frame(&state, a), frame(&state, b)], before);
}

#[test]
fn a_dragged_tiled_window_follows_the_pointer() {
    let (mut state, [a, b]) = two_columns();
    let a_before = frame(&state, a);

    let from = center(a_before);
    let to = point(from.x + 400., from.y + 150.);
    press(&mut state, MouseButton::Left, from);
    drag(&mut state, from, to);

    // The pointer is still over the middle of the window.
    let a_now = frame(&state, a);
    assert_eq!(center(a_now), to);
    assert_eq!((a_now.w, a_now.h), (a_before.w, a_before.h));
    // The column it left closed up.
    assert_eq!(frame(&state, b).x, GAP);

    release(&mut state, MouseButton::Left);
}

#[test]
fn dropping_a_window_on_the_far_side_of_another_swaps_the_columns() {
    let (mut state, [a, b]) = two_columns();
    let [a_before, b_before] = [frame(&state, a), frame(&state, b)];

    // Drop `a` near the right edge of the screen, to the right of `b`.
    let from = center(a_before);
    let to = point(SCREEN.max_x() - 40., from.y);
    press(&mut state, MouseButton::Left, from);
    drag(&mut state, from, to);
    release(&mut state, MouseButton::Left);

    assert_eq!(frame(&state, b), a_before);
    assert_eq!(frame(&state, a), b_before);
    assert_eq!(state.backend().focused_window(), Some(a));
}

#[test]
fn the_other_button_floats_a_window_being_moved() {
    let (mut state, [a, b]) = two_columns();
    let a_before = frame(&state, a);

    let from = center(a_before);
    let to = point(from.x + 400., from.y + 100.);
    press(&mut state, MouseButton::Left, from);
    drag(&mut state, from, to);
    press(&mut state, MouseButton::Right, to);
    release(&mut state, MouseButton::Left);

    // It stays where it was dropped, on top of the one column that is left.
    assert!(state.layout().windows().any(|(_, w)| w.window_id() == a));
    let a_now = frame(&state, a);
    assert_eq!(center(a_now).y, to.y);
    assert_eq!(frame(&state, b).x, GAP);

    // And it can be dragged on from there, without the tug a tiled window gives first.
    let from = center(a_now);
    let to = point(from.x - 200., from.y + 50.);
    press(&mut state, MouseButton::Left, from);
    drag(&mut state, from, to);
    release(&mut state, MouseButton::Left);
    assert_eq!(center(frame(&state, a)), to);
    assert_eq!(frame(&state, b).x, GAP);
}

#[test]
fn a_window_that_closes_mid_drag_ends_the_drag() {
    let (mut state, [a, b]) = two_columns();
    let from = center(frame(&state, a));
    let to = point(from.x + 400., from.y);
    press(&mut state, MouseButton::Left, from);
    drag(&mut state, from, to);

    state.backend_mut().close_window(a);
    state.tick();
    drag(&mut state, to, from);
    release(&mut state, MouseButton::Left);

    assert_eq!(frame(&state, b).x, GAP);
    assert_eq!(state.backend().focused_window(), Some(b));
}

// Resizing.

#[test]
fn dragging_the_right_edge_resizes_the_column() {
    let (mut state, [a, b]) = two_columns();
    let a_before = frame(&state, a);

    // The right third of the window, vertically in the middle.
    let from = point(a_before.max_x() - 20., a_before.y + a_before.h / 2.);
    let to = point(from.x - 200., from.y);
    press(&mut state, MouseButton::Right, from);
    drag(&mut state, from, to);

    let a_now = frame(&state, a);
    assert_eq!(a_now.w, a_before.w - 200.);
    assert_eq!(a_now.x, a_before.x);
    assert_eq!(a_now.h, a_before.h);
    // The next column follows.
    assert_eq!(frame(&state, b).x, a_now.max_x() + GAP);

    release(&mut state, MouseButton::Right);
    assert_eq!(frame(&state, a), a_now);
    assert_eq!(state.backend().focused_window(), Some(a));
}

#[test]
fn two_quick_presses_on_the_side_toggle_full_width() {
    let (mut state, [a, _b]) = two_columns();
    let a_before = frame(&state, a);
    let side = point(a_before.max_x() - 20., a_before.y + a_before.h / 2.);

    press(&mut state, MouseButton::Right, side);
    release(&mut state, MouseButton::Right);
    assert_eq!(frame(&state, a), a_before);

    press(&mut state, MouseButton::Right, side);
    release(&mut state, MouseButton::Right);
    assert_eq!(frame(&state, a).w, SCREEN.w - 2. * GAP);

    // Too slow to be a double click: a plain press again.
    sleep(Duration::from_millis(450));
    let wide = frame(&state, a);
    let side = point(wide.max_x() - 20., wide.y + wide.h / 2.);
    press(&mut state, MouseButton::Right, side);
    release(&mut state, MouseButton::Right);
    assert_eq!(frame(&state, a), wide);
}

#[test]
fn dragging_the_top_left_corner_of_a_floating_window_keeps_the_opposite_corner_in_place() {
    let (mut state, [a, _b]) = two_columns();
    state.do_action(Action::ToggleWindowFloating);
    let before = frame(&state, a);
    state.backend_mut().take_frame_requests();

    let from = point(before.x + 10., before.y + 10.);
    let to = point(from.x + 60., from.y + 40.);
    press(&mut state, MouseButton::Right, from);
    drag(&mut state, from, to);
    release(&mut state, MouseButton::Right);

    let now = frame(&state, a);
    assert_eq!((now.w, now.h), (before.w - 60., before.h - 40.));
    assert_eq!((now.max_x(), now.max_y()), (before.max_x(), before.max_y()));

    // At no point along the way did the far corner move: each step asked for the size and
    // the position together.
    for (id, request) in state.backend_mut().take_frame_requests() {
        if id == a {
            assert_eq!(
                (request.max_x(), request.max_y()),
                (before.max_x(), before.max_y()),
                "{request:?}"
            );
        }
    }
}
