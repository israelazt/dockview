//! The window system's callbacks: start-up, events and the idle tick.

use std::ffi::c_void;

use dockview_app::keys::KeyCommand;
use dockview_platform::render::Renderer;
use windows::Win32::Foundation::HWND;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::{Window, WindowId};

use super::helpers::appearance_of;
use super::keys::command_for;
use super::*;

impl ApplicationHandler for Shell {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let title = if self.core.cli().ui_preview {
            "Dockview — UI preview"
        } else {
            "Dockview"
        };
        let mut attributes = Window::default_attributes().with_title(title);
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
                renderer.set_filter(filter_for(self.core.render_preset()));
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
        self.ui_scale = self
            .core
            .cli()
            .scale
            .unwrap_or(window.scale_factor() as f32);
        self.core
            .set_system_appearance(appearance_of(window.theme()));
        self.host.borrow_mut().hwnd = Some(hwnd);
        self.window = Some(window);
        self.refresh_host_info();
        // A screen started from the command line began before this window
        // existed, so hide the window from it now.
        self.core.apply_capture_exclusion();
        self.core.restore_fullscreen();
        self.apply_host_requests();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                self.core.save_settings();
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
            WindowEvent::ThemeChanged(theme) => {
                self.core.set_system_appearance(appearance_of(Some(theme)))
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.ui_scale = self.core.cli().scale.unwrap_or(scale_factor as f32);
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                if let Some(command) = command_for(&event.logical_key) {
                    // Escape closes an open popover first.
                    let closed_a_popover =
                        command == KeyCommand::Escape && self.controls.close_settings();
                    if !closed_a_popover {
                        self.core.on_key(command);
                    }
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
        if self.renderer.is_none()
            && self.window.is_some()
            && self.last_graphics_try.elapsed() >= GRAPHICS_RETRY
        {
            self.recreate_graphics();
        }
        self.refresh_host_info();
        self.core.tick(self.controls.sources_open());
        self.show_events();
        self.apply_host_requests();
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
        let animating =
            self.controls.is_visible() || self.toasts.is_visible() || self.core.rec_is_on();
        if new_frame || animating || std::mem::take(&mut self.needs_redraw) {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }
}
