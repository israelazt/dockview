//! Pointer state fed to the controls once per frame.

/// Mouse state in logical pixels.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Pointer {
    /// `None` when the cursor is outside the window.
    pub pos: Option<(f32, f32)>,
    /// Primary button is held.
    pub down: bool,
    /// Primary button went down since the last frame.
    pub just_pressed: bool,
    /// Primary button went up since the last frame.
    pub just_released: bool,
    /// Scroll notches since the last frame; positive is up.
    pub wheel: f32,
}

impl Pointer {
    /// Clears the one-frame edge events. Call after each rendered frame.
    pub fn end_frame(&mut self) {
        self.just_pressed = false;
        self.just_released = false;
        self.wheel = 0.0;
    }

    pub fn press(&mut self) {
        self.down = true;
        self.just_pressed = true;
    }

    pub fn release(&mut self) {
        self.down = false;
        self.just_released = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edges_last_one_frame_but_the_held_state_persists() {
        let mut p = Pointer::default();
        p.press();
        assert!(p.down && p.just_pressed);
        p.end_frame();
        assert!(p.down && !p.just_pressed);
        p.release();
        assert!(!p.down && p.just_released);
        p.end_frame();
        assert!(!p.just_released);
    }

    #[test]
    fn end_frame_clears_the_wheel() {
        let mut p = Pointer {
            wheel: 3.0,
            ..Pointer::default()
        };
        p.end_frame();
        assert_eq!(p.wheel, 0.0);
    }
}
