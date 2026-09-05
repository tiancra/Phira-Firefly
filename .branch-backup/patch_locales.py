# -*- coding: utf-8 -*-
import io, os

entries = {
    'zh-CN': """  item-render-backend = 渲染后端
  item-render-backend-sub = 重启后生效
  item-render-backend-auto = 自动
  item-render-backend-wgpu = Vulkan
  item-render-backend-opengl = OpenGL""",
    'zh-TW': """  item-render-backend = 渲染後端
  item-render-backend-sub = 重啟後生效
  item-render-backend-auto = 自動
  item-render-backend-wgpu = Vulkan
  item-render-backend-opengl = OpenGL""",
    'zh-LZH': """  item-render-backend = 渲染后端
  item-render-backend-sub = 重启后生效
  item-render-backend-auto = 自动
  item-render-backend-wgpu = Vulkan
  item-render-backend-opengl = OpenGL""",
    'en-US': """  item-render-backend = Render Backend
  item-render-backend-sub = Requires restart to take effect
  item-render-backend-auto = Auto
  item-render-backend-wgpu = Vulkan
  item-render-backend-opengl = OpenGL""",
}

base = 'phira/locales'
for lang in sorted(os.listdir(base)):
    p = os.path.join(base, lang, 'settings.ftl')
    if not os.path.exists(p):
        continue
    with io.open(p, 'r', encoding='utf-8') as f:
        c = f.read()
    if 'item-render-backend =' in c:
        print('skip', lang)
        continue
    entry = entries.get(lang, entries['en-US'])
    if not c.endswith('\n'):
        c += '\n'
    c += '\n' + entry + '\n'
    with io.open(p, 'w', encoding='utf-8', newline='') as f:
        f.write(c)
    print('patched', lang)
