//! The window system's callbacks: start-up, events and the idle tick.

use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::window::{Window, WindowId};

use super::helpers::appearance_of;
use super::keys::{command_with, text_key_for};
use super::present::StartupFailure;
use super::*;

impl ApplicationHandler for Shell {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let title = if self.core.cli().ui_preview {
            format!("{} — UI preview", dockview_domain::brand::APP_NAME)
        } else {
            dockview_domain::brand::APP_NAME.to_string()
        };
        // The window has no system title bar: the program draws its own.
        let mut attributes =
            crate::os::style_window(Window::default_attributes().with_title(title));
        attributes = match self.core.cli().scale {
            Some(scale) => attributes.with_inner_size(PhysicalSize::new(
                (1280.0 * scale).round(),
                (720.0 * scale).round(),
            )),
            None => attributes.with_inner_size(LogicalSize::new(1280.0, 720.0)),
        };
        let window = match event_loop.create_window(attributes) {
            Ok(w) => w,
            Err(e) => {
                dockview_domain::log_error!("could not create window: {e}");
                self.fail_startup(StartupFailure::Window, &e.to_string());
                event_loop.exit();
                return;
            }
        };

        dockview_domain::log_info!("window created");
        crate::os::round_corners(&window);
        match crate::os::create_presenter(&window) {
            Ok(mut presenter) => {
                presenter.set_filter(filter_for(self.core.render_preset()));
                self.presenter = Some(presenter);
            }
            Err(e) => {
                dockview_domain::log_error!("could not initialise Direct3D: {e}");
                self.fail_startup(StartupFailure::Graphics, &e.to_string());
                event_loop.exit();
                return;
            }
        }
        dockview_domain::log_info!("renderer ready");
        self.ui_scale = self
            .core
            .cli()
            .scale
            .unwrap_or(window.scale_factor() as f32);
        self.core
            .set_system_appearance(appearance_of(window.theme()));
        self.host.borrow_mut().native = crate::os::native_window(&window);
        self.window = Some(window);
        self.refresh_host_info();
        // A screen started from the command line began before this window
        // existed, so hide the window from it now.
        self.core.apply_capture_exclusion();
        self.core.restore_fullscreen();
        self.apply_host_requests();
        // Started with `--tray`: only the icon, until someone opens the window.
        if self.core.cli().tray && self.core.keeps_in_tray() {
            self.hide_window();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => self.request_close(event_loop),
            WindowEvent::Resized(size) => {
                self.needs_redraw = true;
                if let Some(presenter) = self.presenter.as_mut() {
                    if let Err(e) = presenter.resize(size.width, size.height) {
                        dockview_domain::log_warn!("resize failed: {e}");
                    }
                }
            }
            WindowEvent::RedrawRequested => self.render(),
            WindowEvent::CursorMoved { position, .. } => {
                let scale = f64::from(self.ui_scale);
                self.pointer.pos = Some(((position.x / scale) as f32, (position.y / scale) as f32));
                self.cursor_phys = Some((position.x as f32, position.y as f32));
                let chrome = self.chrome();
                self.chrome_input.moved(&chrome, self.cursor_phys);
                self.needs_redraw = true;
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                let state = modifiers.state();
                self.ctrl = state.control_key();
                self.edit_mods = dockview_domain::layer_edit::Modifiers {
                    keep_aspect: state.shift_key(),
                    no_snap: state.alt_key(),
                };
            }
            WindowEvent::CursorLeft { .. } => {
                self.pointer.pos = None;
                self.cursor_phys = None;
                self.chrome_input.cancel();
                self.needs_redraw = true;
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => self.left_button(event_loop, state),
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Right,
                ..
            } => {
                let chrome = self.chrome();
                let command = self
                    .cursor_phys
                    .and_then(|pos| self.chrome_input.press_right(&chrome, pos));
                if let Some(command) = command {
                    self.run_window_command(event_loop, command);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                self.pointer.wheel += match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => (p.y / 40.0) as f32,
                };
                self.needs_redraw = true;
            }
            WindowEvent::ThemeChanged(theme) => {
                self.core.set_system_appearance(appearance_of(Some(theme)))
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.ui_scale = self.core.cli().scale.unwrap_or(scale_factor as f32);
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                if self.studio.is_editing() {
                    // A name is being typed: the keys are for the field and not
                    // for the shortcuts (F, Tab, the arrows...).
                    let text = event.text.as_deref();
                    if let Some(key) = text_key_for(&event.logical_key, text) {
                        self.studio.key(key);
                    }
                } else if let Some(command) = command_with(&event.logical_key, self.ctrl) {
                    self.core.on_key(command);
                }
                self.apply_host_requests();
                self.needs_redraw = true;
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.exit_requested {
            event_loop.exit();
            return;
        }
        if self.presenter.is_none()
            && self.window.is_some()
            && self.last_graphics_try.elapsed() >= GRAPHICS_RETRY
        {
            self.recreate_graphics();
        }
        self.refresh_host_info();
        let lists_shown =
            self.studio.shows(StudioSection::Sources) || self.studio.shows(StudioSection::Scenes);
        self.core.tick(lists_shown);
        self.show_events();
        self.apply_host_requests();
        if self.hidden {
            // Only in the tray: the hotkey and the icon are polled by the tick
            // above a few times a second, and no picture is drawn. The latest
            // picture is taken so the sources keep flowing.
            let slot = self.core.output_slot();
            if let Some(frame) = slot.take_latest() {
                slot.recycle(frame);
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + HIDDEN_TICK));
            return;
        }
        event_loop.set_control_flow(ControlFlow::Poll);
        // Redraw for new video, for input, and while the controls are on
        // screen (hover, drag and fade animations need steady frames).
        let mut new_frame = self.core.output_slot().wait_for_frame(FRAME_WAIT);
        if new_frame && !self.video_frame_due() {
            // The preset shows fewer pictures than arrive: this one is skipped
            // here (taken, not dropped), so it is not counted as lost.
            let slot = self.core.output_slot();
            if let Some(frame) = slot.take_latest() {
                slot.recycle(frame);
            }
            new_frame = false;
        }
        let animating = self.controls.is_visible()
            || self.toasts.is_visible()
            || self.core.rec_is_on()
            || self.studio.is_animating();
        if new_frame || animating || std::mem::take(&mut self.needs_redraw) {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }
}
