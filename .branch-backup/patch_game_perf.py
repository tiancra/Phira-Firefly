# -*- coding: utf-8 -*-
import io
p = 'prpr/src/scene/game.rs'
c = io.open(p, 'r', encoding='utf-8').read()

old = """            draw_rectangle(-1., -ui.top, 2., ui.top * 2., Color::new(0., 0., 0., p));
            pop_camera_state();
        }
        Ok(())
    }

    fn next_scene"""

new = """            draw_rectangle(-1., -ui.top, 2., ui.top * 2., Color::new(0., 0., 0., p));
            pop_camera_state();
        }

        // 实时性能监测（左上角）
        if self.res.config.performance_monitor {
            let current_time = tm.real_time();
            let frame_delta = current_time - self.fps_last_frame_time;
            let inst_fps = if frame_delta > 0.0 { (1.0 / frame_delta) as f32 } else { 0.0 };
            crate::perf_monitor::PerfMonitor::update(current_time, inst_fps);

            let snap = crate::perf_monitor::PerfMonitor::snapshot();
            let mut lines: Vec<String> = vec![format!("FPS {:.0}", snap.fps.max(0.0).min(9999.0))];

            #[cfg(not(target_os = "android"))]
            {
                lines.push(format!("CPU {:.0}%", snap.cpu_usage));
                lines.push(format!("MEM {:.0}%", snap.mem_usage));
                lines.push(format!("GPU {:.0}%", snap.gpu_usage));
                lines.push(format!("DISK {:.0}%", snap.disk_usage));
            }

            push_camera_state();
            set_default_camera();
            let font_size = 22.0f32;
            let line_h = 26.0f32;
            let pad_x = 10.0f32;
            let pad_y = 10.0f32;
            let bg_w = 130.0f32;
            let bg_h = pad_y * 2.0 + lines.len() as f32 * line_h;
            draw_rectangle(pad_x - 4.0, pad_y - 2.0, bg_w, bg_h, Color::new(0.0, 0.0, 0.0, 0.45));
            for (i, line) in lines.iter().enumerate() {
                draw_text(line, pad_x, pad_y + font_size + i as f32 * line_h, font_size, WHITE);
            }
            pop_camera_state();
        }
        Ok(())
    }

    fn next_scene"""

assert old in c, 'pattern not found'
c = c.replace(old, new)
io.open(p, 'w', encoding='utf-8', newline='').write(c)
print('OK')
