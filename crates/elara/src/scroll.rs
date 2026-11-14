//! Turning trackpad and mouse wheel scrolling into niri binds and gestures.
//!
//! niri has two things here. A mouse wheel turned with modifiers held triggers the
//! `WheelScrollUp`, `WheelScrollDown`, `WheelScrollLeft` and `WheelScrollRight` binds from the
//! config. And a three-finger swipe on a touchpad drags the strip of columns sideways or the
//! stack of workspaces up and down, following the fingers.
//!
//! macOS keeps three-finger swipes for itself, so the swipe is mapped to scrolling with two
//! fingers while the mod key is held.

use std::collections::HashMap;
use std::time::Instant;

use elara_macos::{ScrollEvent, ScrollPhase};
use niri_config::{Action, Config, Key, Modifiers, Trigger};

use crate::binds::{find_bind, mod_key};
use crate::keys::modifiers;

/// What a scroll event asks of the window manager.
#[derive(Debug, Clone, PartialEq)]
pub enum ScrollInput {
    /// A wheel step triggered a bind.
    Action(Action),
    /// The fingers moved by this much, in points, in the direction content should move.
    GestureUpdate { delta_x: f64, delta_y: f64 },
    /// The fingers lifted.
    GestureEnd,
}

/// What to do with a scroll event.
#[derive(Debug, Clone, PartialEq)]
pub struct ScrollResponse {
    /// Whether to keep the event from reaching any application.
    pub swallow: bool,
    pub input: Option<ScrollInput>,
}

impl ScrollResponse {
    const PASS: Self = Self {
        swallow: false,
        input: None,
    };
    const SWALLOW: Self = Self {
        swallow: true,
        input: None,
    };
}

/// Decides, scroll event by scroll event, which ones are for the window manager.
#[derive(Debug, Default)]
pub struct ScrollHandler {
    /// When each wheel bind with a cooldown last fired.
    last_fired: HashMap<Key, Instant>,
    /// Whether a two-finger gesture is being fed to the layout.
    gesture_active: bool,
    /// Whether the coasting that follows the last gesture should be dropped.
    swallow_momentum: bool,
}

impl ScrollHandler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Handles one scroll event against `config`. `now` is the current time, which decides
    /// whether a bind's cooldown has passed.
    pub fn handle(&mut self, config: &Config, event: &ScrollEvent, now: Instant) -> ScrollResponse {
        if event.continuous {
            self.handle_gesture(config, event)
        } else {
            self.handle_wheel(config, event, now)
        }
    }

    fn handle_gesture(&mut self, config: &Config, event: &ScrollEvent) -> ScrollResponse {
        // After the fingers lift, the system keeps sending scroll events that slow to a stop.
        // niri's gesture code does its own coasting from the speed of the fingers, so after
        // one of ours these are dropped. They must not reach the application either: it
        // never saw the gesture.
        if event.momentum {
            return if self.swallow_momentum {
                ScrollResponse::SWALLOW
            } else {
                ScrollResponse::PASS
            };
        }

        // Only the mod key alone, as niri's swipe takes no modifiers at all.
        let mod_only = modifiers(event.modifiers) == mod_key(config).to_modifiers();
        if !mod_only {
            self.swallow_momentum = false;
            if self.gesture_active {
                // The mod key was released with the fingers still down.
                self.gesture_active = false;
                return ScrollResponse {
                    swallow: false,
                    input: Some(ScrollInput::GestureEnd),
                };
            }
            return ScrollResponse::PASS;
        }

        if event.phase == ScrollPhase::Ended {
            let was_active = std::mem::take(&mut self.gesture_active);
            self.swallow_momentum = true;
            return ScrollResponse {
                swallow: true,
                input: was_active.then_some(ScrollInput::GestureEnd),
            };
        }

        self.gesture_active = true;
        self.swallow_momentum = false;
        if event.delta_x == 0. && event.delta_y == 0. {
            return ScrollResponse::SWALLOW;
        }
        ScrollResponse {
            swallow: true,
            input: Some(ScrollInput::GestureUpdate {
                delta_x: event.delta_x,
                delta_y: event.delta_y,
            }),
        }
    }

    fn handle_wheel(
        &mut self,
        config: &Config,
        event: &ScrollEvent,
        now: Instant,
    ) -> ScrollResponse {
        let held = modifiers(event.modifiers);
        let (mut delta_x, mut delta_y) = (event.delta_x, event.delta_y);

        // macOS turns a vertical wheel into a horizontal one while Shift is held. niri binds
        // such as Mod+Shift+WheelScrollDown are written for the wheel as it physically is, so
        // undo that.
        if held.contains(Modifiers::SHIFT) && delta_y == 0. {
            (delta_x, delta_y) = (0., delta_x);
        }

        // One step per event, however far the system says an accelerated wheel has scrolled:
        // a bind is an action, and a flick of the wheel should not run it ten times.
        let trigger = if delta_y > 0. {
            Trigger::WheelScrollUp
        } else if delta_y < 0. {
            Trigger::WheelScrollDown
        } else if delta_x > 0. {
            Trigger::WheelScrollLeft
        } else if delta_x < 0. {
            Trigger::WheelScrollRight
        } else {
            return ScrollResponse::PASS;
        };

        let Some(bind) = find_bind(config, trigger, held) else {
            return ScrollResponse::PASS;
        };

        if let Some(cooldown) = bind.cooldown {
            if let Some(last) = self.last_fired.get(&bind.key) {
                if now.saturating_duration_since(*last) < cooldown {
                    return ScrollResponse::SWALLOW;
                }
            }
            self.last_fired.insert(bind.key, now);
        }

        ScrollResponse {
            swallow: true,
            input: Some(ScrollInput::Action(bind.action)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use elara_macos::KeyModifiers;

    use super::*;

    const OPTION: KeyModifiers = KeyModifiers {
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

    fn wheel(delta_x: f64, delta_y: f64, modifiers: KeyModifiers) -> ScrollEvent {
        ScrollEvent {
            delta_x,
            delta_y,
            continuous: false,
            phase: ScrollPhase::None,
            momentum: false,
            modifiers,
        }
    }

    fn fingers(
        delta_x: f64,
        delta_y: f64,
        phase: ScrollPhase,
        modifiers: KeyModifiers,
    ) -> ScrollEvent {
        ScrollEvent {
            delta_x,
            delta_y,
            continuous: true,
            phase,
            momentum: false,
            modifiers,
        }
    }

    fn action(response: ScrollResponse) -> Option<Action> {
        match response.input {
            Some(ScrollInput::Action(action)) => Some(action),
            _ => None,
        }
    }

    #[test]
    fn wheel_binds_from_the_default_config() {
        let config = crate::config::default_config();
        let mut handler = ScrollHandler::new();
        let now = Instant::now();
        let later = |ms| now + Duration::from_millis(ms);

        // Mod+wheel switches workspaces.
        let down = handler.handle(&config, &wheel(0., -1., OPTION), now);
        assert!(down.swallow);
        assert_eq!(action(down), Some(Action::FocusWorkspaceDown));
        let up = handler.handle(&config, &wheel(0., 1., OPTION), later(200));
        assert_eq!(action(up), Some(Action::FocusWorkspaceUp));

        // A wheel that tilts sideways moves between columns.
        let right = handler.handle(&config, &wheel(-1., 0., OPTION), now);
        assert_eq!(action(right), Some(Action::FocusColumnRight));
        let left = handler.handle(&config, &wheel(1., 0., OPTION), now);
        assert_eq!(action(left), Some(Action::FocusColumnLeft));
    }

    #[test]
    fn shift_wheel_is_read_as_the_vertical_wheel_it_is() {
        let config = crate::config::default_config();
        let mut handler = ScrollHandler::new();
        let shift = KeyModifiers {
            shift: true,
            ..OPTION
        };

        // With Shift held macOS reports the vertical wheel on the horizontal axis.
        let down = handler.handle(&config, &wheel(-1., 0., shift), Instant::now());
        assert_eq!(action(down), Some(Action::FocusColumnRight));
        let up = handler.handle(&config, &wheel(1., 0., shift), Instant::now());
        assert_eq!(action(up), Some(Action::FocusColumnLeft));
    }

    #[test]
    fn a_wheel_without_the_mod_key_is_left_alone() {
        let config = crate::config::default_config();
        let mut handler = ScrollHandler::new();

        let plain = handler.handle(&config, &wheel(0., -3., NONE), Instant::now());
        assert_eq!(plain, ScrollResponse::PASS);
    }

    #[test]
    fn accelerated_wheel_steps_count_once_and_cooldowns_hold() {
        let config = crate::config::default_config();
        let mut handler = ScrollHandler::new();
        let start = Instant::now();
        let at = |ms| start + Duration::from_millis(ms);
        let fast = wheel(0., -7., OPTION);

        // The default config gives the workspace binds a 150 ms cooldown.
        assert_eq!(
            action(handler.handle(&config, &fast, at(0))),
            Some(Action::FocusWorkspaceDown)
        );
        assert_eq!(
            handler.handle(&config, &fast, at(50)),
            ScrollResponse::SWALLOW
        );
        assert_eq!(
            handler.handle(&config, &fast, at(149)),
            ScrollResponse::SWALLOW
        );
        assert_eq!(
            action(handler.handle(&config, &fast, at(150))),
            Some(Action::FocusWorkspaceDown)
        );

        // The column binds have none.
        let tilt = wheel(-1., 0., OPTION);
        for ms in [200, 201, 202] {
            assert_eq!(
                action(handler.handle(&config, &tilt, at(ms))),
                Some(Action::FocusColumnRight)
            );
        }
    }

    #[test]
    fn two_fingers_with_the_mod_key_drive_a_gesture() {
        let config = crate::config::default_config();
        let mut handler = ScrollHandler::new();
        let now = Instant::now();
        let mut feed = |event: ScrollEvent| handler.handle(&config, &event, now);

        assert_eq!(
            feed(fingers(0., 0., ScrollPhase::Began, OPTION)),
            ScrollResponse::SWALLOW
        );
        assert_eq!(
            feed(fingers(-12., 1., ScrollPhase::Changed, OPTION)),
            ScrollResponse {
                swallow: true,
                input: Some(ScrollInput::GestureUpdate {
                    delta_x: -12.,
                    delta_y: 1.
                }),
            }
        );
        assert_eq!(
            feed(fingers(0., 0., ScrollPhase::Ended, OPTION)),
            ScrollResponse {
                swallow: true,
                input: Some(ScrollInput::GestureEnd),
            }
        );

        // The coasting that follows is dropped, not passed on and not fed to the layout.
        let coast = ScrollEvent {
            momentum: true,
            ..fingers(-8., 0., ScrollPhase::None, OPTION)
        };
        assert_eq!(feed(coast), ScrollResponse::SWALLOW);
        // Even if the mod key has been let go by then.
        let coast_unmodified = ScrollEvent {
            modifiers: NONE,
            ..coast
        };
        assert_eq!(feed(coast_unmodified), ScrollResponse::SWALLOW);
    }

    #[test]
    fn ordinary_scrolling_and_its_coasting_pass_through() {
        let config = crate::config::default_config();
        let mut handler = ScrollHandler::new();
        let now = Instant::now();
        let mut feed = |event: ScrollEvent| handler.handle(&config, &event, now);

        assert_eq!(
            feed(fingers(0., -20., ScrollPhase::Changed, NONE)),
            ScrollResponse::PASS
        );
        let coast = ScrollEvent {
            momentum: true,
            ..fingers(0., -9., ScrollPhase::None, NONE)
        };
        assert_eq!(feed(coast), ScrollResponse::PASS);

        // Other modifier combinations are not the gesture either: Command+scroll zooms in
        // some applications, and Option+Shift is not the mod key alone.
        let command = KeyModifiers {
            command: true,
            ..NONE
        };
        assert_eq!(
            feed(fingers(5., 0., ScrollPhase::Changed, command)),
            ScrollResponse::PASS
        );
        let option_shift = KeyModifiers {
            shift: true,
            ..OPTION
        };
        assert_eq!(
            feed(fingers(5., 0., ScrollPhase::Changed, option_shift)),
            ScrollResponse::PASS
        );
    }

    #[test]
    fn letting_go_of_the_mod_key_mid_gesture_ends_it() {
        let config = crate::config::default_config();
        let mut handler = ScrollHandler::new();
        let now = Instant::now();
        let mut feed = |event: ScrollEvent| handler.handle(&config, &event, now);

        feed(fingers(-30., 0., ScrollPhase::Changed, OPTION));
        assert_eq!(
            feed(fingers(-30., 0., ScrollPhase::Changed, NONE)),
            ScrollResponse {
                swallow: false,
                input: Some(ScrollInput::GestureEnd),
            }
        );
        // From here on it is ordinary scrolling.
        assert_eq!(
            feed(fingers(-30., 0., ScrollPhase::Changed, NONE)),
            ScrollResponse::PASS
        );
    }
}
