//! Gradients: where one starts and ends, and the colors along the way.
//!
//! This is the arithmetic of niri's border shader, which follows CSS. The system's own
//! gradients only blend sRGB values in a straight line, so for the other color spaces the
//! colors along the way are worked out here and handed over as a row of stops.

use crate::{Point, Rect};

/// The color space the colors of a gradient are blended in, as in CSS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorSpace {
    #[default]
    Srgb,
    SrgbLinear,
    Oklab,
    /// Oklab by lightness, chroma and hue; the hue goes around the color wheel.
    Oklch,
}

/// Which way around the color wheel the hue goes, in a color space that has one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HueInterpolation {
    #[default]
    Shorter,
    Longer,
    Increasing,
    Decreasing,
}

/// A gradient that a shape is filled with in place of a single color.
///
/// The shape's own color is where the gradient starts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gradient {
    /// The color at the end: red, green, blue and opacity, not multiplied by the opacity.
    pub to: [f32; 4],
    /// The rectangle the gradient spans, in global coordinates. The shape may be only a part
    /// of it, and then shows only that part of the gradient.
    pub area: Rect,
    /// The direction the colors run in, in radians: 0 is to the right and a quarter turn
    /// is downwards.
    pub angle: f64,
    pub space: ColorSpace,
    pub hue: HueInterpolation,
}

/// How many pieces a gradient is cut into where the system cannot blend it itself.
const PIECES: usize = 24;

impl Gradient {
    /// Returns the points where the gradient has its first and its last color. Before the
    /// first and past the last the color stays the same.
    ///
    /// As in CSS, the line between them goes through the middle of the area, and is as long
    /// as it takes for the corners of the area to get exactly the first and last color.
    pub fn line(&self) -> (Point, Point) {
        let (dx, dy) = (self.angle.cos(), self.angle.sin());
        let length = (self.area.w * dx).abs() + (self.area.h * dy).abs();
        let center = (
            self.area.x + self.area.w / 2.,
            self.area.y + self.area.h / 2.,
        );
        let half = (dx * length / 2., dy * length / 2.);
        (
            Point {
                x: center.0 - half.0,
                y: center.1 - half.1,
            },
            Point {
                x: center.0 + half.0,
                y: center.1 + half.1,
            },
        )
    }

    /// Returns the colors along the gradient that starts with `from`, evenly spaced, first
    /// to last. Blending neighbors in a straight line gives the gradient.
    pub fn stops(&self, from: [f32; 4]) -> Vec<[f32; 4]> {
        // Colors of one opacity, blended in sRGB, are what the system does by itself.
        let pieces = if self.space == ColorSpace::Srgb && from[3] == self.to[3] {
            1
        } else {
            PIECES
        };
        (0..=pieces)
            .map(|idx| self.color_at(from, idx as f32 / pieces as f32))
            .collect()
    }

    /// Returns the color `t` of the way from `from` to the end, `t` going from 0 to 1.
    pub fn color_at(&self, from: [f32; 4], t: f32) -> [f32; 4] {
        let t = t.clamp(0., 1.);
        let to = self.to;
        let mix = |a: f32, b: f32| a + (b - a) * t;
        let alpha = mix(from[3], to[3]);

        if self.space == ColorSpace::Srgb {
            // Blended with the colors multiplied by their opacity, as niri's shader has them.
            let premul = |c: [f32; 4]| [c[0] * c[3], c[1] * c[3], c[2] * c[3]];
            let (a, b) = (premul(from), premul(to));
            let rgb = [0, 1, 2].map(|i| {
                let value = mix(a[i], b[i]);
                if alpha > 0. {
                    value / alpha
                } else {
                    0.
                }
            });
            return [rgb[0], rgb[1], rgb[2], alpha];
        }

        let a = [from[0], from[1], from[2]].map(srgb_to_linear);
        let b = [to[0], to[1], to[2]].map(srgb_to_linear);
        let linear = match self.space {
            ColorSpace::Srgb | ColorSpace::SrgbLinear => [0, 1, 2].map(|i| mix(a[i], b[i])),
            ColorSpace::Oklab => {
                let (a, b) = (linear_to_oklab(a), linear_to_oklab(b));
                oklab_to_linear([0, 1, 2].map(|i| mix(a[i], b[i])))
            }
            ColorSpace::Oklch => {
                let (a, b) = (
                    oklab_to_oklch(linear_to_oklab(a)),
                    oklab_to_oklch(linear_to_oklab(b)),
                );
                let (h1, h2) = hues(a[2], b[2], self.hue);
                let lch = [mix(a[0], b[0]), mix(a[1], b[1]), mix(h1, h2)];
                oklab_to_linear(oklch_to_oklab(lch))
            }
        };
        let [r, g, b] = linear.map(|value| linear_to_srgb(value).clamp(0., 1.));
        [r, g, b, alpha]
    }
}

/// Adjusts two hues, in degrees, so that going from the first to the second in a straight
/// line goes around the color wheel the way `how` says. The rules are those of CSS.
fn hues(mut h1: f32, mut h2: f32, how: HueInterpolation) -> (f32, f32) {
    let diff = h2 - h1;
    match how {
        HueInterpolation::Shorter => {
            if diff > 180. {
                h1 += 360.;
            } else if diff < -180. {
                h2 += 360.;
            }
        }
        HueInterpolation::Longer => {
            if 0. < diff && diff < 180. {
                h1 += 360.;
            } else if -180. < diff && diff <= 0. {
                h2 += 360.;
            }
        }
        HueInterpolation::Increasing => {
            if diff < 0. {
                h2 += 360.;
            }
        }
        HueInterpolation::Decreasing => {
            if diff > 0. {
                h1 += 360.;
            }
        }
    }
    (h1, h2)
}

fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(value: f32) -> f32 {
    if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1. / 2.4) - 0.055
    }
}

// The matrices are Björn Ottosson's, who defined Oklab.
fn linear_to_oklab([r, g, b]: [f32; 3]) -> [f32; 3] {
    let l = (0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

fn oklab_to_linear([l, a, b]: [f32; 3]) -> [f32; 3] {
    let l_ = (l + 0.396_337_78 * a + 0.215_803_76 * b).powi(3);
    let m_ = (l - 0.105_561_346 * a - 0.063_854_17 * b).powi(3);
    let s_ = (l - 0.089_484_18 * a - 1.291_485_5 * b).powi(3);
    [
        4.076_741_7 * l_ - 3.307_711_6 * m_ + 0.230_969_93 * s_,
        -1.268_438 * l_ + 2.609_757_4 * m_ - 0.341_319_38 * s_,
        -0.004_196_086_3 * l_ - 0.703_418_6 * m_ + 1.707_614_7 * s_,
    ]
}

/// Lightness, chroma, and hue in degrees from 0 to 360.
fn oklab_to_oklch([l, a, b]: [f32; 3]) -> [f32; 3] {
    let hue = b.atan2(a).to_degrees().rem_euclid(360.);
    [l, a.hypot(b), hue]
}

fn oklch_to_oklab([l, c, h]: [f32; 3]) -> [f32; 3] {
    let h = h.to_radians();
    [l, c * h.cos(), c * h.sin()]
}

#[cfg(test)]
mod tests {
    use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI};

    use super::*;

    const RED: [f32; 4] = [1., 0., 0., 1.];
    const BLUE: [f32; 4] = [0., 0., 1., 1.];

    fn gradient(angle: f64, space: ColorSpace) -> Gradient {
        Gradient {
            to: BLUE,
            area: Rect::new(100., 50., 400., 200.),
            angle,
            space,
            hue: HueInterpolation::Shorter,
        }
    }

    fn close(a: [f32; 4], b: [f32; 4]) -> bool {
        a.iter().zip(b).all(|(a, b)| (a - b).abs() < 0.01)
    }

    fn near(a: Point, b: (f64, f64)) -> bool {
        (a.x - b.0).abs() < 1e-6 && (a.y - b.1).abs() < 1e-6
    }

    #[test]
    fn a_gradient_pointing_down_runs_from_the_top_edge_to_the_bottom_edge() {
        let (start, end) = gradient(FRAC_PI_2, ColorSpace::Srgb).line();
        assert!(near(start, (300., 50.)), "{start:?}");
        assert!(near(end, (300., 250.)), "{end:?}");
    }

    #[test]
    fn a_gradient_pointing_left_runs_from_the_right_edge_to_the_left_edge() {
        let (start, end) = gradient(PI, ColorSpace::Srgb).line();
        assert!(near(start, (500., 150.)), "{start:?}");
        assert!(near(end, (100., 150.)), "{end:?}");
    }

    #[test]
    fn a_slanted_gradient_gives_the_corners_its_first_and_last_color() {
        let gradient = gradient(FRAC_PI_4, ColorSpace::Srgb);
        let (start, end) = gradient.line();
        let along = (end.x - start.x, end.y - start.y);
        let length_sq = along.0 * along.0 + along.1 * along.1;
        // How far along the line a point is, from 0 at the start to 1 at the end.
        let fraction =
            |x: f64, y: f64| ((x - start.x) * along.0 + (y - start.y) * along.1) / length_sq;
        assert!(fraction(100., 50.).abs() < 1e-9);
        assert!((fraction(500., 250.) - 1.).abs() < 1e-9);
        // The other two corners are in between.
        assert!((0.1..0.9).contains(&fraction(500., 50.)));
        assert!((0.1..0.9).contains(&fraction(100., 250.)));
    }

    #[test]
    fn every_color_space_starts_and_ends_on_the_given_colors() {
        for space in [
            ColorSpace::Srgb,
            ColorSpace::SrgbLinear,
            ColorSpace::Oklab,
            ColorSpace::Oklch,
        ] {
            let stops = gradient(0., space).stops(RED);
            assert!(close(stops[0], RED), "{space:?}: {:?}", stops[0]);
            let last = *stops.last().unwrap();
            assert!(close(last, BLUE), "{space:?}: {last:?}");
        }
    }

    #[test]
    fn srgb_needs_no_stops_in_between() {
        assert_eq!(gradient(0., ColorSpace::Srgb).stops(RED), [RED, BLUE]);
        assert!(gradient(0., ColorSpace::Oklab).stops(RED).len() > 2);
    }

    #[test]
    fn the_middle_differs_by_color_space() {
        let middle = |space| gradient(0., space).color_at(RED, 0.5);
        assert!(close(middle(ColorSpace::Srgb), [0.5, 0., 0.5, 1.]));
        // Half the light of each, which is brighter than half the sRGB value.
        assert!(close(
            middle(ColorSpace::SrgbLinear),
            [0.735, 0., 0.735, 1.]
        ));
        // What CSS gives for `color-mix(in oklab, red, blue)`: rgb(140, 83, 162).
        let oklab = middle(ColorSpace::Oklab);
        assert!(close(oklab, [0.549, 0.325, 0.635, 1.]), "{oklab:?}");
    }

    #[test]
    fn the_hue_goes_the_way_it_is_told() {
        let middle = |hue| {
            Gradient {
                hue,
                ..gradient(0., ColorSpace::Oklch)
            }
            .color_at(RED, 0.5)
        };
        // Red to blue the short way goes through purple, the long way through green.
        let short = middle(HueInterpolation::Shorter);
        assert!(
            short[1] < 0.2 && short[0] > 0.5 && short[2] > 0.5,
            "{short:?}"
        );
        let long = middle(HueInterpolation::Longer);
        assert!(long[1] > long[0] && long[1] > long[2], "{long:?}");
        // From red, at 29 degrees, up to blue, at 264, is the long way too.
        assert_eq!(middle(HueInterpolation::Increasing), long);
        assert_eq!(middle(HueInterpolation::Decreasing), short);
    }

    #[test]
    fn fading_to_nothing_keeps_the_color() {
        let gradient = Gradient {
            to: [0., 0., 1., 0.],
            ..gradient(0., ColorSpace::Srgb)
        };
        // Halfway to a transparent blue, red is still red, at half the opacity.
        assert!(close(gradient.color_at(RED, 0.5), [1., 0., 0., 0.5]));
        assert!(gradient.stops(RED).len() > 2);
    }
}
