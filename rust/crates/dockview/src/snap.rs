//! `--snap`: takes a screenshot from the command line and exits, without
//! opening the window. The work is `dockview_app::snap_service`; this only
//! hands it the real screen and says how it went.

use dockview_domain::snap::SnapJob;
use dockview_platform::screen_io::PlatformScreen;

/// Runs the job and says how it went; the exit code of the program.
pub fn run(job: &SnapJob) -> i32 {
    let screen = PlatformScreen;
    match dockview_app::snap_service::take(job, &screen, &screen, std::process::id()) {
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
