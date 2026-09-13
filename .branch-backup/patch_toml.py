# -*- coding: utf-8 -*-
import io

# workspace Cargo.toml
p = 'Cargo.toml'
c = io.open(p, 'r', encoding='utf-8').read()
old = '\t"prpr",\n\t"prpr-avc",'
new = '\t"prpr",\n\t"prpr-avc",\n\t"prpr-render",'
assert old in c, 'members pattern missing'
c = c.replace(old, new)
old2 = 'prpr-l10n = { path = "prpr-l10n" }'
new2 = 'prpr-l10n = { path = "prpr-l10n" }\nprpr-render = { path = "prpr-render" }'
assert old2 in c, 'dep pattern missing'
c = c.replace(old2, new2)
io.open(p, 'w', encoding='utf-8', newline='').write(c)
print('workspace OK')

# prpr/Cargo.toml
p = 'prpr/Cargo.toml'
c = io.open(p, 'r', encoding='utf-8-sig').read()
old = 'prpr-l10n = { workspace = true }'
new = 'prpr-l10n = { workspace = true }\nprpr-render = { workspace = true }'
assert old in c, 'prpr dep pattern missing'
c = c.replace(old, new)
io.open(p, 'w', encoding='utf-8', newline='').write(c)
print('prpr OK')
