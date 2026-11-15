//! The overview: every workspace and window drawn zoomed out, over the real windows.

use std::thread::sleep;
use std::time::{Duration, Instant};

use elara::config::default_config;
use elara::keys::KeyHandler;
use elara::pointer::PointerInput;
use elara::scroll::{ScrollHandler, ScrollInput};
use elara::State;
use elara_macos::{
    KeyEvent, KeyModifiers, MockBackend, MouseButton, MouseEvent, MouseEventKind, Overview,
    OverviewItem, OverviewWindow, Point, Rect, ScrollEvent, ScrollPhase, WindowBackend, WindowId,
};
use niri_config::{Action, Config};

const SCREEN: Rect = Rect::new(0., 0., 1800., 1169.);
/// niri's default zoom.
const ZOOM: f64 = 0.5;

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

fn is_parked(state: &State<MockBackend>, id: WindowId) -> bool {
    let frame = frame(state, id);
    frame.x == SCREEN.max_x() - 1. && frame.y == SCREEN.max_y() - 1.
}

fn focused(state: &State<MockBackend>) -> Option<WindowId> {
    state.layout().focus().map(|window| window.window_id())
}

fn overview(state: &State<MockBackend>) -> &Overview {
    state.backend().overview().expect("the overview is showing")
}

/// The card of a window in the overview.
fn card(state: &State<MockBackend>, id: WindowId) -> OverviewWindow {
    overview(state)
        .displays
        .iter()
        .flat_map(|display| &display.items)
        .find_map(|item| match item {
            OverviewItem::Window(window) if window.id == id => Some(window.clone()),
            _ => None,
        })
        .expect("the window has a card")
}

/// A point near the bottom of a rectangle. The workspace above the one showing is cut off
/// by the top of the display, and so are the windows on it; their lower part is in view.
fn foot(rect: Rect) -> Point {
    Point {
        x: rect.x + rect.w / 2.,
        y: rect.max_y() - 20.,
    }
}

fn center(rect: Rect) -> Point {
    Point {
        x: rect.x + rect.w / 2.,
        y: rect.y + rect.h / 2.,
    }
}

/// A press on the overview, which needs no mod key.
fn press(state: &mut State<MockBackend>, button: MouseButton, position: Point) {
    let event = MouseEvent {
        kind: MouseEventKind::Pressed(button),
        position,
        modifiers: NONE,
    };
    state.pointer_input(PointerInput::from_overview(&event));
    state.tick();
}

fn drag(state: &mut State<MockBackend>, from: Point, to: Point) {
    const STEPS: u32 = 20;
    for step in 1..=STEPS {
        let t = f64::from(step) / f64::from(STEPS);
        let position = Point {
            x: from.x + (to.x - from.x) * t,
            y: from.y + (to.y - from.y) * t,
        };
        state.pointer_input(PointerInput::Motion { position });
        state.tick();
    }
}

fn release(state: &mut State<MockBackend>, button: MouseButton) {
    state.pointer_input(PointerInput::Release { button });
    state.tick();
}

/// Windows `a` and `b` side by side on the first workspace, `c` on the second, which is the
/// one showing.
fn two_workspaces() -> (State<MockBackend>, [WindowId; 3]) {
    let mut state = State::new(MockBackend::with_laptop_display(), default_config());
    let a = open(&mut state, "a");
    let b = open(&mut state, "b");
    state.do_action(Action::FocusWorkspaceDown);
    let c = open(&mut state, "c");
    assert!(is_parked(&state, a) && is_parked(&state, b));
    assert!(!is_parked(&state, c));
    (state, [a, b, c])
}

#[test]
fn the_overview_draws_every_workspace_zoomed_out_and_leaves_the_windows_alone() {
    let (mut state, [a, b, c]) = two_workspaces();
    let before = [a, b, c].map(|id| frame(&state, id));
    state.backend_mut().take_frame_requests();

    state.do_action(Action::ToggleOverview);

    assert!(state.overview_showing());
    // Nothing moved: the real windows stay under the picture.
    assert!(state.backend_mut().take_frame_requests().is_empty());
    assert_eq!([a, b, c].map(|id| frame(&state, id)), before);
    // The rings are the overview's to draw now.
    assert!(state.backend().outlines().is_empty());

    let overview = overview(&state);
    assert_eq!(overview.backdrop, [0.15, 0.15, 0.15, 1.]);
    let display = &overview.displays[0];

    // The workspace that is showing is in the middle of the display at half size, with
    // the one before it above and the empty one after it below.
    let showing = display
        .workspaces
        .iter()
        .find(|ws| ws.rect.y > 0. && ws.rect.max_y() < SCREEN.h)
        .expect("the active workspace is wholly in view");
    assert_eq!(showing.rect.w, SCREEN.w * ZOOM);
    assert_eq!(showing.rect.x, SCREEN.w * (1. - ZOOM) / 2.);
    assert_eq!(display.workspaces.len(), 3);

    // The window on it is drawn at half size, at half the distance from the workspace's
    // corner that the real one is from the display's.
    let real = frame(&state, c);
    let drawn = card(&state, c);
    assert_eq!(drawn.rect.w, real.w * ZOOM);
    assert_eq!(drawn.rect.h, real.h * ZOOM);
    assert_eq!(drawn.rect.x, showing.rect.x + real.x * ZOOM);
    assert_eq!(drawn.rect.y, showing.rect.y + real.y * ZOOM);
    assert_eq!(drawn.title.as_deref(), Some("c"));
    assert_eq!(drawn.corner_radius, [8.; 4]);

    // The windows of the workspace above are drawn too, above it.
    assert!(card(&state, a).rect.max_y() < showing.rect.y);
    assert!(card(&state, a).rect.x < card(&state, b).rect.x);

    // The focus ring is drawn around the focused window's card.
    let ring = display
        .items
        .iter()
        .find_map(|item| match item {
            OverviewItem::Shape(shape) if shape.line_width > 0. => Some(*shape),
            _ => None,
        })
        .expect("a ring is drawn");
    assert_eq!(ring.line_width, 4. * ZOOM);
}

#[test]
fn pictures_of_windows_are_asked_for_only_when_the_config_says_so() {
    let (mut state, _) = two_workspaces();
    // The default config file asks for them; a config that says nothing does not.
    state.set_config(Config::parse_mem("").unwrap());
    state.do_action(Action::ToggleOverview);
    state.tick();
    assert_eq!(overview(&state).thumbnails, None);

    let config = Config::parse_mem("overview {\n zoom 0.4\n window-thumbnails\n}").unwrap();
    state.set_config(config);
    state.tick();
    // As large as the windows are drawn when the overview is open.
    assert_eq!(overview(&state).thumbnails, Some(0.4));

    let config = Config::parse_mem("overview {\n window-thumbnails false\n}").unwrap();
    state.set_config(config);
    state.tick();
    assert_eq!(overview(&state).thumbnails, None);
}

#[test]
fn closing_the_overview_takes_the_picture_away() {
    let (mut state, [_, _, c]) = two_workspaces();
    let before = frame(&state, c);

    state.do_action(Action::ToggleOverview);
    state.do_action(Action::ToggleOverview);

    assert!(!state.overview_showing());
    assert!(state.backend().overview().is_none());
    assert_eq!(frame(&state, c), before);
    assert!(!state.backend().outlines().is_empty());

    // The actions that only open or only close do nothing the second time.
    state.do_action(Action::OpenOverview);
    state.do_action(Action::OpenOverview);
    assert!(state.overview_showing());
    state.do_action(Action::CloseOverview);
    state.do_action(Action::CloseOverview);
    assert!(!state.overview_showing());
}

#[test]
fn a_click_on_a_window_goes_to_it_and_closes_the_overview() {
    let (mut state, [a, b, c]) = two_workspaces();
    state.do_action(Action::ToggleOverview);
    assert_eq!(focused(&state), Some(c));

    let on_a = foot(card(&state, a).rect);
    press(&mut state, MouseButton::Left, on_a);
    // Nothing happens until the button comes up: it may yet be a drag.
    assert!(state.overview_showing());
    release(&mut state, MouseButton::Left);

    assert!(!state.overview_showing());
    assert_eq!(focused(&state), Some(a));
    assert_eq!(state.backend().focused_window(), Some(a));
    // Its workspace is the one showing now.
    assert!(!is_parked(&state, a) && !is_parked(&state, b));
    assert!(is_parked(&state, c));
}

#[test]
fn a_click_on_a_workspace_goes_to_it_and_closes_the_overview() {
    let (mut state, [a, _, c]) = two_workspaces();
    state.do_action(Action::ToggleOverview);

    // The first workspace is above the one showing. Its lower left corner is in view, and
    // no window reaches into the gap at its edge.
    let above = overview(&state).displays[0]
        .workspaces
        .iter()
        .map(|ws| ws.rect)
        .min_by(|x, y| x.y.total_cmp(&y.y))
        .unwrap();
    let corner = Point {
        x: above.x + 2.,
        y: above.max_y() - 2.,
    };
    press(&mut state, MouseButton::Left, corner);
    release(&mut state, MouseButton::Left);

    assert!(!state.overview_showing());
    assert!(!is_parked(&state, a));
    assert!(is_parked(&state, c));
}

#[test]
fn dragging_a_window_in_the_overview_moves_it_to_another_workspace() {
    let (mut state, [a, b, c]) = two_workspaces();
    state.do_action(Action::ToggleOverview);

    // Drag `c` from the workspace that is showing onto the one above.
    let from = center(card(&state, c).rect);
    let to = foot(card(&state, b).rect);
    press(&mut state, MouseButton::Left, from);
    drag(&mut state, from, to);
    // While it is dragged, its card follows the pointer.
    let dragged = card(&state, c).rect;
    assert!(dragged.contains(to), "{dragged:?} is not under {to:?}");
    release(&mut state, MouseButton::Left);

    // Dropping does not close the overview.
    assert!(state.overview_showing());
    let workspace_of = |id: WindowId| {
        state
            .layout()
            .workspaces()
            .find(|(_, _, ws)| ws.windows().any(|w| w.window_id() == id))
            .map(|(_, idx, _)| idx)
    };
    assert_eq!(workspace_of(c), workspace_of(a));
}

#[test]
fn dragging_with_the_right_button_scrolls_a_workspace_sideways() {
    let mut state = State::new(MockBackend::with_laptop_display(), default_config());
    let ids = ["a", "b", "c", "d"].map(|app| open(&mut state, app));
    state.do_action(Action::FocusColumnFirst);
    state.do_action(Action::ToggleOverview);
    let before = card(&state, ids[0]).rect.x;

    let from = center(card(&state, ids[1]).rect);
    let to = Point {
        x: from.x - 300.,
        y: from.y,
    };
    press(&mut state, MouseButton::Right, from);
    drag(&mut state, from, to);
    assert!(card(&state, ids[0]).rect.x < before);
    release(&mut state, MouseButton::Right);

    assert!(state.overview_showing());
}

#[test]
fn the_wheel_switches_workspaces_in_the_overview_without_the_mod_key() {
    let (mut state, [a, _, c]) = two_workspaces();
    state.do_action(Action::ToggleOverview);

    let mut handler = ScrollHandler::new();
    let wheel = |delta_y: f64, modifiers: KeyModifiers| ScrollEvent {
        delta_x: 0.,
        delta_y,
        continuous: false,
        phase: ScrollPhase::None,
        momentum: false,
        modifiers,
    };
    let now = Instant::now();
    let up = handler.handle_in_overview(&wheel(1., NONE), now);
    assert_eq!(
        up,
        Some(ScrollInput::Action(Action::FocusWorkspaceUpUnderMouse))
    );
    // niri's cooldown: a second step right behind the first is dropped.
    assert_eq!(handler.handle_in_overview(&wheel(1., NONE), now), None);
    // With Shift it moves between columns. macOS reports that wheel as a horizontal one.
    let shift = KeyModifiers {
        shift: true,
        ..NONE
    };
    let sideways = ScrollEvent {
        delta_x: -1.,
        delta_y: 0.,
        ..wheel(0., shift)
    };
    assert_eq!(
        handler.handle_in_overview(&sideways, now),
        Some(ScrollInput::Action(Action::FocusColumnRightUnderMouse))
    );

    state.overview_scroll(up.unwrap(), center(SCREEN));
    assert!(state.overview_showing());
    state.do_action(Action::ToggleOverview);
    assert!(!is_parked(&state, a));
    assert!(is_parked(&state, c));
}

#[test]
fn keys_in_the_overview_go_to_the_overview() {
    let config = default_config();
    let mut handler = KeyHandler::new();
    let key = |keycode: u16, character: Option<char>, modifiers: KeyModifiers| KeyEvent {
        keycode,
        character,
        modifiers,
        pressed: true,
        is_repeat: false,
    };
    const ESCAPE: u16 = 0x35;
    const RIGHT: u16 = 0x7C;
    const KEY_A: u16 = 0x00;
    const TAB: u16 = 0x30;

    // Outside the overview these are the application's.
    for event in [key(ESCAPE, None, NONE), key(KEY_A, Some('a'), NONE)] {
        assert!(!handler.handle_with_overview(&config, &event, false).swallow);
    }

    let escape = handler.handle_with_overview(&config, &key(ESCAPE, None, NONE), true);
    assert_eq!(escape.action, Some(Action::ToggleOverview));
    let right = handler.handle_with_overview(&config, &key(RIGHT, None, NONE), true);
    assert_eq!(right.action, Some(Action::FocusColumnRight));

    // Typing reaches no window, and neither does the key coming back up.
    let a = key(KEY_A, Some('a'), NONE);
    let typed = handler.handle_with_overview(&config, &a, true);
    assert!(typed.swallow && typed.action.is_none());
    let up = KeyEvent {
        pressed: false,
        ..a
    };
    assert!(handler.handle_with_overview(&config, &up, true).swallow);

    // The system's shortcuts, such as switching applications, still work.
    let command = KeyModifiers {
        command: true,
        ..NONE
    };
    assert!(
        !handler
            .handle_with_overview(&config, &key(TAB, None, command), true)
            .swallow
    );
}

#[test]
fn the_overview_zooms_out_gradually_and_the_windows_wait_under_it() {
    let (mut state, [a, _, c]) = two_workspaces();
    state.enable_animations();
    let real = frame(&state, c);
    state.backend_mut().take_frame_requests();

    state.do_action(Action::ToggleOverview);
    // The first frame covers the window with a card of the same size.
    let first = card(&state, c).rect;
    assert!((first.w - real.w).abs() < 1. && (first.x - real.x).abs() < 1.);

    let started = Instant::now();
    let mut widths = vec![card(&state, c).rect.w];
    while state.is_animating() {
        assert!(started.elapsed() < Duration::from_secs(5), "never settled");
        sleep(Duration::from_millis(8));
        state.tick();
        widths.push(card(&state, c).rect.w);
    }
    assert!(widths.len() > 3, "the overview opened at once: {widths:?}");
    assert_eq!(*widths.last().unwrap(), real.w * ZOOM);
    assert!(state.backend_mut().take_frame_requests().is_empty());

    // Go to the workspace above and close. The windows change places once, when the
    // overview has finished closing, not on every frame on the way.
    state.do_action(Action::FocusWorkspaceUp);
    state.do_action(Action::ToggleOverview);
    while state.is_animating() {
        assert!(state.backend().overview().is_some());
        sleep(Duration::from_millis(8));
        state.tick();
    }
    assert!(state.backend().overview().is_none());
    assert!(!is_parked(&state, a));
    assert!(is_parked(&state, c));
    let moves_of_c = state
        .backend_mut()
        .take_frame_requests()
        .into_iter()
        .filter(|(id, _)| *id == c)
        .count();
    assert_eq!(moves_of_c, 1);
}
