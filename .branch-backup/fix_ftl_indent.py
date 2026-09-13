# -*- coding: utf-8 -*-
import io, os, re

base = 'phira/locales'
keys = [
    'item-render-backend',
    'item-render-backend-sub',
    'item-render-backend-auto',
    'item-render-backend-wgpu',
    'item-render-backend-opengl',
]
for lang in sorted(os.listdir(base)):
    p = os.path.join(base, lang, 'settings.ftl')
    if not os.path.exists(p):
        continue
    with io.open(p, 'r', encoding='utf-8') as f:
        lines = f.readlines()
    changed = False
    for i, line in enumerate(lines):
        stripped = line.lstrip()
        if any(stripped.startswith(k + ' ') or stripped.startswith(k + '=') for k in keys):
            if line != stripped:
                lines[i] = stripped
                changed = True
    if changed:
        with io.open(p, 'w', encoding='utf-8', newline='') as f:
            f.writelines(lines)
        print('fixed', lang)
    else:
        print('ok   ', lang)
