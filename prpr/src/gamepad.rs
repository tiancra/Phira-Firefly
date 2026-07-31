//! 手柄输入支持（仅桌面平台）
//!
//! 当 `use_keyboard` 开启时，手柄按键/扳机映射为键盘式击打，摇杆映射为虚拟触摸
//! 用于 Drag/Flick，菜单键触发暂停。
//!
//! 手柄导航系统：左摇杆在非游玩界面遍历可点击控件，A 确认、B 返回、X 联机。

use macroquad::prelude::*;
use std::cell::RefCell;

use crate::ui::FocusTarget;

#[derive(Default, Clone)]
pub struct GamepadFrame {
    pub key_delta: i32,
    pub keys_down: u32,
    pub touches: Vec<Touch>,
    pub menu_pressed: bool,
}

#[cfg(all(not(target_arch = "wasm32"), not(any(target_os = "android", target_os = "ios", target_env = "ohos"))))]
use gilrs::{Axis, Button, Gilrs};

pub struct GamepadInput {
    #[cfg(all(not(target_arch = "wasm32"), not(any(target_os = "android", target_os = "ios", target_env = "ohos"))))]
    gilrs: Option<Gilrs>,
    #[cfg(all(not(target_arch = "wasm32"), not(any(target_os = "android", target_os = "ios", target_env = "ohos"))))]
    buttons: std::collections::HashSet<Button>,
    #[cfg(all(not(target_arch = "wasm32"), not(any(target_os = "android", target_os = "ios", target_env = "ohos"))))]
    menu_down: bool,
    #[cfg(all(not(target_arch = "wasm32"), not(any(target_os = "android", target_os = "ios", target_env = "ohos"))))]
    sticks: [Option<Vec2>; 2],
    #[cfg(all(not(target_arch = "wasm32"), not(any(target_os = "android", target_os = "ios", target_env = "ohos"))))]
    last_left_stick: Vec2,
    #[cfg(all(not(target_arch = "wasm32"), not(any(target_os = "android", target_os = "ios", target_env = "ohos"))))]
    a_was_down: bool,
    #[cfg(all(not(target_arch = "wasm32"), not(any(target_os = "android", target_os = "ios", target_env = "ohos"))))]
    b_was_down: bool,
    #[cfg(all(not(target_arch = "wasm32"), not(any(target_os = "android", target_os = "ios", target_env = "ohos"))))]
    x_was_down: bool,
}

impl Default for GamepadInput {
    fn default() -> Self {
        Self::new()
    }
}

impl GamepadInput {
    pub fn new() -> Self {
        #[cfg(all(not(target_arch = "wasm32"), not(any(target_os = "android", target_os = "ios", target_env = "ohos"))))]
        {
            Self {
                gilrs: Gilrs::new().ok(),
                buttons: Default::default(),
                menu_down: false,
                sticks: [None; 2],
                last_left_stick: Vec2::ZERO,
                a_was_down: false,
                b_was_down: false,
                x_was_down: false,
            }
        }
        #[cfg(not(all(not(target_arch = "wasm32"), not(any(target_os = "android", target_os = "ios", target_env = "ohos")))))]
        {
            Self {}
        }
    }

    pub fn poll(&mut self) -> GamepadFrame {
        #[cfg(not(all(not(target_arch = "wasm32"), not(any(target_os = "android", target_os = "ios", target_env = "ohos")))))]
        {
            return GamepadFrame::default();
        }
        #[cfg(all(not(target_arch = "wasm32"), not(any(target_os = "android", target_os = "ios", target_env = "ohos"))))]
        {
            let mut frame = GamepadFrame::default();
            let Some(gilrs) = self.gilrs.as_mut() else {
                return frame;
            };

            while gilrs.next_event().is_some() {}

            const DEADZONE: f32 = 0.25;
            const STICK_IDS: [u64; 2] = [u64::MAX - 2000, u64::MAX - 2001];
            const STICK_AXES: [(Axis, Axis); 2] = [(Axis::LeftStickX, Axis::LeftStickY), (Axis::RightStickX, Axis::RightStickY)];
            const HIT_BUTTONS: [Button; 12] = [
                Button::South,
                Button::East,
                Button::North,
                Button::West,
                Button::LeftTrigger,
                Button::LeftTrigger2,
                Button::RightTrigger,
                Button::RightTrigger2,
                Button::DPadUp,
                Button::DPadDown,
                Button::DPadLeft,
                Button::DPadRight,
            ];

            let mut current = std::collections::HashSet::new();
            let mut menu_down = false;

            for (_, gp) in gilrs.gamepads() {
                for &btn in &HIT_BUTTONS {
                    if gp.is_pressed(btn) {
                        current.insert(btn);
                    }
                }
                if gp.value(Axis::LeftZ) > 0.5 {
                    current.insert(Button::LeftTrigger2);
                }
                if gp.value(Axis::RightZ) > 0.5 {
                    current.insert(Button::RightTrigger2);
                }

                if gp.is_pressed(Button::Start) || gp.is_pressed(Button::Select) || gp.is_pressed(Button::Mode) {
                    menu_down = true;
                }

                for (i, &(ax_x, ax_y)) in STICK_AXES.iter().enumerate() {
                    let raw = vec2(gp.value(ax_x), gp.value(ax_y));
                    let active = raw.length_squared() > DEADZONE * DEADZONE;
                    if active {
                        let pos = raw.clamp_length_max(1.0);
                        if self.sticks[i].is_none() {
                            frame.touches.push(Touch {
                                id: STICK_IDS[i],
                                phase: TouchPhase::Started,
                                position: pos,
                                time: f64::NEG_INFINITY,
                            });
                        }
                        frame.touches.push(Touch {
                            id: STICK_IDS[i],
                            phase: TouchPhase::Moved,
                            position: pos,
                            time: f64::NEG_INFINITY,
                        });
                        self.sticks[i] = Some(pos);
                    } else if let Some(last) = self.sticks[i].take() {
                        frame.touches.push(Touch {
                            id: STICK_IDS[i],
                            phase: TouchPhase::Ended,
                            position: last,
                            time: f64::NEG_INFINITY,
                        });
                    }
                }
            }

            if menu_down && !self.menu_down {
                frame.menu_pressed = true;
            }
            self.menu_down = menu_down;

            for btn in &current {
                if !self.buttons.contains(btn) {
                    frame.key_delta += 1;
                    frame.keys_down += 1;
                }
            }
            for btn in &self.buttons {
                if !current.contains(btn) {
                    frame.key_delta -= 1;
                }
            }
            self.buttons = current;

            frame
        }
    }

    /// Returns the left stick direction for navigation (if active)
    /// and the button press states (A, B, X rising edges)
    #[cfg(all(not(target_arch = "wasm32"), not(any(target_os = "android", target_os = "ios", target_env = "ohos"))))]
    pub fn nav_input(&mut self) -> NavInput {
        let Some(gilrs) = self.gilrs.as_mut() else {
            return NavInput::default();
        };

        const DEADZONE: f32 = 0.3;
        let mut left_stick = Vec2::ZERO;
        let mut a_down = false;
        let mut b_down = false;
        let mut x_down = false;

        for (_, gp) in gilrs.gamepads() {
            let raw = vec2(gp.value(Axis::LeftStickX), gp.value(Axis::LeftStickY));
            if raw.length_squared() > DEADZONE * DEADZONE {
                left_stick = raw.clamp_length_max(1.0);
            }
            a_down |= gp.is_pressed(Button::South);
            b_down |= gp.is_pressed(Button::East);
            x_down |= gp.is_pressed(Button::West);
        }

        let a_pressed = a_down && !self.a_was_down;
        let b_pressed = b_down && !self.b_was_down;
        let x_pressed = x_down && !self.x_was_down;

        self.a_was_down = a_down;
        self.b_was_down = b_down;
        self.x_was_down = x_down;
        self.last_left_stick = left_stick;

        NavInput {
            left_stick,
            a_pressed,
            b_pressed,
            x_pressed,
        }
    }

    #[cfg(not(all(not(target_arch = "wasm32"), not(any(target_os = "android", target_os = "ios", target_env = "ohos")))))]
    pub fn nav_input(&mut self) -> NavInput {
        NavInput::default()
    }
}

#[derive(Default, Clone, Copy)]
pub struct NavInput {
    pub left_stick: Vec2,
    pub a_pressed: bool,
    pub b_pressed: bool,
    pub x_pressed: bool,
}

/// Navigation state machine: tracks focus index and handles stick-based navigation.
#[derive(Default, Clone)]
pub struct NavState {
    pub focus_index: usize,
    pub phase: f32,
    pub last_stick_dir: Vec2,
    pub nav_cooldown: f32,
}

impl NavState {
    pub fn new() -> Self {
        Self {
            focus_index: 0,
            phase: 0.0,
            last_stick_dir: Vec2::ZERO,
            nav_cooldown: 0.0,
        }
    }

    /// Process navigation update: returns the updated focus index after stick movement
    pub fn update(&mut self, targets: &[FocusTarget], input: NavInput, dt: f32) {
        self.phase += dt * 2.0;

        if targets.is_empty() {
            self.focus_index = 0;
            return;
        }

        if self.focus_index >= targets.len() {
            self.focus_index = 0;
        }

        if self.nav_cooldown > 0.0 {
            self.nav_cooldown -= dt;
        }

        let stick = input.left_stick;
        if stick.length_squared() > 0.3 * 0.3 {
            // Check if stick changed direction significantly
            let dot = stick.dot(self.last_stick_dir);
            if self.nav_cooldown <= 0.0 || dot < 0.5 {
                self.last_stick_dir = stick.normalize();
                self.nav_cooldown = 0.15;

                // Find nearest target in stick direction from current focus
                let current = &targets[self.focus_index];
                let current_center = current.rect.center();
                let mut best_idx = self.focus_index;
                let mut best_score = f32::MAX;

                for (i, target) in targets.iter().enumerate() {
                    if i == self.focus_index {
                        continue;
                    }
                    let tc = target.rect.center();
                    let diff = tc - current_center;
                    let dist = diff.length();
                    if dist < 0.001 {
                        continue;
                    }
                    let dir = diff / dist;
                    let alignment = dir.dot(stick.normalize()).max(0.0);
                    // Prefer elements in the stick direction, weighted by distance
                    let score = if alignment > 0.1 {
                        dist / (alignment + 0.1)
                    } else {
                        f32::MAX
                    };
                    if score < best_score {
                        best_score = score;
                        best_idx = i;
                    }
                }

                if best_idx != self.focus_index && best_score < f32::MAX {
                    self.focus_index = best_idx;
                }
            }
        } else {
            self.last_stick_dir = Vec2::ZERO;
        }
    }

    pub fn current_target<'a>(&self, targets: &'a [FocusTarget]) -> Option<&'a FocusTarget> {
        targets.get(self.focus_index)
    }
}

thread_local! {
    static PENDING: RefCell<GamepadFrame> = RefCell::default();
    static NAV_STATE: RefCell<NavState> = RefCell::new(NavState::new());
    static GAMEPAD: RefCell<GamepadInput> = RefCell::new(GamepadInput::new());
    static LAST_FRAME: RefCell<GamepadFrame> = RefCell::default();
    static LAST_NAV_INPUT: RefCell<NavInput> = RefCell::new(NavInput::default());
}

pub fn push_pending(frame: GamepadFrame) {
    PENDING.with(|it| {
        let mut guard = it.borrow_mut();
        guard.key_delta += frame.key_delta;
        guard.keys_down += frame.keys_down;
        guard.menu_pressed |= frame.menu_pressed;
        guard.touches.extend(frame.touches);
    });
}

pub fn take_pending() -> GamepadFrame {
    PENDING.with(|it| std::mem::take(&mut *it.borrow_mut()))
}

pub fn last_frame() -> GamepadFrame {
    LAST_FRAME.with(|it| it.borrow().clone())
}

pub fn poll_global() {
    GAMEPAD.with(|it| {
        let mut guard = it.borrow_mut();
        let frame = guard.poll();
        LAST_FRAME.with(|f| {
            *f.borrow_mut() = frame.clone();
        });
        push_pending(frame);
        let nav = guard.nav_input();
        LAST_NAV_INPUT.with(|n| {
            *n.borrow_mut() = nav;
        });
    });
}

pub fn nav_input_static() -> NavInput {
    LAST_NAV_INPUT.with(|it| *it.borrow())
}

pub fn get_nav_state() -> NavState {
    NAV_STATE.with(|it| it.borrow().clone())
}

pub fn update_nav(targets: &[FocusTarget], input: NavInput, dt: f32) -> NavState {
    NAV_STATE.with(|it| {
        let mut state = it.borrow_mut();
        state.update(targets, input, dt);
        state.clone()
    })
}

pub fn reset_nav_focus() {
    NAV_STATE.with(|it| {
        it.borrow_mut().focus_index = 0;
    });
}

pub fn push_nav_touch(pos: Vec2) {
    let touch = Touch {
        id: u64::MAX - 3000,
        phase: TouchPhase::Started,
        position: pos,
        time: f64::NEG_INFINITY,
    };
    push_pending(GamepadFrame {
        key_delta: 0,
        keys_down: 0,
        touches: vec![touch.clone(), Touch {
            id: u64::MAX - 3000,
            phase: TouchPhase::Ended,
            position: pos,
            time: f64::NEG_INFINITY,
        }],
        menu_pressed: false,
    });
}

pub fn push_nav_back() {
    BACK_PRESSED.with(|it| {
        *it.borrow_mut() = true;
    });
}

pub fn push_nav_multilang() {
    MULTILANG_PRESSED.with(|it| {
        *it.borrow_mut() = true;
    });
}

thread_local! {
    static BACK_PRESSED: RefCell<bool> = const { RefCell::new(false) };
    static MULTILANG_PRESSED: RefCell<bool> = const { RefCell::new(false) };
}

pub fn take_back_pressed() -> bool {
    BACK_PRESSED.with(|it| std::mem::take(&mut *it.borrow_mut()))
}

pub fn take_multilang_pressed() -> bool {
    MULTILANG_PRESSED.with(|it| std::mem::take(&mut *it.borrow_mut()))
}
