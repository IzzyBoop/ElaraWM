//! Carrying out niri's actions.
//!
//! This follows niri's `State::do_action()` arm by arm for every action that concerns the
//! layout. What niri does around the layout call (warping the pointer, queueing a redraw,
//! closing the screenshot UI) has no counterpart here and is left out. Actions that belong to
//! parts of niri with no macOS equivalent yet are logged and ignored.

use std::process::{Command, Stdio};

use elara_layout::layout::scrolling::ScrollDirection;
use elara_layout::layout::workspace::WorkspaceId;
use elara_layout::layout::{ActivateWindow, LayoutElement as _};
use elara_macos::{WindowBackend, WindowId};
use niri_config::{Action, OutputName, WorkspaceReference};
use smithay::output::Output;

use crate::state::State;

/// A side of a display, for finding the display next to it.
#[derive(Debug, Clone, Copy)]
enum Direction {
    Left,
    Right,
    Up,
    Down,
}

impl<B: WindowBackend> State<B> {
    /// Carries out `action`, then applies the result to the windows.
    ///
    /// Returns `false` if the action is one ElaraWM does not support, in which case nothing
    /// happened.
    pub fn do_action(&mut self, action: Action) -> bool {
        debug!(?action, "action");
        let supported = self.dispatch(action);
        // Whatever the action did to the layout's focus, the system should follow.
        self.focus_dirty = true;
        self.apply();
        supported
    }

    /// Returns whether the action was recognised.
    fn dispatch(&mut self, action: Action) -> bool {
        match action {
            Action::Quit(_skip_confirmation) => {
                info!("quitting as requested");
                self.quit_requested = true;
            }
            Action::Spawn(command) => spawn(command),
            Action::SpawnSh(command) => {
                spawn(vec!["/bin/sh".to_owned(), "-c".to_owned(), command]);
            }

            Action::CloseWindow => {
                if let Some(id) = self.focused_id() {
                    self.close_window(id);
                }
            }
            Action::CloseWindowById(id) => {
                if let Some(id) = self.window_by_id(id) {
                    self.close_window(id);
                }
            }
            Action::FullscreenWindow => {
                if let Some(id) = self.focused_id() {
                    self.layout.toggle_fullscreen(&id);
                }
            }
            Action::FullscreenWindowById(id) => {
                if let Some(id) = self.window_by_id(id) {
                    self.layout.toggle_fullscreen(&id);
                }
            }
            Action::ToggleWindowedFullscreen => {
                if let Some(id) = self.focused_id() {
                    self.layout.toggle_windowed_fullscreen(&id);
                }
            }
            Action::ToggleWindowedFullscreenById(id) => {
                if let Some(id) = self.window_by_id(id) {
                    self.layout.toggle_windowed_fullscreen(&id);
                }
            }
            Action::FocusWindow(id) => {
                if let Some(id) = self.window_by_id(id) {
                    self.layout.activate_window(&id);
                }
            }
            Action::FocusWindowInColumn(index) => self.layout.focus_window_in_column(index),
            Action::FocusWindowPrevious => {
                let current = self.focused_id();
                let previous = self
                    .focus_history
                    .iter()
                    .rev()
                    .find(|id| Some(**id) != current && self.windows.contains_key(id))
                    .copied();
                if let Some(id) = previous {
                    self.layout.activate_window(&id);
                }
            }

            Action::MoveColumnLeft => self.layout.move_left(),
            Action::MoveColumnRight => self.layout.move_right(),
            Action::MoveColumnToFirst => self.layout.move_column_to_first(),
            Action::MoveColumnToLast => self.layout.move_column_to_last(),
            Action::MoveColumnLeftOrToMonitorLeft => {
                if let Some(output) = self.output_towards(Direction::Left) {
                    self.layout.move_column_left_or_to_output(&output);
                } else {
                    self.layout.move_left();
                }
            }
            Action::MoveColumnRightOrToMonitorRight => {
                if let Some(output) = self.output_towards(Direction::Right) {
                    self.layout.move_column_right_or_to_output(&output);
                } else {
                    self.layout.move_right();
                }
            }
            Action::MoveColumnToIndex(idx) => self.layout.move_column_to_index(idx),
            Action::MoveWindowDown => self.layout.move_down(),
            Action::MoveWindowUp => self.layout.move_up(),
            Action::MoveWindowDownOrToWorkspaceDown => {
                self.layout.move_down_or_to_workspace_down();
            }
            Action::MoveWindowUpOrToWorkspaceUp => self.layout.move_up_or_to_workspace_up(),
            Action::ConsumeOrExpelWindowLeft => self.layout.consume_or_expel_window_left(None),
            Action::ConsumeOrExpelWindowLeftById(id) => {
                if let Some(id) = self.window_by_id(id) {
                    self.layout.consume_or_expel_window_left(Some(&id));
                }
            }
            Action::ConsumeOrExpelWindowRight => self.layout.consume_or_expel_window_right(None),
            Action::ConsumeOrExpelWindowRightById(id) => {
                if let Some(id) = self.window_by_id(id) {
                    self.layout.consume_or_expel_window_right(Some(&id));
                }
            }
            Action::ConsumeWindowIntoColumn => self.layout.consume_into_column(),
            Action::ExpelWindowFromColumn => self.layout.expel_from_column(),
            Action::SwapWindowLeft => {
                self.layout.swap_window_in_direction(ScrollDirection::Left);
            }
            Action::SwapWindowRight => {
                self.layout.swap_window_in_direction(ScrollDirection::Right);
            }

            Action::FocusColumnLeft => self.layout.focus_left(),
            Action::FocusColumnRight => self.layout.focus_right(),
            Action::FocusColumnFirst => self.layout.focus_column_first(),
            Action::FocusColumnLast => self.layout.focus_column_last(),
            Action::FocusColumnRightOrFirst => self.layout.focus_column_right_or_first(),
            Action::FocusColumnLeftOrLast => self.layout.focus_column_left_or_last(),
            Action::FocusColumn(index) => self.layout.focus_column(index),
            Action::FocusWindowOrMonitorUp => {
                if let Some(output) = self.output_towards(Direction::Up) {
                    self.layout.focus_window_up_or_output(&output);
                } else {
                    self.layout.focus_up();
                }
            }
            Action::FocusWindowOrMonitorDown => {
                if let Some(output) = self.output_towards(Direction::Down) {
                    self.layout.focus_window_down_or_output(&output);
                } else {
                    self.layout.focus_down();
                }
            }
            Action::FocusColumnOrMonitorLeft => {
                if let Some(output) = self.output_towards(Direction::Left) {
                    self.layout.focus_column_left_or_output(&output);
                } else {
                    self.layout.focus_left();
                }
            }
            Action::FocusColumnOrMonitorRight => {
                if let Some(output) = self.output_towards(Direction::Right) {
                    self.layout.focus_column_right_or_output(&output);
                } else {
                    self.layout.focus_right();
                }
            }
            Action::FocusWindowDown => self.layout.focus_down(),
            Action::FocusWindowUp => self.layout.focus_up(),
            Action::FocusWindowDownOrColumnLeft => self.layout.focus_down_or_left(),
            Action::FocusWindowDownOrColumnRight => self.layout.focus_down_or_right(),
            Action::FocusWindowUpOrColumnLeft => self.layout.focus_up_or_left(),
            Action::FocusWindowUpOrColumnRight => self.layout.focus_up_or_right(),
            Action::FocusWindowOrWorkspaceDown => self.layout.focus_window_or_workspace_down(),
            Action::FocusWindowOrWorkspaceUp => self.layout.focus_window_or_workspace_up(),
            Action::FocusWindowTop => self.layout.focus_window_top(),
            Action::FocusWindowBottom => self.layout.focus_window_bottom(),
            Action::FocusWindowDownOrTop => self.layout.focus_window_down_or_top(),
            Action::FocusWindowUpOrBottom => self.layout.focus_window_up_or_bottom(),

            Action::MoveWindowToWorkspaceDown(focus) => self.layout.move_to_workspace_down(focus),
            Action::MoveWindowToWorkspaceUp(focus) => self.layout.move_to_workspace_up(focus),
            Action::MoveWindowToWorkspace(reference, focus) => {
                if let Some((mut output, index)) = self.find_output_and_workspace_index(reference) {
                    // The source output is always the active output, so if the target output is
                    // also the active output, we don't need to use move_to_output().
                    if output.as_ref() == self.layout.active_output() {
                        output = None;
                    }
                    let activate = if focus {
                        ActivateWindow::Smart
                    } else {
                        ActivateWindow::No
                    };
                    if let Some(output) = output {
                        self.layout
                            .move_to_output(None, &output, Some(index), activate);
                    } else {
                        self.layout.move_to_workspace(None, index, activate);
                    }
                }
            }
            Action::MoveWindowToWorkspaceById {
                window_id,
                reference,
                focus,
            } => {
                let Some(id) = self.window_by_id(window_id) else {
                    return true;
                };
                if let Some((output, index)) = self.find_output_and_workspace_index(reference) {
                    let activate = if focus {
                        ActivateWindow::Smart
                    } else {
                        ActivateWindow::No
                    };
                    if let Some(output) = output {
                        self.layout
                            .move_to_output(Some(&id), &output, Some(index), activate);
                    } else {
                        self.layout.move_to_workspace(Some(&id), index, activate);
                    }
                }
            }
            Action::MoveColumnToWorkspaceDown(focus) => {
                self.layout.move_column_to_workspace_down(focus);
            }
            Action::MoveColumnToWorkspaceUp(focus) => {
                self.layout.move_column_to_workspace_up(focus);
            }
            Action::MoveColumnToWorkspace(reference, focus) => {
                if let Some((mut output, index)) = self.find_output_and_workspace_index(reference) {
                    if output.as_ref() == self.layout.active_output() {
                        output = None;
                    }
                    if let Some(output) = output {
                        self.layout
                            .move_column_to_output(&output, Some(index), focus);
                    } else {
                        self.layout.move_column_to_workspace(index, focus);
                    }
                }
            }

            Action::FocusWorkspaceDown => self.layout.switch_workspace_down(),
            Action::FocusWorkspaceUp => self.layout.switch_workspace_up(),
            Action::FocusWorkspace(reference) => {
                if let Some((mut output, index)) = self.find_output_and_workspace_index(reference) {
                    if output.as_ref() == self.layout.active_output() {
                        output = None;
                    }
                    if let Some(output) = output {
                        self.layout.focus_output(&output);
                        self.layout.switch_workspace(index);
                    } else if self.config.input.workspace_auto_back_and_forth {
                        self.layout.switch_workspace_auto_back_and_forth(index);
                    } else {
                        self.layout.switch_workspace(index);
                    }
                }
            }
            Action::FocusWorkspacePrevious => self.layout.switch_workspace_previous(),
            Action::MoveWorkspaceDown => self.layout.move_workspace_down(),
            Action::MoveWorkspaceUp => self.layout.move_workspace_up(),
            Action::MoveWorkspaceToIndex(new_idx) => {
                let new_idx = new_idx.saturating_sub(1);
                self.layout.move_workspace_to_idx(None, new_idx);
            }
            Action::MoveWorkspaceToIndexByRef { new_idx, reference } => {
                if let Some(res) = self.find_output_and_workspace_index(reference) {
                    let new_idx = new_idx.saturating_sub(1);
                    self.layout.move_workspace_to_idx(Some(res), new_idx);
                }
            }
            Action::SetWorkspaceName(name) => self.layout.set_workspace_name(name, None),
            Action::SetWorkspaceNameByRef { name, reference } => {
                self.layout.set_workspace_name(name, Some(reference));
            }
            Action::UnsetWorkspaceName => self.layout.unset_workspace_name(None),
            Action::UnsetWorkSpaceNameByRef(reference) => {
                self.layout.unset_workspace_name(Some(reference));
            }

            Action::ToggleColumnTabbedDisplay => self.layout.toggle_column_tabbed_display(),
            Action::SetColumnDisplay(display) => self.layout.set_column_display(display),
            Action::SwitchPresetColumnWidth => self.layout.toggle_width(true),
            Action::SwitchPresetColumnWidthBack => self.layout.toggle_width(false),
            Action::SwitchPresetWindowWidth => self.layout.toggle_window_width(None, true),
            Action::SwitchPresetWindowWidthBack => self.layout.toggle_window_width(None, false),
            Action::SwitchPresetWindowWidthById(id) => {
                if let Some(id) = self.window_by_id(id) {
                    self.layout.toggle_window_width(Some(&id), true);
                }
            }
            Action::SwitchPresetWindowWidthBackById(id) => {
                if let Some(id) = self.window_by_id(id) {
                    self.layout.toggle_window_width(Some(&id), false);
                }
            }
            Action::SwitchPresetWindowHeight => self.layout.toggle_window_height(None, true),
            Action::SwitchPresetWindowHeightBack => self.layout.toggle_window_height(None, false),
            Action::SwitchPresetWindowHeightById(id) => {
                if let Some(id) = self.window_by_id(id) {
                    self.layout.toggle_window_height(Some(&id), true);
                }
            }
            Action::SwitchPresetWindowHeightBackById(id) => {
                if let Some(id) = self.window_by_id(id) {
                    self.layout.toggle_window_height(Some(&id), false);
                }
            }
            Action::CenterColumn => self.layout.center_column(),
            Action::CenterWindow => self.layout.center_window(None),
            Action::CenterWindowById(id) => {
                if let Some(id) = self.window_by_id(id) {
                    self.layout.center_window(Some(&id));
                }
            }
            Action::CenterVisibleColumns => self.layout.center_visible_columns(),
            Action::MaximizeColumn => self.layout.toggle_full_width(),
            Action::MaximizeWindowToEdges => {
                if let Some(id) = self.focused_id() {
                    self.layout.toggle_maximized(&id);
                }
            }
            Action::MaximizeWindowToEdgesById(id) => {
                if let Some(id) = self.window_by_id(id) {
                    self.layout.toggle_maximized(&id);
                }
            }
            Action::SetColumnWidth(change) => self.layout.set_column_width(change),
            Action::SetWindowWidth(change) => self.layout.set_window_width(None, change),
            Action::SetWindowWidthById { id, change } => {
                if let Some(id) = self.window_by_id(id) {
                    self.layout.set_window_width(Some(&id), change);
                }
            }
            Action::SetWindowHeight(change) => self.layout.set_window_height(None, change),
            Action::SetWindowHeightById { id, change } => {
                if let Some(id) = self.window_by_id(id) {
                    self.layout.set_window_height(Some(&id), change);
                }
            }
            Action::ResetWindowHeight => self.layout.reset_window_height(None),
            Action::ResetWindowHeightById(id) => {
                if let Some(id) = self.window_by_id(id) {
                    self.layout.reset_window_height(Some(&id));
                }
            }
            Action::ExpandColumnToAvailableWidth => {
                self.layout.expand_column_to_available_width();
            }

            Action::FocusMonitorLeft => self.focus_output(self.output_towards(Direction::Left)),
            Action::FocusMonitorRight => self.focus_output(self.output_towards(Direction::Right)),
            Action::FocusMonitorDown => self.focus_output(self.output_towards(Direction::Down)),
            Action::FocusMonitorUp => self.focus_output(self.output_towards(Direction::Up)),
            Action::FocusMonitorPrevious => self.focus_output(self.output_cycled(false)),
            Action::FocusMonitorNext => self.focus_output(self.output_cycled(true)),
            Action::FocusMonitor(name) => self.focus_output(self.output_by_name(&name)),

            Action::MoveWindowToMonitorLeft => {
                self.move_window_to_output(self.output_towards(Direction::Left));
            }
            Action::MoveWindowToMonitorRight => {
                self.move_window_to_output(self.output_towards(Direction::Right));
            }
            Action::MoveWindowToMonitorDown => {
                self.move_window_to_output(self.output_towards(Direction::Down));
            }
            Action::MoveWindowToMonitorUp => {
                self.move_window_to_output(self.output_towards(Direction::Up));
            }
            Action::MoveWindowToMonitorPrevious => {
                self.move_window_to_output(self.output_cycled(false));
            }
            Action::MoveWindowToMonitorNext => {
                self.move_window_to_output(self.output_cycled(true));
            }
            Action::MoveWindowToMonitor(name) => {
                self.move_window_to_output(self.output_by_name(&name));
            }
            Action::MoveWindowToMonitorById { id, output } => {
                if let (Some(id), Some(output)) =
                    (self.window_by_id(id), self.output_by_name(&output))
                {
                    self.layout
                        .move_to_output(Some(&id), &output, None, ActivateWindow::Smart);
                }
            }

            Action::MoveColumnToMonitorLeft => {
                self.move_column_to_output(self.output_towards(Direction::Left));
            }
            Action::MoveColumnToMonitorRight => {
                self.move_column_to_output(self.output_towards(Direction::Right));
            }
            Action::MoveColumnToMonitorDown => {
                self.move_column_to_output(self.output_towards(Direction::Down));
            }
            Action::MoveColumnToMonitorUp => {
                self.move_column_to_output(self.output_towards(Direction::Up));
            }
            Action::MoveColumnToMonitorPrevious => {
                self.move_column_to_output(self.output_cycled(false));
            }
            Action::MoveColumnToMonitorNext => {
                self.move_column_to_output(self.output_cycled(true));
            }
            Action::MoveColumnToMonitor(name) => {
                self.move_column_to_output(self.output_by_name(&name));
            }

            Action::MoveWorkspaceToMonitorLeft => {
                self.move_workspace_to_output(self.output_towards(Direction::Left));
            }
            Action::MoveWorkspaceToMonitorRight => {
                self.move_workspace_to_output(self.output_towards(Direction::Right));
            }
            Action::MoveWorkspaceToMonitorDown => {
                self.move_workspace_to_output(self.output_towards(Direction::Down));
            }
            Action::MoveWorkspaceToMonitorUp => {
                self.move_workspace_to_output(self.output_towards(Direction::Up));
            }
            Action::MoveWorkspaceToMonitorPrevious => {
                self.move_workspace_to_output(self.output_cycled(false));
            }
            Action::MoveWorkspaceToMonitorNext => {
                self.move_workspace_to_output(self.output_cycled(true));
            }
            Action::MoveWorkspaceToMonitor(name) => {
                self.move_workspace_to_output(self.output_by_name(&name));
            }
            Action::MoveWorkspaceToMonitorByRef {
                output_name,
                reference,
            } => {
                if let Some((output, old_idx)) = self.find_output_and_workspace_index(reference) {
                    if let Some(new_output) = self.output_by_name(&output_name) {
                        self.layout
                            .move_workspace_to_output_by_id(old_idx, output, &new_output);
                    }
                }
            }

            Action::ToggleWindowFloating => self.layout.toggle_window_floating(None),
            Action::ToggleWindowFloatingById(id) => {
                if let Some(id) = self.window_by_id(id) {
                    self.layout.toggle_window_floating(Some(&id));
                }
            }
            Action::MoveWindowToFloating => self.layout.set_window_floating(None, true),
            Action::MoveWindowToFloatingById(id) => {
                if let Some(id) = self.window_by_id(id) {
                    self.layout.set_window_floating(Some(&id), true);
                }
            }
            Action::MoveWindowToTiling => self.layout.set_window_floating(None, false),
            Action::MoveWindowToTilingById(id) => {
                if let Some(id) = self.window_by_id(id) {
                    self.layout.set_window_floating(Some(&id), false);
                }
            }
            Action::FocusFloating => self.layout.focus_floating(),
            Action::FocusTiling => self.layout.focus_tiling(),
            Action::SwitchFocusBetweenFloatingAndTiling => {
                self.layout.switch_focus_floating_tiling();
            }
            Action::MoveFloatingWindowById { id, x, y } => {
                let id = match id {
                    Some(id) => match self.window_by_id(id) {
                        Some(id) => Some(id),
                        None => return true,
                    },
                    None => None,
                };
                self.layout.move_floating_window(id.as_ref(), x, y, true);
            }

            // Everything else drives a part of niri that has no macOS counterpart yet: the
            // screenshot UI, the overview, screencasts, the recent-windows switcher, keyboard
            // layouts, virtual terminals and monitor power.
            other => {
                warn!("action not supported on macOS yet: {other:?}");
                return false;
            }
        }
        true
    }

    fn focused_id(&self) -> Option<WindowId> {
        self.layout.focus().map(|window| *window.id())
    }

    /// Resolves the numeric window id that actions and IPC use.
    fn window_by_id(&self, id: u64) -> Option<WindowId> {
        let id = WindowId(id);
        self.windows.contains_key(&id).then_some(id)
    }

    fn close_window(&mut self, id: WindowId) {
        if let Err(err) = self.backend.close(id) {
            debug!(?id, %err, "could not close window");
        }
    }

    fn focus_output(&mut self, output: Option<Output>) {
        if let Some(output) = output {
            self.layout.focus_output(&output);
        }
    }

    fn move_window_to_output(&mut self, output: Option<Output>) {
        if let Some(output) = output {
            self.layout
                .move_to_output(None, &output, None, ActivateWindow::Smart);
            self.layout.focus_output(&output);
        }
    }

    fn move_column_to_output(&mut self, output: Option<Output>) {
        if let Some(output) = output {
            self.layout.move_column_to_output(&output, None, true);
            self.layout.focus_output(&output);
        }
    }

    fn move_workspace_to_output(&mut self, output: Option<Output>) {
        if let Some(output) = output {
            self.layout.move_workspace_to_output(&output);
        }
    }

    /// niri's `find_output_and_workspace_index()`.
    fn find_output_and_workspace_index(
        &self,
        reference: WorkspaceReference,
    ) -> Option<(Option<Output>, usize)> {
        let (index, workspace) = match reference {
            WorkspaceReference::Index(index) => {
                return Some((None, index.saturating_sub(1) as usize));
            }
            WorkspaceReference::Name(name) => self.layout.find_workspace_by_name(&name)?,
            WorkspaceReference::Id(id) => {
                let id = WorkspaceId::specific(id);
                self.layout.find_workspace_by_id(id)?
            }
        };

        Some((workspace.current_output().cloned(), index))
    }

    fn output_by_name(&self, name: &str) -> Option<Output> {
        self.displays
            .iter()
            .find(|display| {
                display
                    .output
                    .user_data()
                    .get::<OutputName>()
                    .is_some_and(|output_name| output_name.matches(name))
            })
            .map(|display| display.output.clone())
    }

    /// Returns the display next to the active one on the given side.
    ///
    /// As in niri: of the displays whose center lies on that side and that overlap the active
    /// display's row (for left and right) or column (for up and down), the nearest.
    fn output_towards(&self, direction: Direction) -> Option<Output> {
        let active = self.layout.active_output()?;
        let current = self
            .displays
            .iter()
            .find(|display| display.output == *active)?
            .info
            .frame;
        let center_x = |frame: &elara_macos::Rect| frame.x + frame.w / 2.;
        let center_y = |frame: &elara_macos::Rect| frame.y + frame.h / 2.;

        self.displays
            .iter()
            .filter(|display| display.output != *active)
            .filter_map(|display| {
                let frame = &display.info.frame;
                let same_row = frame.y < current.max_y() && current.y < frame.max_y();
                let same_column = frame.x < current.max_x() && current.x < frame.max_x();
                let distance = match direction {
                    Direction::Left if same_row => center_x(&current) - center_x(frame),
                    Direction::Right if same_row => center_x(frame) - center_x(&current),
                    Direction::Up if same_column => center_y(&current) - center_y(frame),
                    Direction::Down if same_column => center_y(frame) - center_y(&current),
                    _ => return None,
                };
                (distance > 0.).then_some((distance, display))
            })
            .min_by(|(a, _), (b, _)| a.total_cmp(b))
            .map(|(_, display)| display.output.clone())
    }

    /// Returns the display after or before the active one, wrapping around.
    fn output_cycled(&self, forward: bool) -> Option<Output> {
        let active = self.layout.active_output()?;
        let count = self.displays.len();
        let index = self
            .displays
            .iter()
            .position(|display| display.output == *active)?;
        let next = if forward {
            (index + 1) % count
        } else {
            (index + count - 1) % count
        };
        (next != index).then(|| self.displays[next].output.clone())
    }
}

/// Starts the programs the config lists under `spawn-at-startup` and `spawn-sh-at-startup`.
pub fn spawn_at_startup(config: &niri_config::Config) {
    for entry in &config.spawn_at_startup {
        spawn(entry.command.clone());
    }
    for entry in &config.spawn_sh_at_startup {
        spawn(vec![
            "/bin/sh".to_owned(),
            "-c".to_owned(),
            entry.command.clone(),
        ]);
    }
}

/// Starts a program, detached from ElaraWM.
fn spawn(command: Vec<String>) {
    let Some((program, args)) = command.split_first() else {
        return;
    };

    let result = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();

    match result {
        // Wait for it on a thread of its own so that it does not linger as a zombie.
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
        Err(err) => warn!("could not spawn {program:?}: {err}"),
    }
}
