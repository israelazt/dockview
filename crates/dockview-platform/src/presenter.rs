//! Direct3D 11 behind the `Presenter` port: the renderer that draws the video and
//! the interface renderer that draws on top, as one object.

use dockview_domain::draw::DrawList;
use dockview_domain::frame::Frame;
use dockview_domain::layout::Rect;
use dockview_domain::presenter::{PresentError, Presenter, Screenshot};
use dockview_domain::render_plan::ScaleFilter;
use windows::Win32::Foundation::HWND;

use crate::render::{is_device_lost, Filter, Renderer};
use crate::ui_gpu::UiGpu;

pub struct D3dPresenter {
    renderer: Renderer,
    ui_gpu: Option<UiGpu>,
}

impl D3dPresenter {
    /// The graphics for the window `hwnd`, `width` by `height` pixels.
    pub fn new(hwnd: HWND, width: u32, height: u32) -> Result<Self, PresentError> {
        let renderer = Renderer::new(hwnd, width, height).map_err(error_of)?;
        // Without the interface renderer the video still shows.
        let ui_gpu = match UiGpu::new(renderer.device()) {
            Ok(gpu) => Some(gpu),
            Err(e) => {
                dockview_domain::log_error!("could not initialise the interface renderer: {e}");
                None
            }
        };
        Ok(Self { renderer, ui_gpu })
    }
}

fn error_of(e: windows::core::Error) -> PresentError {
    PresentError {
        message: e.to_string(),
        device_lost: is_device_lost(&e),
    }
}

fn filter_of(filter: ScaleFilter) -> Filter {
    match filter {
        ScaleFilter::Nearest => Filter::Nearest,
        ScaleFilter::Linear => Filter::Linear,
        ScaleFilter::Bicubic => Filter::Bicubic,
    }
}

impl Presenter for D3dPresenter {
    fn size(&self) -> (u32, u32) {
        self.renderer.size()
    }

    fn resize(&mut self, width: u32, height: u32) -> Result<(), PresentError> {
        self.renderer.resize(width, height).map_err(error_of)
    }

    fn set_filter(&mut self, filter: ScaleFilter) {
        self.renderer.set_filter(filter_of(filter));
    }

    fn set_video_area(&mut self, area: Option<Rect>) {
        self.renderer.set_video_area(area);
    }

    fn wait_for_slot(&self, timeout_ms: u32) {
        self.renderer.wait_for_present_slot(timeout_ms);
    }

    fn upload(&mut self, frame: &Frame) -> Result<(), PresentError> {
        self.renderer.upload(frame).map_err(error_of)
    }

    fn present(
        &mut self,
        ui: &DrawList<'_>,
        capture: bool,
    ) -> Result<Option<Screenshot>, PresentError> {
        let gpu = &mut self.ui_gpu;
        let shot = self
            .renderer
            .draw(
                &mut |_device, context, _size| {
                    if let Some(gpu) = gpu.as_mut() {
                        if let Err(e) = gpu.render(context, ui) {
                            dockview_domain::log_warn!("ui draw failed: {e}");
                        }
                    }
                },
                capture,
            )
            .map_err(error_of)?;
        Ok(shot.map(|s| Screenshot {
            width: s.width,
            height: s.height,
            rgba: s.rgba,
        }))
    }

    fn gpu_ms(&self) -> Option<f32> {
        self.renderer.gpu_ms()
    }

    fn device_lost(&self) -> bool {
        self.renderer.device_lost()
    }
}
