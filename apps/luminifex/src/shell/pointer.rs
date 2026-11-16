//! The mouse and the cursor of the window: which part of the window a press
//! belongs to, and the shape of the cursor over each part.

use dockview_domain::window_chrome::Zone;
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
                    !matches!(chrome.zone_at(x, y), Zone::Content | Zone::Nav | Zone::Rail)
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
        // Settings has a view of its own: only it and the tabs can be pointed at.
        let pointer = if self.core.mode() == dockview_domain::navigation::AppMode::Settings {
            self.settings_page.wants_pointer() || self.navbar.wants_pointer()
        } else {
            self.controls.wants_pointer()
                || self.studio.wants_pointer()
                || self.side.wants_pointer()
                || self.canvas_bar.wants_pointer()
                || self.navbar.wants_pointer()
                || self.hud.wants_pointer()
                || self.source_picker.wants_pointer()
                || self.rail.wants_pointer()
        };
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
        let hidden = self.motion.layout_fullscreen() && !self.controls.is_visible();
        if hidden != self.cursor_hidden {
            self.cursor_hidden = hidden;
            window.set_cursor_visible(!hidden);
        }
    }
}
