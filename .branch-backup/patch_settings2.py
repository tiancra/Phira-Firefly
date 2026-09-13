# -*- coding: utf-8 -*-
import io
p = 'phira/src/page/settings.rs'
c = io.open(p, 'r', encoding='utf-8').read()

# 1. field type
old = "    render_backend_btn: DRectButton,\n"
new = "    render_backend_btn: ChooseButton,\n"
assert old in c, 'field'
c = c.replace(old, new)

# 2. init
old = "            render_backend_btn: DRectButton::new(),\n"
new = """            render_backend_btn: ChooseButton::new()
                .with_options(vec![
                    tl!("item-render-backend-auto").to_string(),
                    tl!("item-render-backend-wgpu").to_string(),
                    tl!("item-render-backend-opengl").to_string(),
                ])
                .with_selected(match get_data().config.render_backend {
                    RenderBackend::Auto => 0,
                    RenderBackend::Wgpu => 1,
                    RenderBackend::OpenGl => 2,
                })
                .with_bottom(false),
"""
assert old in c, 'init'
c = c.replace(old, new)

# 3. top_touch
old = """    pub fn top_touch(&mut self, touch: &Touch, t: f32) -> bool {
        if self.lang_btn.top_touch(touch, t) {
            return true;
        }
        false
    }"""
new = """    pub fn top_touch(&mut self, touch: &Touch, t: f32) -> bool {
        if self.lang_btn.top_touch(touch, t) {
            return true;
        }
        if self.render_backend_btn.top_touch(touch, t) {
            return true;
        }
        false
    }"""
assert old in c, 'top_touch'
c = c.replace(old, new)

# 4. touch: cycle -> popup
old = """        if self.render_backend_btn.touch(touch, t) {
            config.render_backend = match config.render_backend {
                RenderBackend::Auto => RenderBackend::Wgpu,
                RenderBackend::Wgpu => RenderBackend::OpenGl,
                RenderBackend::OpenGl => RenderBackend::Auto,
            };
            return Ok(Some(true));
        }"""
new = """        if self.render_backend_btn.touch(touch, t) {
            return Ok(Some(false));
        }"""
assert old in c, 'touch'
c = c.replace(old, new)

# 5. update: add update + changed handler
old = """        self.lang_btn.update(t);
        let data = get_data_mut();
        if self.lang_btn.changed() {"""
new = """        self.lang_btn.update(t);
        self.render_backend_btn.update(t);
        let data = get_data_mut();
        if self.render_backend_btn.changed() {
            data.config.render_backend = match self.render_backend_btn.selected() {
                0 => RenderBackend::Auto,
                1 => RenderBackend::Wgpu,
                _ => RenderBackend::OpenGl,
            };
            return Ok(true);
        }
        if self.lang_btn.changed() {"""
assert old in c, 'update'
c = c.replace(old, new)

# 6. render item + render_top
old = """        item! {
            let backend_name = match config.render_backend {
                RenderBackend::Auto => tl!("item-render-backend-auto"),
                RenderBackend::Wgpu => tl!("item-render-backend-wgpu"),
                RenderBackend::OpenGl => tl!("item-render-backend-opengl"),
            };
            render_title(ui, tl!("item-render-backend"), Some(tl!("item-render-backend-sub")));
            self.render_backend_btn.render_text(ui, rr, t, backend_name, 0.5, false);
        }
        self.lang_btn.render_top(ui, t, 1.);"""
new = """        item! {
            self.render_backend_btn.set_options(vec![
                tl!("item-render-backend-auto").to_string(),
                tl!("item-render-backend-wgpu").to_string(),
                tl!("item-render-backend-opengl").to_string(),
            ]);
            self.render_backend_btn.set_selected(match config.render_backend {
                RenderBackend::Auto => 0,
                RenderBackend::Wgpu => 1,
                RenderBackend::OpenGl => 2,
            });
            render_title(ui, tl!("item-render-backend"), Some(tl!("item-render-backend-sub")));
            self.render_backend_btn.render(ui, rr, t);
        }
        self.lang_btn.render_top(ui, t, 1.);
        self.render_backend_btn.render_top(ui, t, 1.);"""
assert old in c, 'render'
c = c.replace(old, new)

io.open(p, 'w', encoding='utf-8', newline='').write(c)
print('OK')
