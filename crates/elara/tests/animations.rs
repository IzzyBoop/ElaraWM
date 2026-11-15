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

#[test]
fn moving_a_column_glides_it_past_its_neighbor() {
    let (mut state, [a, b, _c]) = animated(default_config());
    state.do_action(Action::FocusColumnFirst);
    settle(&mut state, b);
    let (left, right) = (frame(&state, a).x, frame(&state, b).x);
    assert!(left < right);

    // `a` and `b` trade places: niri's window-movement animation.
    state.do_action(Action::MoveColumnRight);
    state.tick();
    assert!(state.is_animating());
    let mut path_a = vec![frame(&state, a).x];
    let mut path_b = vec![frame(&state, b).x];
    let started = Instant::now();
    while state.is_animating() {
        assert!(started.elapsed() < Duration::from_secs(5));
        sleep(Duration::from_millis(8));
        state.tick();
        path_a.push(frame(&state, a).x);
        path_b.push(frame(&state, b).x);
    }

    assert_eq!(frame(&state, a).x, right);
    assert_eq!(frame(&state, b).x, left);
    // Both went through places in between, not there in one step.
    for path in [&path_a, &path_b] {
        let between = path.iter().filter(|x| **x > left && **x < right).count();
        assert!(between > 4, "expected a glide, got {path:?}");
    }
}

// Pictures of windows where the windows cannot be.

/// Switches from a workspace holding `d` to the one above it, holding `c`, and returns what
/// was shown of `c` on every frame: its own frame, and the picture of it, if any.
fn arrive_from_above(config: Config) -> Vec<(Rect, Option<elara_macos::Ghost>)> {
    let (mut state, [_a, _b, c]) = animated(config);
    state.do_action(Action::FocusWorkspaceDown);
    settle(&mut state, c);
    let _d = open(&mut state, "d");

    state.do_action(Action::FocusWorkspaceUp);
    let started = Instant::now();
    let mut shown = Vec::new();
    while state.is_animating() {
        assert!(started.elapsed() < Duration::from_secs(5));
        sleep(Duration::from_millis(8));
        state.tick();
        let ghost = state
            .backend()
            .ghosts()
            .iter()
            .find(|ghost| ghost.window == c);
        shown.push((frame(&state, c), ghost.copied()));
    }
    assert!(state.backend().ghosts().is_empty());
    shown
}

#[test]
fn a_window_arriving_from_above_is_shown_as_a_picture_until_it_can_be_there() {
    let config = Config::parse_mem("overview {\n window-thumbnails\n}").unwrap();
    let shown = arrive_from_above(config);

    // While the window is kept out of sight, its picture slides down from under the menu
    // bar, clipped there.
    let pictures: Vec<_> = shown
        .iter()
        .filter_map(|(frame, ghost)| Some((frame, (*ghost)?)))
        .collect();
    assert!(pictures.len() > 4, "expected a slide, got {shown:?}");
    for (frame, ghost) in &pictures {
        assert!(is_parked(**frame), "picture and window both showing");
        assert!(ghost.rect.y < 39. && ghost.rect.max_y() > 39., "{ghost:?}");
        assert_eq!(ghost.clip, Rect::new(0., 39., SCREEN.w, SCREEN.h - 39.));
    }
    let tops: Vec<f64> = pictures.iter().map(|(_, ghost)| ghost.rect.y).collect();
    assert!(tops.windows(2).all(|pair| pair[0] <= pair[1]), "{tops:?}");

    // Once the window is there itself, the picture is gone, and it does not come back.
    let first_real = shown.iter().position(|(frame, _)| !is_parked(*frame));
    let first_real = first_real.expect("the window never arrived");
    assert!(shown[first_real..].iter().all(|(_, ghost)| ghost.is_none()));
    // No frame in between with neither, once the picture has started.
    let first_picture = shown.iter().position(|(_, ghost)| ghost.is_some()).unwrap();
    for (frame, ghost) in &shown[first_picture..] {
        assert!(ghost.is_some() || !is_parked(*frame));
    }
}

#[test]
fn no_pictures_are_shown_unless_the_config_asks_for_them() {
    let shown = arrive_from_above(default_config());
    assert!(shown.iter().all(|(_, ghost)| ghost.is_none()));
}
