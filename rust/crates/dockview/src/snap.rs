//! `--snap`: takes a screenshot from the command line and exits, without
//! opening the window. It is the capture half of the screenshot tool, usable on
//! its own (and the way to test it against the real screen).

use std::path::Path;

use dockview_domain::image::{Image, ImageFormat};
use dockview_domain::snap::{find_window, SnapJob, SnapTarget};
use dockview_platform::{imagefile, snapshot, targets};

/// The format of the file called `path`, by its extension.
fn format_of(path: &Path) -> Result<ImageFormat, String> {
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
fn capture(job: &SnapJob) -> Result<Image, String> {
    match &job.target {
        SnapTarget::Full => snapshot::capture_virtual_screen(job.cursor),
        SnapTarget::Monitor(index) => snapshot::capture_monitor(*index, job.cursor),
        SnapTarget::Rect(rect) => snapshot::capture_rect(*rect, job.cursor),
        SnapTarget::Window(text) => {
            let windows = targets::list_windows();
            let window = find_window(&windows, text, std::process::id())
                .ok_or_else(|| format!("no window has '{text}' in its title"))?;
            snapshot::capture_window(window.handle, job.cursor)
        }
    }
}

/// Takes the screenshot and puts it where the job says. Returns what was done,
/// for the log.
pub fn take(job: &SnapJob) -> Result<String, String> {
    // Check the file name before spending time on the capture.
    let format = job.out.as_deref().map(format_of).transpose()?;
    let image = capture(job)?;
    if image.is_all_black() {
        dockview_domain::log_warn!(
            "the picture is all black: the target may be protected or not showing"
        );
    }
    let mut done = format!("{}x{}", image.width, image.height);
    if let (Some(path), Some(format)) = (&job.out, format) {
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("could not create the folder: {e}"))?;
        }
        imagefile::save(&image, path, format)?;
        done.push_str(&format!(", saved {}", path.display()));
    }
    if job.copy {
        imagefile::copy_to_clipboard(&image)?;
        done.push_str(", copied");
    }
    Ok(done)
}

/// Runs the job and says how it went; the exit code of the program.
pub fn run(job: &SnapJob) -> i32 {
    match take(job) {
        Ok(done) => {
            dockview_domain::log_info!("snap: {done}");
            0
        }
        Err(e) => {
            dockview_domain::log_error!("snap: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dockview_domain::desktop::PxRect;
    use std::path::PathBuf;

    fn job(target: SnapTarget, out: Option<PathBuf>) -> SnapJob {
        SnapJob {
            target,
            out,
            cursor: false,
            copy: false,
        }
    }

    #[test]
    fn the_file_name_decides_the_format() {
        assert_eq!(format_of(Path::new("a/b.PNG")), Ok(ImageFormat::Png));
        assert_eq!(format_of(Path::new("x.jpeg")), Ok(ImageFormat::Jpeg));
        assert!(format_of(Path::new("x.bmp")).is_err());
        assert!(format_of(Path::new("noext")).is_err());
    }

    #[test]
    fn a_bad_file_name_fails_before_anything_is_captured() {
        let e = take(&job(SnapTarget::Full, Some(PathBuf::from("x.bmp")))).unwrap_err();
        assert!(e.contains(".png"), "{e}");
    }

    #[test]
    fn a_window_that_is_not_there_is_said() {
        let out = std::env::temp_dir()
            .join("dockview-snap-tests")
            .join("w.png");
        let e = take(&job(
            SnapTarget::Window("no window has this title 1f3a9".into()),
            Some(out),
        ))
        .unwrap_err();
        assert!(e.contains("no window"), "{e}");
    }

    #[test]
    fn a_corner_of_the_screen_is_saved_where_asked_creating_the_folder() {
        if snapshot::desktop().monitors.is_empty() {
            return; // no display in this session
        }
        let dir = std::env::temp_dir()
            .join("dockview-snap-tests")
            .join("nested");
        let _ = std::fs::remove_dir_all(&dir);
        let out = dir.join("corner.png");
        let primary = snapshot::desktop().monitors[0].bounds;
        let done = take(&job(
            SnapTarget::Rect(PxRect::new(primary.x, primary.y, 40, 30)),
            Some(out.clone()),
        ))
        .unwrap();
        assert!(done.starts_with("40x30"), "{done}");
        assert!(std::fs::read(&out).unwrap().starts_with(b"\x89PNG"));
    }
}
