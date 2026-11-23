//! What is drawn around and between windows: the focus ring, borders and the insert hint.

use elara::config::default_config;
use elara::pointer::PointerInput;
use elara::State;
use elara_macos::{
    ColorSpace, HueInterpolation, MockBackend, MouseButton, Outline, Point, Rect, WindowBackend,
    WindowId, WindowKind,
};
use niri_config::{Action, Config};

const SCREEN: Rect = Rect::new(0., 0., 1800., 1169.);
const GAP: f64 = 16.;
/// niri's default focus ring.
const RING_WIDTH: f64 = 4.;
const ACTIVE: [f32; 4] = [127. / 255., 200. / 255., 1., 1.];
/// The corners a window is taken to have, plus the width of the ring around them.
const RING_RADIUS: f64 = 16. + RING_WIDTH;

/// niri's built-in defaults with `text` on top. The gaps and column widths are the same as
/// in the default config file.
fn config(text: &str) -> Config {
    Config::parse_mem(text).unwrap()
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

fn outlines(state: &State<MockBackend>) -> Vec<Outline> {
    state.backend().outlines().to_vec()
}

/// The rectangle `width` outside `frame` on every side.
fn around(frame: Rect, width: f64) -> Rect {
    Rect::new(
        frame.x - width,
        frame.y - width,
        frame.w + 2. * width,
        frame.h + 2. * width,
    )
}

fn two_columns(config: Config) -> (State<MockBackend>, [WindowId; 2]) {
    let mut state = State::new(MockBackend::with_laptop_display(), config);
    let a = open(&mut state, "a");
    let b = open(&mut state, "b");
    state.do_action(Action::FocusColumnFirst);
    (state, [a, b])
}

#[test]
fn nothing_is_drawn_without_windows() {
    let state = State::new(MockBackend::with_laptop_display(), default_config());
    assert!(outlines(&state).is_empty());
}

#[test]
fn the_focus_ring_goes_around_the_focused_window() {
    let (mut state, [a, b]) = two_columns(default_config());

    let expected = |frame: Rect| Outline {
        rect: around(frame, RING_WIDTH),
        line_width: RING_WIDTH,
        corner_radius: [RING_RADIUS; 4],
        color: ACTIVE,
        gradient: None,
    };
    assert_eq!(outlines(&state), [expected(frame(&state, a))]);

    // It fits in the gap the layout leaves around the window.
    const { assert!(RING_WIDTH < GAP) };

    state.do_action(Action::FocusColumnRight);
    assert_eq!(outlines(&state), [expected(frame(&state, b))]);
}

#[test]
fn the_ring_follows_a_click_on_another_window() {
    let (mut state, [_a, b]) = two_columns(default_config());
    state.backend_mut().user_focus(b);
    state.tick();
    assert_eq!(
        outlines(&state)[0].rect,
        around(frame(&state, b), RING_WIDTH)
    );
}

#[test]
fn a_fullscreen_window_has_no_ring() {
    let (mut state, _) = two_columns(default_config());
    state.do_action(Action::FullscreenWindow);
    assert!(outlines(&state).is_empty(), "{:?}", outlines(&state));
}

#[test]
fn a_ring_switched_off_in_the_config_is_not_drawn() {
    let (state, _) = two_columns(config("layout { focus-ring { off; }; }"));
    assert!(outlines(&state).is_empty());
}

#[test]
fn the_config_sets_width_and_color_and_a_rule_sets_the_corners() {
    let (state, [a, _b]) = two_columns(config(
        r##"
        layout { focus-ring { width 8; active-color "#ff0000"; }; }
        window-rule { geometry-corner-radius 0; }
        "##,
    ));
    assert_eq!(
        outlines(&state),
        [Outline {
            rect: around(frame(&state, a), 8.),
            line_width: 8.,
            // Around square corners the ring is square too, as in niri.
            corner_radius: [0.; 4],
            color: [1., 0., 0., 1.],
            gradient: None,
        }]
    );
}

#[test]
fn a_border_is_drawn_around_every_window_in_view() {
    let (state, [a, b]) = two_columns(config(
        r##"
        layout {
            focus-ring { off; }
            border { width 2; active-color "#ff0000"; inactive-color "#00ff00"; }
        }
        "##,
    ));
    let drawn = outlines(&state);
    assert_eq!(drawn.len(), 2, "{drawn:?}");

    let of = |id: WindowId| {
        *drawn
            .iter()
            .find(|outline| outline.rect == around(frame(&state, id), 2.))
            .unwrap_or_else(|| panic!("no border around {id:?} in {drawn:?}"))
    };
    assert_eq!(of(a).color, [1., 0., 0., 1.]);
    assert_eq!(of(b).color, [0., 1., 0., 1.]);
    assert_eq!(of(a).line_width, 2.);

    // The layout made room for it: the windows are still a gap apart, counting the borders.
    assert_eq!(frame(&state, b).x - frame(&state, a).max_x(), GAP + 2. * 2.);
}

#[test]
fn a_window_scrolled_out_of_view_has_nothing_drawn_where_it_is_kept() {
    let mut state = State::new(MockBackend::with_laptop_display(), default_config());
    let _a = open(&mut state, "a");
    let _b = open(&mut state, "b");
    let c = open(&mut state, "c");
    // `c` has focus and is in view; go back to the first column, which scrolls it out.
    state.do_action(Action::FocusColumnFirst);
    assert_eq!(frame(&state, c).x, SCREEN.max_x() - 1.);

    for outline in outlines(&state) {
        assert!(outline.rect.max_x() < SCREEN.max_x(), "{outline:?}");
    }
}

#[test]
fn outlines_are_cleared_when_the_windows_are_released() {
    let (mut state, _) = two_columns(default_config());
    assert!(!outlines(&state).is_empty());
    state.release_windows();
    assert!(outlines(&state).is_empty());
}

#[test]
fn dragging_a_window_shows_where_it_would_land() {
    let (mut state, [a, b]) = two_columns(default_config());
    let a_frame = frame(&state, a);
    let from = Point {
        x: a_frame.x + a_frame.w / 2.,
        y: a_frame.y + a_frame.h / 2.,
    };
    // Far enough to leave the column, and over to the right of `b`.
    let to = Point {
        x: SCREEN.max_x() - 40.,
        y: from.y,
    };

    state.pointer_input(PointerInput::Press {
        button: MouseButton::Left,
        position: from,
        mod_down: true,
    });
    state.tick();
    for step in 1..=20 {
        let t = f64::from(step) / 20.;
        state.pointer_input(PointerInput::Motion {
            position: Point {
                x: from.x + (to.x - from.x) * t,
                y: from.y,
            },
        });
        state.tick();
    }

    let drawn = outlines(&state);
    // A filled shape, in niri's default hint color (the ring's, half transparent), to the right of
    // the column that is left.
    let hint = drawn
        .iter()
        .find(|outline| outline.line_width == 0.)
        .unwrap_or_else(|| panic!("no insert hint in {drawn:?}"));
    assert_eq!(hint.color, [127. / 255., 200. / 255., 1., 128. / 255.]);
    assert!(hint.rect.x >= frame(&state, b).max_x(), "{hint:?}");
    assert!(hint.rect.h > 500., "{hint:?}");

    // The window under the pointer carries the ring, drawn over the hint.
    let ring = drawn.last().unwrap();
    assert_eq!(ring.rect, around(frame(&state, a), RING_WIDTH));
    assert_eq!(ring.color, ACTIVE);

    state.pointer_input(PointerInput::Release {
        button: MouseButton::Left,
    });
    state.tick();

    // Dropped: the hint is gone and the ring is around the window in its new column.
    let drawn = outlines(&state);
    assert_eq!(drawn.len(), 1, "{drawn:?}");
    assert_eq!(drawn[0].rect, around(frame(&state, a), RING_WIDTH));
}

// Tabbed columns.

/// One column holding `a` and `b` as tabs, with `a` showing.
fn tabbed_column() -> (State<MockBackend>, [WindowId; 2]) {
    let (mut state, [a, b]) = two_columns(default_config());
    // Pull `b` into the first column, under `a`, then show the column as tabs.
    state.do_action(Action::ConsumeWindowIntoColumn);
    state.do_action(Action::ToggleColumnTabbedDisplay);
    state.do_action(Action::FocusWindowTop);
    (state, [a, b])
}

fn tabs(state: &State<MockBackend>) -> Vec<Outline> {
    outlines(state)
        .into_iter()
        .filter(|outline| outline.line_width == 0.)
        .collect()
}

#[test]
fn a_tabbed_column_shows_one_window_and_a_tab_for_each() {
    let (state, [a, b]) = tabbed_column();

    // `a` fills the column; `b` is behind it, which here means out of sight.
    let shown = frame(&state, a);
    assert_eq!(shown.h, SCREEN.h - 39. - 2. * GAP);
    assert_eq!(frame(&state, b).x, SCREEN.max_x() - 1.);

    // Two tabs in the gap to the left of the column, one above the other.
    let tabs = tabs(&state);
    assert_eq!(tabs.len(), 2, "{tabs:?}");
    for tab in &tabs {
        assert!(tab.rect.max_x() <= shown.x, "{tab:?}");
        assert!(tab.rect.x >= 0., "{tab:?}");
        assert!(tab.rect.y >= shown.y && tab.rect.max_y() <= shown.max_y());
    }
    assert!(tabs[0].rect.max_y() <= tabs[1].rect.y);
    // The tab of the window that is showing stands out from the other.
    assert_ne!(tabs[0].color, tabs[1].color);

    // The ring is still there, around the window that is showing.
    assert!(outlines(&state)
        .iter()
        .any(|outline| outline.rect == around(shown, RING_WIDTH)));
}

#[test]
fn a_stacked_column_has_no_tabs() {
    let (mut state, _) = two_columns(default_config());
    state.do_action(Action::ConsumeWindowIntoColumn);
    assert!(tabs(&state).is_empty());
}

#[test]
fn clicking_a_tab_switches_to_its_window() {
    let (mut state, [a, b]) = tabbed_column();
    let tabs_before = tabs(&state);
    let second = tabs_before[1].rect;
    let on_second = Point {
        x: second.x + second.w / 2.,
        y: second.y + second.h / 2.,
    };
    let shown = frame(&state, a);

    // A plain press there is for the window manager; one on the window is not.
    assert!(state.takes_press(MouseButton::Left, on_second, false));
    assert!(!state.takes_press(MouseButton::Right, on_second, false));
    let on_window = Point {
        x: shown.x + 50.,
        y: shown.y + 50.,
    };
    assert!(!state.takes_press(MouseButton::Left, on_window, false));

    state.pointer_input(PointerInput::Press {
        button: MouseButton::Left,
        position: on_second,
        mod_down: false,
    });
    state.pointer_input(PointerInput::Release {
        button: MouseButton::Left,
    });
    state.tick();

    // `b` took the place of `a`, and has focus.
    assert_eq!(frame(&state, b), shown);
    assert_eq!(frame(&state, a).x, SCREEN.max_x() - 1.);
    assert_eq!(state.backend().focused_window(), Some(b));
    // The highlight moved to the second tab.
    let tabs_after = tabs(&state);
    assert_eq!(tabs_after[1].color, tabs_before[0].color);
    assert_eq!(tabs_after[0].color, tabs_before[1].color);
}

#[test]
fn a_hidden_tab_that_picks_its_own_height_stays_out_of_sight() {
    let (mut state, [a, b]) = tabbed_column();
    // `b` is a terminal: it only takes heights that hold a whole number of rows, so it never
    // has quite the height the column asks of it.
    state.backend_mut().set_height_step(b, 17.);

    // Show the terminal, then the other window again.
    let column_x = frame(&state, a).x;
    state.do_action(Action::FocusWindowDown);
    state.tick();
    let shown = frame(&state, b);
    assert_eq!(shown.x, column_x);
    assert_ne!(shown.h, SCREEN.h - 39. - 2. * GAP);

    state.do_action(Action::FocusWindowUp);
    state.tick();
    assert_eq!(frame(&state, b).x, SCREEN.max_x() - 1.);

    // The column changes width while the terminal is hidden. The terminal takes the new
    // width and is still hidden afterwards.
    state.do_action(Action::SwitchPresetColumnWidth);
    state.tick();
    assert_eq!(frame(&state, b).w, frame(&state, a).w);
    assert_eq!(frame(&state, b).x, SCREEN.max_x() - 1.);
}

// Windows the system is dragging.

#[test]
fn nothing_is_drawn_around_a_window_while_the_system_drags_it() {
    let mut state = State::new(MockBackend::with_laptop_display(), default_config());
    let a = open(&mut state, "a");
    state.do_action(Action::ToggleWindowFloating);
    state.tick();
    let ring = |state: &State<MockBackend>| {
        let around = around(frame(state, a), RING_WIDTH);
        outlines(state).iter().any(|outline| outline.rect == around)
    };
    assert!(ring(&state));

    // The user takes the window by its title bar. The system moves it and says so after the
    // fact, so the ring would trail behind it; it is not drawn at all.
    state.system_press(true);
    state.tick();
    assert!(ring(&state), "a press alone moves nothing");
    for step in 1..=3 {
        let mut dragged = frame(&state, a);
        dragged.x += 40. * f64::from(step);
        state.backend_mut().user_set_frame(a, dragged);
        state.tick();
        assert_eq!(frame(&state, a), dragged);
        assert!(outlines(&state).is_empty(), "{:?}", outlines(&state));
    }

    // Let go, and the ring is back, around the window where it was dropped.
    state.system_press(false);
    state.tick();
    assert!(ring(&state));

    // A window that moves with no button down is not being dragged.
    let mut moved = frame(&state, a);
    moved.y += 30.;
    state.backend_mut().user_set_frame(a, moved);
    state.tick();
    assert!(ring(&state));
}

#[test]
fn switching_a_column_to_tabs_and_back_shows_no_window_out_of_place() {
    let (mut state, [a, b]) = two_columns(default_config());
    state.do_action(Action::ConsumeWindowIntoColumn);
    state.tick();
    let column = frame(&state, a);
    state.backend_mut().take_frame_requests();

    // Every place a window is put on the way must be in the column or out of sight, never
    // in the empty part of the screen beside the column.
    let check = |state: &mut State<MockBackend>, what: &str| {
        for (id, asked) in state.backend_mut().take_frame_requests() {
            let hidden = asked.x == SCREEN.max_x() - 1.;
            assert!(
                hidden || asked.x == column.x,
                "{what}: {id:?} was put at {asked:?}"
            );
        }
    };

    state.do_action(Action::ToggleColumnTabbedDisplay);
    state.tick();
    check(&mut state, "to tabs");
    assert_eq!(frame(&state, b).x, SCREEN.max_x() - 1.);
    assert_eq!(frame(&state, b).h, frame(&state, a).h);

    state.do_action(Action::FocusWindowDown);
    state.tick();
    check(&mut state, "other tab");

    state.do_action(Action::ToggleColumnTabbedDisplay);
    state.tick();
    check(&mut state, "to a stack");
    assert_eq!(frame(&state, a).x, column.x);
    assert_eq!(frame(&state, b).x, column.x);
    assert!(frame(&state, b).y >= frame(&state, a).max_y());
}

#[test]
fn a_gradient_in_the_config_fills_the_ring() {
    let (state, [a, _b]) = two_columns(config(
        r##"layout { focus-ring { active-gradient from="#ff0000" to="#0000ff" angle=45; }; }"##,
    ));
    let ring = around(frame(&state, a), RING_WIDTH);
    let [outline] = outlines(&state)[..] else {
        panic!("{:?}", outlines(&state));
    };
    assert_eq!(outline.rect, ring);
    assert_eq!(outline.color, [1., 0., 0., 1.]);

    let gradient = outline.gradient.expect("the ring has a gradient");
    assert_eq!(gradient.to, [0., 0., 1., 1.]);
    // It spans the ring, from the bottom left to the top right as in CSS at 45 degrees.
    assert_eq!(gradient.area, ring);
    let (start, end) = gradient.line();
    assert!(start.x < end.x && start.y > end.y, "{start:?} {end:?}");
    assert_eq!(gradient.space, ColorSpace::Srgb);
}

#[test]
fn the_default_angle_runs_a_gradient_from_top_to_bottom() {
    let (state, [a, _b]) = two_columns(config(
        r##"layout { focus-ring { active-gradient from="#ff0000" to="#0000ff"; }; }"##,
    ));
    let ring = around(frame(&state, a), RING_WIDTH);
    let (start, end) = outlines(&state)[0].gradient.unwrap().line();
    let close = |a: f64, b: f64| (a - b).abs() < 1e-3;
    assert!(close(start.x, end.x), "{start:?} {end:?}");
    assert!(close(start.y, ring.y) && close(end.y, ring.max_y()));
}

#[test]
fn a_gradient_can_span_the_whole_view_and_blend_in_another_color_space() {
    let (state, [a, b]) = two_columns(config(
        r##"
        layout {
            focus-ring { off; }
            border {
                width 4
                active-gradient from="#ff0000" to="#0000ff" relative-to="workspace-view" in="oklch longer hue"
                inactive-gradient from="#ff0000" to="#0000ff" relative-to="workspace-view" in="oklch longer hue"
            }
        }
        "##,
    ));
    let outlines = outlines(&state);
    assert_eq!(outlines.len(), 2);
    // Both borders are windows onto one gradient, which covers what the workspace shows.
    let gradients: Vec<_> = outlines.iter().map(|o| o.gradient.unwrap()).collect();
    assert_eq!(gradients[0].area, gradients[1].area);
    let area = gradients[0].area;
    assert!(area.w > frame(&state, a).w + frame(&state, b).w, "{area:?}");
    assert!(SCREEN.contains_rect(&area), "{area:?}");
    assert_eq!(gradients[0].space, ColorSpace::Oklch);
    assert_eq!(gradients[0].hue, HueInterpolation::Longer);
}

#[test]
fn one_color_is_not_a_gradient() {
    let (state, _) = two_columns(config(
        r##"layout { focus-ring { active-gradient from="#ff0000" to="#ff0000"; }; }"##,
    ));
    assert_eq!(outlines(&state)[0].gradient, None);
}

#[test]
fn a_dialog_the_system_does_not_focus_leaves_the_ring_where_it_is() {
    // macOS shows the input source indicator, when the source is switched, as a small
    // dialog of the application being typed in, and never gives it the keyboard.
    let (mut state, [a, _b]) = two_columns(default_config());
    state.backend_mut().user_focus(a);
    state.tick();
    let ring = outlines(&state);
    assert_eq!(ring.len(), 1);

    let indicator =
        state
            .backend_mut()
            .open_window_with("a", "", Rect::new(233., 155., 144., 83.), |info| {
                info.kind = WindowKind::Dialog
            });
    state.tick();
    assert_eq!(outlines(&state), ring, "the ring moved to the indicator");
    assert_eq!(state.layout().focus().map(|w| w.window_id()), Some(a));

    state.backend_mut().close_window(indicator);
    state.tick();
    assert_eq!(outlines(&state), ring);
}

#[test]
fn a_dialog_the_system_focuses_gets_the_ring() {
    let (mut state, [a, _b]) = two_columns(default_config());
    state.backend_mut().user_focus(a);
    state.tick();

    let spot = Rect::new(500., 400., 420., 260.);
    let dialog = state
        .backend_mut()
        .open_window_with("a", "Save", spot, |info| info.kind = WindowKind::Dialog);
    state.backend_mut().user_focus(dialog);
    state.tick();

    assert_eq!(state.layout().focus().map(|w| w.window_id()), Some(dialog));
    let rings = outlines(&state);
    assert_eq!(rings.len(), 1);
    assert_eq!(rings[0].rect, around(spot, RING_WIDTH));
}
