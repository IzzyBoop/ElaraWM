//! The image behind the workspaces of the overview.
//!
//! The image is read once, drawn at the size of the display it is for so that it fills it,
//! blurred if asked, and kept. Showing it is then as cheap as showing any picture.

use std::collections::HashMap;
use std::ffi::c_void;
use std::path::Path;
use std::time::SystemTime;

use objc2::AnyThread;
use objc2_app_kit::NSImage;
use objc2_core_foundation::{CFRetained, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGBitmapContextCreateImage, CGBitmapContextGetData, CGColorSpace,
    CGContext, CGImage, CGImageAlphaInfo, CGInterpolationQuality,
};
use objc2_foundation::NSString;

use crate::{box_blur, BackdropImage, DisplayId, DisplayInfo};

/// How large, in pixels, the blur is made to be in the copy of the image that is blurred.
/// A blur hides detail, so a wide one can be done on a small copy and shown enlarged, which
/// takes a fraction of the time and looks the same.
const BLUR_RADIUS: f64 = 6.;

/// One display's backdrop, and what it was made from.
struct Loaded {
    wanted: BackdropImage,
    /// The size of the display in pixels.
    size: (usize, usize),
    /// When the file was last changed, as of when it was read.
    modified: Option<SystemTime>,
    /// `None` if the file could not be read.
    image: Option<CFRetained<CGImage>>,
}

/// The backdrops in use, one per display.
pub(super) struct Backdrops {
    loaded: HashMap<DisplayId, Loaded>,
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

impl Backdrops {
    pub(super) fn new() -> Self {
        Self {
            loaded: HashMap::new(),
        }
    }

    /// Forgets the backdrops whose files have changed since they were read. For calling
    /// when the overview opens, so that a new picture under the old name is picked up.
    pub(super) fn forget_changed(&mut self) {
        self.loaded
            .retain(|_, loaded| modified(&loaded.wanted.path) == loaded.modified);
    }

    /// Forgets the backdrops of displays that are not among `displays`.
    pub(super) fn retain(&mut self, displays: &[DisplayInfo]) {
        self.loaded
            .retain(|id, _| displays.iter().any(|display| display.id == *id));
    }

    /// Returns `wanted` as it shows on `display`, reading the file if that has not been
    /// done for this display, image and blur.
    pub(super) fn get(
        &mut self,
        display: &DisplayInfo,
        wanted: &BackdropImage,
    ) -> Option<CFRetained<CGImage>> {
        let size = pixel_size(display);
        if let Some(loaded) = self.loaded.get(&display.id) {
            if loaded.wanted == *wanted && loaded.size == size {
                return loaded.image.clone();
            }
        }

        let modified = modified(&wanted.path);
        let image = render(&wanted.path, size, wanted.blur * display.scale);
        if image.is_none() {
            warn!(path = ?wanted.path, "could not read the overview's backdrop image");
        }
        self.loaded.insert(
            display.id,
            Loaded {
                wanted: wanted.clone(),
                size,
                modified,
                image: image.clone(),
            },
        );
        image
    }
}

fn pixel_size(display: &DisplayInfo) -> (usize, usize) {
    let pixels = |points: f64| (points * display.scale).round().max(1.) as usize;
    (pixels(display.frame.w), pixels(display.frame.h))
}

/// Reads the image at `path` and returns it as it fills a display of `size` pixels,
/// blurred by `blur` pixels. The result may be smaller than the display; it has its shape
/// and is meant to be stretched over it.
pub(super) fn render(path: &Path, size: (usize, usize), blur: f64) -> Option<CFRetained<CGImage>> {
    let file = NSString::from_str(path.to_str()?);
    let image = NSImage::initWithContentsOfFile(NSImage::alloc(), &file)?;
    // SAFETY: a null rectangle asks for the image as it is, and there are no hints.
    let image =
        unsafe { image.CGImageForProposedRect_context_hints(std::ptr::null_mut(), None, None) }?;
    let (image_w, image_h) = (
        CGImage::width(Some(&image)) as f64,
        CGImage::height(Some(&image)) as f64,
    );
    if image_w < 1. || image_h < 1. {
        return None;
    }

    // A sharp image is drawn at the size of the display. A blurred one is drawn smaller,
    // by as much as brings the blur down to a few pixels, and never at more than half.
    let shrink = if blur > 0. {
        (BLUR_RADIUS / blur).clamp(1. / 32., 0.5)
    } else {
        1.
    };
    let width = ((size.0 as f64 * shrink).round() as usize).max(1);
    let height = ((size.1 as f64 * shrink).round() as usize).max(1);

    let space = CGColorSpace::new_device_rgb()?;
    // SAFETY: with no buffer given, the context allocates one of the size described.
    let context: CFRetained<CGContext> = unsafe {
        CGBitmapContextCreate(
            std::ptr::null_mut::<c_void>(),
            width,
            height,
            8,
            width * 4,
            Some(&space),
            CGImageAlphaInfo::PremultipliedLast.0,
        )
    }?;
    CGContext::set_interpolation_quality(Some(&context), CGInterpolationQuality::High);

    // Filling the whole of it, centered, with what does not fit cut off.
    let scale = (width as f64 / image_w).max(height as f64 / image_h);
    let (w, h) = (image_w * scale, image_h * scale);
    let rect = CGRect::new(
        CGPoint::new((width as f64 - w) / 2., (height as f64 - h) / 2.),
        CGSize::new(w, h),
    );
    CGContext::draw_image(Some(&context), rect, Some(&image));

    let radius = (blur * shrink).round() as usize;
    if radius > 0 {
        let data = CGBitmapContextGetData(Some(&context)).cast::<u8>();
        if data.is_null() {
            return None;
        }
        // SAFETY: the context was made with `height` rows of `width * 4` bytes, it is
        // alive, and nothing else looks at its pixels while they are changed here.
        let pixels = unsafe { std::slice::from_raw_parts_mut(data, width * height * 4) };
        box_blur(pixels, width, height, radius);
    }

    CGBitmapContextCreateImage(Some(&context))
}

/// Returns `wanted` as it shows on `display`, without keeping it. For drawing once.
pub(super) fn render_for(
    display: &DisplayInfo,
    wanted: &BackdropImage,
) -> Option<CFRetained<CGImage>> {
    render(
        &wanted.path,
        pixel_size(display),
        wanted.blur * display.scale,
    )
}
