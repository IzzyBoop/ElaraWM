//! The window manager: niri's layout on one side, the system's windows on the other.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use elara_layout::animation::Clock;
use elara_layout::layout::{ActivateWindow, AddWindowTarget, Layout, LayoutElement as _};
use elara_layout::utils::get_monotonic_time;
use elara_layout::utils::transaction::Transaction;
use elara_layout::window::{ResolvedWindowRules, WindowRef};
use elara_macos::{
    DisplayId, DisplayInfo, Event, Rect, WindowBackend, WindowId, WindowInfo, WindowKind,
};
use niri_config::{Config, OutputName};
use niri_ipc::PositionChange;
use smithay::desktop::layer_map_for_output;
use smithay::output::{Mode, Output, PhysicalProperties, Scale, Subpixel};
use smithay::utils::{Point, Rectangle, Size, Transform};

use crate::window::Window;

/// How many times one pass may go back and forth with the windows before giving up.
///
/// One round places every window. A window that takes a different size than asked makes the
/// layout move its neighbors, which needs a second round. Three leaves room for a window that
/// answers the correction with yet another size.
const MAX_APPLY_ROUNDS: usize = 3;

/// How long after asking the system to focus a window its confirmation is still expected.
const FOCUS_ECHO_WINDOW: Duration = Duration::from_secs(1);

/// A display and the layout output that stands for it.
pub(crate) struct Display {
    pub(crate) info: DisplayInfo,
    pub(crate) output: Output,
}

/// Where a window should be, in global coordinates, and whether any of it shows.
struct Target {
    id: WindowId,
    frame: Rect,
}

/// niri's layout, driving macOS windows through a [`WindowBackend`].
pub struct State<B: WindowBackend> {
    pub(crate) backend: B,
    pub(crate) config: Config,
    clock: Clock,
    pub(crate) layout: Layout<Window>,
    pub(crate) displays: Vec<Display>,
    /// Every window in the layout.
    pub(crate) windows: HashMap<WindowId, Window>,
    /// Windows that exist but are kept out of the layout because they are minimized or in
    /// native fullscreen. They rejoin it when that ends.
    sidelined: HashMap<WindowId, WindowInfo>,
    /// Whether the layout's idea of the focused window may differ from the system's.
    pub(crate) focus_dirty: bool,
    /// The window ElaraWM last asked the system to focus, and when. The system confirms a
    /// focus change with the same event it sends when the user switches windows. That
    /// confirmation arrives later, possibly after further actions, and must not be mistaken
    /// for the user switching back.
    pending_focus: Option<(WindowId, Instant)>,
    /// Windows in the order they last had focus, most recent last.
    pub(crate) focus_history: Vec<WindowId>,
    /// Set by the `quit` action.
    pub(crate) quit_requested: bool,
    /// True until the windows that already existed at startup have been added.
    is_at_startup: bool,
}

impl<B: WindowBackend> State<B> {
    /// Takes over the windows the backend knows about.
    ///
    /// Window positions snap into place: niri's animations still run inside the layout, but
    /// complete instantly, because every animation frame would be a round trip to another
    /// process.
    pub fn new(backend: B, config: Config) -> Self {
        let mut clock = Clock::with_time(get_monotonic_time());
        clock.set_complete_instantly(true);
        let layout = Layout::new(clock.clone(), &config);

        let mut state = Self {
            backend,
            config,
            clock,
            layout,
            displays: Vec::new(),
            windows: HashMap::new(),
            sidelined: HashMap::new(),
            focus_dirty: false,
            pending_focus: None,
            focus_history: Vec::new(),
            quit_requested: false,
            is_at_startup: true,
        };

        state.sync_displays();

        // Add existing windows left to right, display by display, so the columns start out in
        // the order the windows already had on screen.
        let mut existing = state.backend.windows();
        existing.sort_by(|a, b| {
            let key = |w: &WindowInfo| (state.display_index_at(w.frame), w.frame.x, w.id);
            key(a)
                .partial_cmp(&key(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for info in existing {
            state.add_window(info);
        }
        state.is_at_startup = false;

        // Whatever the backend queued while being read is already reflected above.
        state.backend.drain_events();

        if let Some(focused) = state.backend.focused_window() {
            if state.windows.contains_key(&focused) {
                state.layout.activate_window(&focused);
            }
        }

        state.apply();
        state
    }

    pub fn backend(&self) -> &B {
        &self.backend
    }

    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    pub fn layout(&self) -> &Layout<Window> {
        &self.layout
    }

    pub fn layout_mut(&mut self) -> &mut Layout<Window> {
        &mut self.layout
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Switches to a new config, as after the config file was edited.
    ///
    /// The layout picks up its new options at once, and every window's rules are resolved
    /// again. As in niri, rules that only apply when a window opens, such as `open-floating`,
    /// do not move windows that are already open.
    pub fn set_config(&mut self, config: Config) {
        self.layout.update_config(&config);
        self.config = config;

        let ids: Vec<WindowId> = self.windows.keys().copied().collect();
        for id in ids {
            self.recompute_rules(id);
        }

        self.apply();
    }

    /// Handles everything the system reported since the last call, then puts every window
    /// where the layout says it belongs.
    pub fn tick(&mut self) {
        // The usable part of a display changes without any display event when the Dock or
        // the menu bar is shown or hidden, so the displays are compared on every pass.
        self.sync_displays();

        for event in self.backend.drain_events() {
            self.handle_event(event);
        }
        self.apply();
    }

    fn handle_event(&mut self, event: Event) {
        trace!(?event, "handling event");
        match event {
            Event::WindowOpened(info) => self.add_window(info),
            Event::WindowClosed(id) => {
                self.sidelined.remove(&id);
                self.remove_window(id);
            }
            Event::WindowFrameChanged(id, frame) => self.window_frame_changed(id, frame),
            Event::WindowTitleChanged(id, title) => {
                if let Some(info) = self.sidelined.get_mut(&id) {
                    info.title = title.clone();
                }
                if let Some(window) = self.windows.get(&id) {
                    window.set_title(title);
                    self.recompute_rules(id);
                }
            }
            Event::WindowMinimized(id, minimized) => {
                self.set_sidelined(id, minimized, |info| &mut info.minimized);
            }
            Event::WindowFullscreen(id, fullscreen) => {
                self.set_sidelined(id, fullscreen, |info| &mut info.fullscreen);
            }
            Event::FocusChanged(Some(id)) => {
                // The system confirming a focus change ElaraWM asked for is not news.
                if let Some((pending, since)) = self.pending_focus.take() {
                    if pending == id && since.elapsed() < FOCUS_ECHO_WINDOW {
                        return;
                    }
                }
                if self.windows.contains_key(&id) {
                    self.layout.activate_window(&id);
                }
            }
            // Focus went to something that is not managed. The layout keeps its focus; it will
            // hear about the next managed window that gets it.
            Event::FocusChanged(None) => {}
            Event::DisplaysChanged => self.sync_displays(),
        }
    }

    // Displays.

    /// Brings the layout's outputs in line with the connected displays.
    fn sync_displays(&mut self) {
        let infos = self.backend.displays();

        let (kept, gone): (Vec<Display>, Vec<Display>) = std::mem::take(&mut self.displays)
            .into_iter()
            .partition(|display| infos.iter().any(|info| info.id == display.info.id));
        self.displays = kept;
        for removed in gone {
            debug!(name = removed.info.name, "display disconnected");
            self.layout.remove_output(&removed.output);
        }

        for info in infos {
            if let Some(known) = self.displays.iter_mut().find(|d| d.info.id == info.id) {
                if known.info != info {
                    debug!(name = info.name, "display changed");
                    configure_output(&known.output, &info);
                    known.info = info;
                    self.layout.update_output_size(&known.output);
                }
            } else {
                debug!(name = info.name, "display connected");
                let output = new_output(&info);
                self.layout.add_output(output.clone(), None);
                self.displays.push(Display { info, output });
            }
        }

        // Keep the backend's order, primary first.
        let order: Vec<DisplayId> = self.backend.displays().iter().map(|d| d.id).collect();
        self.displays
            .sort_by_key(|d| order.iter().position(|id| *id == d.info.id));
    }

    /// Index of the display that holds most of `frame`, judged by its center.
    fn display_index_at(&self, frame: Rect) -> Option<usize> {
        let center = elara_macos::Point {
            x: frame.x + frame.w / 2.,
            y: frame.y + frame.h / 2.,
        };
        self.displays
            .iter()
            .position(|d| d.info.frame.contains(center))
            .or_else(|| {
                self.displays
                    .iter()
                    .position(|d| d.info.frame.intersects(&frame))
            })
    }

    fn display_for_output(&self, output: &Output) -> Option<&Display> {
        self.displays.iter().find(|d| d.output == *output)
    }

    // Windows.

    fn compute_rules(&self, window: WindowRef<'_>) -> ResolvedWindowRules {
        ResolvedWindowRules::compute(&self.config.window_rules, window, self.is_at_startup)
    }

    fn add_window(&mut self, info: WindowInfo) {
        if info.kind == WindowKind::Other || self.windows.contains_key(&info.id) {
            return;
        }
        if info.minimized || info.fullscreen {
            self.sidelined.insert(info.id, info);
            return;
        }

        let rules = self.compute_rules(WindowRef {
            app_id: info.app_id.as_deref(),
            title: info.title.as_deref(),
            ..Default::default()
        });
        let window = Window::new(&info, rules.clone());
        let id = info.id;

        // niri floats windows that have a parent, which is how Wayland marks dialogs. macOS
        // marks them with a subrole instead.
        let is_dialog = info.kind == WindowKind::Dialog;
        let is_floating =
            rules.compute_open_floating(is_dialog, window.min_size(), window.max_size());

        // Where the window goes, as niri decides it: a rule naming a workspace, else a rule
        // naming an output, else the focused workspace. Windows that were already open when
        // ElaraWM started stay on the display they are on.
        let workspace_id = rules
            .open_on_workspace
            .as_deref()
            .and_then(|name| self.layout.find_workspace_by_name(name))
            .map(|(_, ws)| ws.id());
        let output = rules
            .open_on_output
            .as_deref()
            .and_then(|name| {
                self.displays.iter().find(|d| {
                    d.output
                        .user_data()
                        .get::<OutputName>()
                        .is_some_and(|n| n.matches(name))
                })
            })
            .or_else(|| {
                self.is_at_startup
                    .then(|| self.display_index_at(info.frame))
                    .flatten()
                    .map(|idx| &self.displays[idx])
            })
            .map(|d| d.output.clone());

        let workspace = workspace_id
            .and_then(|id| self.layout.find_workspace_by_id(id).map(|(_, ws)| ws))
            .or_else(|| {
                output
                    .as_ref()
                    .and_then(|o| self.layout.monitor_for_output(o))
                    .map(|mon| mon.active_workspace_ref())
            })
            .or_else(|| self.layout.active_workspace());
        let (width, height, initial_size) = match workspace {
            Some(ws) => {
                let width = ws.resolve_default_width(rules.default_width, is_floating);
                let height = ws.resolve_default_height(rules.default_height, is_floating);
                let limits = (window.min_size(), window.max_size());
                let size = ws.new_window_size(width, height, is_floating, &rules, limits);
                (width, height, Some(size))
            }
            None => (None, None, None),
        };

        // A Wayland compositor tells a new window its size before the window first appears,
        // so by the time niri's layout sees a window it already has the size of its column.
        // A macOS window appears with a size of its own choosing. Resize it first, so the
        // layout starts from what the window actually agreed to rather than from a size that
        // is about to change under it. A zero dimension means the window keeps its own.
        if let Some(size) = initial_size {
            let mut request = info.frame;
            if size.w > 0 {
                request.w = f64::from(size.w);
            }
            if size.h > 0 {
                request.h = f64::from(size.h);
            }
            if request != info.frame {
                match self.backend.set_frame(id, request) {
                    Ok(actual) => {
                        window.record_response(request, actual);
                    }
                    Err(err) => debug!(?id, %err, "could not give new window its initial size"),
                }
            }
        }

        let target = if let Some(id) = workspace_id {
            AddWindowTarget::Workspace(id)
        } else if let Some(output) = &output {
            AddWindowTarget::Output(output)
        } else {
            AddWindowTarget::Auto
        };
        let activate = match rules.open_focused {
            Some(true) => ActivateWindow::Yes,
            Some(false) => ActivateWindow::No,
            None => ActivateWindow::Smart,
        };
        let is_full_width = rules.open_maximized.unwrap_or(false);

        debug!(?id, app_id = ?info.app_id, is_floating, "adding window to the layout");
        let landed_on = self
            .layout
            .add_window(
                window.clone(),
                target,
                width,
                height,
                is_full_width,
                is_floating,
                activate,
            )
            .cloned();
        self.windows.insert(id, window);

        if rules.open_fullscreen == Some(true) {
            self.layout.set_fullscreen(&id, true);
        }

        // niri centers a new floating window, since a Wayland client cannot choose its own
        // position. A macOS application can and does, typically over its parent window, so
        // unless a rule says otherwise the window stays where its application put it.
        if is_floating && rules.default_floating_position.is_none() {
            if let Some(output) = landed_on {
                let frame = self.windows[&id].frame();
                self.keep_floating_position(id, frame, &output);
            }
        }

        self.focus_dirty = true;
    }

    /// Tells the layout that a floating window sits at `frame`.
    fn keep_floating_position(&mut self, id: WindowId, frame: Rect, output: &Output) {
        let Some(display) = self.display_for_output(output) else {
            return;
        };
        let Some(mon) = self.layout.monitor_for_output(output) else {
            return;
        };
        // The layout takes floating positions relative to the working area.
        let working_area = mon.working_area();
        let x = frame.x - display.info.frame.x - working_area.loc.x;
        let y = frame.y - display.info.frame.y - working_area.loc.y;
        self.layout.move_floating_window(
            Some(&id),
            PositionChange::SetFixed(x),
            PositionChange::SetFixed(y),
            false,
        );
    }

    fn remove_window(&mut self, id: WindowId) {
        if self.windows.remove(&id).is_none() {
            return;
        }
        debug!(?id, "removing window from the layout");
        self.layout.remove_window(&id, Transaction::new());
        self.focus_history.retain(|other| *other != id);
        // niri picks which window is focused next; macOS has picked one too, and they need not
        // agree.
        self.focus_dirty = true;
    }

    /// Takes a window out of the layout while it is minimized or in native fullscreen, and
    /// puts it back afterwards.
    fn set_sidelined(
        &mut self,
        id: WindowId,
        value: bool,
        field: impl Fn(&mut WindowInfo) -> &mut bool,
    ) {
        if let Some(info) = self.sidelined.get_mut(&id) {
            *field(info) = value;
            if !info.minimized && !info.fullscreen {
                let mut info = self.sidelined.remove(&id).unwrap();
                // The frame recorded when the window was sidelined is stale by now.
                if let Some(current) = self.backend.windows().into_iter().find(|w| w.id == id) {
                    info.frame = current.frame;
                    info.title = current.title;
                }
                self.add_window(info);
            }
            return;
        }

        if !value {
            return;
        }
        let Some(window) = self.windows.get(&id) else {
            return;
        };
        let mut info = WindowInfo {
            id,
            pid: window.pid(),
            app_id: window.app_id().map(str::to_owned),
            title: window.title(),
            frame: window.frame(),
            kind: if window.is_floating() {
                WindowKind::Dialog
            } else {
                WindowKind::Standard
            },
            resizable: window.min_size() != window.max_size() || window.min_size().w == 0,
            movable: true,
            minimized: false,
            fullscreen: false,
        };
        *field(&mut info) = true;
        self.remove_window(id);
        self.sidelined.insert(id, info);
    }

    fn window_frame_changed(&mut self, id: WindowId, frame: Rect) {
        if let Some(info) = self.sidelined.get_mut(&id) {
            info.frame = frame;
            return;
        }
        let Some(window) = self.windows.get(&id) else {
            return;
        };
        // The system reports every change, including the ones ElaraWM asked for.
        if window.frame() == frame {
            return;
        }

        debug!(
            id = id.0,
            app = window.app_id().unwrap_or("?"),
            was = ?window.frame(),
            now = ?frame,
            "window changed frame on its own"
        );
        let size_changed = window.record_external_frame(frame);
        let is_floating = window.is_floating();
        if size_changed && is_floating {
            window.accept_current_size();
        }
        if size_changed {
            self.layout.update_window(&id, None);
        }

        // A tiled window that was dragged away is put back by the next `apply()`. A floating
        // window stays where the user dropped it.
        if is_floating {
            let output = self
                .layout
                .monitors()
                .find(|mon| mon.has_window(&id))
                .map(|mon| mon.output().clone());
            if let Some(output) = output {
                self.keep_floating_position(id, frame, &output);
            }
        }
    }

    /// Re-resolves a window's rules after its title or the config changed.
    fn recompute_rules(&mut self, id: WindowId) {
        let Some(window) = self.windows.get(&id) else {
            return;
        };
        let is_focused = window.is_activated();
        let rules = window.with_rule_ref(is_focused, |window| self.compute_rules(window));

        let mut changed = false;
        self.layout.with_windows_mut(|window, _| {
            if *window.id() == id && *window.rules() != rules {
                window.set_rules(rules.clone());
                changed = true;
            }
        });
        if changed {
            self.layout.update_window(&id, None);
        }
    }

    // Applying the layout.

    /// Makes the system match the layout: every window is asked to take the frame the layout
    /// computed for it, and the layout is told what each window actually did.
    pub fn apply(&mut self) {
        for round in 0..MAX_APPLY_ROUNDS {
            self.clock.set_unadjusted(get_monotonic_time());
            self.layout.advance_animations();
            // Lets the layout update each window's activated and floating state and issue
            // size requests.
            self.layout.refresh(true);

            let mut any_response = false;
            for target in self.targets() {
                let Some(window) = self.windows.get(&target.id) else {
                    continue;
                };
                // Nothing to do if the window is there already, or was asked for exactly this
                // and answered with something else that it has kept since.
                if window.frame() == target.frame || window.last_request() == Some(target.frame) {
                    continue;
                }

                match self.backend.set_frame(target.id, target.frame) {
                    Ok(actual) => {
                        debug!(
                            id = target.id.0,
                            app = window.app_id().unwrap_or("?"),
                            asked = ?target.frame,
                            got = ?actual,
                            "placed window"
                        );
                        if window.record_response(target.frame, actual) {
                            self.layout.update_window(&target.id, None);
                            any_response = true;
                        }
                    }
                    Err(err) => {
                        // The window may be closing or its application hung. Its events will
                        // say which; until then, do not hammer it.
                        debug!(id = ?target.id, %err, "could not place window");
                        window.record_response(target.frame, window.frame());
                    }
                }
            }

            if !any_response {
                break;
            }
            trace!(
                round,
                "windows answered with different sizes, placing again"
            );
        }

        self.sync_focus();

        if let Some(focused) = self.layout.focus().map(|window| window.window_id()) {
            if self.focus_history.last() != Some(&focused) {
                self.focus_history.retain(|other| *other != focused);
                self.focus_history.push(focused);
            }
        }
    }

    /// Whether the `quit` action has run.
    pub fn should_quit(&self) -> bool {
        self.quit_requested
    }

    /// Computes where the layout wants every window, in global coordinates.
    fn targets(&self) -> Vec<Target> {
        let mut targets = Vec::with_capacity(self.windows.len());
        let mut placed = Vec::with_capacity(self.windows.len());

        for mon in self.layout.monitors() {
            let Some(display) = self.display_for_output(mon.output()) else {
                continue;
            };
            let origin = display.info.frame.origin();

            // Workspaces that are on screen right now. With instant animations that is the
            // active one only.
            for (ws, ws_geo) in mon.workspaces_with_render_geo() {
                for (tile, tile_pos, visible) in ws.tiles_with_render_positions() {
                    let window = tile.window();
                    let loc = ws_geo.loc + tile_pos + tile.window_loc();
                    let size = window.target_size();
                    let frame = Rect::new(
                        origin.x + loc.x.round(),
                        origin.y + loc.y.round(),
                        f64::from(size.w),
                        f64::from(size.h),
                    );

                    // Columns scrolled out of view lie beyond the edge of the output. On
                    // Wayland they are simply not drawn. Here they would show up on the
                    // neighboring display, so they are parked instead. The same goes for a
                    // column that is partly in view at an edge shared with another display:
                    // the part past the edge would be drawn over there. A floating window is
                    // the user's to place, across displays if they like.
                    let spills = !window.is_floating()
                        && self
                            .other_displays(display)
                            .any(|other| frame.intersects(&other.frame));
                    let frame = if visible && frame.intersects(&display.info.frame) && !spills {
                        frame
                    } else {
                        self.parked(frame, display)
                    };
                    placed.push(window.window_id());
                    targets.push(Target {
                        id: window.window_id(),
                        frame,
                    });
                }
            }

            // Everything else on this monitor is on a workspace that is not showing.
            for window in mon.windows() {
                let id = window.window_id();
                if placed.contains(&id) {
                    continue;
                }
                let size = window.target_size();
                let frame = Rect::new(0., 0., f64::from(size.w), f64::from(size.h));
                placed.push(id);
                targets.push(Target {
                    id,
                    frame: self.parked(frame, display),
                });
            }
        }

        targets
    }

    fn other_displays<'a>(
        &'a self,
        display: &'a Display,
    ) -> impl Iterator<Item = &'a DisplayInfo> + 'a {
        self.displays
            .iter()
            .map(|other| &other.info)
            .filter(move |other| other.id != display.info.id)
    }

    /// Returns where to keep a window of `display` that should not be visible.
    ///
    /// macOS has no way to hide another application's window short of minimizing it or hiding
    /// the whole application, both of which the user would see. Instead the window is moved so
    /// that only one corner pixel of it remains on its display. Which corner depends on the
    /// arrangement: the rest of the window must hang into empty space, not onto another
    /// display.
    fn parked(&self, frame: Rect, display: &Display) -> Rect {
        let area = display.info.frame;
        let candidates = [
            // Hanging off the bottom-right corner, then bottom-left, top-right, top-left.
            Rect::new(area.max_x() - 1., area.max_y() - 1., frame.w, frame.h),
            Rect::new(area.x - frame.w + 1., area.max_y() - 1., frame.w, frame.h),
            Rect::new(area.max_x() - 1., area.y - frame.h + 1., frame.w, frame.h),
            Rect::new(
                area.x - frame.w + 1.,
                area.y - frame.h + 1.,
                frame.w,
                frame.h,
            ),
        ];

        candidates
            .into_iter()
            .find(|candidate| {
                !self
                    .other_displays(display)
                    .any(|other| candidate.intersects(&other.frame))
            })
            // Surrounded on every corner: nothing better to do than the default.
            .unwrap_or(candidates[0])
    }

    /// Gives the system's keyboard focus to the window the layout has focused.
    ///
    /// Focus normally flows the other way: the user clicks a window and the layout follows.
    /// This runs after the layout changed its focus on its own, such as when the focused
    /// window closed and niri's rules, not the system's, pick the successor.
    fn sync_focus(&mut self) {
        if !std::mem::take(&mut self.focus_dirty) {
            return;
        }
        let Some(wanted) = self.layout.focus().map(|window| window.window_id()) else {
            return;
        };
        if self.backend.focused_window() == Some(wanted) {
            return;
        }
        match self.backend.focus(wanted) {
            Ok(()) => self.pending_focus = Some((wanted, Instant::now())),
            Err(err) => debug!(id = ?wanted, %err, "could not focus window"),
        }
    }

    /// Requests that the layout's focused window gets the system's keyboard focus on the next
    /// [`apply()`](Self::apply). Call this after changing the layout's focus.
    pub fn request_focus_sync(&mut self) {
        self.focus_dirty = true;
    }

    /// Gives the windows back: each returns to the frame it had before ElaraWM took it over.
    ///
    /// Call this before exiting. Windows on other workspaces are kept just past a corner of
    /// the screen, and nothing else would ever bring them back; and a user who stops ElaraWM
    /// should find their desktop as they left it rather than frozen in its last tiled state.
    /// A window whose original place is no longer on any display, such as one that was on a
    /// display since unplugged, is put on the display it is on now.
    pub fn release_windows(&mut self) {
        let mut offset = 0.;
        let mut ids: Vec<WindowId> = self.windows.keys().copied().collect();
        ids.sort();

        for id in ids {
            let window = &self.windows[&id];
            let original = window.original_frame();
            let is_on_screen = |frame: Rect| {
                self.displays
                    .iter()
                    .any(|d| is_reachable(frame, &d.info.visible_frame))
            };

            let restored = if is_on_screen(original) {
                original
            } else {
                let area = self
                    .layout
                    .monitors()
                    .find(|mon| mon.has_window(&id))
                    .and_then(|mon| self.display_for_output(mon.output()))
                    .or(self.displays.first())
                    .map(|d| d.info.visible_frame);
                let Some(area) = area else {
                    continue;
                };

                // Cascade them so that they do not end up in one pile.
                let frame = Rect::new(
                    area.x + offset,
                    area.y + offset,
                    original.w.min(area.w - offset),
                    original.h.min(area.h - offset),
                );
                offset = (offset + 32.) % (area.h / 2.).max(32.);
                frame
            };

            if window.frame() == restored {
                continue;
            }
            if let Err(err) = self.backend.set_frame(id, restored) {
                debug!(?id, %err, "could not give window its original frame back");
            }
        }
    }
}

/// Whether enough of a window is inside `area` for the user to grab it.
fn is_reachable(frame: Rect, area: &Rect) -> bool {
    const MARGIN: f64 = 40.;
    frame.x < area.max_x() - MARGIN
        && frame.max_x() > area.x + MARGIN
        && frame.y < area.max_y() - MARGIN
        && frame.max_y() > area.y + MARGIN
}

/// Creates the layout output that stands for a display.
fn new_output(info: &DisplayInfo) -> Output {
    let output = Output::new(
        info.name.clone(),
        PhysicalProperties {
            size: Size::from((0, 0)),
            subpixel: Subpixel::Unknown,
            make: String::new(),
            model: info.name.clone(),
            serial_number: info.id.0.to_string(),
        },
    );
    // Outputs are matched against names in the config through this. On Linux the first name is
    // the connector, such as "eDP-1"; here it is the display's name.
    output.user_data().insert_if_missing(|| OutputName {
        connector: info.name.clone(),
        make: None,
        model: None,
        serial: None,
    });
    configure_output(&output, info);
    output
}

/// Updates a layout output to match its display.
fn configure_output(output: &Output, info: &DisplayInfo) {
    let mode = Mode {
        size: Size::from((
            (info.frame.w * info.scale).round() as i32,
            (info.frame.h * info.scale).round() as i32,
        )),
        refresh: info.refresh_mhz.unwrap_or(60_000) as i32,
    };
    output.change_current_state(
        Some(mode),
        Some(Transform::Normal),
        Some(Scale::Fractional(info.scale)),
        Some(Point::from((
            info.frame.x.round() as i32,
            info.frame.y.round() as i32,
        ))),
    );
    output.set_preferred(mode);

    // The layout tiles inside the output's "non-exclusive zone". On Linux that is what panels
    // leave free; here it is what the menu bar and the Dock leave free.
    let visible = info.visible_frame;
    layer_map_for_output(output).set_non_exclusive_zone(Some(Rectangle::new(
        Point::from((
            (visible.x - info.frame.x).round() as i32,
            (visible.y - info.frame.y).round() as i32,
        )),
        Size::from((visible.w.round() as i32, visible.h.round() as i32)),
    )));
}
