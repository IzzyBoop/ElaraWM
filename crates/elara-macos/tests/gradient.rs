//! Checks what a gradient looks like once drawn. Nothing is put on screen.
#![cfg(target_os = "macos")]

use elara_macos::{rasterize, ColorSpace, Gradient, HueInterpolation, Outline, Rect, Size};

const RED: [f32; 4] = [1., 0., 0., 1.];
const BLUE: [f32; 4] = [0., 0., 1., 1.];

/// A ring 10 wide around a 200 by 100 picture, red to blue along `angle`.
fn ring(angle: f64, area: Rect) -> Outline {
    Outline {
        rect: Rect::new(0., 0., 200., 100.),
        line_width: 10.,
        corner_radius: [0.; 4],
        color: RED,
        gradient: Some(Gradient {
            to: BLUE,
            area,
            angle: angle.to_radians(),
            space: ColorSpace::Srgb,
            hue: HueInterpolation::Shorter,
        }),
    }
}

/// Draws `outline` at one pixel per point and returns a way to read the pixel at a point.
fn drawn(outline: Outline) -> impl Fn(usize, usize) -> [u8; 4] {
    let (width, _height, pixels) = rasterize(&[outline], Size { w: 200., h: 100. }, 1.);
    move |x, y| {
        let at = (y * width + x) * 4;
        [pixels[at], pixels[at + 1], pixels[at + 2], pixels[at + 3]]
    }
}

fn close(pixel: [u8; 4], expected: [u8; 4]) -> bool {
    pixel.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 12)
}

#[test]
fn a_gradient_to_the_right_goes_from_the_first_color_on_the_left_to_the_last() {
    let pixel = drawn(ring(0., Rect::new(0., 0., 200., 100.)));
    assert!(close(pixel(1, 50), [255, 0, 0, 255]), "{:?}", pixel(1, 50));
    assert!(
        close(pixel(198, 50), [0, 0, 255, 255]),
        "{:?}",
        pixel(198, 50)
    );
    // Halfway along the top edge it is halfway between.
    assert!(
        close(pixel(100, 5), [128, 0, 128, 255]),
        "{:?}",
        pixel(100, 5)
    );
    // And only the ring is drawn.
    assert_eq!(pixel(100, 50), [0, 0, 0, 0]);
}

#[test]
fn a_gradient_downwards_has_the_first_color_at_the_top() {
    let pixel = drawn(ring(90., Rect::new(0., 0., 200., 100.)));
    assert!(
        close(pixel(100, 1), [255, 0, 0, 255]),
        "{:?}",
        pixel(100, 1)
    );
    assert!(
        close(pixel(100, 98), [0, 0, 255, 255]),
        "{:?}",
        pixel(100, 98)
    );
    assert!(
        close(pixel(5, 50), [128, 0, 128, 255]),
        "{:?}",
        pixel(5, 50)
    );
}

#[test]
fn a_shape_shows_its_part_of_a_larger_gradient() {
    // The ring is the left half of the area the gradient spans, so it ends halfway.
    let pixel = drawn(ring(0., Rect::new(0., 0., 400., 100.)));
    assert!(close(pixel(1, 50), [255, 0, 0, 255]), "{:?}", pixel(1, 50));
    assert!(
        close(pixel(198, 50), [128, 0, 128, 255]),
        "{:?}",
        pixel(198, 50)
    );
}

#[test]
fn past_the_ends_the_color_stays() {
    // The gradient spans only the middle of the ring.
    let pixel = drawn(ring(0., Rect::new(80., 0., 40., 100.)));
    assert!(close(pixel(1, 50), [255, 0, 0, 255]), "{:?}", pixel(1, 50));
    assert!(
        close(pixel(198, 50), [0, 0, 255, 255]),
        "{:?}",
        pixel(198, 50)
    );
}
