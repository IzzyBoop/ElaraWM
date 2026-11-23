//! [`WindowBackend`] over the Accessibility API.
//!
//! How it learns about changes: each application gets an `AXObserver`, attached to the main run
//! loop, which calls back when the application's windows are created, moved, resized, retitled
//! and so on. `NSWorkspace` reports applications launching and quitting, and CoreGraphics reports
//! display changes. All of these callbacks only append to a queue; nothing is interpreted until
//! [`WindowBackend::drain_events`], so no callback ever re-enters the backend.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::ffi::c_void;
use std::ptr::NonNull;
use std::time::{Duration, Instant};

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{NSObjectProtocol, ProtocolObject};
use objc2_app_kit::{
    NSApplicationActivationOptions, NSApplicationActivationPolicy, NSRunningApplication,
    NSWorkspace, NSWorkspaceActiveSpaceDidChangeNotification,
    NSWorkspaceDidActivateApplicationNotification, NSWorkspaceDidLaunchApplicationNotification,
    NSWorkspaceDidTerminateApplicationNotification,
};
use objc2_application_services::{AXError, AXObserver, AXUIElement};
use objc2_core_foundation::{kCFRunLoopDefaultMode, CFRetained, CFRunLoop, CFString};
use objc2_core_graphics::{
    CGDirectDisplayID, CGDisplayChangeSummaryFlags, CGDisplayRegisterReconfigurationCallback,
    CGDisplayRemoveReconfigurationCallback,
};
use objc2_foundation::{MainThreadMarker, NSNotification, NSNotificationName};

use super::ghosts::GhostPanels;
use super::hotkey_overlay::HotkeyOverlayPanels;
use super::overlay::Overlay;
use super::overview::OverviewPanels;
use super::thumbnails::{Thumbnails, Wanted};
use super::{accessibility_trusted, ax, display, spaces};
use crate::{
    DisplayInfo, Error, Event, Ghost, HotkeyOverlay, Outline, Overview, OverviewItem, Rect,
    WindowBackend, WindowId, WindowInfo, WindowKind,
};

/// How long to wait for an application to answer one Accessibility request.
///
/// The system default is six seconds, which would freeze window management whenever one
/// application hangs.
const MESSAGING_TIMEOUT_SECS: f32 = 0.5;

/// How many pictures of windows are kept for the next time their workspace slides, beyond
/// those in use. Each is as large as its window, so this bounds the memory they take.
const GHOST_PICTURES_KEPT: usize = 8;

/// How long to keep trying to observe an application that has just launched. Applications
/// refuse Accessibility requests until they have finished starting up.
const APP_RETRY_WINDOW: Duration = Duration::from_secs(15);
const APP_RETRY_INTERVAL: Duration = Duration::from_millis(250);

const APP_NOTIFICATIONS: &[&str] = &[
    "AXWindowCreated",
    "AXFocusedWindowChanged",
    "AXMainWindowChanged",
    "AXApplicationActivated",
    "AXApplicationHidden",
    "AXApplicationShown",
];

const WINDOW_NOTIFICATIONS: &[&str] = &[
    "AXUIElementDestroyed",
    "AXWindowMoved",
    "AXWindowResized",
    "AXTitleChanged",
    "AXWindowMiniaturized",
    "AXWindowDeminiaturized",
];

/// Something a system callback reported, not yet interpreted.
enum Raw {
    Ax {
        pid: i32,
        element: CFRetained<AXUIElement>,
        notification: String,
    },
    AppsChanged,
    FrontmostAppChanged,
    DisplaysChanged,
    SpaceChanged,
}

thread_local! {
    /// Filled by system callbacks, which all run on the main thread's run loop.
    static RAW: RefCell<Vec<Raw>> = const { RefCell::new(Vec::new()) };
}

fn push_raw(raw: Raw) {
    RAW.with(|queue| queue.borrow_mut().push(raw));
}

/// Called by an `AXObserver`. `refcon` carries the application's process id.
unsafe extern "C-unwind" fn ax_callback(
    _observer: NonNull<AXObserver>,
    element: NonNull<AXUIElement>,
    notification: NonNull<CFString>,
    refcon: *mut c_void,
) {
    // SAFETY: the system passes valid objects that live for the duration of the callback.
    // Retaining the element keeps it alive after the callback returns.
    let (element, notification) = unsafe {
        (
            CFRetained::retain(element),
            notification.as_ref().to_string(),
        )
    };
    push_raw(Raw::Ax {
        pid: refcon as usize as i32,
        element,
        notification,
    });
}

/// Called by CoreGraphics before and after any display configuration change.
unsafe extern "C-unwind" fn display_callback(
    _display: CGDirectDisplayID,
    flags: CGDisplayChangeSummaryFlags,
    _user_info: *mut c_void,
) {
    // Each change is announced twice; only the second call describes the new state.
    if !flags.contains(CGDisplayChangeSummaryFlags::BeginConfigurationFlag) {
        push_raw(Raw::DisplaysChanged);
    }
}

struct App {
    bundle_id: Option<String>,
    element: CFRetained<AXUIElement>,
    observer: CFRetained<AXObserver>,
}

struct Window {
    info: WindowInfo,
    element: CFRetained<AXUIElement>,
    /// How many checks in a row have disagreed with `info.on_other_space`.
    space_votes: u8,
}

/// How often to look at which windows are on screen.
const SPACE_CHECK_INTERVAL: Duration = Duration::from_secs(1);

/// How many checks in a row must miss a window before it is taken to be on another Space. A
/// window that is being moved can be missed once: the two sources of its frame are read a
/// moment apart.
const LEAVE_CHECKS: u8 = 3;

/// How many checks in a row must find a window before it is taken to be on the managed
/// Space. One is not enough: while macOS slides from one Space to the next, windows of both
/// are on screen.
const JOIN_CHECKS: u8 = 2;

/// An application that could not be observed yet.
struct PendingApp {
    bundle_id: Option<String>,
    first_seen: Instant,
    last_try: Instant,
}

type ObserverToken = Retained<ProtocolObject<dyn NSObjectProtocol>>;

/// The [`WindowBackend`] for macOS.
///
/// It must be created and used on the main thread, and its events only arrive while the main
/// run loop is running.
pub struct MacBackend {
    mtm: MainThreadMarker,
    apps: HashMap<i32, App>,
    pending_apps: HashMap<i32, PendingApp>,
    windows: BTreeMap<WindowId, Window>,
    focused: Option<WindowId>,
    events: Vec<Event>,
    next_id: u64,
    workspace_observers: Vec<ObserverToken>,
    /// The windows ElaraWM draws in. They also mark the Space it manages.
    overlay: Option<Overlay>,
    /// Whether the overlay has ever been seen on screen. Until it has, there is no telling
    /// Spaces apart, and everything is taken to be on the managed one.
    home_seen: bool,
    /// Whether the user is looking at the managed Space, as of the last pass.
    home_active: bool,
    /// When the windows on screen were last looked at, or when the Space last changed if
    /// that is later: a check waits out the interval after either.
    space_checked: Instant,
    /// The outlines last asked for, to tell when nothing changed.
    outlines: Vec<Outline>,
    /// The windows the overview is drawn in, once it has been shown.
    overview_panels: Option<OverviewPanels>,
    /// The overview last asked for, to tell when nothing changed.
    overview: Option<Overview>,
    /// The windows the list of key bindings is drawn in, once it has been shown.
    hotkey_panels: Option<HotkeyOverlayPanels>,
    /// The list of key bindings on screen, if any.
    hotkey_overlay: Option<HotkeyOverlay>,
    /// Pictures of windows for the overview, if the config asks for them.
    thumbnails: Thumbnails,
    /// Which pictures the overview is showing: [`Thumbnails::version()`] as of then.
    thumbnails_shown: u64,
    /// The windows that pictures of windows are drawn in where the windows cannot be, once
    /// there has been one to draw.
    ghost_panels: Option<GhostPanels>,
    /// The pictures last asked for, to tell when nothing changed.
    ghosts: Vec<Ghost>,
    /// The pictures themselves. Kept apart from the overview's, which are smaller and are
    /// dropped on other occasions.
    ghost_pictures: Thumbnails,
    /// Which pictures are showing: [`Thumbnails::version()`] as of then.
    ghost_pictures_shown: u64,
    /// The windows pictures were last asked for.
    ghost_pictures_of: Vec<WindowId>,
}

impl MacBackend {
    /// Starts observing every running application.
    ///
    /// Fails with [`Error::NotTrusted`] if the Accessibility permission has not been granted.
    ///
    /// # Panics
    ///
    /// Panics if called from a thread other than the main thread.
    pub fn new() -> Result<Self, Error> {
        let mtm = MainThreadMarker::new().expect("MacBackend must be created on the main thread");
        if !accessibility_trusted() {
            return Err(Error::NotTrusted);
        }

        let mut backend = Self {
            mtm,
            apps: HashMap::new(),
            pending_apps: HashMap::new(),
            windows: BTreeMap::new(),
            focused: None,
            events: Vec::new(),
            next_id: 0,
            workspace_observers: Vec::new(),
            overlay: None,
            home_seen: false,
            home_active: true,
            space_checked: Instant::now(),
            outlines: Vec::new(),
            overview_panels: None,
            overview: None,
            hotkey_panels: None,
            hotkey_overlay: None,
            thumbnails: Thumbnails::new(),
            thumbnails_shown: 0,
            ghost_panels: None,
            ghosts: Vec::new(),
            ghost_pictures: Thumbnails::quiet(),
            ghost_pictures_shown: 0,
            ghost_pictures_of: Vec::new(),
        };

        // Before any window is looked at: the overlay marks the Space that is showing now.
        let mut overlay = Overlay::new();
        overlay.set(&display::displays(mtm), &[]);
        backend.overlay = Some(overlay);
        backend.refresh_home();

        backend.observe_workspace();
        // SAFETY: the callback is a valid function for the whole life of the process and takes
        // no user data. It is removed again in `Drop`.
        unsafe {
            CGDisplayRegisterReconfigurationCallback(Some(display_callback), std::ptr::null_mut());
        }

        backend.sync_apps();
        // Windows on other Spaces start out unmanaged. No waiting here: nothing is moving.
        backend.check_spaces(true);
        backend.refresh_focus();
        // Windows that existed before ElaraWM started are part of the initial state, which
        // callers read with `windows()`, not news.
        backend.events.clear();
        Ok(backend)
    }

    fn observe_workspace(&mut self) {
        let center = NSWorkspace::sharedWorkspace().notificationCenter();
        let mut observe = |name: &NSNotificationName, raw: fn() -> Raw| {
            let block = RcBlock::new(move |_: NonNull<NSNotification>| push_raw(raw()));
            // SAFETY: the block takes the one argument the notification center passes, and
            // captures nothing that could be invalidated.
            let token = unsafe {
                center.addObserverForName_object_queue_usingBlock(Some(name), None, None, &block)
            };
            self.workspace_observers.push(token);
        };

        // SAFETY: the notification names are valid statics.
        unsafe {
            observe(NSWorkspaceDidLaunchApplicationNotification, || {
                Raw::AppsChanged
            });
            observe(NSWorkspaceDidTerminateApplicationNotification, || {
                Raw::AppsChanged
            });
            observe(NSWorkspaceDidActivateApplicationNotification, || {
                Raw::FrontmostAppChanged
            });
            observe(NSWorkspaceActiveSpaceDidChangeNotification, || {
                Raw::SpaceChanged
            });
        }
    }

    /// Brings the set of observed applications in line with the applications that are running.
    fn sync_apps(&mut self) {
        let running: HashMap<i32, Option<String>> = NSWorkspace::sharedWorkspace()
            .runningApplications()
            .iter()
            // Only applications that show in the Dock have windows worth managing. This skips
            // menu bar extras and background agents.
            .filter(|app| app.activationPolicy() == NSApplicationActivationPolicy::Regular)
            .map(|app| {
                let bundle_id = app.bundleIdentifier().map(|id| id.to_string());
                (app.processIdentifier(), bundle_id)
            })
            .collect();

        let gone: Vec<i32> = self
            .apps
            .keys()
            .chain(self.pending_apps.keys())
            .filter(|pid| !running.contains_key(pid))
            .copied()
            .collect();
        for pid in gone {
            self.forget_app(pid);
        }

        let now = Instant::now();
        for (pid, bundle_id) in running {
            if self.apps.contains_key(&pid) || self.pending_apps.contains_key(&pid) {
                continue;
            }
            if let Err(err) = self.observe_app(pid, bundle_id.clone()) {
                trace!(
                    pid,
                    ?err,
                    "application not ready to be observed, will retry"
                );
                self.pending_apps.insert(
                    pid,
                    PendingApp {
                        bundle_id,
                        first_seen: now,
                        last_try: now,
                    },
                );
            }
        }
    }

    fn retry_pending_apps(&mut self) {
        let now = Instant::now();
        let due: Vec<i32> = self
            .pending_apps
            .iter()
            .filter(|(_, app)| now.duration_since(app.last_try) >= APP_RETRY_INTERVAL)
            .map(|(pid, _)| *pid)
            .collect();

        for pid in due {
            let Some(pending) = self.pending_apps.remove(&pid) else {
                continue;
            };
            match self.observe_app(pid, pending.bundle_id.clone()) {
                Ok(()) => debug!(pid, "application became observable"),
                Err(err) if now.duration_since(pending.first_seen) < APP_RETRY_WINDOW => {
                    trace!(pid, ?err, "application still not observable");
                    self.pending_apps.insert(
                        pid,
                        PendingApp {
                            last_try: now,
                            ..pending
                        },
                    );
                }
                Err(err) => {
                    warn!(
                        pid,
                        bundle_id = ?pending.bundle_id,
                        ?err,
                        "giving up on observing application; its windows will not be managed"
                    );
                }
            }
        }
    }

    fn observe_app(&mut self, pid: i32, bundle_id: Option<String>) -> Result<(), AXError> {
        // SAFETY: creating an element for a pid has no preconditions; a dead pid yields an
        // element whose every request fails.
        let element = unsafe { AXUIElement::new_application(pid) };
        // SAFETY: the element is valid.
        let _ = unsafe { element.set_messaging_timeout(MESSAGING_TIMEOUT_SECS) };

        let mut observer: *mut AXObserver = std::ptr::null_mut();
        // SAFETY: `observer` is a valid place for one pointer, and the callback has the
        // signature the API expects.
        let err =
            unsafe { AXObserver::create(pid, Some(ax_callback), NonNull::from(&mut observer)) };
        if err != AXError::Success {
            return Err(err);
        }
        let observer = NonNull::new(observer).ok_or(AXError::Failure)?;
        // SAFETY: on success the function stored a +1 retained observer.
        let observer = unsafe { CFRetained::from_raw(observer) };

        let refcon = pid as usize as *mut c_void;
        for (idx, name) in APP_NOTIFICATIONS.iter().enumerate() {
            let name = CFString::from_str(name);
            // SAFETY: all arguments are valid; `refcon` is an integer, never dereferenced.
            let err = unsafe { observer.add_notification(&element, &name, refcon) };
            // An application that is still starting refuses the first registration. Later
            // failures mean the application does not support that notification, which is fine.
            if idx == 0 && err != AXError::Success {
                return Err(err);
            }
        }

        if let Some(run_loop) = CFRunLoop::main() {
            // SAFETY: the observer is valid, and so is the mode constant.
            unsafe {
                let source = observer.run_loop_source();
                run_loop.add_source(Some(&source), kCFRunLoopDefaultMode);
            }
        }

        self.apps.insert(
            pid,
            App {
                bundle_id,
                element: element.clone(),
                observer,
            },
        );

        for window in ax::windows(&element) {
            self.track_window(pid, window);
        }
        Ok(())
    }

    fn forget_app(&mut self, pid: i32) {
        self.pending_apps.remove(&pid);
        let Some(app) = self.apps.remove(&pid) else {
            return;
        };

        if let Some(run_loop) = CFRunLoop::main() {
            // SAFETY: the observer is valid, and so is the mode constant.
            unsafe {
                let source = app.observer.run_loop_source();
                run_loop.remove_source(Some(&source), kCFRunLoopDefaultMode);
            }
        }

        let closed: Vec<WindowId> = self
            .windows
            .iter()
            .filter(|(_, window)| window.info.pid == pid)
            .map(|(id, _)| *id)
            .collect();
        for id in closed {
            self.untrack_window(id);
        }
    }

    fn find_window(&self, pid: i32, element: &AXUIElement) -> Option<WindowId> {
        self.windows
            .iter()
            // Elements are compared with CFEqual: two references to one window are equal even
            // when they are different objects.
            .find(|(_, window)| window.info.pid == pid && *window.element == *element)
            .map(|(id, _)| *id)
    }

    /// Starts tracking a window if it is one worth managing. Returns its id either way.
    fn track_window(&mut self, pid: i32, element: CFRetained<AXUIElement>) -> Option<WindowId> {
        if let Some(id) = self.find_window(pid, &element) {
            return Some(id);
        }

        if ax::string(&element, ax::ROLE).as_deref() != Some("AXWindow") {
            return None;
        }
        let subrole = ax::string(&element, ax::SUBROLE);
        let kind = match subrole.as_deref() {
            Some("AXStandardWindow") => WindowKind::Standard,
            Some("AXDialog" | "AXSystemDialog" | "AXFloatingWindow" | "AXSystemFloatingWindow") => {
                WindowKind::Dialog
            }
            _ => WindowKind::Other,
        };
        if kind == WindowKind::Other {
            trace!(pid, ?subrole, "not managing a window of this kind");
            return None;
        }
        let frame = ax::frame(&element)?;
        let app = self.apps.get(&pid)?;

        let refcon = pid as usize as *mut c_void;
        for name in WINDOW_NOTIFICATIONS {
            let name = CFString::from_str(name);
            // SAFETY: all arguments are valid; `refcon` is an integer, never dereferenced.
            let _ = unsafe { app.observer.add_notification(&element, &name, refcon) };
        }

        self.next_id += 1;
        let id = WindowId(self.next_id);
        let info = WindowInfo {
            id,
            pid,
            app_id: app.bundle_id.clone(),
            title: ax::string(&element, ax::TITLE),
            frame,
            kind,
            resizable: ax::is_settable(&element, ax::SIZE),
            movable: ax::is_settable(&element, ax::POSITION),
            minimized: ax::boolean(&element, ax::MINIMIZED).unwrap_or(false),
            fullscreen: ax::boolean(&element, ax::FULLSCREEN).unwrap_or(false),
            on_other_space: !self.new_window_is_here(),
        };
        debug!(
            ?id,
            pid,
            app_id = ?info.app_id,
            ?kind,
            ?subrole,
            size = ?(info.frame.w, info.frame.h),
            "tracking window"
        );

        self.events.push(Event::WindowOpened(info.clone()));
        self.windows.insert(
            id,
            Window {
                info,
                element,
                space_votes: 0,
            },
        );
        Some(id)
    }

    /// Whether ElaraWM can tell macOS Spaces apart, which it does by watching a window of
    /// its own. If the system never lists that window as on screen, every window is taken
    /// to be on the managed Space.
    pub fn can_tell_spaces(&self) -> bool {
        self.home_seen
    }

    /// How many of the windows shown as pictures in place of themselves there is a picture
    /// of. For checking that pictures can be taken.
    pub fn ghost_picture_count(&self) -> usize {
        self.ghosts
            .iter()
            .filter(|ghost| self.ghost_pictures.get(ghost.window).is_some())
            .count()
    }

    /// How many windows there is a picture of for the overview to show. For checking that
    /// pictures can be taken.
    pub fn thumbnail_count(&self) -> usize {
        self.windows
            .keys()
            .filter(|id| self.thumbnails.get(**id).is_some())
            .count()
    }

    /// Whether a window that has just come to light is on the managed Space.
    ///
    /// A window appears where the user is, so this comes down to whether they are looking
    /// at the managed Space at this moment. That is asked of the window server here and
    /// now: applications report the windows they have on a Space only once it is shown, so
    /// such windows turn up during the very switch away, before anything else has noticed
    /// it. Checking the windows on screen settles the matter either way afterwards.
    fn new_window_is_here(&self) -> bool {
        if !self.home_seen {
            return true;
        }
        if !self.home_active {
            return false;
        }
        spaces::OnScreen::read()
            .is_none_or(|on_screen| on_screen.has_window_of(std::process::id() as i32))
    }

    /// Re-reads whether the user is looking at the managed Space.
    fn refresh_home(&mut self) {
        let listed = self
            .overlay
            .as_ref()
            .is_some_and(|overlay| !overlay.on_screen_frames().is_empty());
        self.home_seen |= listed;
        let active = listed || !self.home_seen;
        if active != self.home_active {
            debug!(active, "managed Space is showing: changed");
            self.home_active = active;
        }
    }

    /// Starts the wait before the next check over, and forgets what recent checks found.
    /// Called when the Space changes: for a moment after that, what is on screen says little.
    fn restart_space_checks(&mut self) {
        self.space_checked = Instant::now();
        for window in self.windows.values_mut() {
            window.space_votes = 0;
        }
    }

    /// Works out which windows are on the managed Space, if that is the one showing.
    ///
    /// A window changes sides only after several checks in a row say so, unless `at_once`
    /// is set, which is for startup, when nothing is moving.
    fn check_spaces(&mut self, at_once: bool) {
        self.space_checked = Instant::now();
        if !self.home_active || !self.home_seen {
            trace!(
                home_active = self.home_active,
                home_seen = self.home_seen,
                "not checking which windows are on screen"
            );
            return;
        }
        let Some(on_screen) = spaces::OnScreen::read() else {
            debug!("the window server did not list the windows on screen");
            return;
        };
        // The overlay must be in the very list the windows are looked up in. Whether the
        // managed Space is showing was read a moment earlier and may no longer hold.
        if !on_screen.has_window_of(std::process::id() as i32) {
            trace!("the managed Space is not the one on screen");
            return;
        }
        trace!(?on_screen, "windows on screen");

        for (id, window) in &mut self.windows {
            // Neither kind is on screen, and neither is managed while it lasts.
            if window.info.minimized || window.info.fullscreen {
                continue;
            }
            let pid = window.info.pid;
            let present = on_screen.has(pid, window.info.frame)
                // The frame on record may be a moment old.
                || ax::frame(&window.element).is_some_and(|frame| on_screen.has(pid, frame));

            let on_other_space = !present;
            if window.info.on_other_space == on_other_space {
                window.space_votes = 0;
                continue;
            }
            window.space_votes = window.space_votes.saturating_add(1);
            let needed = match (at_once, on_other_space) {
                (true, _) => 1,
                (false, true) => LEAVE_CHECKS,
                (false, false) => JOIN_CHECKS,
            };
            if window.space_votes < needed {
                continue;
            }

            debug!(?id, on_other_space, "window changed Space");
            window.space_votes = 0;
            window.info.on_other_space = on_other_space;
            self.events
                .push(Event::WindowOnOtherSpace(*id, on_other_space));
        }
    }

    fn untrack_window(&mut self, id: WindowId) {
        if self.windows.remove(&id).is_none() {
            return;
        }
        debug!(?id, "window closed");
        self.events.push(Event::WindowClosed(id));
        if self.focused == Some(id) {
            self.focused = None;
            self.events.push(Event::FocusChanged(None));
        }
    }

    /// Re-reads which window has keyboard focus and reports a change.
    fn refresh_focus(&mut self) {
        let frontmost = NSWorkspace::sharedWorkspace().frontmostApplication();
        let pid = frontmost.as_ref().map(|app| app.processIdentifier());
        let mut described = None;
        let focused = pid.and_then(|pid| {
            let app = self.apps.get(&pid)?;
            let element = ax::element(&app.element, ax::FOCUSED_WINDOW)?;
            described = Some((
                ax::string(&element, ax::ROLE),
                ax::string(&element, ax::SUBROLE),
                ax::frame(&element).map(|f| (f.w, f.h)),
            ));
            self.track_window(pid, element)
        });

        if focused != self.focused {
            // What macOS says has focus, managed or not, to tell from a log what took it.
            // No titles: they say what the user is reading.
            debug!(
                frontmost = ?frontmost.as_ref().and_then(|app| app.bundleIdentifier()).map(|id| id.to_string()),
                observed = pid.is_some_and(|pid| self.apps.contains_key(&pid)),
                window = ?described,
                ?focused,
                "focus changed"
            );
            self.focused = focused;
            self.events.push(Event::FocusChanged(focused));
        }
    }

    fn refresh_frame(&mut self, id: WindowId) {
        let Some(window) = self.windows.get_mut(&id) else {
            return;
        };

        if let Some(frame) = ax::frame(&window.element) {
            if frame != window.info.frame {
                window.info.frame = frame;
                self.events.push(Event::WindowFrameChanged(id, frame));
            }
        }

        // Entering and leaving native fullscreen has no notification of its own; it shows up as
        // a resize.
        if let Some(fullscreen) = ax::boolean(&window.element, ax::FULLSCREEN) {
            if fullscreen != window.info.fullscreen {
                window.info.fullscreen = fullscreen;
                self.events.push(Event::WindowFullscreen(id, fullscreen));
            }
        }
    }

    fn set_minimized(&mut self, id: WindowId, minimized: bool) {
        if let Some(window) = self.windows.get_mut(&id) {
            if window.info.minimized != minimized {
                window.info.minimized = minimized;
                self.events.push(Event::WindowMinimized(id, minimized));
            }
        }
    }

    fn handle_ax(&mut self, pid: i32, element: CFRetained<AXUIElement>, notification: &str) {
        match notification {
            "AXWindowCreated" => {
                self.track_window(pid, element);
            }
            "AXFocusedWindowChanged" | "AXMainWindowChanged" | "AXApplicationActivated" => {
                self.refresh_focus();
            }
            "AXApplicationHidden" | "AXApplicationShown" => {
                let hidden = notification == "AXApplicationHidden";
                let ids: Vec<WindowId> = self
                    .windows
                    .iter()
                    .filter(|(_, window)| window.info.pid == pid)
                    .map(|(id, _)| *id)
                    .collect();
                for id in ids {
                    // A window that was minimized before its application was hidden stays
                    // minimized when the application is shown again.
                    let minimized = hidden
                        || ax::boolean(&self.windows[&id].element, ax::MINIMIZED).unwrap_or(false);
                    self.set_minimized(id, minimized);
                }
            }
            _ => {
                let Some(id) = self.find_window(pid, &element) else {
                    return;
                };
                match notification {
                    "AXUIElementDestroyed" => self.untrack_window(id),
                    "AXWindowMoved" | "AXWindowResized" => self.refresh_frame(id),
                    "AXTitleChanged" => {
                        let title = ax::string(&element, ax::TITLE);
                        let window = self.windows.get_mut(&id).unwrap();
                        if window.info.title != title {
                            window.info.title = title.clone();
                            self.events.push(Event::WindowTitleChanged(id, title));
                        }
                    }
                    "AXWindowMiniaturized" => self.set_minimized(id, true),
                    "AXWindowDeminiaturized" => self.set_minimized(id, false),
                    other => trace!(other, "ignoring Accessibility notification"),
                }
            }
        }
    }

    fn error(window: WindowId, err: AXError) -> Error {
        match err {
            AXError::InvalidUIElement => Error::NoSuchWindow(window),
            AXError::APIDisabled => Error::NotTrusted,
            other => Error::Unresponsive {
                window,
                code: other.0,
            },
        }
    }
}

impl WindowBackend for MacBackend {
    fn displays(&self) -> Vec<DisplayInfo> {
        display::displays(self.mtm)
    }

    fn windows(&self) -> Vec<WindowInfo> {
        self.windows.values().map(|w| w.info.clone()).collect()
    }

    fn window_frame(&self, window: WindowId) -> Option<Rect> {
        self.windows.get(&window).map(|w| w.info.frame)
    }

    fn focused_window(&self) -> Option<WindowId> {
        self.focused
    }

    fn set_frame(&mut self, id: WindowId, frame: Rect) -> Result<Rect, Error> {
        let window = self.windows.get_mut(&id).ok_or(Error::NoSuchWindow(id))?;
        let element = &window.element;
        let err = |err| Self::error(id, err);

        // A window that already has the size only needs moving. That is one request instead of
        // three, which matters when windows are moved every frame.
        let resize = window.info.resizable
            && (window.info.frame.w != frame.w || window.info.frame.h != frame.h);

        // Size, position, size. A window is not allowed to extend past the display it is on, so
        // a resize can be cut short at the old position and has to be repeated at the new one;
        // and resizing first avoids the window briefly hanging off the display at the new one.
        if resize {
            ax::set_size(element, frame.w, frame.h).map_err(err)?;
        }
        if window.info.movable {
            ax::set_position(element, frame.x, frame.y).map_err(err)?;
        }
        if resize {
            ax::set_size(element, frame.w, frame.h).map_err(err)?;
        }

        // What the window ended up with is the truth, whatever was asked for. The change is
        // not reported as an event here: the application will also send move and resize
        // notifications, and `refresh_frame()` reports it from those.
        let actual = ax::frame(element).ok_or(Error::Unresponsive {
            window: id,
            code: AXError::CannotComplete.0,
        })?;
        window.info.frame = actual;
        Ok(actual)
    }

    fn focus(&mut self, id: WindowId) -> Result<(), Error> {
        let window = self.windows.get(&id).ok_or(Error::NoSuchWindow(id))?;
        let pid = window.info.pid;

        // Make the window its application's main window and raise it within the application,
        // then bring the application forward. The order matters: activating first would briefly
        // focus whichever window the application had in front.
        let _ = ax::set_boolean(&window.element, ax::MAIN, true);
        ax::perform(&window.element, ax::ACTION_RAISE).map_err(|err| Self::error(id, err))?;

        if let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid) {
            app.activateWithOptions(NSApplicationActivationOptions::empty());
        }
        Ok(())
    }

    fn close(&mut self, id: WindowId) -> Result<(), Error> {
        let window = self.windows.get(&id).ok_or(Error::NoSuchWindow(id))?;
        let button = ax::element(&window.element, ax::CLOSE_BUTTON).ok_or(Error::Unresponsive {
            window: id,
            code: AXError::AttributeUnsupported.0,
        })?;
        ax::perform(&button, ax::ACTION_PRESS).map_err(|err| Self::error(id, err))
    }

    fn home_space_active(&self) -> bool {
        self.home_active
    }

    fn set_outlines(&mut self, outlines: &[Outline]) {
        // This is called on every pass; most of them change nothing.
        if self.outlines == outlines && (self.overlay.is_some() || outlines.is_empty()) {
            return;
        }
        self.outlines = outlines.to_vec();
        let displays = display::displays(self.mtm);
        self.overlay
            .get_or_insert_with(Overlay::new)
            .set(&displays, outlines);
    }

    fn set_ghosts(&mut self, ghosts: &[Ghost]) {
        // This is called on every pass, and on nearly all of them there are none.
        if ghosts.is_empty() && self.ghosts.is_empty() {
            return;
        }

        if !ghosts.is_empty() {
            // New pictures when other windows need one, and now and then while the same
            // ones do. Until they come, the last picture taken of a window is shown: it is
            // rarely far off, and a window that turns up halfway is worse.
            let ids: Vec<WindowId> = ghosts.iter().map(|ghost| ghost.window).collect();
            if self.ghost_pictures.due(ids != self.ghost_pictures_of) {
                let wanted = ids
                    .iter()
                    .filter_map(|id| self.windows.get(id))
                    .map(|window| Wanted {
                        id: window.info.id,
                        pid: window.info.pid,
                        frame: window.info.frame,
                        title: window.info.title.clone(),
                    })
                    .collect();
                let scale = display::displays(self.mtm)
                    .iter()
                    .map(|display| display.scale)
                    .fold(1., f64::max);
                self.ghost_pictures
                    .refresh(wanted, scale, GHOST_PICTURES_KEPT);
                self.ghost_pictures_of = ids;
            }
        } else {
            // The next slide is a new occasion, even with the same windows.
            self.ghost_pictures_of.clear();
        }

        let version = self.ghost_pictures.version();
        if self.ghosts == ghosts && self.ghost_pictures_shown == version {
            return;
        }
        self.ghosts = ghosts.to_vec();
        self.ghost_pictures_shown = version;
        let displays = display::displays(self.mtm);
        let pictures = &self.ghost_pictures;
        self.ghost_panels
            .get_or_insert_with(GhostPanels::new)
            .set(&displays, ghosts, &|id| pictures.get(id));
    }

    fn set_overview(&mut self, overview: Option<&Overview>) {
        match overview.and_then(|overview| Some((overview, overview.thumbnails?))) {
            Some((overview, zoom)) => {
                if self.thumbnails.due(self.overview.is_none()) {
                    let wanted = overview
                        .displays
                        .iter()
                        .flat_map(|display| &display.items)
                        .filter_map(|item| match item {
                            OverviewItem::Window(window) => self.windows.get(&window.id),
                            OverviewItem::Shape(_) => None,
                        })
                        .map(|window| Wanted {
                            id: window.info.id,
                            pid: window.info.pid,
                            frame: window.info.frame,
                            title: window.info.title.clone(),
                        })
                        .collect();
                    let scale = display::displays(self.mtm)
                        .iter()
                        .map(|display| display.scale)
                        .fold(1., f64::max);
                    self.thumbnails.refresh(wanted, zoom * scale, 0);
                }
            }
            // The pictures are kept while the overview is closed, to have something to show
            // the moment it opens again. They go when the config stops asking for them.
            None if overview.is_some() => self.thumbnails.clear(),
            None => {}
        }

        // This is called on every pass; most of them change nothing.
        let version = self.thumbnails.version();
        if self.overview.as_ref() == overview && self.thumbnails_shown == version {
            return;
        }
        self.overview = overview.cloned();
        self.thumbnails_shown = version;
        let displays = display::displays(self.mtm);
        let windows = &self.windows;
        let thumbnails = &self.thumbnails;
        let wants_pictures = overview.is_some_and(|overview| overview.thumbnails.is_some());
        self.overview_panels
            .get_or_insert_with(OverviewPanels::new)
            .set(
                &displays,
                overview,
                &|id| windows.get(&id).map(|window| window.info.pid),
                &|id| thumbnails.get(id).filter(|_| wants_pictures),
            );
    }

    fn set_hotkey_overlay(&mut self, overlay: Option<&HotkeyOverlay>) {
        if self.hotkey_overlay.as_ref() == overlay {
            return;
        }
        self.hotkey_overlay = overlay.cloned();
        let displays = display::displays(self.mtm);
        self.hotkey_panels
            .get_or_insert_with(HotkeyOverlayPanels::new)
            .set(&displays, overlay);
    }

    fn drain_events(&mut self) -> Vec<Event> {
        // First of all: what follows depends on which Space is showing.
        self.refresh_home();

        let raw = RAW.with(|queue| std::mem::take(&mut *queue.borrow_mut()));
        for raw in raw {
            match raw {
                Raw::Ax {
                    pid,
                    element,
                    notification,
                } => self.handle_ax(pid, element, &notification),
                Raw::AppsChanged => self.sync_apps(),
                Raw::FrontmostAppChanged => self.refresh_focus(),
                Raw::DisplaysChanged => {
                    if self.events.last() != Some(&Event::DisplaysChanged) {
                        self.events.push(Event::DisplaysChanged);
                    }
                    // A new display needs an overlay of its own.
                    if let Some(overlay) = &mut self.overlay {
                        overlay.set(&display::displays(self.mtm), &self.outlines);
                    }
                    // The list of key bindings is in the middle of each display.
                    if let (Some(panels), Some(overlay)) =
                        (&mut self.hotkey_panels, &self.hotkey_overlay)
                    {
                        panels.set(&display::displays(self.mtm), Some(overlay));
                    }
                }
                Raw::SpaceChanged => self.restart_space_checks(),
            }
        }

        self.retry_pending_apps();

        if self.space_checked.elapsed() >= SPACE_CHECK_INTERVAL {
            self.check_spaces(false);
        }
        std::mem::take(&mut self.events)
    }
}

impl Drop for MacBackend {
    fn drop(&mut self) {
        let center = NSWorkspace::sharedWorkspace().notificationCenter();
        for token in self.workspace_observers.drain(..) {
            // SAFETY: the token came from this notification center.
            unsafe { center.removeObserver(ProtocolObject::as_ref(&token)) };
        }

        // SAFETY: removes exactly the registration made in `new()`.
        unsafe {
            CGDisplayRemoveReconfigurationCallback(Some(display_callback), std::ptr::null_mut());
        }

        let pids: Vec<i32> = self.apps.keys().copied().collect();
        for pid in pids {
            self.forget_app(pid);
        }
    }
}
