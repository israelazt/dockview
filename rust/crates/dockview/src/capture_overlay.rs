//! The selection overlay for the screenshot tool, on Windows: the platform's
//! window and graphics, the portable controller of the domain for what the
//! pointer means, and Dockview's own interface engine (`dockview_ui::snip`) for
//! how it looks and moves. This glues them together and runs the short loop
//! while the user chooses.

use std::time::Instant;

use dockview_domain::desktop::{Desktop, PxRect};
use dockview_domain::image::Image;
use dockview_domain::overlay_input::OverlayController;
use dockview_domain::ports::{PickOptions, SelectionOverlay};
use dockview_domain::select::{Mode, Selection, Step};
use dockview_platform::overlay_gpu::OverlayGpu;
use dockview_platform::overlay_window::{cursor_position, OverlayWindow};
use dockview_platform::ui_gpu::UiGpu;
use dockview_ui::snip::{SnipChrome, SnipView};
use dockview_ui::Ui;

use crate::shell::helpers::system_animations_enabled;

/// The real overlay.
pub struct CaptureOverlay;

impl SelectionOverlay for CaptureOverlay {
    fn pick(
        &self,
        frozen: &Image,
        desktop: &Desktop,
        windows: Vec<PxRect>,
        options: &PickOptions,
    ) -> Option<Selection> {
        let bounds = desktop.bounds()?;
        // The buttons go on the monitor the cursor is on (the primary if unknown).
        let monitor = cursor_position()
            .and_then(|(x, y)| desktop.monitor_at(x, y))
            .or_else(|| desktop.monitors.first())?;
        let mut controller = OverlayController::new(options.mode, bounds, windows, monitor);
        match controller.start() {
            Step::Done(selection) => return Some(selection),
            Step::Cancel => return None,
            Step::Continue => {}
        }

        let mut window = OverlayWindow::new(bounds)?;
        let mut gpu = OverlayGpu::new(window.hwnd(), bounds.w, bounds.h).ok()?;
        gpu.set_picture(frozen).ok()?;
        let mut ui_gpu = UiGpu::new(gpu.device()).ok()?;
        let mut ui = Ui::new();
        let mut chrome = SnipChrome::new();
        let scale = controller.scale_percent() as f32 / 100.0;
        let animations = system_animations_enabled();
        let origin = (bounds.x, bounds.y);
        let mut last = Instant::now();

        loop {
            let now = Instant::now();
            let dt = now.duration_since(last).as_secs_f32().min(0.1);
            last = now;
            for input in window.pump() {
                match controller.apply(input) {
                    Step::Continue => {}
                    Step::Done(selection) => return Some(selection),
                    Step::Cancel => return None,
                }
            }
            window.set_arrow_cursor(
                controller.mode() == Mode::Window || controller.hovered_button().is_some(),
            );
            let view = controller.view();
            let snip = SnipView {
                view: &view,
                mode: controller.mode(),
                origin,
                toolbar: controller.toolbar(),
                toolbar_back: controller.toolbar_back(),
                hovered: controller.hovered_button(),
                pressed: controller.button_pressed(),
                lang: options.lang,
            };
            let drawn = gpu.draw(&mut |_device, context, size| {
                ui.begin(size, scale);
                chrome.frame(&mut ui, &snip, dt, animations);
                if let Err(e) = ui_gpu.render(context, &ui.draw_list()) {
                    dockview_domain::log_warn!("the capture overlay could not draw: {e}");
                }
            });
            if let Err(e) = drawn {
                dockview_domain::log_warn!("the capture overlay stopped: {e}");
                return None;
            }
        }
    }
}
