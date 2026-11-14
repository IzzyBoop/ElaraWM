//! Moving and resizing windows by dragging them with the mod key held.
//!
//! In niri, holding Mod and dragging a window with the left mouse button moves it, and
//! dragging with the right button resizes it by the nearest edge or corner. A tiled window
//! that is dragged leaves its column and is dropped wherever the pointer is: between two
//! columns, into a column, or onto another workspace. Pressing the other button while moving
//! switches the window between floating and tiled. Two quick right presses on the side of a
//! tiled window toggle its column between its width and the full width, and two on its top
//! or bottom reset its height.
//!
//! niri does this with pointer grabs: `src/input/mod.rs`, `move_grab.rs` and `resize_grab.rs`.
//! This file has both halves of the same thing. [`PointerHandler`] sits in the input tap and
//! decides which mouse events applications never get to see. The methods on [`State`] carry
//! them out against the layout.

use elara_layout::utils::{get_monotonic_time, ResizeEdge};
use elara_macos::{
    MouseButton, MouseEvent, MouseEventKind, Point as GlobalPoint, WindowBackend, WindowId,
};
use niri_config::Config;
use smithay::output::Output;
use smithay::utils::{Logical, Point};

use crate::binds::mod_key;
use crate::keys::modifiers;
use crate::State;

/// How close together two presses must be to count as a double click. niri's value.
const DOUBLE_CLICK_TIME: std::time::Duration = std::time::Duration::from_millis(400);

/// How far the pointer must travel before a press turns into a move. niri's value.
const MOVE_START_DISTANCE: f64 = 8.;

/// What a mouse event asks of the window manager.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PointerInput {
    Press {
        button: MouseButton,
        position: GlobalPoint,
    },
    Motion {
        position: GlobalPoint,
    },
    Release {
        button: MouseButton,
    },
}

/// What to do with a mouse event.
#[derive(Debug, Clone, PartialEq)]
pub struct PointerResponse {
    /// Whether to keep the event from reaching any application.
    pub swallow: bool,
    /// What to hand to [`State`]. A press that arrives while the previous one was never
    /// released yields two: the missing release, then the press.
    pub inputs: Vec<PointerInput>,
}

impl PointerResponse {
    fn pass() -> Self {
        Self {
            swallow: false,
            inputs: Vec::new(),
        }
    }

    fn swallow(inputs: Vec<PointerInput>) -> Self {
        Self {
            swallow: true,
            inputs,
        }
    }
}

/// Decides, mouse event by mouse event, which ones are for the window manager.
///
/// A press with the mod key held on a managed window starts a grab. From then until that
/// button is released, every mouse event belongs to the window manager, whether or not the mod
/// key is still held: an application that never saw the button go down must not see it come
/// up.
#[derive(Debug, Default)]
pub struct PointerHandler {
    /// The button that started the grab in progress.
    grab: Option<MouseButton>,
    /// The other button, if it was pressed during the grab and is still down.
    other_down: bool,
}

impl PointerHandler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Handles one mouse event against `config`. `can_grab` says whether a press of this
    /// button at this position would start a move or a resize; see [`State::can_grab`].
    pub fn handle(
        &mut self,
        config: &Config,
        event: &MouseEvent,
        can_grab: impl FnOnce(MouseButton, GlobalPoint) -> bool,
    ) -> PointerResponse {
        let position = event.position;
        match (event.kind, self.grab) {
            (MouseEventKind::Pressed(button), None) => {
                // As in niri, other modifiers may be held as well.
                let mod_down = modifiers(event.modifiers).contains(mod_key(config).to_modifiers());
                if !mod_down || !can_grab(button, position) {
                    return PointerResponse::pass();
                }
                self.grab = Some(button);
                self.other_down = false;
                PointerResponse::swallow(vec![PointerInput::Press { button, position }])
            }
            (MouseEventKind::Pressed(button), Some(held)) if button == held => {
                // The release never arrived, which happens if the system switched the tap off
                // for a moment. End the old grab and treat this as a new press.
                self.grab = None;
                let mut response = self.handle(config, event, can_grab);
                response
                    .inputs
                    .insert(0, PointerInput::Release { button: held });
                response
            }
            (MouseEventKind::Pressed(button), Some(_)) => {
                self.other_down = true;
                PointerResponse::swallow(vec![PointerInput::Press { button, position }])
            }
            (MouseEventKind::Dragged, Some(_)) => {
                PointerResponse::swallow(vec![PointerInput::Motion { position }])
            }
            (MouseEventKind::Released(button), Some(held)) if button == held => {
                self.grab = None;
                PointerResponse::swallow(vec![PointerInput::Release { button }])
            }
            (MouseEventKind::Released(_), _) if self.other_down => {
                // The release of a press that was swallowed, during the grab or just after.
                self.other_down = false;
                PointerResponse::swallow(Vec::new())
            }
            _ => PointerResponse::pass(),
        }
    }
}

/// A window being moved or resized with the mouse.
#[derive(Debug)]
pub(crate) enum PointerGrab {
    Move {
        window: WindowId,
        button: MouseButton,
        /// Where the button went down.
        start: GlobalPoint,
        start_output: Output,
        start_pos_within_output: Point<f64, Logical>,
        /// Where the pointer was last seen.
        last: GlobalPoint,
        /// Whether the pointer has travelled far enough for the layout to have been told.
        /// Until then this may still turn out to be a click.
        moving: bool,
    },
    Resize {
        window: WindowId,
        button: MouseButton,
        start: GlobalPoint,
    },
    /// A press that did all it was going to do when it happened, such as the second of a
    /// double click.
    Spent { button: MouseButton },
}

impl PointerGrab {
    fn button(&self) -> MouseButton {
        match self {
            Self::Move { button, .. } | Self::Resize { button, .. } | Self::Spent { button } => {
                *button
            }
        }
    }
}

fn delta(from: GlobalPoint, to: GlobalPoint) -> Point<f64, Logical> {
    Point::from((to.x - from.x, to.y - from.y))
}

impl<B: WindowBackend> State<B> {
    /// Returns the display under a point in global coordinates, as the layout's output and
    /// the point within it.
    fn output_under(&self, position: GlobalPoint) -> Option<(Output, Point<f64, Logical>)> {
        let display = self
            .displays
            .iter()
            .find(|display| display.info.frame.contains(position))?;
        let pos_within_output = Point::from((
            position.x - display.info.frame.x,
            position.y - display.info.frame.y,
        ));
        Some((display.output.clone(), pos_within_output))
    }

    /// Returns the window the layout has at a point in global coordinates.
    fn window_under(&self, position: GlobalPoint) -> Option<WindowId> {
        let (output, pos_within_output) = self.output_under(position)?;
        let (window, _hit) = self.layout.window_under(&output, pos_within_output)?;
        Some(window.window_id())
    }

    /// Whether pressing `button` at `position` with the mod key held would start moving or
    /// resizing a window, and so must be kept from the application under the pointer.
    ///
    /// The layout decides which window is at a position, from where it put them. A window it
    /// does not manage, such as a menu or a popover, is not known to it: a press on one of
    /// those lands on whichever managed window is beneath.
    pub fn can_grab(&self, button: MouseButton, position: GlobalPoint) -> bool {
        if self.window_under(position).is_none() {
            return false;
        }
        match button {
            MouseButton::Left => true,
            MouseButton::Right => self.resize_edges_under(position).is_some(),
        }
    }

    fn resize_edges_under(&self, position: GlobalPoint) -> Option<ResizeEdge> {
        let (output, pos_within_output) = self.output_under(position)?;
        self.layout
            .resize_edges_under(&output, pos_within_output)
            .filter(|edges| !edges.is_empty())
    }

    /// Carries out a mouse event that [`PointerHandler`] took. The windows follow on the next
    /// [`tick()`](Self::tick).
    pub fn pointer_input(&mut self, input: PointerInput) {
        self.refresh_clock();
        match input {
            PointerInput::Press { button, position } => self.pointer_press(button, position),
            PointerInput::Motion { position } => self.pointer_motion(position),
            PointerInput::Release { button } => self.pointer_release(button),
        }
    }

    /// niri's `on_pointer_button()`, for a press with the mod key held.
    fn pointer_press(&mut self, button: MouseButton, position: GlobalPoint) {
        if let Some(grab) = &self.pointer_grab {
            // When moving with the left button, right toggles floating, and vice versa.
            if matches!(grab, PointerGrab::Move { .. }) && grab.button() != button {
                self.toggle_floating_while_moving(position);
            }
            return;
        }

        let Some((output, pos_within_output)) = self.output_under(position) else {
            return;
        };
        let Some(window) = self.window_under(position) else {
            return;
        };
        // Whatever comes of the press, it was swallowed, and so will its release be.
        self.pointer_grab = Some(PointerGrab::Spent { button });
        self.focus_dirty = true;

        match button {
            MouseButton::Left => {
                debug!(id = window.0, "mod+left press on window");
                self.layout.activate_window(&window);
                self.pointer_grab = Some(PointerGrab::Move {
                    window,
                    button,
                    start: position,
                    start_output: output,
                    start_pos_within_output: pos_within_output,
                    last: position,
                    moving: false,
                });
            }
            MouseButton::Right => {
                let Some(edges) = self.resize_edges_under(position) else {
                    self.layout.activate_window(&window);
                    return;
                };
                debug!(id = window.0, ?edges, "mod+right press on window");

                // See if we got a double resize-click gesture.
                let time = get_monotonic_time();
                let handle = &self.windows[&window];
                let last_cell = handle.last_interactive_resize_start();
                let mut last = last_cell.get();
                last_cell.set(Some((time, edges)));

                // Floating windows don't have either of the double-resize-click gestures, so
                // just allow it to resize.
                if handle.is_floating() {
                    last = None;
                    last_cell.set(None);
                }

                if let Some((last_time, last_edges)) = last {
                    if time.saturating_sub(last_time) <= DOUBLE_CLICK_TIME {
                        // Allow quick resize after a triple click.
                        last_cell.set(None);

                        let intersection = edges.intersection(last_edges);
                        if intersection.intersects(ResizeEdge::LEFT_RIGHT) {
                            self.layout.activate_window(&window);
                            self.layout.toggle_full_width();
                        }
                        if intersection.intersects(ResizeEdge::TOP_BOTTOM) {
                            self.layout.activate_window(&window);
                            self.layout.reset_window_height(Some(&window));
                        }
                        return;
                    }
                }

                self.layout.activate_window(&window);
                if self.layout.interactive_resize_begin(window, edges) {
                    self.resizing = Some((window, edges));
                    self.pointer_grab = Some(PointerGrab::Resize {
                        window,
                        button,
                        start: position,
                    });
                }
            }
        }
    }

    /// niri's `MoveGrab::on_frame()` and `ResizeGrab::motion()`.
    fn pointer_motion(&mut self, position: GlobalPoint) {
        let ongoing = match &mut self.pointer_grab {
            None | Some(PointerGrab::Spent { .. }) => return,
            Some(PointerGrab::Resize { window, start, .. }) => {
                self.windows.contains_key(window)
                    && self
                        .layout
                        .interactive_resize_update(window, delta(*start, position))
            }
            Some(PointerGrab::Move {
                window,
                start,
                start_output,
                start_pos_within_output,
                last,
                moving,
                ..
            }) => {
                let mut delta_since_last = delta(*last, position);
                *last = position;

                if !*moving {
                    // Check if the window has closed.
                    if !self.windows.contains_key(window) {
                        self.drop_grab();
                        return;
                    }

                    // Check if the gesture moved far enough to decide.
                    let c = delta(*start, position);
                    if c.x * c.x + c.y * c.y < MOVE_START_DISTANCE * MOVE_START_DISTANCE {
                        return;
                    }
                    if !self.layout.interactive_move_begin(
                        *window,
                        start_output,
                        *start_pos_within_output,
                    ) {
                        // Can no longer start the move.
                        self.drop_grab();
                        return;
                    }
                    *moving = true;
                    // Apply the whole delta that accumulated during recognizing.
                    delta_since_last = c;
                }

                let window = *window;
                // Between displays there is nothing to move over; wait for the pointer to
                // arrive somewhere.
                let Some((output, pos_within_output)) = self.output_under(position) else {
                    return;
                };
                self.layout.interactive_move_update(
                    &window,
                    delta_since_last,
                    output,
                    pos_within_output,
                )
            }
        };

        if !ongoing {
            self.drop_grab();
        }
    }

    /// niri's `MoveGrab::on_toggle_floating()`.
    fn toggle_floating_while_moving(&mut self, position: GlobalPoint) {
        let Some(PointerGrab::Move {
            window,
            start,
            start_output,
            start_pos_within_output,
            moving,
            ..
        }) = &mut self.pointer_grab
        else {
            return;
        };
        let window = *window;

        // Start move if still recognizing.
        if !*moving {
            let started =
                self.layout
                    .interactive_move_begin(window, start_output, *start_pos_within_output);
            if !started {
                self.drop_grab();
                return;
            }
            *moving = true;

            // Apply the delta accumulated during recognizing.
            let accumulated = delta(*start, position);
            let ongoing = self.output_under(position).is_some_and(|(output, pos)| {
                self.layout
                    .interactive_move_update(&window, accumulated, output, pos)
            });
            if !ongoing {
                self.drop_grab();
                return;
            }
        }

        self.layout.toggle_window_floating(Some(&window));
    }

    fn pointer_release(&mut self, button: MouseButton) {
        if self
            .pointer_grab
            .as_ref()
            .is_some_and(|grab| grab.button() == button)
        {
            self.drop_grab();
        }
    }

    /// Ends the grab, as when its button is released. niri's `on_ungrab()`.
    fn drop_grab(&mut self) {
        match self.pointer_grab.take() {
            None | Some(PointerGrab::Spent { .. }) => return,
            Some(PointerGrab::Move { window, moving, .. }) => {
                if moving {
                    debug!(id = window.0, "dropping moved window");
                    self.layout.interactive_move_end(&window);
                } else {
                    // Activate the window on release.
                    self.layout.activate_window(&window);
                }
            }
            Some(PointerGrab::Resize { window, .. }) => {
                self.layout.interactive_resize_end(&window);
            }
        }
        // Where the window was dropped decides what is focused now.
        self.focus_dirty = true;
    }
}
