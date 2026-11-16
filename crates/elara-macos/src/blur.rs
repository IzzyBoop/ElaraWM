//! Blurring a picture, for the backdrop of the overview.
//!
//! The system can blur, but through Core Image, which is a framework this crate does not
//! otherwise need. A backdrop is blurred once and kept, so doing it here costs nothing
//! that matters: three passes of a box blur, which together come close to a Gaussian one.

/// Blurs `pixels`, an image of `width` by `height` with four bytes per pixel, in place.
/// Every pixel becomes the average of those up to `radius` away, three times over. Past
/// the edges the image is taken to go on with the color of the edge.
pub fn box_blur(pixels: &mut [u8], width: usize, height: usize, radius: usize) {
    assert_eq!(pixels.len(), width * height * 4, "four bytes per pixel");
    if radius == 0 || width == 0 || height == 0 {
        return;
    }

    let mut line = vec![0u8; width.max(height) * 4];
    for _ in 0..3 {
        // Along the rows, then along the columns.
        for y in 0..height {
            blur_line(pixels, y * width * 4, 4, width, radius, &mut line);
        }
        for x in 0..width {
            blur_line(pixels, x * 4, width * 4, height, radius, &mut line);
        }
    }
}

/// Blurs one row or column: `count` pixels, the first at `start` and each `step` bytes
/// after the one before. `line` is scratch space.
fn blur_line(
    pixels: &mut [u8],
    start: usize,
    step: usize,
    count: usize,
    radius: usize,
    line: &mut [u8],
) {
    let at = |idx: isize| start + (idx.clamp(0, count as isize - 1) as usize) * step;
    let window = (2 * radius + 1) as u32;
    let radius = radius as isize;

    // The sum of the pixels around the first one, then slid along one pixel at a time.
    let mut sum = [0u32; 4];
    for idx in -radius..=radius {
        let from = at(idx);
        for channel in 0..4 {
            sum[channel] += u32::from(pixels[from + channel]);
        }
    }
    for idx in 0..count as isize {
        for channel in 0..4 {
            line[idx as usize * 4 + channel] = ((sum[channel] + window / 2) / window) as u8;
        }
        let (leaving, entering) = (at(idx - radius), at(idx + radius + 1));
        for channel in 0..4 {
            sum[channel] += u32::from(pixels[entering + channel]);
            sum[channel] -= u32::from(pixels[leaving + channel]);
        }
    }
    for idx in 0..count {
        let to = start + idx * step;
        pixels[to..to + 4].copy_from_slice(&line[idx * 4..idx * 4 + 4]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(width: usize, height: usize, color: impl Fn(usize, usize) -> [u8; 4]) -> Vec<u8> {
        (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .flat_map(|(x, y)| color(x, y))
            .collect()
    }

    #[test]
    fn one_color_stays_as_it_is() {
        let mut pixels = image(9, 7, |_, _| [10, 200, 30, 255]);
        let before = pixels.clone();
        box_blur(&mut pixels, 9, 7, 3);
        assert_eq!(pixels, before);
    }

    #[test]
    fn no_radius_changes_nothing() {
        let mut pixels = image(5, 5, |x, y| [(x * 50) as u8, (y * 50) as u8, 0, 255]);
        let before = pixels.clone();
        box_blur(&mut pixels, 5, 5, 0);
        assert_eq!(pixels, before);
    }

    #[test]
    fn a_hard_edge_becomes_a_slope() {
        // Black on the left, white on the right.
        let (width, height) = (40, 4);
        let mut pixels = image(width, height, |x, _| {
            let v = if x < 20 { 0 } else { 255 };
            [v, v, v, 255]
        });
        box_blur(&mut pixels, width, height, 4);

        let red = |x: usize| pixels[(width + x) * 4];
        // Far from the edge nothing changed.
        assert_eq!(red(2), 0);
        assert_eq!(red(37), 255);
        // Across it the values climb without a jump, through the middle.
        let slope: Vec<u8> = (8..32).map(red).collect();
        assert!(slope.windows(2).all(|pair| pair[0] <= pair[1]), "{slope:?}");
        assert!(
            slope.windows(2).all(|pair| pair[1] - pair[0] < 40),
            "{slope:?}"
        );
        assert!((100..156).contains(&red(20)), "{}", red(20));
        // Opaque stays opaque.
        assert!(pixels.chunks(4).all(|pixel| pixel[3] == 255));
    }

    #[test]
    fn a_dot_spreads_the_same_way_in_every_direction() {
        let size = 21;
        let mut pixels = image(size, size, |x, y| {
            if (x, y) == (10, 10) {
                [255, 255, 255, 255]
            } else {
                [0, 0, 0, 255]
            }
        });
        box_blur(&mut pixels, size, size, 2);
        let red = |x: usize, y: usize| pixels[(y * size + x) * 4];
        assert!(red(10, 10) > 0);
        assert_eq!(red(10, 7), red(10, 13));
        assert_eq!(red(7, 10), red(13, 10));
        assert_eq!(red(7, 10), red(10, 7));
        // It does not reach the corners.
        assert_eq!(red(0, 0), 0);
    }
}
