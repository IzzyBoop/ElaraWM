//! The strip and the workspaces gliding into place.

use std::thread::sleep;
use std::time::{Duration, Instant};

use elara::config::default_config;
use elara::State;
use elara_macos::{MockBackend, Rect, WindowId};
use niri_config::{Action, Config};

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

/// Three half-width columns with the last focused, and animations on.
fn animated(config: Config) -> (State<MockBackend>, [WindowId; 3]) {
    let mut state = State::new(MockBackend::with_laptop_display(), config);
    let a = open(&mut state, "a");
    let b = open(&mut state, "b");
    let c = open(&mut state, "c");
    state.enable_animations();
    (state, [a, b, c])
}

/// Ticks once per frame until nothing is moving. Returns every x position `id` had on the way.
fn settle(state: &mut State<MockBackend>, id: WindowId) -> Vec<f64> {
    let started = Instant::now();
    let mut seen = vec![frame(state, id).x];
    while state.is_animating() {
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "animation did not finish"
        );
        sleep(Duration::from_millis(8));
        state.tick();
        let x = frame(state, id).x;
        if seen.last() != Some(&x) {
            seen.push(x);
        }
    }
    seen
}

#[test]
fn focusing_a_column_out_of_view_glides_the_strip() {
    let (mut state, [a, b, _c]) = animated(default_config());
    let start = frame(&state, b).x;

    state.do_action(Action::FocusColumnFirst);
    assert!(state.is_animating());
    let path = settle(&mut state, b);

    // `b` went from the left half to the right half through positions in between, always
    // moving the same way.
    assert_eq!(path.first(), Some(&start));
    assert_eq!(path.last(), Some(&(GAP + 876. + GAP)));
    assert!(path.len() > 4, "expected a glide, got {path:?}");
    assert!(path.windows(2).all(|pair| pair[0] < pair[1]), "{path:?}");
    assert_eq!(frame(&state, a).x, GAP);
}

#[test]
fn a_released_gesture_glides_to_rest() {
    let (mut state, [a, b, _c]) = animated(default_config());
    state.do_action(Action::FocusColumnFirst);
    settle(&mut state, b);

    // Drag the strip a third of a column to the left, slowly, and let go.
    for _ in 0..6 {
        state.gesture_update(-30., 0.);
        state.tick();
        sleep(Duration::from_millis(40));
    }
    let released_at = frame(&state, b).x;
    state.gesture_end();
    state.tick();
    let path = settle(&mut state, b);

    // It did not jump: it left from where the fingers let go and came to rest on a column
    // edge, through positions in between.
    assert_eq!(path.first(), Some(&released_at));
    assert!(path.len() > 3, "expected a glide, got {path:?}");
    let rest = frame(&state, b).x;
    assert!(
        rest == GAP + 876. + GAP || rest == GAP,
        "came to rest off a column edge at {rest}"
    );
    let _ = a;
}

/// Ticks once per frame until nothing is moving, recording where each window was after every
/// frame.
fn record(state: &mut State<MockBackend>, ids: &[WindowId]) -> Vec<Vec<Rect>> {
    let started = Instant::now();
    let mut frames = vec![ids.iter().map(|id| frame(state, *id)).collect()];
    while state.is_animating() {
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "animation did not finish"
        );
        sleep(Duration::from_millis(8));
        state.tick();
        frames.push(ids.iter().map(|id| frame(state, *id)).collect());
    }
    frames
}

fn is_parked(frame: Rect) -> bool {
    frame.x == SCREEN.max_x() - 1.
}

#[test]
fn switching_to_the_workspace_below_slides_it_in_from_the_bottom() {
    let (mut state, [_a, _b, c]) = animated(default_config());
    state.do_action(Action::FocusWorkspaceDown);
    settle(&mut state, c);
    let d = open(&mut state, "d");
    state.do_action(Action::FocusWorkspaceUp);
    settle(&mut state, c);

    state.do_action(Action::FocusWorkspaceDown);
    let frames = record(&mut state, &[c, d]);

    // The arriving window comes up from the bottom of the screen to its place.
    let mut arriving: Vec<f64> = frames
        .iter()
        .map(|f| f[1])
        .filter(|f| !is_parked(*f))
        .map(|f| f.y)
        .collect();
    arriving.dedup();
    assert!(arriving.len() > 4, "expected a slide, got {arriving:?}");
    assert!(
        arriving.windows(2).all(|pair| pair[0] > pair[1]),
        "{arriving:?}"
    );
    assert_eq!(arriving.last(), Some(&55.));

    // The leaving window would have to go up past the menu bar, which macOS does not allow.
    // It must not be left stuck under the menu bar while the other one slides in: once its
    // place is above the top, it is hidden.
    let leaving: Vec<Rect> = frames.iter().map(|f| f[0]).collect();
    let stuck = leaving
        .iter()
        .filter(|f| !is_parked(**f) && f.y == 39.)
        .count();
    assert!(
        stuck <= 1,
        "the leaving window sat under the menu bar for {stuck} frames"
    );
    assert!(is_parked(*leaving.last().unwrap()));
}

#[test]
fn switching_to_the_workspace_above_slides_the_old_one_out_at_the_bottom() {
    let (mut state, [_a, _b, c]) = animated(default_config());
    state.do_action(Action::FocusWorkspaceDown);
    settle(&mut state, c);
    let d = open(&mut state, "d");

    state.do_action(Action::FocusWorkspaceUp);
    let frames = record(&mut state, &[c, d]);

    // The leaving window goes down and off the bottom.
    let mut leaving: Vec<f64> = frames
        .iter()
        .map(|f| f[1])
        .filter(|f| !is_parked(*f))
        .map(|f| f.y)
        .collect();
    leaving.dedup();
    assert!(leaving.len() > 4, "expected a slide, got {leaving:?}");
    assert!(
        leaving.windows(2).all(|pair| pair[0] < pair[1]),
        "{leaving:?}"
    );

    // The arriving one comes from above the top, where it cannot be shown. It appears once
    // its place is below the menu bar, which is the last few points of its way down, and is
    // never shown stuck anywhere else.
    for f in frames.iter().map(|f| f[0]).filter(|f| !is_parked(*f)) {
        assert!(
            (39. ..=55.).contains(&f.y),
            "the arriving window was shown out of place at {f:?}"
        );
    }
    assert_eq!(frame(&state, c).y, 55.);
}

#[test]
fn opening_and_resizing_still_take_effect_at_once() {
    let (mut state, [_a, _b, c]) = animated(default_config());

    state.do_action(Action::SwitchPresetColumnWidth);
    let wide = frame(&state, c).w;
    assert!(wide > 876.);
    // The size is final at once. Only the view may still be gliding to keep the column in
    // sight.
    settle(&mut state, c);
    assert_eq!(frame(&state, c).w, wide);

    let d = open(&mut state, "d");
    assert_eq!(frame(&state, d).w, 876.);
}

#[test]
fn niris_own_switch_turns_the_glide_off() {
    let config = Config::parse_mem("animations {\n off\n}").unwrap();
    let (mut state, [a, b, _c]) = animated(config);

    state.do_action(Action::FocusColumnFirst);

    assert!(!state.is_animating());
    assert_eq!(frame(&state, a).x, GAP);
    assert_eq!(frame(&state, b).x, GAP + 876. + GAP);
}

#[test]
fn without_enabling_animations_nothing_glides() {
    let mut state = State::new(MockBackend::with_laptop_display(), default_config());
    let a = open(&mut state, "a");
    open(&mut state, "b");
    open(&mut state, "c");

    state.do_action(Action::FocusColumnFirst);

    assert!(!state.is_animating());
    assert_eq!(frame(&state, a).x, GAP);
}
