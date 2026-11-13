//! The window: drawing a frame, the cursor, full screen and screenshots.

use super::*;

impl App {
    pub(super) fn window_hwnd(&self) -> Option<HWND> {
        match self.window.as_ref()?.window_handle().map(|h| h.as_raw()) {
            Ok(RawWindowHandle::Win32(h)) => Some(HWND(h.hwnd.get() as *mut c_void)),
            _ => None,
        }
    }

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
        let hidden = self.fullscreen && !self.controls.is_visible();
        if hidden != self.cursor_hidden {
            self.cursor_hidden = hidden;
            window.set_cursor_visible(!hidden);
        }
    }

    pub(super) fn render(&mut self) {
        let model = self.model();
        let rec = self.rec_view();
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;
        let pointer = self.pointer;
        let scale = self.ui_scale;
        // The first seconds include start-up work; wait for steady numbers.
        let settled = !self.cli.show_stats || self.telemetry.windows() >= 3;
        let capture =
            self.cli.screenshot.is_some() && self.rendered >= SCREENSHOT_AFTER_FRAMES && settled;

        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        // Wait for a free present slot *before* picking the frame so we always
        // show the freshest one.
        renderer.wait_for_present_slot(100);
        let mut arrived = None;
        if let Some(frame) = self.slot.take_latest() {
            arrived = Some(frame.arrived);
            self.video_size = (frame.width, frame.height);
            if let Err(e) = renderer.upload(&frame) {
                eprintln!("upload failed: {e}");
            }
            self.slot.recycle(frame);
            self.rendered += 1;
        }

        let (ui, ui_gpu, controls) = (&mut self.ui, &mut self.ui_gpu, &mut self.controls);
        let (stats_panel, stats) = (&mut self.stats_panel, &self.stats);
        let (stats_open, animations) = (self.stats_open, self.animations);
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
                    ui::rec::draw(
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
            Err(e) => eprintln!("present failed: {e}"),
        }
        if let Some(arrived) = arrived {
            self.telemetry
                .note_latency(arrived.elapsed().as_secs_f64() * 1000.0);
        }

        self.pointer.end_frame();
        for action in self.controls.take_actions() {
            self.apply(action);
        }
        self.update_cursor();
    }

    pub(super) fn save_screenshot(&mut self, shot: &render::Screenshot) {
        self.exit_requested = true;
        let Some(path) = &self.cli.screenshot else {
            return;
        };
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            let _ = std::fs::create_dir_all(dir);
        }
        let result = std::fs::File::create(path)
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

    pub(super) fn set_fullscreen(&mut self, on: bool) {
        self.fullscreen = on;
        if let Some(window) = &self.window {
            window.set_fullscreen(on.then_some(Fullscreen::Borderless(None)));
        }
        self.save_settings();
    }
}
