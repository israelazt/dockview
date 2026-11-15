//! The window: drawing a frame, the cursor and screenshots.

use dockview_domain::layout::Rect as PhysicalRect;
use dockview_domain::presenter::Screenshot;
use dockview_domain::render_plan::{render_plan, video_frame_due};
use dockview_domain::view::RecState;
use dockview_domain::window_chrome::Zone;
use dockview_ui::input::Pointer;
use dockview_ui::studio::route_pointer;
use dockview_ui::titlebar::TitleBarState;
use winit::event::ElementState;
use winit::event_loop::ActiveEventLoop;
use winit::window::CursorIcon;

use super::*;

impl Shell {
    /// The left mouse button: the window's frame takes the presses that start on
    /// it (and the release that ends them); everything else is for the content.
    pub(super) fn left_button(&mut self, event_loop: &ActiveEventLoop, state: ElementState) {
        let chrome = self.chrome();
        let pos = self.cursor_phys;
        let command = match state {
            ElementState::Pressed => {
                let now_ms = self.started.elapsed().as_millis() as u64;
                let command = pos.and_then(|p| self.chrome_input.press(&chrome, p, now_ms));
                // The navigation row is the interface's own, like the content.
                let on_frame = pos.is_some_and(|(x, y)| {
                    !matches!(chrome.zone_at(x, y), Zone::Content | Zone::Nav)
                });
                if on_frame {
                    self.chrome_held = true;
                } else {
                    self.pointer.press();
                }
                command
            }
            ElementState::Released => {
                let command = self.chrome_input.release(&chrome, pos);
                if std::mem::take(&mut self.chrome_held) {
                    // The content never saw the press, so it does not see the release.
                } else {
                    self.pointer.release();
                }
                command
            }
        };
        if let Some(command) = command {
            self.run_window_command(event_loop, command);
        }
        self.needs_redraw = true;
    }

    /// Pointer shape and visibility follow what the controls are doing.
    pub(super) fn update_cursor(&mut self) {
        let Some(window) = &self.window else {
            return;
        };
        let pointer = self.controls.wants_pointer()
            || self.studio.wants_pointer()
            || self.navbar.wants_pointer();
        let frame = self
            .cursor_phys
            .map(|(x, y)| self.chrome().zone_at(x, y).cursor())
            .map_or(CursorIcon::Default, super::cursor::chrome_cursor);
        let icon = match self.scene_editor.hover() {
            _ if frame != CursorIcon::Default => frame,
            Some(handle) if !pointer => {
                super::cursor::cursor_for(handle, self.scene_editor.is_dragging())
            }
            _ if pointer => CursorIcon::Pointer,
            _ => CursorIcon::Default,
        };
        if icon != self.cursor_icon {
            self.cursor_icon = icon;
            window.set_cursor(icon);
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
        let Some(window) = &self.window else {
            return;
        };
        dockview_domain::log_warn!("the graphics device was lost: building the renderer again");
        // The old swap chain has to go before a new one can use the window.
        self.presenter = None;
        self.last_graphics_try = Instant::now();
        match crate::os::create_presenter(window) {
            Ok(mut presenter) => {
                presenter.set_filter(filter_for(self.core.render_preset()));
                // The glyphs lived in a texture of the lost device.
                self.ui.text.reset_atlas();
                self.presenter = Some(presenter);
            }
            Err(e) => dockview_domain::log_warn!("could not build the renderer yet: {e}"),
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
        // Hidden in the tray: nothing to draw.
        if self.hidden {
            return;
        }
        self.refresh_host_info();
        let model = self.core.model();
        let rec = model.rec;
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;
        let chrome = self.chrome();
        let bar_px = chrome.bar_height();
        let bar_state = TitleBarState {
            hover: self.chrome_input.hover(),
            pressed: self.chrome_input.pressed(),
            maximized: chrome.maximized,
        };
        let pointer = self.content_pointer(&chrome);
        let nav_pointer = self.nav_pointer(&chrome);
        let scale = self.ui_scale;
        // The Studio panel opens (and in full screen, goes at once) and takes
        // its room; the pointer belongs to the video or to the panel.
        self.studio
            .update(model.studio_visible(), model.fullscreen, dt);
        let window_size = self.window.as_ref().map_or((1, 1), |w| {
            let size = w.inner_size();
            (size.width, size.height)
        });
        // Below the title bar and the navigation row: the content has the rest
        // of the window.
        let top_px = bar_px + chrome.nav_height();
        let content_size = (
            window_size.0,
            (window_size.1 as f32 - top_px).max(1.0) as u32,
        );
        let place = studio::place(content_size, scale, self.studio.openness());
        let (video_pointer, panel_pointer) = route_pointer(&pointer, place.panel);
        // The first seconds include start-up work; wait for steady numbers.
        let settled = !self.core.cli().show_stats || self.core.measurement_windows() >= 3;
        let capture = self.core.cli().screenshot.is_some()
            && self.core.rendered_frames() >= SCREENSHOT_AFTER_FRAMES
            && settled;

        let Some(presenter) = self.presenter.as_mut() else {
            return;
        };
        // Wait for a free present slot *before* picking the frame so we always
        // show the freshest one.
        presenter.wait_for_slot(100);
        // The video fits the room the panel leaves (all of it when closed).
        // With the bar, the video always has an area of its own, under it.
        let area = (top_px > 0.0 || place.panel.is_some()).then_some(PhysicalRect {
            y: place.video.y + top_px,
            ..place.video
        });
        presenter.set_video_area(area);
        let mut arrived = None;
        let mut graphics_lost = false;
        let slot = Arc::clone(self.core.output_slot());
        if let Some(frame) = slot.take_latest() {
            arrived = Some(frame.arrived);
            if let Err(e) = presenter.upload(&frame) {
                dockview_domain::log_warn!("upload failed: {e}");
                graphics_lost |= e.device_lost;
            }
            self.core.frame_uploaded((frame.width, frame.height));
            if self.core.rendered_frames() == 1 {
                dockview_domain::log_info!("first picture shown");
            }
            slot.recycle(frame);
            self.last_video_shown = now;
        }

        let (ui, controls) = (&mut self.ui, &mut self.controls);
        let (scene_editor, edit_mods) = (&mut self.scene_editor, &self.edit_mods);
        let studio = &mut self.studio;
        let navbar = &mut self.navbar;
        let stats_panel = &mut self.stats_panel;
        let (stats, stats_open) = (self.core.stats(), self.core.stats_open());
        let animations = self.animations;
        let toasts = &mut self.toasts;
        {
            {
                ui.begin(presenter.size(), scale);
                ui.set_inset_top(top_px / scale.max(0.1));
                // What floats over the video anchors to the video's room.
                // The layer editor sits on the picture, under the controls; what
                // the controls (the pill) use is not the editor's.
                let editor_pointer = if controls.over_controls() {
                    Pointer {
                        pos: None,
                        wheel: 0.0,
                        just_pressed: false,
                        ..video_pointer
                    }
                } else {
                    video_pointer
                };
                let v = place.video;
                let area = (v.x / scale, v.y / scale, v.w / scale, v.h / scale);
                scene_editor.frame(ui, &model, area, &editor_pointer, *edit_mods);
                ui.limit_width(Some(place.video_width));
                controls.frame(ui, &model, &video_pointer, dt);
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
                // The panel, on top, in the room it took.
                ui.limit_width(None);
                studio.frame(
                    ui,
                    &model,
                    stats,
                    place.panel,
                    place.full_width,
                    &panel_pointer,
                    dt,
                );
                // The tabs and the status, under the title bar.
                if bar_px > 0.0 {
                    ui.set_inset_top(bar_px / scale.max(0.1));
                    navbar.frame(ui, &model, &nav_pointer);
                }
                // The window's own title bar, over everything.
                ui.set_inset_top(0.0);
                if bar_px > 0.0 {
                    dockview_ui::titlebar::draw(
                        ui,
                        &model.theme,
                        model.lang,
                        &bar_state,
                        &dockview_domain::brand::version_label(crate::os::renderer_name()),
                    );
                }
            }
        }
        match presenter.present(&ui.draw_list(), capture) {
            Ok(Some(shot)) => self.save_screenshot(&shot),
            Ok(None) => {}
            Err(e) => {
                dockview_domain::log_warn!("present failed: {e}");
                graphics_lost |= e.device_lost;
            }
        }
        graphics_lost |= self.presenter.as_ref().is_some_and(|p| p.device_lost());
        if graphics_lost {
            self.recreate_graphics();
        }
        if let Some(arrived) = arrived {
            self.core
                .note_latency(arrived.elapsed().as_secs_f64() * 1000.0);
        }

        self.pointer.end_frame();
        let actions = self.controls.take_actions();
        if let Some(section) = self
            .controls
            .take_studio_request()
            .or_else(|| self.navbar.take_studio_request())
        {
            self.studio.open_section(section);
        }
        let editor = self.scene_editor.take_actions();
        for action in actions
            .into_iter()
            .chain(self.navbar.take_actions())
            .chain(self.studio.take_actions())
            .chain(editor)
        {
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
            Err(e) => dockview_domain::log_error!("could not save {}: {e}", path.display()),
        }
    }
}
