//! The daemon's core loop against an in-memory system: windows come and go, and niri's layout
//! decides where they sit.

use elara::State;
use elara_macos::{
    DisplayId, DisplayInfo, MockBackend, Rect, Size, WindowBackend, WindowId, WindowKind,
};
use niri_config::{Action, Config};

/// The display `MockBackend::with_laptop_display()` creates.
const SCREEN: Rect = Rect::new(0., 0., 1800., 1169.);
/// What its menu bar leaves free.
const USABLE: Rect = Rect::new(0., 39., 1800., 1130.);

// niri's default config: 16 px gaps and new columns half the width of the working area.
const GAP: f64 = 16.;
const COLUMN_W: f64 = (USABLE.w - GAP) * 0.5 - GAP;
const COLUMN_H: f64 = USABLE.h - GAP * 2.;

fn config(extra: &str) -> Config {
    let text = format!(
        "{}\n{extra}",
        include_str!("../../../resources/default-config.kdl")
    );
    Config::parse_mem(&text).expect("config must parse")
}

fn start(backend: MockBackend) -> State<MockBackend> {
    State::new(backend, config(""))
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

/// Whether the window has been moved out of sight at the bottom-right corner of the display.
fn is_parked(state: &State<MockBackend>, id: WindowId) -> bool {
    let frame = frame(state, id);
    frame.x == SCREEN.max_x() - 1. && frame.y == SCREEN.max_y() - 1.
}

fn column(index: usize) -> Rect {
    Rect::new(
        GAP + (COLUMN_W + GAP) * index as f64,
        USABLE.y + GAP,
        COLUMN_W,
        COLUMN_H,
    )
}

#[test]
fn first_window_fills_the_left_column_below_the_menu_bar() {
    let mut state = start(MockBackend::with_laptop_display());
    let a = open(&mut state, "a");

    assert_eq!(frame(&state, a), column(0));
    assert_eq!(frame(&state, a), Rect::new(16., 55., 876., 1098.));
}

#[test]
fn new_windows_open_to_the_right_and_take_focus() {
    let mut state = start(MockBackend::with_laptop_display());
    let a = open(&mut state, "a");
    let b = open(&mut state, "b");

    assert_eq!(frame(&state, a), column(0));
    assert_eq!(frame(&state, b), column(1));
    // The layout focused the new window, and the system followed.
    assert_eq!(state.backend().focused_window(), Some(b));
    assert_eq!(state.layout().focus().map(|w| w.window_id()), Some(b));
}

#[test]
fn windows_already_open_keep_their_left_to_right_order() {
    let mut backend = MockBackend::with_laptop_display();
    let right = backend.open_window("right", "r", Rect::new(1000., 100., 500., 400.));
    let left = backend.open_window("left", "l", Rect::new(50., 300., 500., 400.));

    let state = start(backend);

    assert_eq!(frame(&state, left), column(0));
    assert_eq!(frame(&state, right), column(1));
}

#[test]
fn a_third_column_scrolls_the_view_and_parks_what_leaves_it() {
    let mut state = start(MockBackend::with_laptop_display());
    let a = open(&mut state, "a");
    let b = open(&mut state, "b");
    let c = open(&mut state, "c");

    // The strip is wider than the screen now. The view moved right just far enough to show
    // the new, focused column, so `b` and `c` are visible and `a` is out of view on the left.
    assert!(is_parked(&state, a));
    assert_eq!(frame(&state, c).max_x(), USABLE.max_x() - GAP);
    assert_eq!(frame(&state, b).max_x(), frame(&state, c).x - GAP);
    // Parked windows keep the size the layout gave them, ready to come back.
    assert_eq!(frame(&state, a).size(), column(0).size());

    // Clicking the hidden window is not possible, but focusing it some other way, such as
    // Cmd-Tab, scrolls back to it.
    state.backend_mut().user_focus(a);
    state.tick();
    assert_eq!(frame(&state, a), column(0));
    assert_eq!(frame(&state, b), column(1));
    assert!(is_parked(&state, c));
}

#[test]
fn closing_the_focused_window_focuses_the_one_niri_picks() {
    let mut state = start(MockBackend::with_laptop_display());
    let a = open(&mut state, "a");
    let b = open(&mut state, "b");
    let c = open(&mut state, "c");
    assert_eq!(state.backend().focused_window(), Some(c));

    state.backend_mut().close_window(c);
    state.tick();

    // The mock leaves nothing focused after a close, as if macOS had picked something
    // unmanaged. niri activates the column to the left of a closed last column.
    assert_eq!(state.layout().focus().map(|w| w.window_id()), Some(b));
    assert_eq!(state.backend().focused_window(), Some(b));
    assert_eq!(frame(&state, a), column(0));
    assert_eq!(frame(&state, b), column(1));
}

#[test]
fn a_window_that_refuses_its_size_pushes_its_neighbours() {
    let mut state = start(MockBackend::with_laptop_display());
    let a = state
        .backend_mut()
        .open_window("wide", "wide", Rect::new(0., 100., 1200., 700.));
    state
        .backend_mut()
        .set_size_limits(a, Size { w: 1000., h: 300. }, None);
    state.tick();
    let b = open(&mut state, "b");

    // `a` would not shrink to half the screen, so its column is as wide as the window is.
    assert_eq!(frame(&state, a).w, 1000.);
    assert_eq!(frame(&state, a).h, COLUMN_H);
    // `b` is focused and has to be fully visible, so the view scrolled; the gap between the
    // two columns is intact either way.
    assert_eq!(frame(&state, b).w, COLUMN_W);
    assert_eq!(frame(&state, b).max_x(), USABLE.max_x() - GAP);
    assert_eq!(frame(&state, a).max_x(), frame(&state, b).x - GAP);

    // The limit was learned from the refusal, so the window is not asked again and again.
    state.backend_mut().take_frame_requests();
    state.tick();
    state.tick();
    assert!(state.backend_mut().take_frame_requests().is_empty());
}

#[test]
fn dialogs_float_where_their_application_put_them() {
    let mut state = start(MockBackend::with_laptop_display());
    let a = open(&mut state, "a");

    let spot = Rect::new(500., 400., 420., 260.);
    let dialog = state
        .backend_mut()
        .open_window_with("a", "Save", spot, |info| info.kind = WindowKind::Dialog);
    state.tick();

    assert_eq!(frame(&state, dialog), spot);
    // The tiled window underneath is untouched.
    assert_eq!(frame(&state, a), column(0));

    // Dragging a floating window moves it for good.
    let dropped = Rect::new(900., 600., 420., 260.);
    state.backend_mut().user_set_frame(dialog, dropped);
    state.tick();
    state.tick();
    assert_eq!(frame(&state, dialog), dropped);
}

#[test]
fn windows_that_cannot_be_resized_float() {
    let mut state = start(MockBackend::with_laptop_display());
    let spot = Rect::new(700., 300., 300., 200.);
    let fixed = state
        .backend_mut()
        .open_window_with("calc", "Calculator", spot, |info| info.resizable = false);
    state.tick();

    assert_eq!(frame(&state, fixed), spot);
}

#[test]
fn a_dragged_tiled_window_is_put_back() {
    let mut state = start(MockBackend::with_laptop_display());
    let a = open(&mut state, "a");

    state
        .backend_mut()
        .user_set_frame(a, Rect::new(400., 300., COLUMN_W, COLUMN_H));
    state.tick();

    assert_eq!(frame(&state, a), column(0));
}

#[test]
fn minimizing_removes_a_window_from_the_strip_and_restoring_returns_it() {
    let mut state = start(MockBackend::with_laptop_display());
    let a = open(&mut state, "a");
    let b = open(&mut state, "b");

    state.backend_mut().set_minimized(a, true);
    state.tick();
    assert_eq!(state.layout().windows().count(), 1);
    // niri keeps the focused column where it is on screen when a column beside it goes away.
    assert_eq!(frame(&state, b), column(1));

    // A minimized window is left alone.
    let untouched = frame(&state, a);
    state.tick();
    assert_eq!(frame(&state, a), untouched);

    state.backend_mut().set_minimized(a, false);
    state.tick();
    assert_eq!(state.layout().windows().count(), 2);
    // It comes back as a new column to the right of the focused one, and is focused.
    assert_eq!(state.layout().focus().map(|w| w.window_id()), Some(a));
    assert_eq!(frame(&state, a).size(), column(0).size());
    assert_eq!(frame(&state, b).max_x(), frame(&state, a).x - GAP);
    assert!(SCREEN.intersects(&frame(&state, a)) && !is_parked(&state, a));
}

#[test]
fn native_fullscreen_windows_are_left_alone() {
    let mut state = start(MockBackend::with_laptop_display());
    let a = open(&mut state, "a");
    let b = open(&mut state, "b");

    state.backend_mut().set_fullscreen(b, true);
    state.backend_mut().user_set_frame(b, SCREEN);
    state.tick();

    assert_eq!(frame(&state, b), SCREEN);
    assert_eq!(frame(&state, a), column(0));
    assert_eq!(state.layout().windows().count(), 1);
}

#[test]
fn the_dock_appearing_shrinks_the_columns() {
    let mut state = start(MockBackend::with_laptop_display());
    let a = open(&mut state, "a");

    let mut display = state.backend().displays().remove(0);
    display.visible_frame = Rect::new(0., 39., 1800., 1050.);
    state.backend_mut().add_display(display);
    state.tick();

    assert_eq!(frame(&state, a), Rect::new(16., 55., 876., 1050. - 32.));
}

#[test]
fn each_display_has_its_own_strip() {
    let mut backend = MockBackend::with_laptop_display();
    backend.add_display(DisplayInfo {
        id: DisplayId(2),
        name: "External".to_owned(),
        frame: Rect::new(1800., 0., 2560., 1440.),
        visible_frame: Rect::new(1800., 25., 2560., 1415.),
        scale: 1.,
        refresh_mhz: Some(60_000),
        is_primary: false,
    });
    let laptop = backend.open_window("laptop", "l", Rect::new(100., 100., 500., 400.));
    let external = backend.open_window("external", "e", Rect::new(2500., 300., 500., 400.));

    let state = start(backend);

    assert_eq!(frame(&state, laptop), column(0));
    let expected_w = (2560. - GAP) * 0.5 - GAP;
    assert_eq!(
        frame(&state, external),
        Rect::new(1800. + GAP, 25. + GAP, expected_w, 1415. - GAP * 2.)
    );
}

#[test]
fn window_rules_from_the_config_apply() {
    let backend = MockBackend::with_laptop_display();
    let mut state = State::new(
        backend,
        config(
            r#"
            window-rule {
                match app-id="^com\\.example\\.notes$"
                open-floating true
            }
            window-rule {
                match app-id="browser"
                default-column-width { proportion 1.0; }
            }
            "#,
        ),
    );

    let spot = Rect::new(300., 200., 640., 480.);
    let notes = state
        .backend_mut()
        .open_window("com.example.notes", "Notes", spot);
    state.tick();
    assert_eq!(frame(&state, notes), spot);

    let browser = open(&mut state, "org.example.browser");
    assert_eq!(frame(&state, browser).w, USABLE.w - GAP * 2.);
}

#[test]
fn releasing_gives_every_window_its_original_frame_back() {
    let mut backend = MockBackend::with_laptop_display();
    let before_a = Rect::new(120., 90., 700., 500.);
    let a = backend.open_window("a", "a", before_a);
    let mut state = start(backend);

    let before_b = Rect::new(300., 200., 640., 480.);
    let b = open(&mut state, "b");
    let c = open(&mut state, "c");
    assert!(is_parked(&state, a));
    assert_ne!(frame(&state, b), before_b);

    state.release_windows();

    // Windows that were there before ElaraWM started, windows opened since, parked or not.
    assert_eq!(frame(&state, a), before_a);
    assert_eq!(frame(&state, b), before_b);
    assert_eq!(frame(&state, c), before_b);
}

#[test]
fn releasing_a_window_whose_display_is_gone_puts_it_on_a_remaining_one() {
    let mut backend = MockBackend::with_laptop_display();
    backend.add_display(DisplayInfo {
        id: DisplayId(2),
        name: "External".to_owned(),
        frame: Rect::new(1800., 0., 2560., 1440.),
        visible_frame: Rect::new(1800., 25., 2560., 1415.),
        scale: 1.,
        refresh_mhz: Some(60_000),
        is_primary: false,
    });
    let a = backend.open_window("a", "a", Rect::new(2500., 300., 500., 400.));
    let mut state = start(backend);

    state.backend_mut().remove_display(DisplayId(2));
    state.tick();
    state.release_windows();

    let restored = frame(&state, a);
    assert!(restored.x >= USABLE.x && restored.max_x() <= USABLE.max_x());
    assert!(restored.y >= USABLE.y && restored.max_y() <= USABLE.max_y());
    assert_eq!(restored.size(), Size { w: 500., h: 400. });
}

#[test]
fn a_settled_layout_sends_no_requests() {
    let mut state = start(MockBackend::with_laptop_display());
    open(&mut state, "a");
    open(&mut state, "b");
    open(&mut state, "c");
    state.backend_mut().take_frame_requests();

    for _ in 0..5 {
        state.tick();
    }

    assert!(state.backend_mut().take_frame_requests().is_empty());
}

#[test]
fn a_window_that_rounds_its_size_still_shares_a_column() {
    // A terminal takes only heights that fit whole rows of text. Asked for the height of the
    // screen it takes the nearest such height, which can be a little more. That is not the
    // smallest it can be: it must still give up half the column to a second window.
    let mut state = start(MockBackend::with_laptop_display());
    let other = open(&mut state, "other");
    let terminal = open(&mut state, "terminal");
    state.backend_mut().set_height_step(terminal, 17.);
    // Have it asked for its height again, now that it rounds.
    state.do_action(Action::FocusColumnLeft);
    state.do_action(Action::FocusColumnRight);
    state.do_action(Action::MaximizeColumn);
    state.do_action(Action::MaximizeColumn);
    let alone = frame(&state, terminal);
    assert_ne!(
        alone.h, 1098.,
        "the terminal should have rounded its height"
    );

    // Stack `terminal` under `other`.
    state.do_action(Action::ConsumeOrExpelWindowLeft);

    let (top, bottom) = (frame(&state, other), frame(&state, terminal));
    assert_eq!(top.x, bottom.x);
    // Both get about half, give or take a row of text.
    for frame in [top, bottom] {
        assert!((frame.h - 541.).abs() <= 17., "{top:?} {bottom:?}");
    }
    assert!(bottom.y >= top.max_y());
}

#[test]
fn a_window_with_a_minimum_close_to_what_was_asked_is_found_out() {
    // A window that will not go below 520 points, in a column where it is asked for 541 and
    // then, once a third window joins, for less. The first answer could be rounding; the
    // same answer to a different request is a limit.
    let mut state = start(MockBackend::with_laptop_display());
    let a = open(&mut state, "a");
    let stubborn = open(&mut state, "stubborn");
    let c = open(&mut state, "c");
    state
        .backend_mut()
        .set_size_limits(stubborn, elara_macos::Size { w: 100., h: 520. }, None);
    state.do_action(Action::FocusColumnFirst);
    state.do_action(Action::ConsumeWindowIntoColumn);
    state.do_action(Action::ConsumeWindowIntoColumn);

    // Three windows in one column: `stubborn` keeps its 520 and the others share the rest
    // without overlapping it.
    let frames = [frame(&state, a), frame(&state, stubborn), frame(&state, c)];
    assert_eq!(frames[1].h, 520., "{frames:?}");
    assert!(frames[0].max_y() <= frames[1].y, "{frames:?}");
    assert!(frames[1].max_y() <= frames[2].y, "{frames:?}");
    assert!(frames[2].max_y() <= SCREEN.max_y(), "{frames:?}");
}

// Resizing a tiled window by its edge, which the system does on its own.

#[test]
fn dragging_the_edge_of_a_tiled_window_resizes_its_column() {
    let mut state = start(MockBackend::with_laptop_display());
    let a = open(&mut state, "a");
    let b = open(&mut state, "b");
    state.do_action(Action::FocusColumnFirst);
    state.tick();
    let (was_a, was_b) = (frame(&state, a), frame(&state, b));

    // The user takes `a` by its right edge and pulls it 100 points wider, in two steps.
    state.system_press(true);
    for wider in [40., 100.] {
        let mut dragged = was_a;
        dragged.w += wider;
        state.backend_mut().user_set_frame(a, dragged);
        state.tick();
        // It is left as the user has it, and its neighbor makes room as it goes.
        assert_eq!(frame(&state, a), dragged);
        assert_eq!(frame(&state, b).x, was_b.x + wider);
    }
    state.system_press(false);
    state.tick();
    assert_eq!(frame(&state, a).w, was_a.w + 100.);
    assert_eq!(frame(&state, a).x, was_a.x);
    assert_eq!(frame(&state, b).x, was_b.x + 100.);

    // By the left edge: the window keeps the width and goes back to where its column is.
    state.system_press(true);
    let mut dragged = frame(&state, a);
    dragged.x -= 10.;
    dragged.w += 10.;
    state.backend_mut().user_set_frame(a, dragged);
    state.tick();
    assert_eq!(frame(&state, a), dragged);
    state.system_press(false);
    state.tick();
    assert_eq!(frame(&state, a).w, was_a.w + 110.);
    assert_eq!(frame(&state, a).x, was_a.x);
}

#[test]
fn dragging_the_edge_between_two_windows_of_a_column_resizes_them() {
    let mut state = start(MockBackend::with_laptop_display());
    let a = open(&mut state, "a");
    let b = open(&mut state, "b");
    state.do_action(Action::ConsumeOrExpelWindowLeft);
    state.tick();
    let (was_a, was_b) = (frame(&state, a), frame(&state, b));
    assert_eq!(was_a.x, was_b.x);

    // The bottom edge of the upper window, pulled 80 points up.
    state.system_press(true);
    let mut dragged = was_a;
    dragged.h -= 80.;
    state.backend_mut().user_set_frame(a, dragged);
    state.tick();
    state.system_press(false);
    state.tick();

    assert_eq!(frame(&state, a).h, was_a.h - 80.);
    // The lower window takes up what was given.
    assert_eq!(frame(&state, b).y, was_b.y - 80.);
    assert_eq!(frame(&state, b).h, was_b.h + 80.);
}

#[test]
fn a_tiled_window_dragged_away_is_put_back_when_it_is_let_go() {
    let mut state = start(MockBackend::with_laptop_display());
    let a = open(&mut state, "a");
    let was = frame(&state, a);

    state.system_press(true);
    let mut dragged = was;
    dragged.x += 200.;
    dragged.y += 100.;
    state.backend_mut().user_set_frame(a, dragged);
    state.tick();
    // Not pulled out from under the pointer.
    assert_eq!(frame(&state, a), dragged);

    state.system_press(false);
    state.tick();
    assert_eq!(frame(&state, a), was);
}
