//! niri's actions, carried out on an in-memory system.

use elara::binds::find_bind;
use elara::State;
use elara_macos::{DisplayId, DisplayInfo, MockBackend, Rect, WindowBackend, WindowId};
use niri_config::{Action, Config, Modifiers, Trigger, WorkspaceReference};
use niri_ipc::SizeChange;
use smithay::input::keyboard::Keysym;

const SCREEN: Rect = Rect::new(0., 0., 1800., 1169.);
const USABLE: Rect = Rect::new(0., 39., 1800., 1130.);
const GAP: f64 = 16.;
const COLUMN_W: f64 = (USABLE.w - GAP) * 0.5 - GAP;
const COLUMN_H: f64 = USABLE.h - GAP * 2.;

fn start(backend: MockBackend) -> State<MockBackend> {
    State::new(backend, Config::load_default())
}

fn open(state: &mut State<MockBackend>, app: &str) -> WindowId {
    let id = state
        .backend_mut()
        .open_window(app, app, Rect::new(300., 200., 640., 480.));
    state.tick();
    id
}

/// A state with windows `a` and `b` side by side, `b` focused.
fn two_columns() -> (State<MockBackend>, WindowId, WindowId) {
    let mut state = start(MockBackend::with_laptop_display());
    let a = open(&mut state, "a");
    let b = open(&mut state, "b");
    (state, a, b)
}

fn frame(state: &State<MockBackend>, id: WindowId) -> Rect {
    state.backend().frame(id)
}

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

fn focused(state: &State<MockBackend>) -> Option<WindowId> {
    state.backend().focused_window()
}

#[test]
fn focus_actions_move_the_system_focus() {
    let (mut state, a, b) = two_columns();
    assert_eq!(focused(&state), Some(b));

    state.do_action(Action::FocusColumnLeft);
    assert_eq!(focused(&state), Some(a));

    // There is nothing further left; niri stays put.
    state.do_action(Action::FocusColumnLeft);
    assert_eq!(focused(&state), Some(a));

    state.do_action(Action::FocusColumnRight);
    assert_eq!(focused(&state), Some(b));

    state.do_action(Action::FocusColumnFirst);
    assert_eq!(focused(&state), Some(a));
    state.do_action(Action::FocusColumnLast);
    assert_eq!(focused(&state), Some(b));
}

#[test]
fn moving_a_column_swaps_it_with_its_neighbour() {
    let (mut state, a, b) = two_columns();

    state.do_action(Action::MoveColumnLeft);

    assert_eq!(frame(&state, b), column(0));
    assert_eq!(frame(&state, a), column(1));
    assert_eq!(focused(&state), Some(b));
}

#[test]
fn preset_widths_cycle_and_maximize_fills_the_working_area() {
    let (mut state, a, _b) = two_columns();
    state.do_action(Action::FocusColumnLeft);
    let width = |state: &State<MockBackend>| frame(state, a).w;
    let preset = |proportion: f64| ((USABLE.w - GAP) * proportion - GAP).floor();

    // The default config has presets of one third, one half and two thirds. A new column
    // starts at one half.
    assert_eq!(width(&state), preset(0.5));
    state.do_action(Action::SwitchPresetColumnWidth);
    assert_eq!(width(&state), preset(0.66667));
    state.do_action(Action::SwitchPresetColumnWidth);
    assert_eq!(width(&state), preset(0.33333));
    state.do_action(Action::SwitchPresetColumnWidthBack);
    assert_eq!(width(&state), preset(0.66667));

    state.do_action(Action::MaximizeColumn);
    assert_eq!(width(&state), USABLE.w - GAP * 2.);
    assert_eq!(frame(&state, a).x, GAP);
    state.do_action(Action::MaximizeColumn);
    assert_eq!(width(&state), preset(0.66667));

    state.do_action(Action::SetColumnWidth(SizeChange::SetFixed(700)));
    assert_eq!(width(&state), 700.);
    state.do_action(Action::SetColumnWidth(SizeChange::AdjustFixed(-100)));
    assert_eq!(width(&state), 600.);
}

#[test]
fn consuming_a_window_stacks_it_in_the_column() {
    let (mut state, a, b) = two_columns();

    // Pull `b` into the column on its left.
    state.do_action(Action::ConsumeOrExpelWindowLeft);

    let (top, bottom) = (frame(&state, a), frame(&state, b));
    assert_eq!(top.x, column(0).x);
    assert_eq!(bottom.x, column(0).x);
    assert_eq!(top.w, COLUMN_W);
    assert_eq!(bottom.w, COLUMN_W);
    // They share the column's height, with a gap between them.
    assert_eq!(top.y, USABLE.y + GAP);
    assert_eq!(bottom.y, top.max_y() + GAP);
    assert_eq!(bottom.max_y(), USABLE.max_y() - GAP);
    assert_eq!(top.h, bottom.h);

    state.do_action(Action::FocusWindowUp);
    assert_eq!(focused(&state), Some(a));

    // And push it back out to a column of its own.
    state.do_action(Action::FocusWindowDown);
    state.do_action(Action::ConsumeOrExpelWindowRight);
    assert_eq!(frame(&state, a), column(0));
    assert_eq!(frame(&state, b), column(1));
}

#[test]
fn switching_workspaces_parks_and_restores_windows() {
    let (mut state, a, b) = two_columns();

    state.do_action(Action::FocusWorkspaceDown);
    assert!(is_parked(&state, a));
    assert!(is_parked(&state, b));

    let c = open(&mut state, "c");
    assert_eq!(frame(&state, c), column(0));

    state.do_action(Action::FocusWorkspaceUp);
    assert_eq!(frame(&state, a), column(0));
    assert_eq!(frame(&state, b), column(1));
    assert!(is_parked(&state, c));
    // Focus returned to the window that had it on that workspace.
    assert_eq!(focused(&state), Some(b));

    state.do_action(Action::FocusWorkspace(WorkspaceReference::Index(2)));
    assert_eq!(frame(&state, c), column(0));
    assert_eq!(focused(&state), Some(c));

    state.do_action(Action::FocusWorkspacePrevious);
    assert_eq!(focused(&state), Some(b));
}

#[test]
fn moving_a_column_to_another_workspace_follows_it() {
    let (mut state, a, b) = two_columns();

    state.do_action(Action::MoveColumnToWorkspaceDown(true));

    assert_eq!(frame(&state, b), column(0));
    assert!(is_parked(&state, a));
    assert_eq!(focused(&state), Some(b));

    // Without `focus`, the view stays behind.
    state.do_action(Action::MoveColumnToWorkspaceUp(false));
    assert!(is_parked(&state, b));
    assert!(is_parked(&state, a));
    state.do_action(Action::FocusWorkspaceUp);
    assert!(!is_parked(&state, a) && !is_parked(&state, b));
}

#[test]
fn fullscreen_covers_the_display_and_can_be_undone() {
    let (mut state, a, b) = two_columns();

    // niri's fullscreen is the whole display. macOS keeps windows out from under the menu bar,
    // so the window ends up with everything below it.
    state.do_action(Action::FullscreenWindow);
    assert_eq!(frame(&state, b), USABLE);

    // It is not asked again and again for the part it cannot have.
    state.backend_mut().take_frame_requests();
    state.tick();
    state.tick();
    let again = state.backend_mut().take_frame_requests();
    assert!(again.is_empty(), "{again:?}");

    state.do_action(Action::FullscreenWindow);
    assert_eq!(frame(&state, b), column(1));
    assert_eq!(frame(&state, a), column(0));
}

#[test]
fn floating_can_be_toggled() {
    let (mut state, a, b) = two_columns();
    let is_floating = |state: &State<MockBackend>, id: WindowId| {
        let mut windows = state.layout().windows();
        windows
            .find(|(_, window)| window.window_id() == id)
            .is_some_and(|(_, window)| window.is_floating())
    };
    assert!(!is_floating(&state, b));

    state.do_action(Action::ToggleWindowFloating);
    assert!(is_floating(&state, b));
    assert!(!is_floating(&state, a));
    // The strip is left with one column, and the floating window keeps focus.
    assert_eq!(frame(&state, a), column(0));
    assert_eq!(focused(&state), Some(b));

    state.do_action(Action::SwitchFocusBetweenFloatingAndTiling);
    assert_eq!(focused(&state), Some(a));

    state.do_action(Action::FocusFloating);
    assert_eq!(focused(&state), Some(b));
    state.do_action(Action::ToggleWindowFloating);
    assert!(!is_floating(&state, b));
    assert_eq!(frame(&state, b).size(), column(1).size());
    assert_eq!(frame(&state, a).max_x(), frame(&state, b).x - GAP);
}

#[test]
fn close_window_asks_the_focused_window_to_close() {
    let (mut state, a, b) = two_columns();

    state.do_action(Action::CloseWindow);
    state.tick();

    assert!(state.backend().window(b).is_none());
    assert_eq!(focused(&state), Some(a));
    assert_eq!(state.layout().windows().count(), 1);
}

#[test]
fn focus_window_previous_returns_to_the_last_focused_window() {
    let mut state = start(MockBackend::with_laptop_display());
    let a = open(&mut state, "a");
    let _b = open(&mut state, "b");
    let c = open(&mut state, "c");

    state.do_action(Action::FocusColumnFirst);
    assert_eq!(focused(&state), Some(a));

    state.do_action(Action::FocusWindowPrevious);
    assert_eq!(focused(&state), Some(c));
    state.do_action(Action::FocusWindowPrevious);
    assert_eq!(focused(&state), Some(a));
}

#[test]
fn actions_by_id_target_the_named_window() {
    let (mut state, a, b) = two_columns();

    state.do_action(Action::FocusWindow(a.0));
    assert_eq!(focused(&state), Some(a));

    state.do_action(Action::SetWindowWidthById {
        id: b.0,
        change: SizeChange::SetFixed(500),
    });
    assert_eq!(frame(&state, b).w, 500.);
    assert_eq!(frame(&state, a).w, COLUMN_W);

    // An id that names no window does nothing.
    state.do_action(Action::CloseWindowById(9999));
    state.tick();
    assert_eq!(state.layout().windows().count(), 2);
}

#[test]
fn monitor_actions_follow_the_arrangement_of_displays() {
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
    let mut state = start(backend);
    let a = open(&mut state, "a");
    let on_external = |state: &State<MockBackend>, id| frame(state, id).x >= 1800.;
    assert!(!on_external(&state, a));

    // Nothing is to the left of the laptop display.
    state.do_action(Action::MoveColumnToMonitorLeft);
    assert!(!on_external(&state, a));

    state.do_action(Action::MoveColumnToMonitorRight);
    assert!(on_external(&state, a));
    assert_eq!(frame(&state, a).x, 1800. + GAP);
    assert_eq!(frame(&state, a).y, 25. + GAP);

    // New windows open on the focused display.
    let b = open(&mut state, "b");
    assert!(on_external(&state, b));

    state.do_action(Action::FocusMonitorLeft);
    let c = open(&mut state, "c");
    assert!(!on_external(&state, c));

    state.do_action(Action::MoveWindowToMonitor("External".to_owned()));
    assert!(on_external(&state, c));

    state.do_action(Action::FocusMonitorNext);
    state.do_action(Action::FocusMonitorNext);
    let d = open(&mut state, "d");
    assert!(on_external(&state, d));
}

#[test]
fn quit_is_reported_to_the_caller() {
    let (mut state, _a, _b) = two_columns();
    assert!(!state.should_quit());

    state.do_action(Action::Quit(false));

    assert!(state.should_quit());
}

#[test]
fn spawn_starts_a_program() {
    let (mut state, _a, _b) = two_columns();
    let marker = std::env::temp_dir().join(format!("elara-spawn-test-{}", std::process::id()));
    let _ = std::fs::remove_file(&marker);

    state.do_action(Action::Spawn(vec![
        "/usr/bin/touch".to_owned(),
        marker.display().to_string(),
    ]));

    let appeared = (0..200).any(|_| {
        std::thread::sleep(std::time::Duration::from_millis(10));
        marker.exists()
    });
    let _ = std::fs::remove_file(&marker);
    assert!(appeared, "the spawned program did not run");
}

#[test]
fn unsupported_actions_change_nothing() {
    let (mut state, a, b) = two_columns();
    state.backend_mut().take_frame_requests();

    state.do_action(Action::ToggleKeyboardShortcutsInhibit);
    state.do_action(Action::PowerOffMonitors);

    assert!(state.backend_mut().take_frame_requests().is_empty());
    assert_eq!(frame(&state, a), column(0));
    assert_eq!(frame(&state, b), column(1));
}

#[test]
fn a_key_press_runs_the_bound_action() {
    let (mut state, a, b) = two_columns();
    let press = |state: &mut State<MockBackend>, key: Keysym, modifiers: Modifiers| {
        let bind = find_bind(state.config(), Trigger::Keysym(key), modifiers);
        if let Some(bind) = bind {
            state.do_action(bind.action);
        }
    };

    // The default config binds Mod+Left, and Mod is Option here.
    press(&mut state, Keysym::Left, Modifiers::ALT);
    assert_eq!(focused(&state), Some(a));

    press(&mut state, Keysym::Right, Modifiers::ALT | Modifiers::CTRL);
    assert_eq!(frame(&state, a), column(1));
    assert_eq!(frame(&state, b), column(0));

    // Command+Left is not bound and is left to the application.
    press(&mut state, Keysym::Left, Modifiers::SUPER);
    assert_eq!(focused(&state), Some(a));
}

#[test]
fn a_column_in_the_right_half_can_grow_past_what_fits_from_there() {
    let (mut state, _a, b) = two_columns();
    assert_eq!(frame(&state, b), column(1));

    // `b` starts mid-screen. Full width does not fit between there and the right edge; the
    // view has to scroll so that the column starts at the left.
    state.do_action(Action::MaximizeColumn);
    assert_eq!(
        frame(&state, b),
        Rect::new(GAP, USABLE.y + GAP, USABLE.w - GAP * 2., COLUMN_H)
    );

    state.do_action(Action::MaximizeColumn);
    assert_eq!(frame(&state, b).w, COLUMN_W);

    // Two thirds does not fit from mid-screen either. Put the column back there first.
    state.do_action(Action::FocusColumnLeft);
    state.do_action(Action::FocusColumnRight);
    assert_eq!(frame(&state, b), column(1));
    state.do_action(Action::SwitchPresetColumnWidth);
    let two_thirds = ((USABLE.w - GAP) * 0.66667 - GAP).floor();
    assert_eq!(frame(&state, b).w, two_thirds);
    assert_eq!(frame(&state, b).max_x(), USABLE.max_x() - GAP);

    // Being cut short at the screen edge is not the window refusing a size: afterwards it can
    // still be given any width.
    state.do_action(Action::SetColumnWidth(SizeChange::SetFixed(1500)));
    assert_eq!(frame(&state, b).w, 1500.);
}

#[test]
fn the_workspace_label_is_the_number_or_the_name() {
    let (mut state, _, _) = two_columns();
    assert_eq!(state.workspace_label().as_deref(), Some("1"));

    state.do_action(Action::FocusWorkspaceDown);
    assert_eq!(state.workspace_label().as_deref(), Some("2"));

    let config = Config::parse_mem("workspace \"chat\"").unwrap();
    let state = State::new(MockBackend::with_laptop_display(), config);
    assert_eq!(state.workspace_label().as_deref(), Some("chat"));
}
