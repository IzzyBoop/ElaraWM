//! Showing the overview: one window per display that covers it and draws every workspace and
//! window zoomed out.
//!
//! The windows of other applications cannot be drawn smaller than they are, so the overview
//! does not show them at all. Each is stood in for by a card with its application's icon and
//! its title. The window that shows the cards sits above the ordinary windows and below the
//! Dock and the menu bar, and unlike the one that shows outlines it takes the mouse: what is
//! clicked, dragged and scrolled on it is queued for [`take_overview_input()`].

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::c_void;
use std::rc::Rc;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::{define_class, msg_send, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSBackingStoreType, NSColor, NSEvent, NSEventModifierFlags, NSEventPhase,
    NSFont, NSImage, NSPanel, NSRunningApplication, NSView, NSWindowCollectionBehavior,
    NSWindowLevel, NSWindowStyleMask,
};
use objc2_core_foundation::{CFRetained, CFType, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGColor, CGColorSpace, CGContext, CGImageAlphaInfo,
};
use objc2_foundation::{NSObjectProtocol, NSString};
use objc2_quartz_core::{
    kCAAlignmentCenter, kCAFillRuleEvenOdd, kCAGravityResizeAspect, kCATruncationEnd, CALayer,
    CAShapeLayer, CATextLayer, CATransaction,
};

use super::overlay::{appkit_rect, color, path};
use crate::{
    DisplayId, DisplayInfo, KeyModifiers, MouseButton, MouseEvent, MouseEventKind, Outline,
    Overview, OverviewDisplay, OverviewInput, OverviewItem, OverviewWindow, Point, Rect,
    ScrollEvent, ScrollPhase, WindowId,
};

/// Where the overview is among the windows on screen: over every ordinary window, floating
/// ones included, and one below the Dock, which is at 20. The Dock and the menu bar stay
/// usable.
const LEVEL: NSWindowLevel = 19;

/// The smallest icon worth drawing, in points.
const MIN_ICON: f64 = 12.;
/// The largest an icon gets.
const MAX_ICON: f64 = 96.;
/// The size of a card's title, and the height of its line.
const TITLE_FONT_SIZE: f64 = 12.;
const TITLE_HEIGHT: f64 = 16.;
/// The space between the icon and the title, and beside the title.
const TITLE_GAP: f64 = 8.;

thread_local! {
    /// What was done on the overview since it was last asked for. Filled from inside the
    /// event loop, on the main thread.
    static INPUT: RefCell<Vec<OverviewInput>> = const { RefCell::new(Vec::new()) };
    /// The height of the primary display, which AppKit measures from the bottom of.
    static PRIMARY_HEIGHT: Cell<f64> = const { Cell::new(0.) };
}

/// Returns what the user clicked, dragged and scrolled on the overview since the last call,
/// oldest first.
pub fn take_overview_input() -> Vec<OverviewInput> {
    INPUT.with(|input| std::mem::take(&mut *input.borrow_mut()))
}

fn modifiers(event: &NSEvent) -> KeyModifiers {
    let flags = event.modifierFlags();
    KeyModifiers {
        control: flags.contains(NSEventModifierFlags::Control),
        shift: flags.contains(NSEventModifierFlags::Shift),
        option: flags.contains(NSEventModifierFlags::Option),
        command: flags.contains(NSEventModifierFlags::Command),
    }
}

/// Where the pointer is, in the global coordinates window frames use.
fn pointer_position() -> Point {
    let location = NSEvent::mouseLocation();
    Point {
        x: location.x,
        y: PRIMARY_HEIGHT.get() - location.y,
    }
}

fn push_mouse(event: &NSEvent, kind: MouseEventKind) {
    let input = OverviewInput::Mouse(MouseEvent {
        kind,
        position: pointer_position(),
        modifiers: modifiers(event),
    });
    INPUT.with(|queue| queue.borrow_mut().push(input));
}

/// The button of an event for a button past the first two.
fn other_button(event: &NSEvent) -> Option<MouseButton> {
    match event.buttonNumber() {
        2 => Some(MouseButton::Middle),
        3 => Some(MouseButton::Back),
        4 => Some(MouseButton::Forward),
        _ => None,
    }
}

define_class!(
    // SAFETY: `NSView` may be subclassed, the methods overridden here are meant to be, and
    // the class does not implement `Drop`.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "ElaraOverviewView"]
    struct OverviewView;

    impl OverviewView {
        /// The overview is never the active window, and a click on it should still count.
        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            push_mouse(event, MouseEventKind::Pressed(MouseButton::Left));
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            push_mouse(event, MouseEventKind::Dragged);
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, event: &NSEvent) {
            push_mouse(event, MouseEventKind::Released(MouseButton::Left));
        }

        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, event: &NSEvent) {
            push_mouse(event, MouseEventKind::Pressed(MouseButton::Right));
        }

        #[unsafe(method(rightMouseDragged:))]
        fn right_mouse_dragged(&self, event: &NSEvent) {
            push_mouse(event, MouseEventKind::Dragged);
        }

        #[unsafe(method(rightMouseUp:))]
        fn right_mouse_up(&self, event: &NSEvent) {
            push_mouse(event, MouseEventKind::Released(MouseButton::Right));
        }

        #[unsafe(method(otherMouseDown:))]
        fn other_mouse_down(&self, event: &NSEvent) {
            if let Some(button) = other_button(event) {
                push_mouse(event, MouseEventKind::Pressed(button));
            }
        }

        #[unsafe(method(otherMouseDragged:))]
        fn other_mouse_dragged(&self, event: &NSEvent) {
            push_mouse(event, MouseEventKind::Dragged);
        }

        #[unsafe(method(otherMouseUp:))]
        fn other_mouse_up(&self, event: &NSEvent) {
            if let Some(button) = other_button(event) {
                push_mouse(event, MouseEventKind::Released(button));
            }
        }

        #[unsafe(method(scrollWheel:))]
        fn scroll_wheel(&self, event: &NSEvent) {
            let phase = event.phase();
            let phase = if phase.contains(NSEventPhase::Began) {
                ScrollPhase::Began
            } else if phase.contains(NSEventPhase::Changed) {
                ScrollPhase::Changed
            } else if phase.intersects(NSEventPhase::Ended | NSEventPhase::Cancelled) {
                ScrollPhase::Ended
            } else {
                ScrollPhase::None
            };
            let scroll = ScrollEvent {
                delta_x: event.scrollingDeltaX(),
                delta_y: event.scrollingDeltaY(),
                // Trackpads report points; wheels report steps.
                continuous: event.hasPreciseScrollingDeltas(),
                phase,
                momentum: event.momentumPhase() != NSEventPhase::None,
                modifiers: modifiers(event),
            };
            let input = OverviewInput::Scroll(scroll, pointer_position());
            INPUT.with(|queue| queue.borrow_mut().push(input));
        }
    }

    unsafe impl NSObjectProtocol for OverviewView {}
);

impl OverviewView {
    fn new(mtm: MainThreadMarker, frame: CGRect) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        // SAFETY: `initWithFrame:` is the designated initializer of `NSView`.
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }
}

/// The colors of a card, which follow the system's light or dark appearance.
struct Style {
    card: Retained<CGColor>,
    border: Retained<CGColor>,
    text: Retained<CGColor>,
}

impl Style {
    fn current(mtm: MainThreadMarker) -> Self {
        // The system colors are light or dark depending on the appearance that is current
        // when they are asked for their components, which outside of drawing is not
        // necessarily the one the user chose.
        let style = Rc::new(RefCell::new(None));
        let resolve = RcBlock::new({
            let style = Rc::clone(&style);
            move || {
                *style.borrow_mut() = Some(Style {
                    card: NSColor::windowBackgroundColor().CGColor(),
                    border: NSColor::separatorColor().CGColor(),
                    text: NSColor::labelColor().CGColor(),
                });
            }
        });
        NSApplication::sharedApplication(mtm)
            .effectiveAppearance()
            .performAsCurrentDrawingAppearance(&resolve);
        let style = style.borrow_mut().take();
        style.expect("the block runs before the call returns")
    }
}

/// A window's card: a rounded rectangle with the application's icon and the window's title.
struct Card {
    layer: Retained<CALayer>,
    icon: Retained<CALayer>,
    title: Retained<CATextLayer>,
    /// What the title says now.
    text: Option<String>,
    has_icon: bool,
}

impl Card {
    fn new(style: &Style, icon: Option<&NSImage>, scale: f64) -> Self {
        let layer = CALayer::new();
        layer.setBackgroundColor(Some(&style.card));
        layer.setBorderColor(Some(&style.border));
        layer.setBorderWidth(1.);
        layer.setMasksToBounds(true);

        let icon_layer = CALayer::new();
        icon_layer.setContentsScale(scale);
        // SAFETY: the gravity is a valid static.
        icon_layer.setContentsGravity(unsafe { kCAGravityResizeAspect });
        if let Some(icon) = icon {
            // SAFETY: an image is one of the kinds of object a layer accepts as its contents.
            unsafe { icon_layer.setContents(Some(icon)) };
        }
        layer.addSublayer(&icon_layer);

        let title = CATextLayer::new();
        title.setContentsScale(scale);
        title.setFontSize(TITLE_FONT_SIZE);
        title.setForegroundColor(Some(&style.text));
        let font = NSFont::systemFontOfSize(TITLE_FONT_SIZE);
        // SAFETY: a font object is what a text layer takes for its font, and it is the same
        // object whether it is seen as an `NSFont` or as the Core Text type bridged to it.
        // The statics are valid.
        unsafe {
            title.setFont(Some(&*Retained::as_ptr(&font).cast::<CFType>()));
            title.setTruncationMode(kCATruncationEnd);
            title.setAlignmentMode(kCAAlignmentCenter);
        }
        layer.addSublayer(&title);

        Self {
            layer,
            icon: icon_layer,
            title,
            text: None,
            has_icon: icon.is_some(),
        }
    }

    /// Puts the card at `frame`, which is in its parent's coordinates, and lays out what is
    /// on it.
    fn update(&mut self, frame: CGRect, window: &OverviewWindow) {
        self.layer.setFrame(frame);
        let (w, h) = (frame.size.width, frame.size.height);
        // A layer rounds all of its corners alike.
        let radius = window.corner_radius[0].clamp(0., w.min(h) / 2.);
        self.layer.setCornerRadius(radius);

        if self.text != window.title {
            let text = NSString::from_str(window.title.as_deref().unwrap_or(""));
            // SAFETY: a string is one of the kinds of object a text layer accepts.
            unsafe { self.title.setString(Some(&text)) };
            self.text.clone_from(&window.title);
        }

        // The icon, with the title under it if there is room for both. Inside the card the
        // y axis points up, as it does for every layer that is not told otherwise.
        let side = if self.has_icon {
            (w.min(h) * 0.4).min(MAX_ICON)
        } else {
            0.
        };
        let show_icon = side >= MIN_ICON;
        let side = if show_icon { side } else { 0. };
        let has_text = self.text.as_ref().is_some_and(|text| !text.is_empty());
        let with_title = TITLE_HEIGHT + if show_icon { TITLE_GAP } else { 0. };
        let show_title = has_text && w >= 80. && h >= side + with_title + 2. * TITLE_GAP;

        let content = side + if show_title { with_title } else { 0. };
        let top = ((h - content) / 2.).round();

        self.icon.setHidden(!show_icon);
        self.icon.setFrame(CGRect::new(
            CGPoint::new(((w - side) / 2.).round(), h - top - side),
            CGSize::new(side, side),
        ));
        self.title.setHidden(!show_title);
        self.title.setFrame(CGRect::new(
            CGPoint::new(TITLE_GAP, h - top - content),
            CGSize::new((w - 2. * TITLE_GAP).max(0.), TITLE_HEIGHT),
        ));
    }
}

/// The layers that draw one display's part of the overview.
struct Scene {
    /// Covers the display and is filled with the backdrop.
    root: Retained<CALayer>,
    workspaces: Vec<Retained<CALayer>>,
    shapes: Vec<Retained<CAShapeLayer>>,
    cards: HashMap<WindowId, Card>,
}

impl Scene {
    fn new() -> Self {
        Self {
            root: CALayer::new(),
            workspaces: Vec::new(),
            shapes: Vec::new(),
            cards: HashMap::new(),
        }
    }

    /// Draws `part` of an overview on `display`, with what was drawn before as the starting
    /// point. `icon` finds the icon of a window's application.
    fn update(
        &mut self,
        display: &DisplayInfo,
        backdrop: [f32; 4],
        part: Option<&OverviewDisplay>,
        style: &Style,
        icon: &mut dyn FnMut(WindowId) -> Option<Retained<NSImage>>,
    ) {
        let bounds = CGRect::new(
            CGPoint::new(0., 0.),
            CGSize::new(display.frame.w, display.frame.h),
        );
        self.root.setFrame(bounds);
        self.root.setBackgroundColor(Some(&cg_color(backdrop)));
        // A workspace that is partly out of view at the top or bottom ends at the display.
        self.root.setMasksToBounds(true);

        // Layers measure from the bottom left of their parent; the overview measures from
        // the top left of the desktop.
        let local = |rect: Rect| {
            CGRect::new(
                CGPoint::new(
                    rect.x - display.frame.x,
                    display.frame.h - (rect.y - display.frame.y) - rect.h,
                ),
                CGSize::new(rect.w, rect.h),
            )
        };

        let workspaces = part.map_or(&[][..], |part| &part.workspaces[..]);
        while self.workspaces.len() > workspaces.len() {
            self.workspaces.pop().unwrap().removeFromSuperlayer();
        }
        while self.workspaces.len() < workspaces.len() {
            let layer = CALayer::new();
            self.root.addSublayer(&layer);
            self.workspaces.push(layer);
        }
        for (layer, workspace) in self.workspaces.iter().zip(workspaces) {
            layer.setFrame(local(workspace.rect));
            layer.setBackgroundColor(Some(&cg_color(workspace.color)));
            match workspace.shadow {
                Some(shadow) => {
                    let [r, g, b, a] = shadow.color;
                    layer.setShadowColor(Some(&cg_color([r, g, b, 1.])));
                    layer.setShadowOpacity(a);
                    // Down is towards smaller y here.
                    layer.setShadowOffset(CGSize::new(shadow.offset.0, -shadow.offset.1));
                    // The radius is that of the blur, which reaches about twice as far.
                    layer.setShadowRadius(shadow.softness / 2.);
                }
                None => layer.setShadowOpacity(0.),
            }
        }

        let items = part.map_or(&[][..], |part| &part.items[..]);
        let shape_count = items
            .iter()
            .filter(|item| matches!(item, OverviewItem::Shape(_)))
            .count();
        while self.shapes.len() > shape_count {
            self.shapes.pop().unwrap().removeFromSuperlayer();
        }
        while self.shapes.len() < shape_count {
            let shape = CAShapeLayer::new();
            // SAFETY: the fill rule is a valid static.
            shape.setFillRule(unsafe { kCAFillRuleEvenOdd });
            // Paths are laid out from the top left, like everything else in this crate.
            shape.setGeometryFlipped(true);
            self.root.addSublayer(&shape);
            self.shapes.push(shape);
        }

        let mut shapes = self.shapes.iter();
        let mut present = Vec::new();
        for (idx, item) in items.iter().enumerate() {
            // Layers of one parent are stacked by this, whatever order they were added in.
            let z = (idx + 1) as f64;
            match item {
                OverviewItem::Shape(outline) => {
                    let Some(shape) = shapes.next() else {
                        continue;
                    };
                    let outline = Outline {
                        rect: Rect::new(
                            outline.rect.x - display.frame.x,
                            outline.rect.y - display.frame.y,
                            outline.rect.w,
                            outline.rect.h,
                        ),
                        ..*outline
                    };
                    shape.setFrame(bounds);
                    shape.setContentsScale(display.scale);
                    shape.setPath(Some(&path(&outline)));
                    shape.setFillColor(Some(&color(&outline)));
                    shape.setZPosition(z);
                }
                OverviewItem::Window(window) => {
                    let card = self.cards.entry(window.id).or_insert_with(|| {
                        let card = Card::new(style, icon(window.id).as_deref(), display.scale);
                        self.root.addSublayer(&card.layer);
                        card
                    });
                    card.update(local(window.rect), window);
                    card.layer.setZPosition(z);
                    present.push(window.id);
                }
            }
        }

        self.cards.retain(|id, card| {
            let keep = present.contains(id);
            if !keep {
                card.layer.removeFromSuperlayer();
            }
            keep
        });
    }
}

fn cg_color(color: [f32; 4]) -> CFRetained<CGColor> {
    let [r, g, b, a] = color;
    CGColor::new_srgb(r.into(), g.into(), b.into(), a.into())
}

/// The overview window of one display.
struct Surface {
    display: DisplayId,
    /// The display's frame the window was last given.
    frame: Rect,
    window: Retained<NSPanel>,
    scene: Scene,
}

impl Surface {
    fn new(mtm: MainThreadMarker, display: &DisplayInfo, primary_height: f64) -> Self {
        let rect = appkit_rect(display.frame, primary_height);
        // A panel that does not activate its application: clicking the overview must not
        // take the menu bar away from the application the user was in.
        let style = NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel;
        // SAFETY: the arguments are a valid rectangle and valid constants. The panel is told
        // below not to release itself when closed, so the reference held here stays the
        // owning one.
        let window = unsafe {
            let window = NSPanel::initWithContentRect_styleMask_backing_defer(
                NSPanel::alloc(mtm),
                rect,
                style,
                NSBackingStoreType::Buffered,
                false,
            );
            window.setReleasedWhenClosed(false);
            window
        };
        // The backdrop may be given an opacity in the config.
        window.setOpaque(false);
        window.setBackgroundColor(Some(&NSColor::clearColor()));
        window.setHasShadow(false);
        // Panels go away when their application is not the active one, which this one
        // never is.
        window.setHidesOnDeactivate(false);
        window.setBecomesKeyOnlyIfNeeded(true);
        window.setLevel(LEVEL);
        // As for the window that shows outlines: hidden by Mission Control, never switched to.
        window.setCollectionBehavior(
            NSWindowCollectionBehavior::Transient | NSWindowCollectionBehavior::IgnoresCycle,
        );

        let bounds = CGRect::new(CGPoint::new(0., 0.), rect.size);
        let scene = Scene::new();
        scene.root.setFrame(bounds);
        let view = OverviewView::new(mtm, bounds);
        // Giving the view its layer before asking it to use one makes the layer ours to
        // arrange, instead of AppKit's.
        view.setLayer(Some(&scene.root));
        view.setWantsLayer(true);
        window.setContentView(Some(&view));

        Self {
            display: display.id,
            frame: display.frame,
            window,
            scene,
        }
    }
}

/// The overview windows, one per display. They exist from the first time the overview is
/// shown, and are on screen only while it is.
pub struct OverviewPanels {
    mtm: MainThreadMarker,
    surfaces: Vec<Surface>,
    shown: bool,
    /// Icons by process, kept while the overview is showing.
    icons: HashMap<i32, Option<Retained<NSImage>>>,
}

impl OverviewPanels {
    /// Prepares to show the overview. No window is made until then.
    ///
    /// # Panics
    ///
    /// Panics if called from a thread other than the main thread.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let mtm = MainThreadMarker::new().expect("the overview is shown on the main thread");
        Self {
            mtm,
            surfaces: Vec::new(),
            shown: false,
            icons: HashMap::new(),
        }
    }

    /// Shows exactly `overview` on `displays`, or nothing. `pid_of` says which process a
    /// window belongs to.
    pub fn set(
        &mut self,
        displays: &[DisplayInfo],
        overview: Option<&Overview>,
        pid_of: &dyn Fn(WindowId) -> Option<i32>,
    ) {
        // The windows of a display that is gone go with it.
        self.surfaces.retain(|surface| {
            let keep = displays.iter().any(|d| d.id == surface.display);
            if !keep {
                surface.window.close();
            }
            keep
        });

        let Some(overview) = overview else {
            if self.shown {
                self.shown = false;
                self.icons.clear();
                CATransaction::begin();
                CATransaction::setDisableActions(true);
                for surface in &mut self.surfaces {
                    surface.window.orderOut(None);
                    // The cards are made again next time, with the colors and icons of then.
                    for (_, card) in surface.scene.cards.drain() {
                        card.layer.removeFromSuperlayer();
                    }
                }
                CATransaction::commit();
                CATransaction::flush();
            }
            return;
        };

        // AppKit measures from the bottom of the primary display.
        let Some(primary_height) = displays.first().map(|d| d.frame.h) else {
            return;
        };
        PRIMARY_HEIGHT.set(primary_height);
        let style = Style::current(self.mtm);

        CATransaction::begin();
        // The layout animates the overview frame by frame; nothing eases on its own.
        CATransaction::setDisableActions(true);

        for display in displays {
            let idx = self
                .surfaces
                .iter()
                .position(|surface| surface.display == display.id);
            let surface = match idx {
                Some(idx) => &mut self.surfaces[idx],
                None => {
                    self.surfaces
                        .push(Surface::new(self.mtm, display, primary_height));
                    self.surfaces.last_mut().unwrap()
                }
            };
            if surface.frame != display.frame {
                surface.frame = display.frame;
                let rect = appkit_rect(display.frame, primary_height);
                surface.window.setFrame_display(rect, false);
            }

            let part = overview.displays.iter().find(|d| d.display == display.id);
            let icons = &mut self.icons;
            surface
                .scene
                .update(display, overview.backdrop, part, &style, &mut |id| {
                    let pid = pid_of(id)?;
                    icons.entry(pid).or_insert_with(|| icon_of(pid)).clone()
                });

            if !self.shown {
                surface.window.orderFrontRegardless();
            }
        }
        self.shown = true;

        CATransaction::commit();
        // Do not wait for the event loop to come round: frames of an animation are drawn
        // from inside one pass of it.
        CATransaction::flush();
    }
}

impl Drop for OverviewPanels {
    fn drop(&mut self) {
        for surface in &self.surfaces {
            surface.window.close();
        }
    }
}

fn icon_of(pid: i32) -> Option<Retained<NSImage>> {
    NSRunningApplication::runningApplicationWithProcessIdentifier(pid)?.icon()
}

/// Draws one display's part of `overview` into an image, without putting anything on screen.
/// `pid_of` says which process a window belongs to, for its icon.
///
/// Returns the image's width and height in pixels and its pixels, top row first, four bytes
/// each: red, green, blue and alpha, with the colors multiplied by the alpha. For checking
/// what the overview would show.
///
/// # Panics
///
/// Panics if called from a thread other than the main thread.
pub fn render_overview(
    overview: &Overview,
    display: &DisplayInfo,
    pid_of: &dyn Fn(WindowId) -> Option<i32>,
) -> (usize, usize, Vec<u8>) {
    let mtm = MainThreadMarker::new().expect("the overview is drawn on the main thread");
    let style = Style::current(mtm);
    let mut scene = Scene::new();
    let part = overview.displays.iter().find(|d| d.display == display.id);
    scene.update(display, overview.backdrop, part, &style, &mut |id| {
        icon_of(pid_of(id)?)
    });
    scene.root.layoutIfNeeded();

    let scale = display.scale;
    let width = (display.frame.w * scale).round().max(1.) as usize;
    let height = (display.frame.h * scale).round().max(1.) as usize;
    let mut pixels = vec![0u8; width * height * 4];

    let space = CGColorSpace::new_device_rgb().expect("the device RGB color space exists");
    // SAFETY: the buffer is `height` rows of `width * 4` bytes, as described to the context,
    // and outlives it: the context is dropped before `pixels` is returned.
    let context: CFRetained<CGContext> = unsafe {
        CGBitmapContextCreate(
            pixels.as_mut_ptr().cast::<c_void>(),
            width,
            height,
            8,
            width * 4,
            Some(&space),
            CGImageAlphaInfo::PremultipliedLast.0,
        )
    }
    .expect("a plain RGBA bitmap context can be created");
    CGContext::scale_ctm(Some(&context), scale, scale);
    scene.root.renderInContext(&context);
    drop(context);

    (width, height, pixels)
}
