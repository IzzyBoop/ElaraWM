//! An in-memory [`WindowBackend`] for tests.

use std::collections::BTreeMap;

use crate::{
    DisplayId, DisplayInfo, Error, Event, Outline, Rect, Size, WindowBackend, WindowId, WindowInfo,
    WindowKind,
};

/// A window in a [`MockBackend`], with the limits its pretend application enforces.
#[derive(Debug, Clone)]
struct MockWindow {
    info: WindowInfo,
    min_size: Size,
    max_size: Option<Size>,
}

/// A [`WindowBackend`] that keeps windows and displays in memory.
///
/// It behaves like well-mannered macOS applications do: a window takes the frame it is asked to
/// take, except that it keeps its size within its limits, ignores moves or resizes it does not
/// allow, will not be resized to reach past the right or bottom edge of the display it is on
/// or be placed under the menu bar, and reports every change back as an event. Tests drive the
/// "user" and "application" side through the methods on this type and the window manager side
/// through [`WindowBackend`].
#[derive(Debug, Default)]
pub struct MockBackend {
    displays: Vec<DisplayInfo>,
    windows: BTreeMap<WindowId, MockWindow>,
    focused: Option<WindowId>,
    events: Vec<Event>,
    next_id: u64,
    /// Every `set_frame` request received, in order, for tests to inspect.
    frame_requests: Vec<(WindowId, Rect)>,
    /// The outlines last asked for.
    outlines: Vec<Outline>,
}

impl MockBackend {
    /// Creates a backend with no displays and no windows.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a backend with one 1800x1169 display at scale 2 with a 39 point menu bar, which is
    /// what a 16-inch MacBook Pro reports at its default resolution with the Dock hidden.
    pub fn with_laptop_display() -> Self {
        let mut backend = Self::new();
        backend.add_display(DisplayInfo {
            id: DisplayId(1),
            name: "Built-in Retina Display".to_owned(),
            frame: Rect::new(0., 0., 1800., 1169.),
            visible_frame: Rect::new(0., 39., 1800., 1130.),
            scale: 2.,
            refresh_mhz: Some(120_000),
            is_primary: true,
        });
        backend.drain_events();
        backend
    }

    /// Connects a display.
    pub fn add_display(&mut self, display: DisplayInfo) {
        self.displays.retain(|d| d.id != display.id);
        self.displays.push(display);
        self.sort_displays();
        self.events.push(Event::DisplaysChanged);
    }

    /// Disconnects a display.
    pub fn remove_display(&mut self, id: DisplayId) {
        self.displays.retain(|d| d.id != id);
        self.events.push(Event::DisplaysChanged);
    }

    /// Changes the part of a display that the menu bar and the Dock leave free, as happens
    /// when the Dock is shown or hidden. Like macOS, this reports no event.
    pub fn set_visible_frame(&mut self, id: DisplayId, visible_frame: Rect) {
        if let Some(display) = self.displays.iter_mut().find(|d| d.id == id) {
            display.visible_frame = visible_frame;
        }
    }

    fn sort_displays(&mut self) {
        self.displays.sort_by_key(|d| (!d.is_primary, d.id));
    }

    /// Opens a standard, resizable, movable window owned by `app_id`.
    pub fn open_window(&mut self, app_id: &str, title: &str, frame: Rect) -> WindowId {
        self.open_window_with(app_id, title, frame, |_| {})
    }

    /// Opens a window, letting `adjust` change its properties before it is announced.
    pub fn open_window_with(
        &mut self,
        app_id: &str,
        title: &str,
        frame: Rect,
        adjust: impl FnOnce(&mut WindowInfo),
    ) -> WindowId {
        self.next_id += 1;
        let id = WindowId(self.next_id);
        let mut info = WindowInfo {
            id,
            pid: 1000 + self.next_id as i32,
            app_id: Some(app_id.to_owned()),
            title: Some(title.to_owned()),
            frame,
            kind: WindowKind::Standard,
            resizable: true,
            movable: true,
            minimized: false,
            fullscreen: false,
        };
        adjust(&mut info);
        info.id = id;

        self.windows.insert(
            id,
            MockWindow {
                info: info.clone(),
                min_size: Size { w: 1., h: 1. },
                max_size: None,
            },
        );
        self.events.push(Event::WindowOpened(info));
        id
    }

    /// Sets the smallest and largest size the window's application will accept.
    pub fn set_size_limits(&mut self, id: WindowId, min: Size, max: Option<Size>) {
        if let Some(window) = self.windows.get_mut(&id) {
            window.min_size = min;
            window.max_size = max;
        }
    }

    /// The application closes the window.
    pub fn close_window(&mut self, id: WindowId) {
        if self.windows.remove(&id).is_some() {
            self.events.push(Event::WindowClosed(id));
            if self.focused == Some(id) {
                self.focused = None;
                self.events.push(Event::FocusChanged(None));
            }
        }
    }

    /// The user drags or resizes the window. Size limits still apply, as they do on macOS.
    pub fn user_set_frame(&mut self, id: WindowId, frame: Rect) {
        self.apply_frame(id, frame, true);
    }

    /// The user clicks the window, or switches to it some other way.
    pub fn user_focus(&mut self, id: WindowId) {
        if self.windows.contains_key(&id) && self.focused != Some(id) {
            self.focused = Some(id);
            self.events.push(Event::FocusChanged(Some(id)));
        }
    }

    /// The window's title changes.
    pub fn set_title(&mut self, id: WindowId, title: &str) {
        if let Some(window) = self.windows.get_mut(&id) {
            window.info.title = Some(title.to_owned());
            self.events
                .push(Event::WindowTitleChanged(id, Some(title.to_owned())));
        }
    }

    /// The window is minimized or restored.
    pub fn set_minimized(&mut self, id: WindowId, minimized: bool) {
        if let Some(window) = self.windows.get_mut(&id) {
            if window.info.minimized != minimized {
                window.info.minimized = minimized;
                self.events.push(Event::WindowMinimized(id, minimized));
            }
        }
    }

    /// The window enters or leaves native fullscreen.
    pub fn set_fullscreen(&mut self, id: WindowId, fullscreen: bool) {
        if let Some(window) = self.windows.get_mut(&id) {
            if window.info.fullscreen != fullscreen {
                window.info.fullscreen = fullscreen;
                self.events.push(Event::WindowFullscreen(id, fullscreen));
            }
        }
    }

    /// Returns the current state of a window.
    pub fn window(&self, id: WindowId) -> Option<&WindowInfo> {
        self.windows.get(&id).map(|w| &w.info)
    }

    /// Returns the current frame of a window. Panics if it does not exist.
    pub fn frame(&self, id: WindowId) -> Rect {
        self.windows[&id].info.frame
    }

    /// Returns every `set_frame` request received so far and forgets them.
    /// Returns the outlines that are on screen.
    pub fn outlines(&self) -> &[Outline] {
        &self.outlines
    }

    pub fn take_frame_requests(&mut self) -> Vec<(WindowId, Rect)> {
        std::mem::take(&mut self.frame_requests)
    }

    fn apply_frame(&mut self, id: WindowId, requested: Rect, by_user: bool) -> Option<Rect> {
        let window = self.windows.get_mut(&id)?;
        let mut frame = window.info.frame;

        // The user can always drag a window; only programmatic moves respect `movable`.
        if window.info.movable || by_user {
            frame.x = requested.x;
            frame.y = requested.y;
        }
        if window.info.resizable {
            let mut w = requested.w.max(window.min_size.w);
            let mut h = requested.h.max(window.min_size.h);
            if let Some(max) = window.max_size {
                w = w.min(max.w);
                h = h.min(max.h);
            }

            // macOS cuts a resize short at the edges of the display the window is on. A window
            // may hang off a display, having been moved there, but it cannot be grown past the
            // edge.
            let resized = w != window.info.frame.w || h != window.info.frame.h;
            let origin = crate::Point {
                x: frame.x,
                y: frame.y,
            };
            if let Some(display) = self.displays.iter().find(|d| d.frame.contains(origin)) {
                // Nor can a window be put under the menu bar.
                if !by_user && frame.y < display.visible_frame.y {
                    frame.y = display.visible_frame.y;
                }
                if resized && !by_user {
                    w = w
                        .min(display.frame.max_x() - frame.x)
                        .max(window.min_size.w);
                    h = h
                        .min(display.frame.max_y() - frame.y)
                        .max(window.min_size.h);
                }
            }

            frame.w = w;
            frame.h = h;
        }

        if frame != window.info.frame {
            window.info.frame = frame;
            self.events.push(Event::WindowFrameChanged(id, frame));
        }
        Some(frame)
    }
}

impl WindowBackend for MockBackend {
    fn displays(&self) -> Vec<DisplayInfo> {
        self.displays.clone()
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

    fn set_frame(&mut self, window: WindowId, frame: Rect) -> Result<Rect, Error> {
        self.frame_requests.push((window, frame));
        self.apply_frame(window, frame, false)
            .ok_or(Error::NoSuchWindow(window))
    }

    fn focus(&mut self, window: WindowId) -> Result<(), Error> {
        if !self.windows.contains_key(&window) {
            return Err(Error::NoSuchWindow(window));
        }
        self.user_focus(window);
        Ok(())
    }

    fn close(&mut self, window: WindowId) -> Result<(), Error> {
        if !self.windows.contains_key(&window) {
            return Err(Error::NoSuchWindow(window));
        }
        self.close_window(window);
        Ok(())
    }

    fn drain_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    fn set_outlines(&mut self, outlines: &[Outline]) {
        self.outlines = outlines.to_vec();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opening_and_closing_report_events() {
        let mut backend = MockBackend::with_laptop_display();
        assert!(backend.drain_events().is_empty());

        let a = backend.open_window("com.example.a", "A", Rect::new(0., 39., 800., 600.));
        let b = backend.open_window("com.example.b", "B", Rect::new(100., 100., 400., 300.));
        assert_ne!(a, b);

        let events = backend.drain_events();
        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], Event::WindowOpened(info) if info.id == a));
        assert!(matches!(&events[1], Event::WindowOpened(info) if info.id == b));
        assert_eq!(backend.windows().len(), 2);

        backend.user_focus(b);
        backend.close_window(b);
        assert_eq!(
            backend.drain_events(),
            vec![
                Event::FocusChanged(Some(b)),
                Event::WindowClosed(b),
                Event::FocusChanged(None),
            ]
        );
        assert_eq!(backend.focused_window(), None);
        assert_eq!(backend.close(b), Err(Error::NoSuchWindow(b)));
    }

    #[test]
    fn set_frame_returns_what_the_window_accepted() {
        let mut backend = MockBackend::with_laptop_display();
        let id = backend.open_window("com.example.a", "A", Rect::new(0., 39., 800., 600.));
        backend.set_size_limits(
            id,
            Size { w: 400., h: 300. },
            Some(Size { w: 1000., h: 900. }),
        );
        backend.drain_events();

        // Within limits: taken as asked.
        let frame = Rect::new(10., 50., 500., 400.);
        assert_eq!(backend.set_frame(id, frame), Ok(frame));
        assert_eq!(
            backend.drain_events(),
            vec![Event::WindowFrameChanged(id, frame)]
        );

        // Below the minimum and above the maximum: clamped, position still applied.
        let got = backend
            .set_frame(id, Rect::new(20., 60., 100., 2000.))
            .unwrap();
        assert_eq!(got, Rect::new(20., 60., 400., 900.));

        // Asking for the current frame changes nothing and reports nothing.
        backend.drain_events();
        assert_eq!(backend.set_frame(id, got), Ok(got));
        assert!(backend.drain_events().is_empty());

        assert_eq!(backend.take_frame_requests().len(), 3);
        assert!(backend.take_frame_requests().is_empty());
    }

    #[test]
    fn fixed_windows_ignore_programmatic_changes() {
        let mut backend = MockBackend::with_laptop_display();
        let start = Rect::new(100., 100., 300., 200.);
        let id = backend.open_window_with("com.example.a", "Prefs", start, |info| {
            info.resizable = false;
            info.movable = false;
            info.kind = WindowKind::Dialog;
        });

        assert_eq!(
            backend.set_frame(id, Rect::new(0., 0., 900., 900.)),
            Ok(start)
        );

        // The user can still drag it, but not resize it.
        backend.user_set_frame(id, Rect::new(50., 60., 900., 900.));
        assert_eq!(backend.frame(id), Rect::new(50., 60., 300., 200.));
    }

    #[test]
    fn displays_are_listed_primary_first() {
        let mut backend = MockBackend::with_laptop_display();
        backend.add_display(DisplayInfo {
            id: DisplayId(0),
            name: "External".to_owned(),
            frame: Rect::new(1800., 0., 2560., 1440.),
            visible_frame: Rect::new(1800., 0., 2560., 1440.),
            scale: 1.,
            refresh_mhz: Some(60_000),
            is_primary: false,
        });
        assert_eq!(backend.drain_events(), vec![Event::DisplaysChanged]);

        let ids: Vec<_> = backend.displays().iter().map(|d| d.id).collect();
        assert_eq!(ids, vec![DisplayId(1), DisplayId(0)]);

        backend.remove_display(DisplayId(0));
        assert_eq!(backend.displays().len(), 1);
    }
}
