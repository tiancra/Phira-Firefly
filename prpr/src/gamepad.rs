//! 手柄输入支持（仅桌面平台）
//!
//! 当 `use_keyboard` 开启时，手柄按键/扳机映射为键盘式击打，摇杆映射为虚拟触摸
//! 用于 Drag/Flick，菜单键触发暂停。

use macroquad::prelude::*;
use std::cell::RefCell;

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
            }
        }
        #[cfg(not(all(not(target_arch = "wasm32"), not(any(target_os = "android", target_os = "ios", target_env = "ohos")))))]
        {
            Self
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

            // 更新 gilrs 内部状态
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
                // 扳机轴也视为按键
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
                        // 摇杆 y 轴向上为正，屏幕 local 坐标 y 轴向上为正，直接使用
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

            // 菜单键只在按下瞬间触发
            if menu_down && !self.menu_down {
                frame.menu_pressed = true;
            }
            self.menu_down = menu_down;

            // 计算本帧按键净变化，与键盘 key_delta 机制一致
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
}

thread_local! {
    static PENDING: RefCell<GamepadFrame> = RefCell::default();
}

/// 将一帧手柄输入累加到待处理队列，供 Judge::on_new_frame 合并。
pub fn push_pending(frame: GamepadFrame) {
    PENDING.with(|it| {
        let mut guard = it.borrow_mut();
        guard.key_delta += frame.key_delta;
        guard.keys_down += frame.keys_down;
        guard.menu_pressed |= frame.menu_pressed;
        guard.touches.extend(frame.touches);
    });
}

/// 取走并清空待处理队列。
pub fn take_pending() -> GamepadFrame {
    PENDING.with(|it| std::mem::take(&mut *it.borrow_mut()))
}
