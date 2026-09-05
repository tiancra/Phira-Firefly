# -*- coding: utf-8 -*-
import io
p = 'phira/src/page/settings.rs'
c = io.open(p, 'r', encoding='utf-8').read()

# 1. field
old = "    show_avg_fps_btn: DRectButton,\n    dc_pause_btn: DRectButton,"
new = "    show_avg_fps_btn: DRectButton,\n    perf_monitor_btn: DRectButton,\n    dc_pause_btn: DRectButton,"
assert old in c, 'field'
c = c.replace(old, new)

# 2. init
old = "            show_avg_fps_btn: DRectButton::new(),\n            dc_pause_btn: DRectButton::new(),"
new = "            show_avg_fps_btn: DRectButton::new(),\n            perf_monitor_btn: DRectButton::new(),\n            dc_pause_btn: DRectButton::new(),"
assert old in c, 'init'
c = c.replace(old, new)

# 3. touch: after show_avg_fps
old = """        if self.show_avg_fps_btn.touch(touch, t) {
            config.show_avg_fps ^= true;
            return Ok(Some(true));
        }
        if self.dc_pause_btn.touch(touch, t) {"""
new = """        if self.show_avg_fps_btn.touch(touch, t) {
            config.show_avg_fps ^= true;
            return Ok(Some(true));
        }
        if self.perf_monitor_btn.touch(touch, t) {
            config.performance_monitor ^= true;
            return Ok(Some(true));
        }
        if self.dc_pause_btn.touch(touch, t) {"""
assert old in c, 'touch'
c = c.replace(old, new)

# 4. render: after show_avg_fps item
old = """        item! {
            render_title(ui, tl!("item-show-avg-fps"), Some(tl!("item-show-avg-fps-sub")));
            render_switch(ui, rr, t, &mut self.show_avg_fps_btn, config.show_avg_fps);
        }
        item! {
            render_title(ui, tl!("item-dc-pause"), None);"""
new = """        item! {
            render_title(ui, tl!("item-show-avg-fps"), Some(tl!("item-show-avg-fps-sub")));
            render_switch(ui, rr, t, &mut self.show_avg_fps_btn, config.show_avg_fps);
        }
        item! {
            render_title(ui, tl!("item-perf-monitor"), Some(tl!("item-perf-monitor-sub")));
            render_switch(ui, rr, t, &mut self.perf_monitor_btn, config.performance_monitor);
        }
        item! {
            render_title(ui, tl!("item-dc-pause"), None);"""
assert old in c, 'render'
c = c.replace(old, new)

io.open(p, 'w', encoding='utf-8', newline='').write(c)
print('OK')
