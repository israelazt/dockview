//! The desktop as the screenshot tool sees it: monitors placed on one big
//! virtual screen, in physical pixels. A monitor to the left of or above the
//! primary one has negative coordinates, and each may have its own scale, so
//! every rectangle here is in virtual-screen pixels and says nothing about
//! scales.

/// A rectangle in virtual-screen pixels. `x` and `y` can be negative.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PxRect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl PxRect {
    pub const fn new(x: i32, y: i32, w: u32, h: u32) -> Self {
        Self { x, y, w, h }
    }

    /// The rectangle between two corners, in any order.
    pub fn from_corners(a: (i32, i32), b: (i32, i32)) -> Self {
        let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
        let (y0, y1) = (a.1.min(b.1), a.1.max(b.1));
        Self::new(x0, y0, (x1 - x0) as u32, (y1 - y0) as u32)
    }

    pub fn right(&self) -> i32 {
        self.x + self.w as i32
    }

    pub fn bottom(&self) -> i32 {
        self.y + self.h as i32
    }

    pub fn is_empty(&self) -> bool {
        self.w == 0 || self.h == 0
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }

    /// The part both rectangles cover, or `None` when they only touch or miss.
    pub fn intersect(&self, other: &PxRect) -> Option<PxRect> {
        let (x0, y0) = (self.x.max(other.x), self.y.max(other.y));
        let (x1, y1) = (
            self.right().min(other.right()),
            self.bottom().min(other.bottom()),
        );
        (x1 > x0 && y1 > y0).then(|| PxRect::new(x0, y0, (x1 - x0) as u32, (y1 - y0) as u32))
    }

    /// The smallest rectangle holding both.
    pub fn union(&self, other: &PxRect) -> PxRect {
        let (x0, y0) = (self.x.min(other.x), self.y.min(other.y));
        let (x1, y1) = (
            self.right().max(other.right()),
            self.bottom().max(other.bottom()),
        );
        PxRect::new(x0, y0, (x1 - x0) as u32, (y1 - y0) as u32)
    }
}

/// One monitor on the virtual screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopMonitor {
    pub bounds: PxRect,
    /// Windows' name for it, e.g. `\\.\DISPLAY1`.
    pub name: String,
    pub primary: bool,
    /// The display scale in percent (100, 125, 150...): the interface of the
    /// selection overlay is drawn at this scale on this monitor.
    pub scale_percent: u32,
}

/// All the monitors together.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Desktop {
    pub monitors: Vec<DesktopMonitor>,
}

impl Desktop {
    pub fn new(monitors: Vec<DesktopMonitor>) -> Self {
        Self { monitors }
    }

    /// The rectangle that holds every monitor (the "virtual screen"), or
    /// `None` without monitors. Gaps between monitors are inside it.
    pub fn bounds(&self) -> Option<PxRect> {
        self.monitors
            .iter()
            .map(|m| m.bounds)
            .reduce(|a, b| a.union(&b))
    }

    /// The monitor that holds the point, if any (the point can be in a gap).
    pub fn monitor_at(&self, x: i32, y: i32) -> Option<&DesktopMonitor> {
        self.monitors.iter().find(|m| m.bounds.contains(x, y))
    }

    /// The monitor that holds most of `rect`; the first on a tie. `None` when
    /// `rect` touches none.
    pub fn monitor_of(&self, rect: &PxRect) -> Option<&DesktopMonitor> {
        self.monitors
            .iter()
            .filter_map(|m| {
                m.bounds
                    .intersect(rect)
                    .map(|i| (m, u64::from(i.w) * u64::from(i.h)))
            })
            .fold(
                None,
                |best: Option<(&DesktopMonitor, u64)>, (m, area)| match best {
                    Some((_, a)) if a >= area => best,
                    _ => Some((m, area)),
                },
            )
            .map(|(m, _)| m)
    }

    /// `rect` cut to the virtual screen; `None` when nothing of it is on it.
    pub fn clamp(&self, rect: &PxRect) -> Option<PxRect> {
        self.bounds()?.intersect(rect)
    }

    /// Where `rect` is inside the image of the whole virtual screen (whose
    /// top-left corner is the top-left of [`Desktop::bounds`]): the rectangle
    /// moved so that the corner is the origin, as `(x, y, w, h)` in image pixels.
    pub fn in_virtual_image(&self, rect: &PxRect) -> Option<(u32, u32, u32, u32)> {
        let bounds = self.bounds()?;
        let r = bounds.intersect(rect)?;
        Some(((r.x - bounds.x) as u32, (r.y - bounds.y) as u32, r.w, r.h))
    }

    /// Whether the monitors are not all at the same scale (the overlay has to
    /// be drawn per monitor then).
    pub fn has_mixed_scales(&self) -> bool {
        self.monitors
            .windows(2)
            .any(|w| w[0].scale_percent != w[1].scale_percent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(x: i32, y: i32, w: u32, h: u32, scale: u32, primary: bool) -> DesktopMonitor {
        DesktopMonitor {
            bounds: PxRect::new(x, y, w, h),
            name: format!("M{x}"),
            primary,
            scale_percent: scale,
        }
    }

    /// A 1440p monitor at 150 % to the left of the primary 1080p one at 100 %,
    /// a little higher: the left one has negative x and y.
    fn two() -> Desktop {
        Desktop::new(vec![
            monitor(0, 0, 1920, 1080, 100, true),
            monitor(-2560, -200, 2560, 1440, 150, false),
        ])
    }

    #[test]
    fn rectangles_intersect_union_and_order_their_corners() {
        let a = PxRect::new(0, 0, 100, 100);
        let b = PxRect::new(50, 50, 100, 100);
        assert_eq!(a.intersect(&b), Some(PxRect::new(50, 50, 50, 50)));
        assert_eq!(a.union(&b), PxRect::new(0, 0, 150, 150));
        // Touching is not overlapping.
        assert_eq!(a.intersect(&PxRect::new(100, 0, 10, 10)), None);
        assert_eq!(
            PxRect::from_corners((30, 40), (10, 5)),
            PxRect::new(10, 5, 20, 35)
        );
        assert!(a.contains(0, 0) && !a.contains(100, 50));
    }

    #[test]
    fn the_virtual_screen_holds_every_monitor_including_negative_ones() {
        let d = two();
        assert_eq!(d.bounds(), Some(PxRect::new(-2560, -200, 4480, 1440)));
        assert_eq!(Desktop::default().bounds(), None);
    }

    #[test]
    fn a_point_belongs_to_one_monitor_or_to_a_gap() {
        let d = two();
        assert!(d.monitor_at(10, 10).unwrap().primary);
        assert_eq!(d.monitor_at(-1, 0).unwrap().scale_percent, 150);
        // Below the primary monitor, but inside the left one's rows: a gap on
        // the right side of the virtual screen.
        assert!(d.monitor_at(1000, 1200).is_none());
        assert!(d.monitor_at(-3000, 0).is_none());
    }

    #[test]
    fn a_rectangle_belongs_to_the_monitor_that_holds_most_of_it() {
        let d = two();
        // Mostly on the left monitor, a bit over the seam.
        let r = PxRect::new(-300, 100, 400, 200);
        assert_eq!(d.monitor_of(&r).unwrap().scale_percent, 150);
        let r = PxRect::new(-100, 100, 400, 200);
        assert_eq!(d.monitor_of(&r).unwrap().scale_percent, 100);
        assert!(d.monitor_of(&PxRect::new(5000, 0, 10, 10)).is_none());
    }

    #[test]
    fn a_selection_is_cut_to_the_virtual_screen() {
        let d = two();
        assert_eq!(
            d.clamp(&PxRect::new(1800, 900, 400, 400)),
            Some(PxRect::new(1800, 900, 120, 340))
        );
        assert_eq!(d.clamp(&PxRect::new(9000, 0, 10, 10)), None);
    }

    #[test]
    fn a_rectangle_is_found_in_the_image_of_the_whole_virtual_screen() {
        let d = two();
        // The image starts at (-2560, -200): the primary monitor's corner is
        // 2560 px right and 200 px down in it.
        assert_eq!(
            d.in_virtual_image(&PxRect::new(0, 0, 100, 50)),
            Some((2560, 200, 100, 50))
        );
        assert_eq!(
            d.in_virtual_image(&PxRect::new(-2560, -200, 10, 10)),
            Some((0, 0, 10, 10))
        );
        assert_eq!(d.in_virtual_image(&PxRect::new(9000, 0, 1, 1)), None);
    }

    #[test]
    fn mixed_scales_are_noticed() {
        assert!(two().has_mixed_scales());
        let same = Desktop::new(vec![
            monitor(0, 0, 100, 100, 125, true),
            monitor(100, 0, 100, 100, 125, false),
        ]);
        assert!(!same.has_mixed_scales());
    }
}
