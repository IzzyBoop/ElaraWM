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
    NSWorkspace, NSWorkspaceDidActivateApplicationNotification,
    NSWorkspaceDidLaunchApplicationNotification, NSWorkspaceDidTerminateApplicationNotification,
};
use objc2_application_services::{AXError, AXObserver, AXUIElement};
use objc2_core_foundation::{kCFRunLoopDefaultMode, CFRetained, CFRunLoop, CFString};
use objc2_core_graphics::{
    CGDirectDisplayID, CGDisplayChangeSummaryFlags, CGDisplayRegisterReconfigurationCallback,
    CGDisplayRemoveReconfigurationCallback,
};
use objc2_foundation::{MainThreadMarker, NSNotification, NSNotificationName};

use super::{accessibility_trusted, ax, display};
use crate::{DisplayInfo, Error, Event, Rect, WindowBackend, WindowId, WindowInfo, WindowKind};

/// How long to wait for an application to answer one Accessibility request.
///
/// The system default is six seconds, which would freeze window management whenever one
/// application hangs.
const MESSAGING_TIMEOUT_SECS: f32 = 0.5;

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
}

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
        };

        backend.observe_workspace();
        // SAFETY: the callback is a valid function for the whole life of the process and takes
        // no user data. It is removed again in `Drop`.
        unsafe {
            CGDisplayRegisterReconfigurationCallback(Some(display_callback), std::ptr::null_mut());
        }

        backend.sync_apps();
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
        let kind = match ax::string(&element, ax::SUBROLE).as_deref() {
            Some("AXStandardWindow") => WindowKind::Standard,
            Some("AXDialog" | "AXSystemDialog" | "AXFloatingWindow" | "AXSystemFloatingWindow") => {
                WindowKind::Dialog
            }
            _ => WindowKind::Other,
        };
        if kind == WindowKind::Other {
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
        };
        debug!(?id, pid, app_id = ?info.app_id, ?kind, "tracking window");

        self.events.push(Event::WindowOpened(info.clone()));
        self.windows.insert(id, Window { info, element });
        Some(id)
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
        let focused = NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .map(|app| app.processIdentifier())
            .and_then(|pid| {
                let app = self.apps.get(&pid)?;
                let element = ax::element(&app.element, ax::FOCUSED_WINDOW)?;
                self.track_window(pid, element)
            });

        if focused != self.focused {
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

    fn focused_window(&self) -> Option<WindowId> {
        self.focused
    }

    fn set_frame(&mut self, id: WindowId, frame: Rect) -> Result<Rect, Error> {
        let window = self.windows.get_mut(&id).ok_or(Error::NoSuchWindow(id))?;
        let element = &window.element;
        let err = |err| Self::error(id, err);

        // Size, position, size. A window is not allowed to extend past the display it is on, so
        // a resize can be cut short at the old position and has to be repeated at the new one;
        // and resizing first avoids the window briefly hanging off the display at the new one.
        if window.info.resizable {
            ax::set_size(element, frame.w, frame.h).map_err(err)?;
        }
        if window.info.movable {
            ax::set_position(element, frame.x, frame.y).map_err(err)?;
        }
        if window.info.resizable {
            ax::set_size(element, frame.w, frame.h).map_err(err)?;
        }

        // What the window ended up with is the truth, whatever was asked for. The change is not
        // recorded here: the application will also send move and resize notifications, and
        // `refresh_frame()` reports the change once, from those.
        ax::frame(element).ok_or(Error::Unresponsive {
            window: id,
            code: AXError::CannotComplete.0,
        })
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

    fn drain_events(&mut self) -> Vec<Event> {
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
                }
            }
        }

        self.retry_pending_apps();
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
