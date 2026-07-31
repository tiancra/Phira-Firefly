//! Gamepad test scene: displays a gamepad diagram and highlights pressed buttons/sticks.
//! Exit: Press B, Esc, or hold LB+RB for 3 seconds.

use super::{NextScene, Scene};
use crate::{
    gamepad::raw_state_static,
    time::TimeManager,
    ui::Ui,
};
use macroquad::prelude::*;

pub struct GamepadTestScene {
    back_clicked: bool,
    lb_rb_timer: f32,
}

impl GamepadTestScene {
    pub fn new() -> Self {
        Self { back_clicked: false, lb_rb_timer: 0.0 }
    }
}

impl Scene for GamepadTestScene {
    fn update(&mut self, _tm: &mut TimeManager) -> anyhow::Result<()> {
        Ok(())
    }

    fn render(&mut self, _tm: &mut TimeManager, ui: &mut Ui) -> anyhow::Result<()> {
        let sr = ui.screen_rect();
        let cx = sr.center().x;
        let cy = sr.center().y;

        ui.fill_rect(sr, Color::new(0.12, 0.12, 0.15, 1.0));

        let raw = raw_state_static();
        let dt = get_frame_time();

        // Title
        ui.text("手柄测试 / GAMEPAD TEST")
            .pos(cx, sr.y + 0.06)
            .anchor(0.5, 0.5)
            .size(0.6)
            .color(WHITE)
            .no_baseline()
            .draw();

        // Status
        let status = if !raw.gilrs_ready {
            "gilrs 未初始化 (重启应用或检查驱动)"
        } else if !raw.connected {
            "未检测到手柄 (请连接手柄后按 F9 重试)"
        } else {
            ""
        };
        if !status.is_empty() {
            ui.text(status)
                .pos(cx, sr.y + 0.11)
                .anchor(0.5, 0.5)
                .size(0.35)
                .color(Color::new(1.0, 0.6, 0.3, 1.0))
                .no_baseline()
                .draw();
        } else {
            ui.text("手柄已连接 — 按键或摇杆会在图中高亮显示")
                .pos(cx, sr.y + 0.11)
                .anchor(0.5, 0.5)
                .size(0.28)
                .color(Color::new(0.5, 0.9, 0.5, 1.0))
                .no_baseline()
                .draw();
        }

        // LB+RB hold to exit indicator
        if raw.lb && raw.rb {
            self.lb_rb_timer += dt;
            let progress = (self.lb_rb_timer / 3.0).min(1.0);
            // Progress bar
            let bar_w = 0.3;
            let bar_h = 0.015;
            let bar_x = cx - bar_w / 2.0;
            let bar_y = sr.y + 0.15;
            ui.fill_rect(Rect::new(bar_x, bar_y, bar_w, bar_h), Color::new(0.3, 0.3, 0.35, 1.0));
            ui.fill_rect(Rect::new(bar_x, bar_y, bar_w * progress, bar_h), Color::new(1.0, 0.6, 0.2, 1.0));
            ui.text(format!("长按退出 {:.0}s", 3.0 - self.lb_rb_timer))
                .pos(cx, bar_y + bar_h + 0.015)
                .anchor(0.5, 0.5)
                .size(0.2)
                .color(Color::new(1.0, 0.8, 0.5, 1.0))
                .no_baseline()
                .draw();
            if self.lb_rb_timer >= 3.0 {
                self.back_clicked = true;
            }
        } else {
            self.lb_rb_timer = 0.0;
        }

        // --- Gamepad diagram ---
        let body_w = 0.6;
        let body_h = 0.25;
        let body_rect = Rect::new(cx - body_w / 2.0, cy - body_h / 2.0, body_w, body_h);

        ui.fill_rect(body_rect, Color::new(0.25, 0.25, 0.3, 1.0));

        let handle_r = 0.07;
        ui.fill_circle(body_rect.x + handle_r, body_rect.y + body_rect.h / 2.0, handle_r, Color::new(0.22, 0.22, 0.27, 1.0));
        ui.fill_circle(body_rect.x + body_rect.w - handle_r, body_rect.y + body_rect.h / 2.0, handle_r, Color::new(0.22, 0.22, 0.27, 1.0));

        // D-Pad
        let dp_cx = body_rect.x + body_rect.w * 0.22;
        let dp_cy = body_rect.y + body_rect.h * 0.5;
        let dp_size = 0.022;
        let dp_dist = 0.035;
        let dp_color = |p: bool| if p { Color::new(1.0, 0.6, 0.2, 1.0) } else { Color::new(0.4, 0.4, 0.45, 1.0) };

        ui.fill_circle(dp_cx, dp_cy, dp_size, Color::new(0.35, 0.35, 0.4, 1.0));
        ui.fill_rect(Rect::new(dp_cx - dp_size * 0.6, dp_cy - dp_dist - dp_size, dp_size * 1.2, dp_size), dp_color(raw.dpad_up));
        ui.fill_rect(Rect::new(dp_cx - dp_size * 0.6, dp_cy + dp_dist, dp_size * 1.2, dp_size), dp_color(raw.dpad_down));
        ui.fill_rect(Rect::new(dp_cx - dp_dist - dp_size, dp_cy - dp_size * 0.6, dp_size, dp_size * 1.2), dp_color(raw.dpad_left));
        ui.fill_rect(Rect::new(dp_cx + dp_dist, dp_cy - dp_size * 0.6, dp_size, dp_size * 1.2), dp_color(raw.dpad_right));

        // Face buttons
        let bt_cx = body_rect.x + body_rect.w * 0.78;
        let bt_cy = body_rect.y + body_rect.h * 0.5;
        let bt_r = 0.02;
        let bt_dist = 0.04;
        let fc = |p: bool, b: Color| if p { Color::new(1.0, 0.7, 0.3, 1.0) } else { b };

        ui.fill_circle(bt_cx, bt_cy - bt_dist, bt_r, fc(raw.north, Color::new(1.0, 0.85, 0.2, 1.0)));
        ui.fill_circle(bt_cx, bt_cy + bt_dist, bt_r, fc(raw.south, Color::new(0.2, 0.8, 0.3, 1.0)));
        ui.fill_circle(bt_cx - bt_dist, bt_cy, bt_r, fc(raw.west, Color::new(0.2, 0.4, 0.9, 1.0)));
        ui.fill_circle(bt_cx + bt_dist, bt_cy, bt_r, fc(raw.east, Color::new(0.9, 0.2, 0.2, 1.0)));

        for (lbl, lx, ly, col) in [("Y", bt_cx, bt_cy - bt_dist, BLACK), ("A", bt_cx, bt_cy + bt_dist, BLACK), ("X", bt_cx - bt_dist, bt_cy, WHITE), ("B", bt_cx + bt_dist, bt_cy, WHITE)] {
            ui.text(lbl).pos(lx, ly).anchor(0.5, 0.5).size(0.16).color(col).no_baseline().draw();
        }

        // Left Stick
        let ls_cx = body_rect.x + body_rect.w * 0.35;
        let ls_cy = body_rect.y + body_rect.h * 0.72;
        let ls_r = 0.03;
        ui.fill_circle(ls_cx, ls_cy, ls_r * 1.5, Color::new(0.3, 0.3, 0.35, 1.0));
        let ls_pos = vec2(raw.left_stick.x, -raw.left_stick.y);
        let ls_len = ls_pos.length();
        let ls_clamped = if ls_len > 1.0 { ls_pos / ls_len } else { ls_pos };
        ui.fill_circle(ls_cx + ls_clamped.x * ls_r * 0.7, ls_cy + ls_clamped.y * ls_r * 0.7, ls_r * 0.85, if ls_len > 0.15 { Color::new(1.0, 0.6, 0.2, 0.95) } else { Color::new(0.5, 0.5, 0.55, 1.0) });

        // Right Stick
        let rs_cx = body_rect.x + body_rect.w * 0.65;
        let rs_cy = ls_cy;
        ui.fill_circle(rs_cx, rs_cy, ls_r * 1.5, Color::new(0.3, 0.3, 0.35, 1.0));
        let rs_pos = vec2(raw.right_stick.x, -raw.right_stick.y);
        let rs_len = rs_pos.length();
        let rs_clamped = if rs_len > 1.0 { rs_pos / rs_len } else { rs_pos };
        ui.fill_circle(rs_cx + rs_clamped.x * ls_r * 0.7, rs_cy + rs_clamped.y * ls_r * 0.7, ls_r * 0.85, if rs_len > 0.15 { Color::new(1.0, 0.6, 0.2, 0.95) } else { Color::new(0.5, 0.5, 0.55, 1.0) });

        // Shoulder buttons
        let lb_cx = body_rect.x + body_rect.w * 0.1;
        let lb_cy = body_rect.y - 0.035;
        let rb_cx = body_rect.x + body_rect.w * 0.9;
        let rb_cy = body_rect.y - 0.035;
        let sw = 0.07;
        let sh = 0.03;
        ui.fill_rect(Rect::new(lb_cx - sw / 2.0, lb_cy - sh / 2.0, sw, sh), if raw.lb { Color::new(1.0, 0.6, 0.2, 1.0) } else { Color::new(0.4, 0.4, 0.45, 1.0) });
        ui.text("LB").pos(lb_cx, lb_cy).anchor(0.5, 0.5).size(0.22).color(WHITE).no_baseline().draw();
        ui.fill_rect(Rect::new(rb_cx - sw / 2.0, rb_cy - sh / 2.0, sw, sh), if raw.rb { Color::new(1.0, 0.6, 0.2, 1.0) } else { Color::new(0.4, 0.4, 0.45, 1.0) });
        ui.text("RB").pos(rb_cx, rb_cy).anchor(0.5, 0.5).size(0.22).color(WHITE).no_baseline().draw();

        // Triggers
        let bar_y = body_rect.y + body_rect.h + 0.04;
        let bar_w = 0.12;
        let bar_h = 0.018;

        for (bx, bval, lbl) in [(lb_cx, raw.lt, "LT"), (rb_cx, raw.rt, "RT")] {
            ui.fill_rect(Rect::new(bx - bar_w / 2.0, bar_y, bar_w, bar_h), Color::new(0.3, 0.3, 0.35, 1.0));
            let fill = (bval.abs() * bar_w) as f32;
            if fill > 0.001 {
                ui.fill_rect(Rect::new(bx - bar_w / 2.0, bar_y, fill, bar_h), Color::new(1.0, 0.6, 0.2, 1.0));
            }
            ui.text(format!("{} {:.2}", lbl, bval)).pos(bx, bar_y + bar_h * 2.0).anchor(0.5, 0.5).size(0.18).color(WHITE).no_baseline().draw();
        }

        // Start / Select
        let ss_y = body_rect.y + body_rect.h * 0.15;
        let start_cx = body_rect.x + body_rect.w * 0.42;
        let select_cx = body_rect.x + body_rect.w * 0.58;
        let ss_r = 0.015;
        ui.fill_circle(start_cx, ss_y, ss_r, if raw.start { Color::new(1.0, 0.6, 0.2, 1.0) } else { Color::new(0.35, 0.35, 0.4, 1.0) });
        ui.text("Start").pos(start_cx, ss_y - ss_r * 2.0).anchor(0.5, 0.5).size(0.16).color(WHITE).no_baseline().draw();
        ui.fill_circle(select_cx, ss_y, ss_r, if raw.select { Color::new(1.0, 0.6, 0.2, 1.0) } else { Color::new(0.35, 0.35, 0.4, 1.0) });
        ui.text("Select").pos(select_cx, ss_y - ss_r * 2.0).anchor(0.5, 0.5).size(0.16).color(WHITE).no_baseline().draw();

        // --- Debug panel ---
        let dbg_x = sr.x + 0.03;
        let dbg_y = body_rect.y + body_rect.h + 0.11;
        let ls = 0.025;
        let col = Color::new(0.65, 0.9, 0.65, 1.0);

        ui.text(format!("Axis LX={:+.3} LY={:+.3} RX={:+.3} RY={:+.3}", raw.axis_lx, raw.axis_ly, raw.axis_rx, raw.axis_ry))
            .pos(dbg_x, dbg_y).anchor(0.0, 0.5).size(0.2).color(col).no_baseline().draw();
        ui.text(format!("Axis LZ={:+.3} RZ={:+.3}", raw.axis_lz, raw.axis_rz))
            .pos(dbg_x, dbg_y + ls).anchor(0.0, 0.5).size(0.2).color(col).no_baseline().draw();
        ui.text(format!("Btns: LT1={} LT2={} RT1={} RT2={}  LB={} RB={}", raw.btn_lt1 as i32, raw.btn_lt2 as i32, raw.btn_rt1 as i32, raw.btn_rt2 as i32, raw.lb as i32, raw.rb as i32))
            .pos(dbg_x, dbg_y + ls * 2.0).anchor(0.0, 0.5).size(0.2).color(col).no_baseline().draw();
        ui.text(format!("Face: Y={} A={} X={} B={}  DPad: U={} D={} L={} R={}", raw.north as i32, raw.south as i32, raw.west as i32, raw.east as i32, raw.dpad_up as i32, raw.dpad_down as i32, raw.dpad_left as i32, raw.dpad_right as i32))
            .pos(dbg_x, dbg_y + ls * 3.0).anchor(0.0, 0.5).size(0.2).color(col).no_baseline().draw();
        ui.text(format!("Connected:{} GilrsOK:{}", raw.connected as i32, raw.gilrs_ready as i32))
            .pos(dbg_x, dbg_y + ls * 4.0).anchor(0.0, 0.5).size(0.2).color(col).no_baseline().draw();

        // --- Back button ---
        let btn_rect = Rect::new(cx - 0.08, sr.y + sr.h - 0.06, 0.16, 0.035);
        if ui.button("gp_test_back", btn_rect, "返回 (Esc / LB+RB长按)") {
            self.back_clicked = true;
        }

        Ok(())
    }

    fn next_scene(&mut self, _tm: &mut TimeManager) -> NextScene {
        if self.back_clicked
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