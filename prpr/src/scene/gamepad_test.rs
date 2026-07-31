//! Gamepad test scene: displays a gamepad diagram and highlights pressed buttons/sticks.
//! Accessible by holding LB+RB for 3 seconds on non-gameplay screens, or pressing F9.

use super::{NextScene, Scene};
use crate::{
    gamepad::raw_state_static,
    time::TimeManager,
    ui::Ui,
};
use macroquad::prelude::*;

pub struct GamepadTestScene {
    back_clicked: bool,
}

impl GamepadTestScene {
    pub fn new() -> Self {
        Self { back_clicked: false }
    }
}

impl Scene for GamepadTestScene {
    fn update(&mut self, _tm: &mut TimeManager) -> anyhow::Result<()> {
        Ok(())
    }

    fn render(&mut self, _tm: &mut TimeManager, ui: &mut Ui) -> anyhow::Result<()> {
        let cr = ui.content_rect();
        let cx = cr.center().x;
        let cy = cr.center().y;

        // Dark gray background
        ui.fill_rect(cr, Color::new(0.15, 0.15, 0.18, 1.0));

        let raw = raw_state_static();

        // Title
        ui.text("手柄测试 / GAMEPAD TEST")
            .pos(cx, cr.y + 0.08)
            .anchor(0.5, 0.5)
            .size(0.6)
            .color(WHITE)
            .no_baseline()
            .draw();

        // Status text
        let status = if !raw.gilrs_ready {
            "gilrs 未初始化 (重启应用或检查驱动)"
        } else if !raw.connected {
            "未检测到手柄 (请连接手柄后按 F9 重试)"
        } else {
            ""
        };
        if !status.is_empty() {
            ui.text(status)
                .pos(cx, cr.y + 0.14)
                .anchor(0.5, 0.5)
                .size(0.35)
                .color(Color::new(1.0, 0.6, 0.3, 1.0))
                .no_baseline()
                .draw();
        }

        // --- Gamepad diagram layout ---
        let body_w = 0.65;
        let body_h = 0.28;
        let body_rect = Rect::new(cx - body_w / 2.0, cy - body_h / 2.0, body_w, body_h);

        // Gamepad body
        ui.fill_rect(body_rect, Color::new(0.25, 0.25, 0.3, 1.0));

        // Left handle
        let handle_r = 0.08;
        ui.fill_circle(
            body_rect.x + handle_r,
            body_rect.y + body_rect.h / 2.0,
            handle_r,
            Color::new(0.22, 0.22, 0.27, 1.0),
        );
        // Right handle
        ui.fill_circle(
            body_rect.x + body_rect.w - handle_r,
            body_rect.y + body_rect.h / 2.0,
            handle_r,
            Color::new(0.22, 0.22, 0.27, 1.0),
        );

        // --- D-Pad (left side) ---
        let dp_cx = body_rect.x + body_rect.w * 0.22;
        let dp_cy = body_rect.y + body_rect.h * 0.5;
        let dp_size = 0.025;
        let dp_dist = 0.04;

        let dp_color = |pressed: bool| -> Color {
            if pressed {
                Color::new(1.0, 0.6, 0.2, 1.0)
            } else {
                Color::new(0.4, 0.4, 0.45, 1.0)
            }
        };

        // D-Pad center
        ui.fill_circle(dp_cx, dp_cy, dp_size, Color::new(0.35, 0.35, 0.4, 1.0));
        // Up
        ui.fill_rect(
            Rect::new(dp_cx - dp_size * 0.6, dp_cy - dp_dist - dp_size, dp_size * 1.2, dp_size),
            dp_color(raw.dpad_up),
        );
        // Down
        ui.fill_rect(
            Rect::new(dp_cx - dp_size * 0.6, dp_cy + dp_dist, dp_size * 1.2, dp_size),
            dp_color(raw.dpad_down),
        );
        // Left
        ui.fill_rect(
            Rect::new(dp_cx - dp_dist - dp_size, dp_cy - dp_size * 0.6, dp_size, dp_size * 1.2),
            dp_color(raw.dpad_left),
        );
        // Right
        ui.fill_rect(
            Rect::new(dp_cx + dp_dist, dp_cy - dp_size * 0.6, dp_size, dp_size * 1.2),
            dp_color(raw.dpad_right),
        );

        // --- 4 face buttons (right side, diamond layout) ---
        let bt_cx = body_rect.x + body_rect.w * 0.78;
        let bt_cy = body_rect.y + body_rect.h * 0.5;
        let bt_r = 0.022;
        let bt_dist = 0.045;

        let face_color = |pressed: bool, base: Color| -> Color {
            if pressed {
                Color::new(1.0, 0.7, 0.3, 1.0)
            } else {
                base
            }
        };

        // Y (top - North) - yellow
        ui.fill_circle(bt_cx, bt_cy - bt_dist, bt_r, face_color(raw.north, Color::new(1.0, 0.85, 0.2, 1.0)));
        // A (bottom - South) - green
        ui.fill_circle(bt_cx, bt_cy + bt_dist, bt_r, face_color(raw.south, Color::new(0.2, 0.8, 0.3, 1.0)));
        // X (left - West) - blue
        ui.fill_circle(bt_cx - bt_dist, bt_cy, bt_r, face_color(raw.west, Color::new(0.2, 0.4, 0.9, 1.0)));
        // B (right - East) - red
        ui.fill_circle(bt_cx + bt_dist, bt_cy, bt_r, face_color(raw.east, Color::new(0.9, 0.2, 0.2, 1.0)));

        // Labels
        for (lbl, lx, ly, col) in [
            ("Y", bt_cx, bt_cy - bt_dist, BLACK),
            ("A", bt_cx, bt_cy + bt_dist, BLACK),
            ("X", bt_cx - bt_dist, bt_cy, WHITE),
            ("B", bt_cx + bt_dist, bt_cy, WHITE),
        ] {
            ui.text(lbl)
                .pos(lx, ly)
                .anchor(0.5, 0.5)
                .size(0.18)
                .color(col)
                .no_baseline()
                .draw();
        }

        // --- Left Stick ---
        let ls_cx = body_rect.x + body_rect.w * 0.35;
        let ls_cy = body_rect.y + body_rect.h * 0.75;
        let ls_r = 0.035;

        ui.fill_circle(ls_cx, ls_cy, ls_r * 1.5, Color::new(0.3, 0.3, 0.35, 1.0));
        let ls_pos = vec2(raw.left_stick.x, -raw.left_stick.y);
        let ls_len = ls_pos.length();
        let ls_clamped = if ls_len > 1.0 { ls_pos / ls_len } else { ls_pos };
        let ls_hx = ls_cx + ls_clamped.x * ls_r * 0.7;
        let ls_hy = ls_cy + ls_clamped.y * ls_r * 0.7;
        let ls_active = ls_len > 0.15;
        ui.fill_circle(
            ls_hx,
            ls_hy,
            ls_r * 0.85,
            if ls_active { Color::new(1.0, 0.6, 0.2, 0.95) } else { Color::new(0.5, 0.5, 0.55, 1.0) },
        );

        // --- Right Stick ---
        let rs_cx = body_rect.x + body_rect.w * 0.65;
        let rs_cy = body_rect.y + body_rect.h * 0.75;

        ui.fill_circle(rs_cx, rs_cy, ls_r * 1.5, Color::new(0.3, 0.3, 0.35, 1.0));
        let rs_pos = vec2(raw.right_stick.x, -raw.right_stick.y);
        let rs_len = rs_pos.length();
        let rs_clamped = if rs_len > 1.0 { rs_pos / rs_len } else { rs_pos };
        let rs_hx = rs_cx + rs_clamped.x * ls_r * 0.7;
        let rs_hy = rs_cy + rs_clamped.y * ls_r * 0.7;
        let rs_active = rs_len > 0.15;
        ui.fill_circle(
            rs_hx,
            rs_hy,
            ls_r * 0.85,
            if rs_active { Color::new(1.0, 0.6, 0.2, 0.95) } else { Color::new(0.5, 0.5, 0.55, 1.0) },
        );

        // --- Shoulder buttons (LB/RB) ---
        let lb_cx = body_rect.x + body_rect.w * 0.1;
        let lb_cy = body_rect.y - 0.04;
        let rb_cx = body_rect.x + body_rect.w * 0.9;
        let rb_cy = body_rect.y - 0.04;
        let shoulder_w = 0.08;
        let shoulder_h = 0.035;

        ui.fill_rect(
            Rect::new(lb_cx - shoulder_w / 2.0, lb_cy - shoulder_h / 2.0, shoulder_w, shoulder_h),
            if raw.lb { Color::new(1.0, 0.6, 0.2, 1.0) } else { Color::new(0.4, 0.4, 0.45, 1.0) },
        );
        ui.text("LB")
            .pos(lb_cx, lb_cy)
            .anchor(0.5, 0.5)
            .size(0.25)
            .color(WHITE)
            .no_baseline()
            .draw();

        ui.fill_rect(
            Rect::new(rb_cx - shoulder_w / 2.0, rb_cy - shoulder_h / 2.0, shoulder_w, shoulder_h),
            if raw.rb { Color::new(1.0, 0.6, 0.2, 1.0) } else { Color::new(0.4, 0.4, 0.45, 1.0) },
        );
        ui.text("RB")
            .pos(rb_cx, rb_cy)
            .anchor(0.5, 0.5)
            .size(0.25)
            .color(WHITE)
            .no_baseline()
            .draw();

        // --- Triggers (LT/RT) - analog bar display ---
        let bar_y = body_rect.y + body_rect.h + 0.05;
        let bar_w = 0.15;
        let bar_h = 0.02;

        // LT bar
        ui.fill_rect(
            Rect::new(lb_cx - bar_w / 2.0, bar_y, bar_w, bar_h),
            Color::new(0.3, 0.3, 0.35, 1.0),
        );
        let lt_fill = (raw.lt * bar_w) as f32;
        if lt_fill > 0.001 {
            ui.fill_rect(
                Rect::new(lb_cx - bar_w / 2.0, bar_y, lt_fill, bar_h),
                Color::new(1.0, 0.6, 0.2, 1.0),
            );
        }
        ui.text(format!("LT {:.2}", raw.lt))
            .pos(lb_cx, bar_y + bar_h * 2.2)
            .anchor(0.5, 0.5)
            .size(0.2)
            .color(WHITE)
            .no_baseline()
            .draw();

        // RT bar
        ui.fill_rect(
            Rect::new(rb_cx - bar_w / 2.0, bar_y, bar_w, bar_h),
            Color::new(0.3, 0.3, 0.35, 1.0),
        );
        let rt_fill = (raw.rt * bar_w) as f32;
        if rt_fill > 0.001 {
            ui.fill_rect(
                Rect::new(rb_cx - bar_w / 2.0, bar_y, rt_fill, bar_h),
                Color::new(1.0, 0.6, 0.2, 1.0),
            );
        }
        ui.text(format!("RT {:.2}", raw.rt))
            .pos(rb_cx, bar_y + bar_h * 2.2)
            .anchor(0.5, 0.5)
            .size(0.2)
            .color(WHITE)
            .no_baseline()
            .draw();

        // --- Start / Select buttons ---
        let ss_y = body_rect.y + body_rect.h * 0.15;
        let start_cx = body_rect.x + body_rect.w * 0.42;
        let select_cx = body_rect.x + body_rect.w * 0.58;
        let ss_r = 0.018;

        ui.fill_circle(
            start_cx,
            ss_y,
            ss_r,
            if raw.start { Color::new(1.0, 0.6, 0.2, 1.0) } else { Color::new(0.35, 0.35, 0.4, 1.0) },
        );
        ui.text("Start")
            .pos(start_cx, ss_y - ss_r * 2.0)
            .anchor(0.5, 0.5)
            .size(0.18)
            .color(WHITE)
            .no_baseline()
            .draw();

        ui.fill_circle(
            select_cx,
            ss_y,
            ss_r,
            if raw.select { Color::new(1.0, 0.6, 0.2, 1.0) } else { Color::new(0.35, 0.35, 0.4, 1.0) },
        );
        ui.text("Select")
            .pos(select_cx, ss_y - ss_r * 2.0)
            .anchor(0.5, 0.5)
            .size(0.18)
            .color(WHITE)
            .no_baseline()
            .draw();

        // --- Raw values debug panel ---
        let debug_y = body_rect.y + body_rect.h + 0.15;
        let debug_x = cr.x + 0.05;
        let line_step = 0.03;

        ui.text(format!(
            "LX={:+.2}  LY={:+.2}  RX={:+.2}  RY={:+.2}",
            raw.left_stick.x, raw.left_stick.y,
            raw.right_stick.x, raw.right_stick.y
        ))
            .pos(debug_x, debug_y)
            .anchor(0.0, 0.5)
            .size(0.22)
            .color(Color::new(0.7, 0.9, 0.7, 1.0))
            .no_baseline()
            .draw();

        ui.text(format!(
            "DPad: U={} D={} L={} R={}",
            raw.dpad_up as i32, raw.dpad_down as i32, raw.dpad_left as i32, raw.dpad_right as i32
        ))
            .pos(debug_x, debug_y + line_step)
            .anchor(0.0, 0.5)
            .size(0.22)
            .color(Color::new(0.7, 0.9, 0.7, 1.0))
            .no_baseline()
            .draw();

        ui.text(format!(
            "Face: Y={} A={} X={} B={}  LB={} RB={}  Start={} Select={}",
            raw.north as i32, raw.south as i32, raw.west as i32, raw.east as i32,
            raw.lb as i32, raw.rb as i32,
            raw.start as i32, raw.select as i32
        ))
            .pos(debug_x, debug_y + line_step * 2.0)
            .anchor(0.0, 0.5)
            .size(0.22)
            .color(Color::new(0.7, 0.9, 0.7, 1.0))
            .no_baseline()
            .draw();

        ui.text(format!(
            "Connected: {}  gilrs_ready: {}  LT={:.2}  RT={:.2}",
            raw.connected as i32, raw.gilrs_ready as i32, raw.lt, raw.rt
        ))
            .pos(debug_x, debug_y + line_step * 3.0)
            .anchor(0.0, 0.5)
            .size(0.22)
            .color(Color::new(0.7, 0.9, 0.7, 1.0))
            .no_baseline()
            .draw();

        // --- Back button ---
        let btn_rect = Rect::new(cx - 0.08, cr.y + cr.h - 0.08, 0.16, 0.045);
        if ui.button("gp_test_back", btn_rect, "返回 (B / Esc)") {
            self.back_clicked = true;
        }

        Ok(())
    }

    fn next_scene(&mut self, _tm: &mut TimeManager) -> NextScene {
        if self.back_clicked
            || crate::gamepad::take_back_pressed()
            || is_key_pressed(KeyCode::Escape)
        {
            return NextScene::Pop;
        }
        NextScene::None
    }

    fn nav_enabled(&self) -> bool {
        false
    }
}
