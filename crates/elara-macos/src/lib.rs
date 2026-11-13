//! Access to macOS windows and displays for ElaraWM.
//!
//! ElaraWM is not a compositor: macOS draws and owns every window. What a window manager can do
//! is ask applications, through the Accessibility API, where their windows are and to move and
//! resize them. This crate wraps that, and nothing else: it knows nothing about layout.
//!
//! [`WindowBackend`] is the whole interface. [`MacBackend`] implements it against the system and
//! [`MockBackend`] implements it in memory, so everything above this crate can be tested without
//! a desktop session or the Accessibility permission.
//!
//! All coordinates are in points, in the global space the Accessibility API uses: the origin is
//! the top-left corner of the primary display and y grows downwards.

#[macro_use]
extern crate tracing;

mod geometry;
#[cfg(target_os = "macos")]
mod macos;
mod mock;

use std::fmt;

pub use geometry::{Point, Rect, Size};
#[cfg(target_os = "macos")]
pub use macos::{
    accessibility_trusted, displays, request_accessibility, run as run_main_loop, MacBackend,
};
pub use mock::MockBackend;

/// Identifies a window for as long as it exists. Never reused within one backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WindowId(pub u64);

/// Identifies a display. On macOS this is the `CGDirectDisplayID`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DisplayId(pub u32);

/// A connected display.
#[derive(Debug, Clone, PartialEq)]
pub struct DisplayInfo {
    pub id: DisplayId,
    /// Human-readable name, such as "Built-in Retina Display".
    pub name: String,
    /// The whole display.
    pub frame: Rect,
    /// The part of the display not covered by the menu bar and the Dock.
    pub visible_frame: Rect,
    /// Pixels per point.
    pub scale: f64,
    /// Refresh rate in millihertz, if known.
    pub refresh_mhz: Option<u32>,
    /// Whether this is the display with the menu bar.
    pub is_primary: bool,
}

/// What sort of window this is, which decides how it is managed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowKind {
    /// An ordinary document or application window. Tiled by default.
    Standard,
    /// A dialog, sheet or panel. Floats by default.
    Dialog,
    /// Anything else an application exposes as a window, such as a popover or a tooltip. Not
    /// managed.
    Other,
}

/// A window as last reported by its application.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowInfo {
    pub id: WindowId,
    /// Process that owns the window.
    pub pid: i32,
    /// Bundle identifier of the owning application, such as "com.apple.Terminal".
    pub app_id: Option<String>,
    pub title: Option<String>,
    pub frame: Rect,
    pub kind: WindowKind,
    /// Whether the application lets the window be resized.
    pub resizable: bool,
    /// Whether the application lets the window be moved.
    pub movable: bool,
    pub minimized: bool,
    /// Whether the window is in macOS native fullscreen, on a Space of its own.
    pub fullscreen: bool,
}

/// Something that happened to a window or display since events were last drained.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A window appeared.
    WindowOpened(WindowInfo),
    /// A window is gone.
    WindowClosed(WindowId),
    /// A window was moved or resized, by the user, its application, or ElaraWM itself.
    WindowFrameChanged(WindowId, Rect),
    WindowTitleChanged(WindowId, Option<String>),
    /// A window was minimized (`true`) or restored (`false`). Hiding an application minimizes
    /// all of its windows in this sense.
    WindowMinimized(WindowId, bool),
    /// A window entered (`true`) or left (`false`) native fullscreen.
    WindowFullscreen(WindowId, bool),
    /// Keyboard focus moved to this window, or to nothing that is tracked.
    FocusChanged(Option<WindowId>),
    /// Displays were connected, disconnected, rearranged or reconfigured.
    DisplaysChanged,
}

/// Why a request to a window failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The window no longer exists.
    NoSuchWindow(WindowId),
    /// ElaraWM has not been granted the Accessibility permission.
    NotTrusted,
    /// The application did not answer in time, or refused.
    Unresponsive { window: WindowId, code: i32 },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NoSuchWindow(id) => write!(f, "window {} no longer exists", id.0),
            Error::NotTrusted => write!(f, "the Accessibility permission has not been granted"),
            Error::Unresponsive { window, code } => write!(
                f,
                "the application owning window {} did not carry out the request (AXError {code})",
                window.0
            ),
        }
    }
}

impl std::error::Error for Error {}

/// The operations a window manager needs from the system.
pub trait WindowBackend {
    /// Lists the connected displays, primary first.
    fn displays(&self) -> Vec<DisplayInfo>;

    /// Lists every window currently known, in no particular order.
    fn windows(&self) -> Vec<WindowInfo>;

    /// Returns the window that has keyboard focus, if it is a known window.
    fn focused_window(&self) -> Option<WindowId>;

    /// Asks a window to occupy `frame`.
    ///
    /// Returns the frame the window actually took. Applications may refuse a size, for example
    /// below their minimum, so the result can differ from the request.
    fn set_frame(&mut self, window: WindowId, frame: Rect) -> Result<Rect, Error>;

    /// Gives a window keyboard focus and raises it.
    fn focus(&mut self, window: WindowId) -> Result<(), Error>;

    /// Asks a window to close, as if its close button was pressed. The application may prompt.
    fn close(&mut self, window: WindowId) -> Result<(), Error>;

    /// Returns what happened since the last call, oldest first.
    fn drain_events(&mut self) -> Vec<Event>;
}
