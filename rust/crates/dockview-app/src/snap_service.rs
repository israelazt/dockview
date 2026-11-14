//! Taking a screenshot: picking the target, grabbing it, and putting the
//! picture where the job says. It talks to the system only through the
//! ports of the domain (`ScreenGrabber`, `ImageOutput`), so it is the same on
//! every platform and is tested with fakes.

use std::path::Path;

use dockview_domain::image::{Image, ImageFormat};
use dockview_domain::ports::{ImageOutput, ScreenGrabber};
use dockview_domain::snap::{find_window, SnapJob, SnapTarget};

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

/// Takes the picture `job` asks for.
fn grab(job: &SnapJob, screen: &dyn ScreenGrabber, own_pid: u32) -> Result<Image, String> {
    let desktop = screen.desktop();
    match &job.target {
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
/// for the log. `own_pid` is this program's process, never captured by title.
pub fn take(
    job: &SnapJob,
    screen: &dyn ScreenGrabber,
    out: &dyn ImageOutput,
    own_pid: u32,
) -> Result<String, String> {
    // Check the file name before spending time on the capture.
    let format = job.out.as_deref().map(format_of).transpose()?;
    let image = grab(job, screen, own_pid)?;
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

    fn job(target: SnapTarget, out: Option<&str>, copy: bool) -> SnapJob {
        SnapJob {
            target,
            out: out.map(PathBuf::from),
            cursor: false,
            copy,
        }
    }

    #[test]
    fn the_whole_screen_is_every_monitor_together() {
        let (screen, out) = (FakeScreen::default(), FakeOutput::default());
        let done = take(
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
        take(
            &job(SnapTarget::Monitor(1), Some("m.jpg"), false),
            &screen,
            &out,
            1,
        )
        .unwrap();
        assert_eq!(screen.grabs.borrow()[0], PxRect::new(100, 0, 100, 50));
        assert_eq!(out.saved.borrow()[0].1, ImageFormat::Jpeg);
        let e = take(
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
        take(
            &job(SnapTarget::Window("editor".into()), Some("w.gif"), false),
            &screen,
            &out,
            50,
        )
        .unwrap();
        assert_eq!(*screen.window_grabs.borrow(), [2]);
        let e = take(
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
        let done = take(&job(SnapTarget::Rect(rect), None, true), &screen, &out, 1).unwrap();
        assert_eq!(done, "30x20, copied");
        assert_eq!(screen.grabs.borrow()[0], rect);
        assert_eq!(*out.copied.borrow(), 1);
        assert!(out.saved.borrow().is_empty());
    }

    #[test]
    fn a_bad_file_name_fails_before_anything_is_grabbed() {
        let (screen, out) = (FakeScreen::default(), FakeOutput::default());
        let e = take(
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
        let e = take(
            &job(SnapTarget::Full, Some("a.png"), true),
            &screen,
            &out,
            1,
        )
        .unwrap_err();
        assert_eq!(e, "disk full");
        assert_eq!(*out.copied.borrow(), 0);
    }
}
