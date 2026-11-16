//! The window: drawing a frame, the cursor and screenshots.

use dockview_domain::presenter::Screenshot;
use dockview_domain::render_plan::{render_plan, video_frame_due};
use dockview_domain::view::RecState;
use dockview_ui::input::Pointer;
use dockview_ui::studio::{route_pointer, Scope};
use dockview_ui::titlebar::TitleBarState;

use super::*;

impl Shell {
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
        let settings_mode = model.mode == dockview_domain::navigation::AppMode::Settings;
        let rec = model.rec;
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;
        // Going to full screen and back: the clock of the motion, and the
        // change of the window when the frame has gone out.
        if let Some(change) = self.motion.advance(dt, self.video_desktop) {
            self.set_window_fullscreen(change == fs_motion::Change::Enter);
        }
        // The Studio panel opens (and in full screen, goes at once) and takes
        // its room; the pointer belongs to the video or to the panel.
        // The panel closes as full screen is asked for (animated while the
        // window has not changed yet) and opens as it is left.
        let panel_visible = model.studio_visible();
        let instant = self.motion.layout_fullscreen();
        self.studio.update(panel_visible, instant, dt);
        self.side.update(panel_visible, instant, dt);
        self.studio_openness = self.studio.openness();
        // The rail slides with the panel, so the frame is read after it moved.
        let chrome = self.chrome();
        let bar_px = chrome.bar_height();
        let bar_state = TitleBarState {
            hover: self.chrome_input.hover(),
            pressed: self.chrome_input.pressed(),
            maximized: chrome.maximized,
        };
        let pointer = self.content_pointer(&chrome);
        let nav_pointer = self.nav_pointer(&chrome);
        let rail_pointer = self.rail_pointer(&chrome);
        let scale = self.ui_scale;
        let window_size = self.window.as_ref().map_or((1, 1), |w| {
            let size = w.inner_size();
            (size.width, size.height)
        });
        // Below the title bar and the navigation row: the content has the rest
        // of the window.
        let top_px = bar_px + chrome.nav_height();
        let rail_px = chrome.rail_width();
        // The shortcuts strip sits above the content, right of the rail.
        let strip_px = chrome.strip_height();
        let content_top_px = top_px + strip_px;
        let content_size = (
            (window_size.0 as f32 - rail_px).max(1.0) as u32,
            (window_size.1 as f32 - content_top_px).max(1.0) as u32,
        );
        let place = studio::place(content_size, scale, self.studio.openness());
        // With the left column out, the panel keeps the other four sections.
        self.studio.set_scope(if place.side.is_some() {
            Scope::Main
        } else {
            Scope::Full
        });
        let (rest_pointer, panel_pointer) = route_pointer(&pointer, place.panel);
        let (rest_pointer, side_pointer) = route_pointer(&rest_pointer, place.side);
        let (video_pointer, bar_pointer) = route_pointer(&rest_pointer, place.bar);
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
        let area = picture::video_area(
            &place,
            (rail_px, content_top_px),
            content_top_px > 0.0 || place.panel.is_some(),
            settings_mode,
        );
        // Going to full screen and back: while the picture is between two
        // places it is drawn in between; at rest its place is remembered, so
        // the next motion starts from it (all in desktop coordinates, because
        // the window changes place and size in between).
        let inner = self
            .window
            .as_ref()
            .and_then(|w| w.inner_position().ok())
            .map_or((0.0, 0.0), |p| (p.x as f32, p.y as f32));
        let (shown, at_rest_desktop) =
            self.motion
                .place_picture(area, (window_size.0 as f32, window_size.1 as f32), inner);
        if !self.motion.is_blending() {
            self.video_desktop = at_rest_desktop;
        }
        presenter.set_video_area(shown);
        // The frame fades with the motion (the panel has its own opening on top).
        let frame_opacity = self.motion.frame_opacity();
        let fade = self.studio_openness * frame_opacity;
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

        // The icon of the section the panel is showing, if it shows one.
        let rail_active = model.studio_visible().then(|| {
            [
                StudioSection::Sources,
                StudioSection::Audio,
                StudioSection::Stats,
                StudioSection::Scenes,
            ]
            .into_iter()
            .find(|s| self.studio.shows(*s) || self.side.shows(*s))
        });
        let rail_active = rail_active.flatten();
        let (ui, controls) = (&mut self.ui, &mut self.controls);
        let (scene_editor, edit_mods) = (&mut self.scene_editor, &self.edit_mods);
        let studio = &mut self.studio;
        let side = &mut self.side;
        let hud = &mut self.hud;
        let source_picker = &mut self.source_picker;
        // The readouts show in the viewer: no panel, not in full screen.
        let hud_show = model.hud && !model.mode.shows_panel() && !model.fullscreen;
        let navbar = &mut self.navbar;
        let rail = &mut self.rail;
        let strip = &mut self.strip;
        let stats_panel = &mut self.stats_panel;
        let canvas_bar = &mut self.canvas_bar;
        let (stats, stats_open) = (self.core.stats(), self.core.stats_open());
        let animations = self.animations;
        let toasts = &mut self.toasts;
        let settings_page = &mut self.settings_page;
        {
            {
                ui.begin(presenter.size(), scale);
                ui.set_inset_top(content_top_px / scale.max(0.1));
                ui.set_inset_left(rail_px / scale.max(0.1));
                if settings_mode {
                    let (w, h) = ui.logical_size();
                    settings_page.frame(
                        ui,
                        &model,
                        dockview_ui::Rect::new(0.0, 0.0, w, h),
                        &pointer,
                        dt,
                    );
                    toasts.frame(ui, &model.theme, dt, animations);
                } else {
                    // With the Studio open, the air around the two cards and a fine
                    // frame around the video.
                    studio::draw_card_frame(
                        ui,
                        &model.theme,
                        &place,
                        (
                            content_size.0 as f32 / scale.max(0.1),
                            content_size.1 as f32 / scale.max(0.1),
                        ),
                        fade,
                    );
                    studio::draw_status(ui, &model, stats, &place, fade);
                    studio::draw_bar(ui, canvas_bar, &model, &place, &bar_pointer, fade);
                    // What floats over the video anchors to the video's room.
                    // The layer editor sits on the picture, under the controls; what
                    // the controls (the pill) use is not the editor's.
                    let over_list = source_picker.covers(
                        video_pointer
                            .pos
                            .map(|(x, y)| (x - place.video_logical.x, y - place.video_logical.y)),
                    );
                    let editor_pointer = if controls.over_controls() || over_list {
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
                    // From here on, what floats over the picture is drawn and
                    // pointed at relative to the video card.
                    let vl = place.video_logical;
                    let (rail_l, top_l) =
                        (rail_px / scale.max(0.1), content_top_px / scale.max(0.1));
                    let float_pointer = Pointer {
                        pos: video_pointer.pos.map(|(x, y)| (x - vl.x, y - vl.y)),
                        ..video_pointer
                    };
                    ui.set_inset_left(rail_l + vl.x);
                    ui.set_inset_top(top_l + vl.y);
                    ui.limit_width(Some(place.video_width));
                    // The pill anchors to the bottom of the video, above the
                    // transition bar, not to the window's.
                    ui.limit_height(Some(vl.h));
                    // Under the list the bar must not take the pointer either.
                    let bar_pointer = if source_picker.covers(float_pointer.pos) {
                        Pointer {
                            pos: None,
                            ..float_pointer
                        }
                    } else {
                        float_pointer
                    };
                    controls.frame(ui, &model, &bar_pointer, dt);
                    let hud_h =
                        hud.frame(ui, &model, stats, &float_pointer, hud_show, dt, animations);
                    if hud.take_source_click() {
                        source_picker.toggle();
                    }
                    if !hud_show {
                        source_picker.close();
                    }
                    // What anchors to the top sits under the readouts.
                    ui.set_inset_top(top_l + vl.y + hud_h);
                    ui.limit_height(Some((vl.h - hud_h).max(0.0)));
                    stats_panel.frame(
                        ui,
                        &model.theme,
                        model.lang,
                        stats,
                        &float_pointer,
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
                    // The list of sources, over the rest of what floats.
                    ui.set_inset_top(top_l + vl.y);
                    ui.limit_height(Some(vl.h));
                    source_picker.frame(ui, &model, &float_pointer, hud.source_card());
                    ui.set_inset_top(top_l);
                    ui.set_inset_left(rail_l);
                    // The panel and the left column, on top, in the room they took.
                    ui.limit_width(None);
                    ui.limit_height(None);
                    side.frame(
                        ui,
                        &model,
                        stats,
                        place.side,
                        place.side_full_width,
                        &side_pointer,
                        dt,
                    );
                    studio.frame(
                        ui,
                        &model,
                        stats,
                        place.panel,
                        place.full_width,
                        &panel_pointer,
                        dt,
                    );
                }
                ui.set_opacity(frame_opacity);
                frame::draw(
                    ui,
                    &model,
                    &chrome,
                    frame::Parts {
                        navbar,
                        rail,
                        strip,
                        nav_pointer: &nav_pointer,
                        rail_pointer: &rail_pointer,
                        rail_active,
                        bar_state: &bar_state,
                    },
                    scale,
                    dt,
                );
                ui.set_opacity(1.0);
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

        self.hud_moving = self.hud.is_animating(hud_show);
        self.rendered_any = true;
        self.pointer.end_frame();
        self.hand_over_actions();
        if self.rail.take_log_request() {
            if let Some(path) = crate::os::default_log_path() {
                crate::os::reveal_file(&path);
            }
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
