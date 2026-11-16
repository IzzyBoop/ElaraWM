//! The layout's view of one macOS window.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use elara_layout::layout::{
    ConfigureIntent, InteractiveResizeData, LayoutElement, LayoutElementRenderSnapshot, SizingMode,
};
use elara_layout::render_helpers::offscreen::OffscreenData;
use elara_layout::utils::transaction::Transaction;
use elara_layout::utils::ResizeEdge;
use elara_layout::window::{ResolvedWindowRules, WindowRef};
use elara_macos::{Rect, WindowId, WindowInfo};
use niri_config::CornerRadius;
use smithay::output::{self, Output};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, Serial, Size, Transform};

/// A macOS window as niri's layout sees it.
///
/// In niri this role is played by `Mapped`, which wraps a Wayland toplevel. The layout tells a
/// window what size it should be with [`LayoutElement::request_size`]; a Wayland client answers
/// later by committing a buffer of some size. Here the request is only recorded. [`State`]
/// carries it out through the Accessibility API and records the size the window really took,
/// which need not be the size asked for.
///
/// Cloning yields another handle to the same window: the state is shared, and only the window
/// rules are copied. The layout owns one handle and [`State`] keeps another, so it can reach a
/// window without searching the layout. The layout's handle is the one whose rules count.
///
/// [`State`]: crate::State
#[derive(Debug, Clone)]
pub struct Window {
    inner: Rc<Inner>,
    rules: ResolvedWindowRules,
}

#[derive(Debug)]
struct Inner {
    id: WindowId,
    pid: i32,
    app_id: Option<String>,
    title: RefCell<Option<String>>,
    resizable: bool,

    /// The frame the window had when ElaraWM first saw it, to give back on exit.
    original_frame: Rect,
    /// The frame the window really has, in global coordinates.
    frame: Cell<Rect>,
    /// The last frame asked of the window. Kept so that a request the window answered
    /// differently is not repeated on every pass.
    last_request: Cell<Option<Rect>>,
    /// A size the window was asked for where it had room to take it, and answered with
    /// another that it has kept since.
    settled_for: Cell<Option<(i32, i32)>>,

    /// Size the layout last asked for. A zero dimension means "keep what you have".
    requested_size: Cell<Option<Size<i32, Logical>>>,
    sizing_mode: Cell<SizingMode>,
    pending_sizing_mode: Cell<SizingMode>,
    is_windowed_fullscreen: Cell<bool>,
    is_pending_windowed_fullscreen: Cell<bool>,

    /// Size limits. macOS does not expose them, so they start out unknown (zero) and are
    /// learned whenever the window refuses a size.
    min_size: Cell<Size<i32, Logical>>,
    max_size: Cell<Size<i32, Logical>>,

    /// For width and height: the last time the window took a size close to, but not, the
    /// one asked for, as what was asked and what it took. See [`learn_limits_on_axis()`].
    refusals: Cell<[Option<(i32, i32)>; 2]>,
    /// Width over height, if the window keeps its shape whatever size it is asked for, as
    /// a video player does.
    aspect: Cell<Option<f64>>,
    /// Whether it has been found out if the window keeps its shape.
    shape_known: Cell<bool>,

    is_activated: Cell<bool>,
    is_active_in_column: Cell<bool>,
    is_floating: Cell<bool>,

    /// The interactive resize the window is part of, if any.
    interactive_resize: Cell<Option<InteractiveResize>>,
    /// When a resize of this window was last started with the mouse, and by which edges.
    /// Two in quick succession are niri's double-click gestures.
    last_interactive_resize_start: Cell<Option<(Duration, ResizeEdge)>>,
}

/// Where a window is in an interactive resize. niri's `Mapped` has the same states, spread
/// over a Wayland configure and commit.
#[derive(Debug, Clone, Copy)]
enum InteractiveResize {
    /// The edge is being dragged.
    Ongoing(InteractiveResizeData),
    /// The drag ended. The window may still owe the layout the last size that was asked of
    /// it, and the layout needs to know which edges were dragged when that size arrives.
    Ending(InteractiveResizeData),
}

/// How round the corners of a macOS window are taken to be, in points, unless a window rule
/// sets `geometry-corner-radius`. This is the radius of a plain titled window; windows with
/// a toolbar are rounder.
pub const DEFAULT_CORNER_RADIUS: f32 = 16.;

/// How far a window's answer may be from the size asked for and still be put down to the
/// window rounding the size, not refusing it. Terminals only take sizes that fit a whole
/// number of character cells, and a cell in a large font is about this big.
const SIZE_STEP: i32 = 40;

/// How much of a size is asked for again to find out whether a window keeps its shape.
/// Little enough less to be a small twitch on screen, and enough to tell the answers apart.
const SHAPE_PROBE: f64 = 0.8;

/// Updates what is known of a window's smallest and largest size along one axis, from the
/// size it was asked to take and the size it took. Zero means the limit is not known.
///
/// A window that answers with a larger size has a minimum, and one that answers with a
/// smaller size a maximum, unless it merely rounded: a terminal asked for 1098 points that
/// takes 1104 has no minimum of 1104, and believing that it does leaves no room for a second
/// window in its column. A far-off answer is a limit at once. A nearby one counts as a limit
/// only once the same answer has come back to a different request, which rounding does not
/// do. And a size the window takes later that is past a known limit disproves it.
fn learn_limits_on_axis(
    asked: i32,
    got: i32,
    learn: bool,
    min: &mut i32,
    max: &mut i32,
    refusal: &mut Option<(i32, i32)>,
) {
    // The window is at a size the limits say it cannot have.
    if *min > 0 && got < *min {
        *min = 0;
    }
    if *max > 0 && got > *max {
        *max = 0;
    }

    if !learn || got == asked {
        return;
    }

    let repeated = matches!(*refusal, Some((before, answer)) if answer == got && before != asked);
    if (got - asked).abs() > SIZE_STEP || repeated {
        if got > asked {
            *min = got;
        } else {
            *max = got;
        }
    } else {
        *refusal = Some((asked, got));
    }
}

fn size_of(frame: Rect) -> Size<i32, Logical> {
    Size::from((frame.w.round() as i32, frame.h.round() as i32))
}

impl Window {
    /// Wraps a window the system reported.
    pub fn new(info: &WindowInfo, rules: ResolvedWindowRules) -> Self {
        let size = size_of(info.frame);
        // A window that cannot be resized has one allowed size.
        let limit = if info.resizable {
            Size::default()
        } else {
            size
        };

        let inner = Inner {
            id: info.id,
            pid: info.pid,
            app_id: info.app_id.clone(),
            title: RefCell::new(info.title.clone()),
            resizable: info.resizable,
            original_frame: info.frame,
            frame: Cell::new(info.frame),
            last_request: Cell::new(None),
            settled_for: Cell::new(None),
            requested_size: Cell::new(None),
            sizing_mode: Cell::new(SizingMode::Normal),
            pending_sizing_mode: Cell::new(SizingMode::Normal),
            is_windowed_fullscreen: Cell::new(false),
            is_pending_windowed_fullscreen: Cell::new(false),
            min_size: Cell::new(limit),
            max_size: Cell::new(limit),
            refusals: Cell::new([None; 2]),
            aspect: Cell::new(None),
            shape_known: Cell::new(false),
            is_activated: Cell::new(false),
            is_active_in_column: Cell::new(true),
            is_floating: Cell::new(false),
            interactive_resize: Cell::new(None),
            last_interactive_resize_start: Cell::new(None),
        };
        Self {
            inner: Rc::new(inner),
            rules,
        }
    }

    pub fn window_id(&self) -> WindowId {
        self.inner.id
    }

    /// When a resize of this window was last started with the mouse, and by which edges.
    pub fn last_interactive_resize_start(&self) -> &Cell<Option<(Duration, ResizeEdge)>> {
        &self.inner.last_interactive_resize_start
    }

    /// Forgets an interactive resize that has ended, once the window has been given its last
    /// size.
    pub fn finish_interactive_resize(&self) {
        if let Some(InteractiveResize::Ending(_)) = self.inner.interactive_resize.get() {
            self.inner.interactive_resize.set(None);
        }
    }

    pub fn pid(&self) -> i32 {
        self.inner.pid
    }

    pub fn app_id(&self) -> Option<&str> {
        self.inner.app_id.as_deref()
    }

    pub fn title(&self) -> Option<String> {
        self.inner.title.borrow().clone()
    }

    pub fn set_title(&self, title: Option<String>) {
        *self.inner.title.borrow_mut() = title;
    }

    /// The frame the window really has, in global coordinates.
    pub fn frame(&self) -> Rect {
        self.inner.frame.get()
    }

    /// The frame the window had before ElaraWM touched it.
    pub fn original_frame(&self) -> Rect {
        self.inner.original_frame
    }

    /// The last frame asked of the window, if it has not moved on its own since.
    pub fn last_request(&self) -> Option<Rect> {
        self.inner.last_request.get()
    }

    /// The size the window has declined in favor of the one it has, if any: it was asked for
    /// it where nothing but the window itself stood in the way, and took another. A terminal
    /// does this to fit whole rows of text. Asking again would change nothing.
    pub fn settled_for(&self) -> Option<(i32, i32)> {
        self.inner.settled_for.get()
    }

    /// Notes that the window answered a request for `size` with the size it has now.
    pub fn set_settled_for(&self, size: Option<(i32, i32)>) {
        self.inner.settled_for.set(size);
    }

    /// Records that the window was asked to take `request` and ended up with `actual`.
    ///
    /// Where the window refused the requested size and `learn_limits` is set, the refusal is
    /// remembered as a size limit so the layout stops asking for sizes the window will not
    /// take. The caller clears `learn_limits` when something other than the window may have
    /// cut the size short. The pending sizing mode
    /// becomes current, as it does when a Wayland client acknowledges a configure.
    ///
    /// Returns whether anything the layout cares about changed.
    pub fn record_response(&self, request: Rect, actual: Rect, learn_limits: bool) -> bool {
        let inner = &self.inner;
        let before = (
            self.size(),
            inner.sizing_mode.get(),
            inner.min_size.get(),
            inner.max_size.get(),
        );

        inner.last_request.set(Some(request));
        inner.frame.set(actual);

        if inner.resizable && inner.aspect.get().is_some() {
            // A window that keeps its shape has no limits to learn: every answer that is
            // not what was asked is its shape speaking. An answer far from a request that
            // was already of its shape means the shape is no longer what it was, as when
            // a player opens another video; it is found out again.
            let (asked, got) = (size_of(request), size_of(actual));
            let far = (got.w - asked.w).abs() > SIZE_STEP || (got.h - asked.h).abs() > SIZE_STEP;
            if learn_limits && far {
                inner.aspect.set(None);
                inner.shape_known.set(false);
            }
        } else if inner.resizable {
            let asked = size_of(request);
            let got = size_of(actual);
            let mut min = inner.min_size.get();
            let mut max = inner.max_size.get();
            let mut refusals = inner.refusals.get();
            learn_limits_on_axis(
                asked.w,
                got.w,
                learn_limits,
                &mut min.w,
                &mut max.w,
                &mut refusals[0],
            );
            learn_limits_on_axis(
                asked.h,
                got.h,
                learn_limits,
                &mut min.h,
                &mut max.h,
                &mut refusals[1],
            );
            inner.min_size.set(min);
            inner.max_size.set(max);
            inner.refusals.set(refusals);
        }

        inner.sizing_mode.set(inner.pending_sizing_mode.get());
        inner
            .is_windowed_fullscreen
            .set(inner.is_pending_windowed_fullscreen.get());

        before
            != (
                self.size(),
                inner.sizing_mode.get(),
                inner.min_size.get(),
                inner.max_size.get(),
            )
    }

    /// Whether the window's answer to a request is far enough from it to be a limit of the
    /// window's, which is also what a window that keeps its shape looks like: its answer
    /// has less of one side than was asked for, or more of the other. One more request
    /// tells the two apart; see [`record_shape_probe()`](Self::record_shape_probe).
    pub fn suspects_shape(&self, request: Rect, actual: Rect) -> bool {
        let (asked, got) = (size_of(request), size_of(actual));
        let far = |asked: i32, got: i32| (got - asked).abs() > SIZE_STEP;
        self.inner.resizable
            && !self.inner.shape_known.get()
            && (far(asked.w, got.w) || far(asked.h, got.h))
    }

    /// The request that finds out whether the window keeps its shape: for somewhat less
    /// than was asked for before, in both directions. `None` if the request was for so
    /// little that the answers could not be told apart.
    pub fn shape_probe(request: Rect) -> Option<Rect> {
        let less = |length: f64| (length * SHAPE_PROBE).round();
        let telling = |length: f64| length - less(length) > 2. * f64::from(SIZE_STEP);
        (telling(request.w) && telling(request.h))
            .then(|| Rect::new(request.x, request.y, less(request.w), less(request.h)))
    }

    /// Records the window's answer to `probe`, which came from
    /// [`shape_probe()`](Self::shape_probe), having answered `first` to `request`.
    ///
    /// A window with a limit answers the same way twice: with the limit again, or, where
    /// the second request is within it, with what was asked. One that keeps its shape
    /// answers with a size that is neither, since it follows from the other side.
    pub fn record_shape_probe(&self, request: Rect, first: Rect, probe: Rect, answer: Rect) {
        let inner = &self.inner;
        let (asked, first) = (size_of(request), size_of(first));
        let (probed, got) = (size_of(probe), size_of(answer));
        let far = |a: i32, b: i32| (a - b).abs() > SIZE_STEP;
        let follows = |asked: i32, first: i32, probed: i32, got: i32| {
            far(asked, first) && far(got, first) && far(got, probed)
        };
        let keeps_shape = follows(asked.w, first.w, probed.w, got.w)
            || follows(asked.h, first.h, probed.h, got.h);

        inner.shape_known.set(true);
        if !keeps_shape || got.w <= 0 || got.h <= 0 {
            return;
        }
        // The first answer is the larger and so the more exact, unless it was more than
        // was asked for, where the edge of the display may have cut it short.
        let within = first.w <= asked.w + SIZE_STEP && first.h <= asked.h + SIZE_STEP;
        let shape = if within && first.h > 0 { first } else { got };
        inner
            .aspect
            .set(Some(f64::from(shape.w) / f64::from(shape.h)));
        // Whatever was taken for a limit before was the shape.
        inner.min_size.set(Size::default());
        inner.max_size.set(Size::default());
        inner.refusals.set([None; 2]);
        inner.settled_for.set(None);
    }

    /// Whether the window keeps its shape whatever size it is asked for.
    pub fn keeps_shape(&self) -> bool {
        self.inner.aspect.get().is_some()
    }

    /// Returns `frame` as the window can take it: for a window that keeps its shape, the
    /// largest size of that shape that fits in it, at the same place. Asked for anything
    /// else, such a window picks a size by a rule of its own, which may be far larger.
    pub fn fit(&self, frame: Rect) -> Rect {
        let Some(ratio) = self.inner.aspect.get() else {
            return frame;
        };
        if frame.w <= 0. || frame.h <= 0. {
            return frame;
        }
        // The side that follows is rounded up. A point too little would make it the side
        // the window goes by, and the other would come out a point short of what the
        // layout asked for; a point too much the window simply does not take.
        if frame.w / frame.h > ratio {
            Rect::new(frame.x, frame.y, (frame.h * ratio).ceil(), frame.h)
        } else {
            Rect::new(frame.x, frame.y, frame.w, (frame.w / ratio).ceil())
        }
    }

    /// Records a frame the window took without being asked: the user dragged or resized it, or
    /// its application did.
    ///
    /// Returns whether the size changed.
    pub fn record_external_frame(&self, frame: Rect) -> bool {
        let old = self.size();
        self.inner.frame.set(frame);
        // Whatever was asked before no longer describes the window, so the next pass asks
        // again.
        self.inner.last_request.set(None);
        self.inner.settled_for.set(None);
        old != self.size()
    }

    /// Makes the window's current size the requested one.
    ///
    /// For a floating window that the user resized: the size is theirs to choose, so the
    /// earlier request must not be asserted again.
    pub fn accept_current_size(&self) {
        self.inner.requested_size.set(Some(self.size()));
    }

    /// The size the layout wants the window to have, with "keep what you have" resolved.
    pub fn target_size(&self) -> Size<i32, Logical> {
        let current = self.size();
        let mut size = self.inner.requested_size.get().unwrap_or(current);
        if size.w == 0 {
            size.w = current.w;
        }
        if size.h == 0 {
            size.h = current.h;
        }
        size
    }

    pub fn is_activated(&self) -> bool {
        self.inner.is_activated.get()
    }

    pub fn is_floating(&self) -> bool {
        self.inner.is_floating.get()
    }

    /// Replaces the window's rules, for when the config or the window's properties changed.
    ///
    /// This affects only this handle, so it must be called on the one the layout owns.
    pub fn set_rules(&mut self, rules: ResolvedWindowRules) {
        self.rules = rules;
    }

    /// Describes the window for matching against window rules.
    pub fn with_rule_ref<T>(&self, is_focused: bool, f: impl FnOnce(WindowRef<'_>) -> T) -> T {
        let title = self.inner.title.borrow();
        f(WindowRef {
            app_id: self.inner.app_id.as_deref(),
            title: title.as_deref(),
            is_active: self.inner.is_activated.get(),
            is_focused,
            is_urgent: false,
            is_active_in_column: self.inner.is_active_in_column.get(),
            is_floating: self.inner.is_floating.get(),
            is_window_cast_target: false,
        })
    }
}

impl LayoutElement for Window {
    type Id = WindowId;

    fn id(&self) -> &Self::Id {
        &self.inner.id
    }

    fn size(&self) -> Size<i32, Logical> {
        size_of(self.inner.frame.get())
    }

    fn buf_loc(&self) -> Point<i32, Logical> {
        // macOS window frames have no invisible margin around them.
        Point::from((0, 0))
    }

    fn is_in_input_region(&self, _point: Point<f64, Logical>) -> bool {
        // The system routes pointer input to windows; the layout is never asked to.
        false
    }

    fn request_size(
        &mut self,
        size: Size<i32, Logical>,
        mode: SizingMode,
        _animate: bool,
        _transaction: Option<Transaction>,
    ) {
        self.inner.requested_size.set(Some(size));
        self.inner.pending_sizing_mode.set(mode);
        if mode.is_fullscreen() {
            self.inner.is_pending_windowed_fullscreen.set(false);
        }
    }

    fn min_size(&self) -> Size<i32, Logical> {
        // As in niri's `Mapped`: the window's own limit, tightened by `min-width` and
        // `min-height` window rules.
        self.rules.apply_min_size(self.inner.min_size.get())
    }

    fn max_size(&self) -> Size<i32, Logical> {
        self.rules.apply_max_size(self.inner.max_size.get())
    }

    fn geometry_corner_radius(&self) -> CornerRadius {
        // niri cannot know how round a window's corners are, and takes them to be square
        // unless a `geometry-corner-radius` window rule says otherwise. On macOS every
        // ordinary window has round corners, so that is the assumption here.
        self.rules
            .geometry_corner_radius
            .unwrap_or(CornerRadius::from(DEFAULT_CORNER_RADIUS))
    }

    fn is_wl_surface(&self, wl_surface: &WlSurface) -> bool {
        match *wl_surface {}
    }

    fn has_ssd(&self) -> bool {
        // "Server-side decorations" means the compositor draws the title bar. macOS windows
        // draw their own.
        false
    }

    fn set_preferred_scale_transform(&self, _scale: output::Scale, _transform: Transform) {}

    fn output_enter(&self, _output: &Output) {}

    fn output_leave(&self, _output: &Output) {}

    fn set_offscreen_data(&self, _data: Option<OffscreenData>) {}

    fn set_activated(&mut self, active: bool) {
        self.inner.is_activated.set(active);
    }

    fn set_active_in_column(&mut self, active: bool) {
        self.inner.is_active_in_column.set(active);
    }

    fn set_floating(&mut self, floating: bool) {
        self.inner.is_floating.set(floating);
    }

    fn set_bounds(&self, _bounds: Size<i32, Logical>) {}

    fn is_ignoring_opacity_window_rule(&self) -> bool {
        false
    }

    fn is_urgent(&self) -> bool {
        false
    }

    fn configure_intent(&self) -> ConfigureIntent {
        ConfigureIntent::CanSend
    }

    fn send_pending_configure(&mut self) {
        // Requests are carried out by `State::apply()`, which reads them from the window.
    }

    fn sizing_mode(&self) -> SizingMode {
        self.inner.sizing_mode.get()
    }

    fn pending_sizing_mode(&self) -> SizingMode {
        self.inner.pending_sizing_mode.get()
    }

    fn requested_size(&self) -> Option<Size<i32, Logical>> {
        self.inner.requested_size.get()
    }

    fn is_windowed_fullscreen(&self) -> bool {
        self.inner.is_windowed_fullscreen.get()
    }

    fn is_pending_windowed_fullscreen(&self) -> bool {
        self.inner.is_pending_windowed_fullscreen.get()
    }

    fn request_windowed_fullscreen(&mut self, value: bool) {
        self.inner.is_pending_windowed_fullscreen.set(value);
    }

    fn is_child_of(&self, _parent: &Self) -> bool {
        // The Accessibility API does not say which window a dialog belongs to.
        false
    }

    fn rules(&self) -> &ResolvedWindowRules {
        &self.rules
    }

    fn refresh(&self) {}

    fn take_animation_snapshot(&mut self) -> Option<LayoutElementRenderSnapshot> {
        // There are no window contents to snapshot.
        None
    }

    fn set_interactive_resize(&mut self, data: Option<InteractiveResizeData>) {
        let resize = &self.inner.interactive_resize;
        if let Some(data) = data {
            resize.set(Some(InteractiveResize::Ongoing(data)));
        } else if let Some(InteractiveResize::Ongoing(data)) = resize.get() {
            resize.set(Some(InteractiveResize::Ending(data)));
        }
    }

    fn cancel_interactive_resize(&mut self) {
        self.inner.interactive_resize.set(None);
    }

    fn interactive_resize_data(&self) -> Option<InteractiveResizeData> {
        match self.inner.interactive_resize.get()? {
            InteractiveResize::Ongoing(data) | InteractiveResize::Ending(data) => Some(data),
        }
    }

    fn on_commit(&mut self, _serial: Serial) {}
}
