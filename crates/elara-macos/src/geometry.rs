//! Plain geometry in points, y growing downwards.

/// A position.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// A width and height.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Size {
    pub w: f64,
    pub h: f64,
}

/// A rectangle given by its top-left corner and size.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub const fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }

    pub fn origin(&self) -> Point {
        Point {
            x: self.x,
            y: self.y,
        }
    }

    pub fn size(&self) -> Size {
        Size {
            w: self.w,
            h: self.h,
        }
    }

    /// Right edge.
    pub fn max_x(&self) -> f64 {
        self.x + self.w
    }

    /// Bottom edge.
    pub fn max_y(&self) -> f64 {
        self.y + self.h
    }

    /// Whether `point` lies inside the rectangle. The right and bottom edges are outside.
    pub fn contains(&self, point: Point) -> bool {
        point.x >= self.x && point.x < self.max_x() && point.y >= self.y && point.y < self.max_y()
    }

    /// Whether the two rectangles share any area.
    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.max_x()
            && other.x < self.max_x()
            && self.y < other.max_y()
            && other.y < self.max_y()
    }

    /// Converts a rectangle from AppKit's screen space to the space this crate uses.
    ///
    /// AppKit puts the origin at the bottom-left corner of the primary display with y growing
    /// upwards, and gives rectangles by their bottom-left corner. `primary_height` is the height
    /// of the primary display.
    pub fn from_bottom_left(x: f64, y: f64, w: f64, h: f64, primary_height: f64) -> Self {
        Self {
            x,
            y: primary_height - (y + h),
            w,
            h,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contains_excludes_far_edges() {
        let r = Rect::new(10., 20., 100., 50.);
        assert!(r.contains(Point { x: 10., y: 20. }));
        assert!(r.contains(Point { x: 109.9, y: 69.9 }));
        assert!(!r.contains(Point { x: 110., y: 20. }));
        assert!(!r.contains(Point { x: 10., y: 70. }));
        assert!(!r.contains(Point { x: 9.9, y: 20. }));
    }

    #[test]
    fn intersects_needs_shared_area() {
        let r = Rect::new(0., 0., 100., 100.);
        assert!(r.intersects(&Rect::new(50., 50., 100., 100.)));
        assert!(r.intersects(&Rect::new(-10., -10., 20., 20.)));
        // Touching edges share no area.
        assert!(!r.intersects(&Rect::new(100., 0., 50., 50.)));
        assert!(!r.intersects(&Rect::new(0., 100., 50., 50.)));
        assert!(!r.intersects(&Rect::new(200., 200., 10., 10.)));
    }

    #[test]
    fn appkit_rects_flip_around_the_primary_display() {
        // The primary display itself.
        assert_eq!(
            Rect::from_bottom_left(0., 0., 1800., 1169., 1169.),
            Rect::new(0., 0., 1800., 1169.)
        );
        // Its visible frame with a 39 point menu bar and no Dock.
        assert_eq!(
            Rect::from_bottom_left(0., 0., 1800., 1130., 1169.),
            Rect::new(0., 39., 1800., 1130.)
        );
        // A display arranged above the primary one has positive AppKit y and negative y here.
        assert_eq!(
            Rect::from_bottom_left(0., 1169., 1920., 1080., 1169.),
            Rect::new(0., -1080., 1920., 1080.)
        );
        // A display arranged below it.
        assert_eq!(
            Rect::from_bottom_left(-200., -1080., 1920., 1080., 1169.),
            Rect::new(-200., 1169., 1920., 1080.)
        );
    }
}
