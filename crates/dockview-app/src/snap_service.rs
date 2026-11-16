//! Taking a screenshot: picking the target, grabbing it, and putting the
//! picture where the job says. It talks to the system only through the
//! ports of the domain (`ScreenGrabber`, `ImageOutput`), so it is the same on
//! every platform and is tested with fakes.

use std::path::Path;

use dockview_domain::desktop::PxRect;
use dockview_domain::i18n::Lang;
use dockview_domain::image::{Image, ImageFormat};
use dockview_domain::ports::{ImageOutput, PickOptions, ScreenGrabber, SelectionOverlay};
use dockview_domain::select::Mode;
use dockview_domain::snap::{find_window, SnapJob, SnapTarget};

/// What `take` answers when the user cancelled the selection.
pub const CANCELLED: &str = "cancelled";

/// The format of the file called `path`, by its extension.
pub fn format_of(path: &Path) -> Result<ImageFormat, String> {
    path.file_name()
        .and_then(|n| n.to_str())
        .and_then(ImageFormat::from_name)
        .ok_or_else(|| {
            format!(
                "'{}': the file must end in .png, .jpg or .gif",
                path.display()
            )
        })
}

/// The user chooses on the dimmed screen: the whole screen is frozen first, so
/// what is cut is exactly what was shown. `None` when it was cancelled.
fn choose(
    job: &SnapJob,
    mode: Mode,
    lang: Lang,
    screen: &dyn ScreenGrabber,
    overlay: &dyn SelectionOverlay,
    own_pid: u32,
) -> Result<Option<Image>, String> {
    let desktop = screen.desktop();
    let bounds = desktop.bounds().ok_or("there is no monitor")?;
    let frozen = screen.grab_rect(bounds, job.cursor)?;
    let windows: Vec<PxRect> = screen
        .windows()
        .iter()
        .filter(|w| w.process_id != own_pid)
        .filter_map(|w| screen.window_rect(w.handle))
        .collect();
    let Some(selection) = overlay.pick(&frozen, &desktop, windows, &PickOptions { mode, lang })
    else {
        return Ok(None);
    };
    Ok(selection.cut(&frozen, (bounds.x, bounds.y)))
}

/// Takes the picture `job` asks for.
fn grab(job: &SnapJob, screen: &dyn ScreenGrabber, own_pid: u32) -> Result<Image, String> {
    let desktop = screen.desktop();
    match &job.target {
        SnapTarget::Select(_) => Err("the selection is not a plain grab".into()),
        SnapTarget::Full => {
            let all = desktop.bounds().ok_or("there is no monitor")?;
            screen.grab_rect(all, job.cursor)
        }
        SnapTarget::Monitor(index) => {
            let monitor = desktop
                .monitors
                .get(*index)
                .ok_or_else(|| format!("there is no monitor {index}"))?;
            screen.grab_rect(monitor.bounds, job.cursor)
        }
        SnapTarget::Rect(rect) => screen.grab_rect(*rect, job.cursor),
        SnapTarget::Window(text) => {
            let windows = screen.windows();
            let window = find_window(&windows, text, own_pid)
                .ok_or_else(|| format!("no window has '{text}' in its title"))?;
            screen.grab_window(window.handle, job.cursor)
        }
    }
}

/// Takes the screenshot and puts it where the job says. Returns what was done,
/// for the log. `lang` is the language of the overlay's buttons; `own_pid` is this program's process, never captured by title.
pub fn take(
    job: &SnapJob,
    screen: &dyn ScreenGrabber,
    overlay: &dyn SelectionOverlay,
    out: &dyn ImageOutput,
    lang: Lang,
    own_pid: u32,
) -> Result<String, String> {
    // Check the file name before spending time on the capture.
    let format = job.out.as_deref().map(format_of).transpose()?;
    let image = match job.target {
        SnapTarget::Select(mode) => match choose(job, mode, lang, screen, overlay, own_pid)? {
            Some(image) => image,
            None => return Ok(CANCELLED.into()),
        },
        _ => grab(job, screen, own_pid)?,
    };
    if image.is_all_black() {
        dockview_domain::log_warn!(
            "the picture is all black: the target may be protected or not showing"
        );
    }
    let mut done = format!("{}x{}", image.width, image.height);
    if let (Some(path), Some(format)) = (&job.out, format) {
        out.save(&image, path, format)?;
        done.push_str(&format!(", saved {}", path.display()));
    }
    if job.copy {
        out.copy(&image)?;
        done.push_str(", copied");
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dockview_domain::desktop::{Desktop, DesktopMonitor, PxRect};
    use dockview_domain::select::Selection;
    use dockview_domain::sources::WindowInfo;
    use std::cell::RefCell;
    use std::path::PathBuf;

    /// Two monitors side by side; every grab is noted and gives a solid picture.
    #[derive(Default)]
    struct FakeScreen {
        grabs: RefCell<Vec<PxRect>>,
        window_grabs: RefCell<Vec<isize>>,
    }

    impl ScreenGrabber for FakeScreen {
        fn desktop(&self) -> Desktop {
            let monitor = |x| DesktopMonitor {
                bounds: PxRect::new(x, 0, 100, 50),
                name: format!("M{x}"),
                primary: x == 0,
                scale_percent: 100,
            };
            Desktop::new(vec![monitor(0), monitor(100)])
        }
        fn grab_rect(&self, rect: PxRect, _: bool) -> Result<Image, String> {
            self.grabs.borrow_mut().push(rect);
            Ok(Image::solid(rect.w, rect.h, [10, 20, 30, 255]))
        }
        fn grab_window(&self, handle: isize, _: bool) -> Result<Image, String> {
            self.window_grabs.borrow_mut().push(handle);
            Ok(Image::solid(7, 5, [10, 20, 30, 255]))
        }
        fn windows(&self) -> Vec<WindowInfo> {
            vec![
                WindowInfo {
                    handle: 1,
                    title: "Dockview".into(),
                    process_id: 50,
                },
                WindowInfo {
                    handle: 2,
                    title: "Notes - Editor".into(),
                    process_id: 60,
                },
            ]
        }
        fn window_rect(&self, _: isize) -> Option<PxRect> {
            None
        }
    }

    /// A file written: where, in what format and how big.
    type Written = (PathBuf, ImageFormat, (u32, u32));

    /// An overlay that answers with a ready choice (or a cancel) and notes what it was shown.
    struct FakeOverlay {
        answer: Option<Selection>,
        shown: RefCell<Vec<(PxRect, usize, Mode)>>,
    }

    impl FakeOverlay {
        fn answering(answer: Option<Selection>) -> Self {
            Self {
                answer,
                shown: RefCell::new(Vec::new()),
            }
        }
    }

    impl SelectionOverlay for FakeOverlay {
        fn pick(
            &self,
            frozen: &Image,
            desktop: &Desktop,
            windows: Vec<PxRect>,
            options: &PickOptions,
        ) -> Option<Selection> {
            let bounds = desktop.bounds().unwrap();
            assert_eq!((frozen.width, frozen.height), (bounds.w, bounds.h));
            self.shown
                .borrow_mut()
                .push((bounds, windows.len(), options.mode));
            self.answer.clone()
        }
    }

    #[derive(Default)]
    struct FakeOutput {
        saved: RefCell<Vec<Written>>,
        copied: RefCell<u32>,
        fail_save: bool,
    }

    impl ImageOutput for FakeOutput {
        fn save(&self, image: &Image, path: &Path, format: ImageFormat) -> Result<(), String> {
            if self.fail_save {
                return Err("disk full".into());
            }
            self.saved
                .borrow_mut()
                .push((path.to_owned(), format, (image.width, image.height)));
            Ok(())
        }
        fn copy(&self, _: &Image) -> Result<(), String> {
            *self.copied.borrow_mut() += 1;
            Ok(())
        }
    }

    /// Takes a job that never opens the overlay.
    fn take_plain(
        job: &SnapJob,
        screen: &FakeScreen,
        out: &FakeOutput,
        pid: u32,
    ) -> Result<String, String> {
        take(
            job,
            screen,
            &FakeOverlay::answering(None),
            out,
            Lang::En,
            pid,
        )
    }

    fn job(target: SnapTarget, out: Option<&str>, copy: bool) -> SnapJob {
        SnapJob {
            target,
            out: out.map(PathBuf::from),
            cursor: false,
            copy,
            delay_secs: 0,
        }
    }

    #[test]
    fn the_whole_screen_is_every_monitor_together() {
        let (screen, out) = (FakeScreen::default(), FakeOutput::default());
        let done = take_plain(
            &job(SnapTarget::Full, Some("a.png"), false),
            &screen,
            &out,
            1,
        )
        .unwrap();
        assert_eq!(screen.grabs.borrow()[0], PxRect::new(0, 0, 200, 50));
        assert_eq!(done, "200x50, saved a.png");
        assert_eq!(
            out.saved.borrow()[0],
            (PathBuf::from("a.png"), ImageFormat::Png, (200, 50))
        );
    }

    #[test]
    fn a_monitor_is_found_by_position_and_a_missing_one_is_an_error() {
        let (screen, out) = (FakeScreen::default(), FakeOutput::default());
        take_plain(
            &job(SnapTarget::Monitor(1), Some("m.jpg"), false),
            &screen,
            &out,
            1,
        )
        .unwrap();
        assert_eq!(screen.grabs.borrow()[0], PxRect::new(100, 0, 100, 50));
        assert_eq!(out.saved.borrow()[0].1, ImageFormat::Jpeg);
        let e = take_plain(
            &job(SnapTarget::Monitor(5), Some("m.png"), false),
            &screen,
            &out,
            1,
        )
        .unwrap_err();
        assert!(e.contains("no monitor 5"), "{e}");
    }

    #[test]
    fn a_window_is_found_by_its_title_and_never_our_own() {
        let (screen, out) = (FakeScreen::default(), FakeOutput::default());
        take_plain(
            &job(SnapTarget::Window("editor".into()), Some("w.gif"), false),
            &screen,
            &out,
            50,
        )
        .unwrap();
        assert_eq!(*screen.window_grabs.borrow(), [2]);
        let e = take_plain(
            &job(SnapTarget::Window("dockview".into()), Some("w.png"), false),
            &screen,
            &out,
            50,
        )
        .unwrap_err();
        assert!(e.contains("no window"), "{e}");
    }

    #[test]
    fn a_rectangle_is_grabbed_as_asked_and_can_also_be_copied() {
        let (screen, out) = (FakeScreen::default(), FakeOutput::default());
        let rect = PxRect::new(10, 5, 30, 20);
        let done = take_plain(&job(SnapTarget::Rect(rect), None, true), &screen, &out, 1).unwrap();
        assert_eq!(done, "30x20, copied");
        assert_eq!(screen.grabs.borrow()[0], rect);
        assert_eq!(*out.copied.borrow(), 1);
        assert!(out.saved.borrow().is_empty());
    }

    #[test]
    fn a_bad_file_name_fails_before_anything_is_grabbed() {
        let (screen, out) = (FakeScreen::default(), FakeOutput::default());
        let e = take_plain(
            &job(SnapTarget::Full, Some("x.bmp"), false),
            &screen,
            &out,
            1,
        )
        .unwrap_err();
        assert!(e.contains(".png"), "{e}");
        assert!(screen.grabs.borrow().is_empty());
        assert_eq!(format_of(Path::new("a/b.PNG")), Ok(ImageFormat::Png));
        assert!(format_of(Path::new("noext")).is_err());
    }

    #[test]
    fn a_failed_write_is_reported_and_nothing_is_copied_after_it() {
        let screen = FakeScreen::default();
        let out = FakeOutput {
            fail_save: true,
            ..FakeOutput::default()
        };
        let e = take_plain(
            &job(SnapTarget::Full, Some("a.png"), true),
            &screen,
            &out,
            1,
        )
        .unwrap_err();
        assert_eq!(e, "disk full");
        assert_eq!(*out.copied.borrow(), 0);
    }

    #[test]
    fn choosing_freezes_the_whole_screen_shows_it_and_saves_what_was_picked() {
        let (screen, out) = (FakeScreen::default(), FakeOutput::default());
        let overlay = FakeOverlay::answering(Some(Selection::Rect(PxRect::new(20, 10, 30, 15))));
        let done = take(
            &job(SnapTarget::Select(Mode::Window), Some("p.png"), false),
            &screen,
            &overlay,
            &out,
            Lang::En,
            50,
        )
        .unwrap();
        assert_eq!(done, "30x15, saved p.png");
        // The overlay got the whole screen (200x50) and the mode asked for; the
        // window of our own process (50) is not offered, the other one is.
        assert_eq!(overlay.shown.borrow()[0].2, Mode::Window);
        assert_eq!(screen.grabs.borrow()[0], PxRect::new(0, 0, 200, 50));
        // `window_rect` gives nothing in the fake, so no window rectangles.
        assert_eq!(overlay.shown.borrow()[0].1, 0);
        assert_eq!(out.saved.borrow()[0].2, (30, 15));
    }

    #[test]
    fn a_cancelled_choice_saves_and_copies_nothing_and_is_not_an_error() {
        let (screen, out) = (FakeScreen::default(), FakeOutput::default());
        let overlay = FakeOverlay::answering(None);
        let done = take(
            &job(SnapTarget::Select(Mode::Rectangle), Some("p.png"), true),
            &screen,
            &overlay,
            &out,
            Lang::En,
            1,
        )
        .unwrap();
        assert_eq!(done, "cancelled");
        assert!(out.saved.borrow().is_empty());
        assert_eq!(*out.copied.borrow(), 0);
    }

    #[test]
    fn a_free_form_choice_comes_out_transparent_outside_its_shape() {
        let (screen, out) = (FakeScreen::default(), FakeOutput::default());
        let triangle = Selection::Freeform(vec![(10, 10), (40, 10), (10, 40)]);
        let overlay = FakeOverlay::answering(Some(triangle));
        let done = take(
            &job(SnapTarget::Select(Mode::Freeform), Some("t.png"), false),
            &screen,
            &overlay,
            &out,
            Lang::En,
            1,
        )
        .unwrap();
        assert!(done.starts_with("31x31"), "{done}");
    }
}
