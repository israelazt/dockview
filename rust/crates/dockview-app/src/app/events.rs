//! The window system's callbacks: start-up, events and the idle tick.

use super::*;

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let title = if self.cli.ui_preview {
            "Dockview — UI preview"
        } else {
            "Dockview"
        };
        let mut attributes = Window::default_attributes().with_title(title);
        attributes = match self.cli.scale {
            Some(scale) => attributes.with_inner_size(PhysicalSize::new(
                (1280.0 * scale).round(),
                (720.0 * scale).round(),
            )),
            None => attributes.with_inner_size(LogicalSize::new(1280.0, 720.0)),
        };
        let window = match event_loop.create_window(attributes) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("could not create window: {e}");
                event_loop.exit();
                return;
            }
        };

        let hwnd = match window.window_handle().map(|h| h.as_raw()) {
            Ok(RawWindowHandle::Win32(h)) => HWND(h.hwnd.get() as *mut c_void),
            _ => {
                eprintln!("window has no Win32 handle");
                event_loop.exit();
                return;
            }
        };
        let size = window.inner_size();
        match Renderer::new(hwnd, size.width, size.height) {
            Ok(mut renderer) => {
                renderer.set_filter(filter_for(self.settings.render_preset));
                match UiGpu::new(renderer.device()) {
                    Ok(gpu) => self.ui_gpu = Some(gpu),
                    Err(e) => eprintln!("could not initialise the interface renderer: {e}"),
                }
                self.renderer = Some(renderer);
            }
            Err(e) => {
                eprintln!("could not initialise Direct3D: {e}");
                event_loop.exit();
                return;
            }
        }
        self.ui_scale = self.cli.scale.unwrap_or(window.scale_factor() as f32);
        self.system_appearance = appearance_of(window.theme());
        self.window = Some(window);
        // A screen started from the command line began before this window
        // existed, so hide the window from it now.
        self.set_capture_exclusion(self.wants_exclusion());
        if self.settings.was_fullscreen {
            self.set_fullscreen(true);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                self.save_settings();
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                self.needs_redraw = true;
                if let Some(renderer) = self.renderer.as_mut() {
                    if let Err(e) = renderer.resize(size.width, size.height) {
                        eprintln!("resize failed: {e}");
                    }
                }
            }
            WindowEvent::RedrawRequested => self.render(),
            WindowEvent::CursorMoved { position, .. } => {
                let scale = f64::from(self.ui_scale);
                self.pointer.pos = Some(((position.x / scale) as f32, (position.y / scale) as f32));
                self.needs_redraw = true;
            }
            WindowEvent::CursorLeft { .. } => {
                self.pointer.pos = None;
                self.needs_redraw = true;
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                match state {
                    ElementState::Pressed => self.pointer.press(),
                    ElementState::Released => self.pointer.release(),
                }
                self.needs_redraw = true;
            }
            WindowEvent::MouseWheel { delta, .. } => {
                self.pointer.wheel += match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => (p.y / 40.0) as f32,
                };
                self.needs_redraw = true;
            }
            WindowEvent::ThemeChanged(theme) => self.system_appearance = appearance_of(Some(theme)),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.ui_scale = self.cli.scale.unwrap_or(scale_factor as f32);
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                self.on_key(&event.logical_key);
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
        self.maintain_source();
        self.maintain_overlays();
        self.tick_transition();
        if self.controls.sources_open() && self.sources_refreshed.elapsed() > SOURCES_REFRESH {
            self.refresh_sources();
        }
        self.update_stats();
        self.poll_replay();
        // Redraw for new video, for input, and while the controls are on
        // screen (hover, drag and fade animations need steady frames).
        let new_frame = self.slot.wait_for_frame(FRAME_WAIT);
        let animating = self.controls.is_visible()
            || self.toasts.is_visible()
            || self.replay.rec_view().is_on();
        if new_frame || animating || std::mem::take(&mut self.needs_redraw) {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }
}
