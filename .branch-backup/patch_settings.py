# -*- coding: utf-8 -*-
import io
p = 'phira/src/page/settings.rs'
c = io.open(p, 'r', encoding='utf-8').read()

# 1. import RenderBackend
old = 'use prpr::{\n    config::DynamicBackgroundMode,'
new = 'use prpr::{\n    config::{DynamicBackgroundMode, RenderBackend},'
assert old in c, 'import pattern missing'
c = c.replace(old, new)

# 2. add field to GeneralList
old = """    enable_anys_btn: DRectButton,
    anys_gateway_btn: DRectButton,
    watch_tutorial_btn: DRectButton,"""
new = """    enable_anys_btn: DRectButton,
    anys_gateway_btn: DRectButton,
    watch_tutorial_btn: DRectButton,
    render_backend_btn: DRectButton,"""
assert old in c, 'field pattern missing'
c = c.replace(old, new)

# 3. init in new()
old = """            enable_anys_btn: DRectButton::new(),
            anys_gateway_btn: DRectButton::new(),
            watch_tutorial_btn: DRectButton::new(),"""
new = """            enable_anys_btn: DRectButton::new(),
            anys_gateway_btn: DRectButton::new(),
            watch_tutorial_btn: DRectButton::new(),
            render_backend_btn: DRectButton::new(),"""
assert old in c, 'init pattern missing'
c = c.replace(old, new)

# 4. touch handler before watch_tutorial
old = """        if self.watch_tutorial_btn.touch(touch, t) {
            self.start_tutorial = true;
            return Ok(Some(false));
        }
        Ok(None)"""
new = """        if self.render_backend_btn.touch(touch, t) {
            config.render_backend = match config.render_backend {
                RenderBackend::Auto => RenderBackend::Wgpu,
                RenderBackend::Wgpu => RenderBackend::OpenGl,
                RenderBackend::OpenGl => RenderBackend::Auto,
            };
            return Ok(Some(true));
        }
        if self.watch_tutorial_btn.touch(touch, t) {
            self.start_tutorial = true;
            return Ok(Some(false));
        }
        Ok(None)"""
assert old in c, 'touch pattern missing'
c = c.replace(old, new)

# 5. render item after anys_gateway
old = """        item! {
            render_title(ui, tl!("item-watch-tutorial"), Some(tl!("item-watch-tutorial-sub")));
            self.watch_tutorial_btn.render_text(ui, rr, t, tl!("item-watch-tutorial-btn"), 0.5, true);
        }"""
new = """        item! {
            render_title(ui, tl!("item-watch-tutorial"), Some(tl!("item-watch-tutorial-sub")));
            self.watch_tutorial_btn.render_text(ui, rr, t, tl!("item-watch-tutorial-btn"), 0.5, true);
        }
        item! {
            let backend_name = match config.render_backend {
                RenderBackend::Auto => tl!("item-render-backend-auto"),
                RenderBackend::Wgpu => tl!("item-render-backend-wgpu"),
                RenderBackend::OpenGl => tl!("item-render-backend-opengl"),
            };
            render_title(ui, tl!("item-render-backend"), Some(tl!("item-render-backend-sub")));
            self.render_backend_btn.render_text(ui, rr, t, backend_name, 0.5, false);
        }"""
assert old in c, 'render pattern missing'
c = c.replace(old, new)

io.open(p, 'w', encoding='utf-8', newline='').write(c)
print('OK')
