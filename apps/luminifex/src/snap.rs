//! Taking a screenshot with the real screen, for `--snap` and for the
//! hotkey and the tray. The work is `dockview_app::snap_service`; this only
//! hands it the real screen, the overlay and the delay.

use dockview_domain::i18n::Lang;
use dockview_domain::snap::SnapJob;
use dockview_platform::screen_io::PlatformScreen;

use crate::capture_overlay::CaptureOverlay;

/// Waits for the delay, then takes the screenshot `job` asks for. The answer is
/// what was done (for the log), or `snap_service::CANCELLED`.
pub fn take(job: &SnapJob, lang: Lang) -> Result<String, String> {
    if job.delay_secs > 0 {
        dockview_domain::log_info!("snap: waiting {} s", job.delay_secs);
        std::thread::sleep(std::time::Duration::from_secs(u64::from(job.delay_secs)));
    }
    let screen = PlatformScreen;
    dockview_app::snap_service::take(
        job,
        &screen,
        &CaptureOverlay,
        &screen,
        lang,
        std::process::id(),
    )
}

/// `--snap`: takes the screenshot and says how it went; the exit code of the
/// program.
pub fn run(job: &SnapJob) -> i32 {
    match take(job, dockview_platform::system_lang::detect_system_lang()) {
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
