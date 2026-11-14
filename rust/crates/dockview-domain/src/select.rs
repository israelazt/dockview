//! Choosing what to capture on the dimmed screen: the pure part of the
//! selection overlay. It turns pointer and key events, in virtual-screen
//! pixels, into the rectangle, window or free-form shape the user picked, and
//! says what to draw meanwhile. The window that shows it is the platform's.

use crate::desktop::PxRect;
use crate::image::Image;

/// Smallest side, in pixels, of a rectangle that counts as a selection (a
/// click that wobbles a pixel is not one).
pub const MIN_SIDE: u32 = 3;
/// Points of a free-form path closer than this to the last one are dropped.
const PATH_STEP: i32 = 2;

/// How the area is chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Drag a rectangle.
    #[default]
    Rectangle,
    /// Click a window.
    Window,
    /// Everything: it is chosen as soon as the mode is.
    Full,
    /// Draw a shape with the pointer.
    Freeform,
}

impl Mode {
    pub const ALL: [Mode; 4] = [Mode::Rectangle, Mode::Window, Mode::Full, Mode::Freeform];
}

/// What was picked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    Rect(PxRect),
    /// A closed shape drawn with the pointer: what is outside it is left
    /// transparent.
    Freeform(Vec<(i32, i32)>),
}

impl Selection {
    /// The rectangle that holds the whole selection.
    pub fn bounds(&self) -> PxRect {
        match self {
            Selection::Rect(r) => *r,
            Selection::Freeform(points) => bounding_rect(points),
        }
    }

    /// The picture of the selection, cut from `screen` (an image of the virtual
    /// screen whose corner is `origin`, the corner of the desktop bounds). A
    /// free-form selection is transparent outside its shape.
    pub fn cut(&self, screen: &Image, origin: (i32, i32)) -> Option<Image> {
        let bounds = self.bounds();
        let local = PxRect::new(bounds.x - origin.0, bounds.y - origin.1, bounds.w, bounds.h);
        let mut image = screen.crop(&local)?;
        if let Selection::Freeform(points) = self {
            // The crop may have been cut by the screen's edge: its own corner.
            let corner = (bounds.x.max(origin.0), bounds.y.max(origin.1));
            let shape: Vec<(i32, i32)> = points
                .iter()
                .map(|(x, y)| (x - corner.0, y - corner.1))
                .collect();
            clear_outside(&mut image, &shape);
        }
        Some(image)
    }
}

/// The smallest rectangle holding every point (empty for no points).
pub fn bounding_rect(points: &[(i32, i32)]) -> PxRect {
    let Some(first) = points.first() else {
        return PxRect::new(0, 0, 0, 0);
    };
    let (mut x0, mut y0, mut x1, mut y1) = (first.0, first.1, first.0, first.1);
    for (x, y) in points {
        x0 = x0.min(*x);
        y0 = y0.min(*y);
        x1 = x1.max(*x);
        y1 = y1.max(*y);
    }
    PxRect::new(x0, y0, (x1 - x0 + 1) as u32, (y1 - y0 + 1) as u32)
}

/// Whether the point is inside the closed shape (even-odd rule).
pub fn inside_shape(points: &[(i32, i32)], x: i32, y: i32) -> bool {
    let mut inside = false;
    let mut j = points.len().wrapping_sub(1);
    for i in 0..points.len() {
        let ((xi, yi), (xj, yj)) = (points[i], points[j]);
        if (yi > y) != (yj > y) {
            let at = f64::from(xj - xi) * f64::from(y - yi) / f64::from(yj - yi) + f64::from(xi);
            if f64::from(x) < at {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

/// Makes transparent every pixel of `image` outside the shape (given in the
/// image's own pixels).
fn clear_outside(image: &mut Image, shape: &[(i32, i32)]) {
    let width = image.width as usize;
    for (i, px) in image.bgra.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let (x, y) = ((i % width) as i32, (i / width) as i32);
        if !inside_shape(shape, x, y) {
            *px = [0, 0, 0, 0];
        }
    }
}

/// What reaches the selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Down(i32, i32),
    Move(i32, i32),
    Up(i32, i32),
    Escape,
    SetMode(Mode),
}

/// What the selector decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Keep going.
    Continue,
    Done(Selection),
    Cancel,
}

/// What to draw: the shape being made, and the window the pointer is on.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct View {
    pub mode: Mode,
    /// The rectangle being dragged.
    pub rect: Option<PxRect>,
    /// The window under the pointer (window mode).
    pub hover: Option<PxRect>,
    /// The free-form path so far.
    pub path: Vec<(i32, i32)>,
}

pub struct Selector {
    mode: Mode,
    bounds: PxRect,
    /// Top-level windows, the one on top first.
    windows: Vec<PxRect>,
    /// Where the rectangle drag began.
    anchor: Option<(i32, i32)>,
    pointer: Option<(i32, i32)>,
    path: Vec<(i32, i32)>,
}

impl Selector {
    /// `bounds` is the virtual screen; `windows` the windows' rectangles,
    /// topmost first (for the window mode).
    pub fn new(mode: Mode, bounds: PxRect, windows: Vec<PxRect>) -> Self {
        Self {
            mode,
            bounds,
            windows,
            anchor: None,
            pointer: None,
            path: Vec::new(),
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// What the mode asks for before any input (full screen needs no pointer).
    pub fn start(&self) -> Step {
        if self.mode == Mode::Full {
            Step::Done(Selection::Rect(self.bounds))
        } else {
            Step::Continue
        }
    }

    fn window_at(&self, x: i32, y: i32) -> Option<PxRect> {
        self.windows
            .iter()
            .find(|w| w.contains(x, y))
            .and_then(|w| self.bounds.intersect(w))
    }

    pub fn apply(&mut self, input: Input) -> Step {
        match input {
            Input::Escape => return Step::Cancel,
            Input::SetMode(mode) => {
                self.mode = mode;
                self.anchor = None;
                self.path.clear();
                return self.start();
            }
            Input::Move(x, y) => {
                self.pointer = Some((x, y));
                if self.mode == Mode::Freeform && self.anchor.is_some() {
                    self.extend_path(x, y);
                }
            }
            Input::Down(x, y) => {
                self.pointer = Some((x, y));
                match self.mode {
                    Mode::Rectangle => self.anchor = Some((x, y)),
                    Mode::Freeform => {
                        self.anchor = Some((x, y));
                        self.path = vec![(x, y)];
                    }
                    Mode::Window | Mode::Full => self.anchor = Some((x, y)),
                }
            }
            Input::Up(x, y) => return self.release(x, y),
        }
        Step::Continue
    }

    fn extend_path(&mut self, x: i32, y: i32) {
        let far_enough = self
            .path
            .last()
            .is_none_or(|(lx, ly)| (x - lx).abs() >= PATH_STEP || (y - ly).abs() >= PATH_STEP);
        if far_enough {
            self.path.push((x, y));
        }
    }

    fn release(&mut self, x: i32, y: i32) -> Step {
        let Some(from) = self.anchor.take() else {
            return Step::Continue;
        };
        match self.mode {
            Mode::Rectangle => {
                let rect = PxRect::from_corners(from, (x, y));
                match self.bounds.intersect(&rect) {
                    Some(r) if r.w >= MIN_SIDE && r.h >= MIN_SIDE => Step::Done(Selection::Rect(r)),
                    _ => Step::Continue,
                }
            }
            Mode::Window => match self.window_at(x, y) {
                // The press and the release must be on the same window.
                Some(w) if self.window_at(from.0, from.1) == Some(w) => {
                    Step::Done(Selection::Rect(w))
                }
                _ => Step::Continue,
            },
            Mode::Freeform => {
                self.extend_path(x, y);
                let path = std::mem::take(&mut self.path);
                let area = bounding_rect(&path);
                if path.len() >= 3 && area.w >= MIN_SIDE && area.h >= MIN_SIDE {
                    Step::Done(Selection::Freeform(path))
                } else {
                    Step::Continue
                }
            }
            Mode::Full => Step::Continue,
        }
    }

    /// What to draw now.
    pub fn view(&self) -> View {
        let rect = match (self.mode, self.anchor, self.pointer) {
            (Mode::Rectangle, Some(a), Some(p)) => {
                self.bounds.intersect(&PxRect::from_corners(a, p))
            }
            _ => None,
        };
        let hover = match (self.mode, self.pointer) {
            (Mode::Window, Some((x, y))) => self.window_at(x, y),
            _ => None,
        };
        View {
            mode: self.mode,
            rect,
            hover,
            path: if self.mode == Mode::Freeform {
                self.path.clone()
            } else {
                Vec::new()
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOUNDS: PxRect = PxRect::new(-1000, 0, 3000, 1000);

    fn selector(mode: Mode) -> Selector {
        let windows = vec![
            PxRect::new(100, 100, 300, 200), // on top
            PxRect::new(0, 0, 1000, 800),    // below it
        ];
        Selector::new(mode, BOUNDS, windows)
    }

    fn drag(s: &mut Selector, from: (i32, i32), to: (i32, i32)) -> Step {
        s.apply(Input::Down(from.0, from.1));
        s.apply(Input::Move(to.0, to.1));
        s.apply(Input::Up(to.0, to.1))
    }

    #[test]
    fn dragging_a_rectangle_picks_it_in_any_direction_across_monitors() {
        let mut s = selector(Mode::Rectangle);
        assert_eq!(
            drag(&mut s, (500, 300), (-200, 100)),
            Step::Done(Selection::Rect(PxRect::new(-200, 100, 700, 200)))
        );
    }

    #[test]
    fn the_rectangle_is_shown_while_it_is_dragged_and_cut_to_the_screen() {
        let mut s = selector(Mode::Rectangle);
        s.apply(Input::Down(10, 10));
        s.apply(Input::Move(60, 40));
        assert_eq!(s.view().rect, Some(PxRect::new(10, 10, 50, 30)));
        s.apply(Input::Move(5000, 5000));
        assert_eq!(s.view().rect, Some(PxRect::new(10, 10, 1990, 990)));
    }

    #[test]
    fn a_click_or_a_tiny_wobble_is_not_a_selection() {
        let mut s = selector(Mode::Rectangle);
        assert_eq!(drag(&mut s, (50, 50), (50, 50)), Step::Continue);
        assert_eq!(drag(&mut s, (50, 50), (52, 52)), Step::Continue);
        assert_eq!(s.view().rect, None, "and nothing stays drawn");
        // Then a real one still works.
        assert!(matches!(drag(&mut s, (50, 50), (90, 90)), Step::Done(_)));
    }

    #[test]
    fn escape_cancels_in_every_mode() {
        for mode in Mode::ALL {
            let mut s = selector(mode);
            assert_eq!(s.apply(Input::Escape), Step::Cancel, "{mode:?}");
        }
    }

    #[test]
    fn the_window_under_the_pointer_is_the_topmost_and_is_picked_by_a_click() {
        let mut s = selector(Mode::Window);
        s.apply(Input::Move(150, 150));
        assert_eq!(s.view().hover, Some(PxRect::new(100, 100, 300, 200)));
        s.apply(Input::Move(600, 500));
        assert_eq!(s.view().hover, Some(PxRect::new(0, 0, 1000, 800)));
        s.apply(Input::Move(1500, 900));
        assert_eq!(s.view().hover, None);
        assert_eq!(
            drag(&mut s, (150, 150), (150, 150)),
            Step::Done(Selection::Rect(PxRect::new(100, 100, 300, 200)))
        );
    }

    #[test]
    fn a_window_press_that_ends_on_another_window_picks_nothing() {
        let mut s = selector(Mode::Window);
        assert_eq!(drag(&mut s, (150, 150), (600, 500)), Step::Continue);
        assert_eq!(
            drag(&mut s, (1500, 900), (1500, 900)),
            Step::Continue,
            "no window there"
        );
    }

    #[test]
    fn a_window_that_sticks_out_of_the_screen_is_cut_to_it() {
        let mut s = Selector::new(Mode::Window, BOUNDS, vec![PxRect::new(1800, -50, 500, 300)]);
        assert_eq!(
            drag(&mut s, (1900, 100), (1900, 100)),
            Step::Done(Selection::Rect(PxRect::new(1800, 0, 200, 250)))
        );
    }

    #[test]
    fn full_screen_is_picked_by_choosing_the_mode() {
        let s = selector(Mode::Full);
        assert_eq!(s.start(), Step::Done(Selection::Rect(BOUNDS)));
        let mut s = selector(Mode::Rectangle);
        assert_eq!(s.start(), Step::Continue);
        assert_eq!(
            s.apply(Input::SetMode(Mode::Full)),
            Step::Done(Selection::Rect(BOUNDS))
        );
    }

    #[test]
    fn changing_the_mode_drops_the_drag_in_progress() {
        let mut s = selector(Mode::Rectangle);
        s.apply(Input::Down(10, 10));
        s.apply(Input::Move(90, 90));
        assert_eq!(s.apply(Input::SetMode(Mode::Window)), Step::Continue);
        assert_eq!(s.view().rect, None);
        assert_eq!(s.mode(), Mode::Window);
    }

    #[test]
    fn a_free_form_shape_is_the_path_drawn_and_a_scribble_too_small_is_not_one() {
        let mut s = selector(Mode::Freeform);
        s.apply(Input::Down(100, 100));
        for p in [(120, 100), (140, 120), (120, 150), (100, 130)] {
            s.apply(Input::Move(p.0, p.1));
        }
        assert!(s.view().path.len() >= 4);
        let Step::Done(Selection::Freeform(path)) = s.apply(Input::Up(100, 110)) else {
            panic!("a shape was expected");
        };
        assert_eq!(path[0], (100, 100));
        assert!(path.len() >= 5);
        assert_eq!(s.view().path.len(), 0, "the path is not kept");

        assert_eq!(drag(&mut s, (300, 300), (301, 301)), Step::Continue);
    }

    #[test]
    fn points_that_barely_moved_are_not_added_to_the_path() {
        let mut s = selector(Mode::Freeform);
        s.apply(Input::Down(10, 10));
        for _ in 0..20 {
            s.apply(Input::Move(11, 10));
        }
        assert_eq!(s.view().path.len(), 1);
    }

    #[test]
    fn the_shape_test_follows_the_even_odd_rule() {
        let square = [(0, 0), (10, 0), (10, 10), (0, 10)];
        assert!(inside_shape(&square, 5, 5));
        assert!(!inside_shape(&square, 15, 5));
        assert!(!inside_shape(&square, 5, -1));
        let triangle = [(0, 0), (10, 0), (0, 10)];
        assert!(inside_shape(&triangle, 2, 2));
        assert!(!inside_shape(&triangle, 8, 8));
        assert!(!inside_shape(&[], 1, 1));
    }

    #[test]
    fn a_selection_is_cut_from_the_screen_image_with_a_shape_left_transparent() {
        // A 20×20 screen whose corner is (-10, 0): pixel (x, y) has blue = x.
        let mut bgra = Vec::new();
        for _y in 0..20u8 {
            for x in 0..20u8 {
                bgra.extend_from_slice(&[x, 0, 0, 255]);
            }
        }
        let screen = Image {
            width: 20,
            height: 20,
            bgra,
        };
        let rect = Selection::Rect(PxRect::new(-5, 2, 4, 3));
        let cut = rect.cut(&screen, (-10, 0)).unwrap();
        assert_eq!((cut.width, cut.height), (4, 3));
        assert_eq!(cut.pixel(0, 0), Some([5, 0, 0, 255]), "x = -5 is column 5");

        let shape = Selection::Freeform(vec![(0, 0), (9, 0), (9, 9), (0, 9)]);
        let image = shape.cut(&screen, (-10, 0)).unwrap();
        assert_eq!((image.width, image.height), (10, 10));
        // The corner of the square is inside; a point on the far side is not.
        assert_eq!(image.pixel(2, 2).unwrap()[3], 255);
        let triangle = Selection::Freeform(vec![(0, 0), (9, 0), (0, 9)]);
        let image = triangle.cut(&screen, (-10, 0)).unwrap();
        assert_eq!(image.pixel(1, 1).unwrap()[3], 255);
        assert_eq!(image.pixel(8, 8).unwrap()[3], 0, "outside the shape");
    }

    #[test]
    fn nothing_outside_the_screen_can_be_cut() {
        let screen = Image::solid(10, 10, [1, 2, 3, 255]);
        assert!(Selection::Rect(PxRect::new(100, 100, 5, 5))
            .cut(&screen, (0, 0))
            .is_none());
    }
}
