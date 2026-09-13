import io
p = 'phira/src/lib.rs'
c = io.open(p, 'r', encoding='utf-8').read()
old = '''    data.init().await?;
    set_data(data);
    sync_data();
    save_data()?;

    #[cfg(target_os = "windows")]'''
new = '''    data.init().await?;
    set_data(data);
    sync_data();
    save_data()?;

    // 初始化渲染后端（wgpu/Vulkan 或 OpenGL 兜底）
    let selected_backend = get_data().config.render_backend;
    prpr::render_backend::init_render_backend(selected_backend);

    #[cfg(target_os = "windows")]'''
assert old in c, 'pattern not found'
c = c.replace(old, new)
io.open(p, 'w', encoding='utf-8', newline='').write(c)
print('OK')
