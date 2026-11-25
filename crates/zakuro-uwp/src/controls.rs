//! an Xbox controller as the console's buttons, and the app's own shortcuts.
//!
//! the face buttons go by where they sit, not by their letters: the Xbox
//! controller's bottom button is the console's bottom button, B, and so on.
//!
//! | Xbox                  | 3DS                                  |
//! |-----------------------|--------------------------------------|
//! | A / B / X / Y         | B / A / Y / X                         |
//! | LB / RB               | L / R                                 |
//! | LT / RT               | ZL / ZR                               |
//! | Left stick, D-pad     | Circle Pad, D-pad                     |
//! | Right stick           | C-Stick                               |
//! | Menu / View           | START / SELECT                        |
//! | Right stick click     | touch cursor on or off                |
//!
//! while the touch cursor is on, the right stick moves it over the bottom
//! screen and RT touches. View held with another button is a shortcut, and
//! then sends no SELECT: View + LB changes the screen layout, View + RB shows
//! or hides the frame rate, View + Menu pauses or resumes.

use zakuro_core::services::hid::{InputState, PadState};

/// how far a stick leans before it counts.
const DEADZONE: f32 = 0.2;
/// how far a trigger is pulled before it counts.
const TRIGGER: f32 = 0.35;
/// touch cursor speed, in bottom screen pixels a frame at full lean.
const CURSOR_SPEED: f32 = 6.0;

/// the controller's buttons, as Windows.Gaming.Input numbers them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Buttons(pub u32);

#[allow(dead_code)] // the full table, some only named
impl Buttons {
    pub const MENU: u32 = 0x1;
    pub const VIEW: u32 = 0x2;
    pub const A: u32 = 0x4;
    pub const B: u32 = 0x8;
    pub const X: u32 = 0x10;
    pub const Y: u32 = 0x20;
    pub const DPAD_UP: u32 = 0x40;
    pub const DPAD_DOWN: u32 = 0x80;
    pub const DPAD_LEFT: u32 = 0x100;
    pub const DPAD_RIGHT: u32 = 0x200;
    pub const LEFT_SHOULDER: u32 = 0x400;
    pub const RIGHT_SHOULDER: u32 = 0x800;
    pub const LEFT_THUMBSTICK: u32 = 0x1000;
    pub const RIGHT_THUMBSTICK: u32 = 0x2000;

    pub fn held(self, button: u32) -> bool {
        self.0 & button != 0
    }
}

/// a controller's state at one moment, sticks -1 to 1 with up positive,
/// triggers 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Pad {
    pub buttons: Buttons,
    pub left: [f32; 2],
    pub right: [f32; 2],
    pub left_trigger: f32,
    pub right_trigger: f32,
}

impl Pad {
    /// several controllers as one, any of them pressing a button presses
    /// it, and the stick leant furthest wins.
    pub fn merge(self, other: Pad) -> Pad {
        let further = |a: [f32; 2], b: [f32; 2]| if b[0].hypot(b[1]) > a[0].hypot(a[1]) { b } else { a };
        Pad {
            buttons: Buttons(self.buttons.0 | other.buttons.0),
            left: further(self.left, other.left),
            right: further(self.right, other.right),
            left_trigger: self.left_trigger.max(other.left_trigger),
            right_trigger: self.right_trigger.max(other.right_trigger),
        }
    }
}

/// a shortcut taken this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    NextLayout,
    ToggleFps,
    TogglePause,
    ToggleTouch,
}

/// turns controller readings into the console's input, frame by frame.
#[derive(Debug, Default)]
pub struct Controls {
    previous: Pad,
    /// View was used for a shortcut since it was pressed, so it isn't
    /// SELECT until it is let go.
    view_used: bool,
    /// the touch cursor is on.
    pub touch_mode: bool,
    /// where the touch cursor is, in bottom screen pixels.
    pub cursor: (f32, f32),
}

impl Controls {
    pub fn new() -> Controls {
        Controls { cursor: (160.0, 120.0), ..Controls::default() }
    }

    /// the console's input for this reading, and the shortcuts pressed.
    pub fn update(&mut self, pad: Pad) -> (InputState, Vec<Shortcut>) {
        let pressed = |button: u32| pad.buttons.held(button) && !self.previous.buttons.held(button);
        let view = pad.buttons.held(Buttons::VIEW);
        if !view {
            self.view_used = false;
        }

        let mut shortcuts = Vec::new();
        // buttons that took part in a shortcut aren't the console's
        let mut swallowed = 0;
        if view {
            for (button, shortcut) in [
                (Buttons::LEFT_SHOULDER, Shortcut::NextLayout),
                (Buttons::RIGHT_SHOULDER, Shortcut::ToggleFps),
                (Buttons::MENU, Shortcut::TogglePause),
            ] {
                if pad.buttons.held(button) {
                    swallowed |= button;
                    self.view_used = true;
                    if pressed(button) {
                        shortcuts.push(shortcut);
                    }
                }
            }
        }
        if pressed(Buttons::RIGHT_THUMBSTICK) {
            self.touch_mode = !self.touch_mode;
            shortcuts.push(Shortcut::ToggleTouch);
        }

        let held = |button: u32| pad.buttons.held(button) && swallowed & button == 0;
        let mut buttons = PadState::empty();
        for (button, console) in [
            (Buttons::A, PadState::B),
            (Buttons::B, PadState::A),
            (Buttons::X, PadState::Y),
            (Buttons::Y, PadState::X),
            (Buttons::LEFT_SHOULDER, PadState::L),
            (Buttons::RIGHT_SHOULDER, PadState::R),
            (Buttons::MENU, PadState::START),
            (Buttons::DPAD_UP, PadState::UP),
            (Buttons::DPAD_DOWN, PadState::DOWN),
            (Buttons::DPAD_LEFT, PadState::LEFT),
            (Buttons::DPAD_RIGHT, PadState::RIGHT),
        ] {
            if held(button) {
                buttons |= console;
            }
        }
        if view && !self.view_used {
            buttons |= PadState::SELECT;
        }
        if pad.left_trigger > TRIGGER {
            buttons |= PadState::ZL;
        }

        let mut input = InputState::default();
        if pad.left[0].hypot(pad.left[1]) > DEADZONE {
            input.circle_x = pad.left[0].clamp(-1.0, 1.0);
            input.circle_y = pad.left[1].clamp(-1.0, 1.0);
        }
        let right_leans = pad.right[0].hypot(pad.right[1]) > DEADZONE;
        if self.touch_mode {
            if right_leans {
                // the screen's rows count down, the stick's up
                self.cursor.0 = (self.cursor.0 + pad.right[0] * CURSOR_SPEED).clamp(0.0, 319.0);
                self.cursor.1 = (self.cursor.1 - pad.right[1] * CURSOR_SPEED).clamp(0.0, 239.0);
            }
            if pad.right_trigger > TRIGGER {
                input.touch = Some((self.cursor.0 as u16, self.cursor.1 as u16));
            }
        } else {
            if right_leans {
                input.c_stick_x = pad.right[0].clamp(-1.0, 1.0);
                input.c_stick_y = pad.right[1].clamp(-1.0, 1.0);
            }
            if pad.right_trigger > TRIGGER {
                buttons |= PadState::ZR;
            }
        }
        input.buttons = buttons;
        self.previous = pad;
        (input, shortcuts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pad(buttons: u32) -> Pad {
        Pad { buttons: Buttons(buttons), ..Pad::default() }
    }

    #[test]
    fn face_buttons_go_by_position() {
        let mut controls = Controls::new();
        let (input, _) = controls.update(pad(Buttons::A | Buttons::Y | Buttons::MENU));
        assert_eq!(input.buttons, PadState::B | PadState::X | PadState::START);
    }

    #[test]
    fn view_alone_is_select_and_with_lb_a_shortcut() {
        let mut controls = Controls::new();
        assert_eq!(controls.update(pad(Buttons::VIEW)).0.buttons, PadState::SELECT);
        let (input, shortcuts) = controls.update(pad(Buttons::VIEW | Buttons::LEFT_SHOULDER));
        assert_eq!(shortcuts, vec![Shortcut::NextLayout]);
        assert!(!input.buttons.contains(PadState::L) && !input.buttons.contains(PadState::SELECT));
        // still held, no second shortcut and still no SELECT
        let (input, shortcuts) = controls.update(pad(Buttons::VIEW));
        assert!(shortcuts.is_empty());
        assert!(!input.buttons.contains(PadState::SELECT));
        controls.update(pad(0));
        assert_eq!(controls.update(pad(Buttons::VIEW)).0.buttons, PadState::SELECT);
    }

    #[test]
    fn the_touch_cursor_takes_the_right_stick_and_trigger() {
        let mut controls = Controls::new();
        let (_, shortcuts) = controls.update(pad(Buttons::RIGHT_THUMBSTICK));
        assert_eq!(shortcuts, vec![Shortcut::ToggleTouch]);
        assert!(controls.touch_mode);
        let leaning = Pad { right: [1.0, 1.0], right_trigger: 1.0, ..Pad::default() };
        let (input, _) = controls.update(leaning);
        assert_eq!(input.touch, Some((166, 114)));
        assert_eq!((input.c_stick_x, input.c_stick_y), (0.0, 0.0));
        assert!(!input.buttons.contains(PadState::ZR));
    }

    #[test]
    fn sticks_inside_the_deadzone_rest() {
        let mut controls = Controls::new();
        let (input, _) = controls.update(Pad { left: [0.1, 0.1], right: [0.0, -0.9], ..Pad::default() });
        assert_eq!((input.circle_x, input.circle_y), (0.0, 0.0));
        assert_eq!(input.c_stick_y, -0.9);
    }
}
