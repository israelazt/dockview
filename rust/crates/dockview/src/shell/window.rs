//! The window: drawing a frame, the cursor and screenshots.

use dockview_domain::render_plan::{render_plan, video_frame_due};
use dockview_domain::view::RecState;
use dockview_platform::render::{is_device_lost, Renderer, Screenshot};
use winit::window::CursorIcon;

use super::*;

impl Shell {
    /// Pointer shape and visibility follow what the controls are doing.
    pub(super) fn update_cursor(&mut self) {
        let Some(window) = &self.window else {
            return;
        };
        let pointer = self.controls.wants_pointer();
        if pointer != self.cursor_pointer {
            self.cursor_pointer = pointer;
            window.set_cursor(if pointer {
                CursorIcon::Pointer
            } else {
                CursorIcon::Default
            });
        }
        // While watching full screen, an idle cursor gets out of the way.
        let hidden = self.core.fullscreen() && !self.controls.is_visible();
        if hidden != self.cursor_hidden {
            self.cursor_hidden = hidden;
            window.set_cursor_visible(!hidden);
        }
    }

    /// Builds the renderer and the interface renderer again after the graphics
    /// device was lost (a driver update or reset). If it cannot be done yet, the
    /// idle loop tries again.
    pub(super) fn recreate_graphics(&mut self) {
        let hwnd = self.host.borrow().hwnd;
        let (Some(window), Some(hwnd)) = (&self.window, hwnd) else {
            return;
        };
        let size = window.inner_size();
        eprintln!("the graphics device was lost: building the renderer again");
        // The old swap chain has to go before a new one can use the window.
        self.ui_gpu = None;
        self.renderer = None;
        self.last_graphics_try = Instant::now();
        match Renderer::new(hwnd, size.width, size.height) {
            Ok(mut renderer) => {
                renderer.set_filter(filter_for(self.core.render_preset()));
                self.ui_gpu = UiGpu::new(renderer.device()).ok();
                // The glyphs lived in a texture of the lost device.
                self.ui.text.reset_atlas();
                self.renderer = Some(renderer);
            }
            Err(e) => eprintln!("could not build the renderer yet: {e}"),
        }
        self.needs_redraw = true;
    }

    /// Whether a picture that just arrived should be drawn, under the rate cap
    /// of the render preset.
    pub(super) fn video_frame_due(&self) -> bool {
        let cap = render_plan(self.core.render_preset()).max_video_fps;
        video_frame_due(self.last_video_shown.elapsed(), cap)
    }

    pub(super) fn render(&mut self) {
        self.refresh_host_info();
        let model = self.core.model();
        let rec = model.rec;
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;
        let pointer = self.pointer;
        let scale = self.ui_scale;
        // The first seconds include start-up work; wait for steady numbers.
        let settled = !self.core.cli().show_stats || self.core.measurement_windows() >= 3;
        let capture = self.core.cli().screenshot.is_some()
            && self.core.rendered_frames() >= SCREENSHOT_AFTER_FRAMES
            && settled;

        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        // Wait for a free present slot *before* picking the frame so we always
        // show the freshest one.
        renderer.wait_for_present_slot(100);
        let mut arrived = None;
        let mut graphics_lost = false;
        let slot = Arc::clone(self.core.output_slot());
        if let Some(frame) = slot.take_latest() {
            arrived = Some(frame.arrived);
            if let Err(e) = renderer.upload(&frame) {
                eprintln!("upload failed: {e}");
                graphics_lost |= is_device_lost(&e);
            }
            self.core.frame_uploaded((frame.width, frame.height));
            slot.recycle(frame);
            self.last_video_shown = now;
        }

        let (ui, ui_gpu, controls) = (&mut self.ui, &mut self.ui_gpu, &mut self.controls);
        let stats_panel = &mut self.stats_panel;
        let (stats, stats_open) = (self.core.stats(), self.core.stats_open());
        let animations = self.animations;
        let toasts = &mut self.toasts;
        let result = renderer.draw(
            &mut |_device, context, size| {
                let Some(gpu) = ui_gpu.as_mut() else {
                    return;
                };
                ui.begin(size, scale);
                controls.frame(ui, &model, &pointer, dt);
                stats_panel.frame(
                    ui,
                    &model.theme,
                    model.lang,
                    stats,
                    stats_open,
                    dt,
                    animations,
                );
                if rec.is_on() {
                    dockview_ui::rec::draw(
                        ui,
                        &model.theme,
                        rec.seconds,
                        rec.state == RecState::Starting,
                        animations,
                    );
                }
                toasts.frame(ui, &model.theme, dt, animations);
                if let Err(e) = gpu.render(context, &ui.draw_list()) {
                    eprintln!("ui draw failed: {e}");
                }
            },
            capture,
        );
        match result {
            Ok(Some(shot)) => self.save_screenshot(&shot),
            Ok(None) => {}
            Err(e) => {
                eprintln!("present failed: {e}");
                graphics_lost |= is_device_lost(&e);
            }
        }
        graphics_lost |= self.renderer.as_ref().is_some_and(Renderer::device_lost);
        if graphics_lost {
            self.recreate_graphics();
        }
        if let Some(arrived) = arrived {
            self.core
                .note_latency(arrived.elapsed().as_secs_f64() * 1000.0);
        }

        self.pointer.end_frame();
        for action in self.controls.take_actions() {
            self.core.apply(action);
        }
        self.apply_host_requests();
        self.update_cursor();
    }

    pub(super) fn save_screenshot(&mut self, shot: &Screenshot) {
        self.exit_requested = true;
        let Some(path) = self.core.cli().screenshot.clone() else {
            return;
        };
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            let _ = std::fs::create_dir_all(dir);
        }
        let result = std::fs::File::create(&path)
            .map_err(|e| e.to_string())
            .and_then(|file| {
                let mut encoder =
                    png::Encoder::new(std::io::BufWriter::new(file), shot.width, shot.height);
                encoder.set_color(png::ColorType::Rgba);
                encoder.set_depth(png::BitDepth::Eight);
                encoder
                    .write_header()
                    .and_then(|mut w| w.write_image_data(&shot.rgba))
                    .map_err(|e| e.to_string())
            });
        match result {
            Ok(()) => println!("saved {}", path.display()),
            Err(e) => eprintln!("could not save {}: {e}", path.display()),
        }
    }
}
