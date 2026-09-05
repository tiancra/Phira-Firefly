# -*- coding: utf-8 -*-
import io
p = 'prpr/src/config.rs'
with io.open(p, 'r', encoding='utf-8-sig') as f:
    c = f.read()
old = """#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DynamicBackgroundMode {
    #[default]
    Off,
    StaticBrightness,
    DynamicBrightness,
}"""
new = old + """

/// Rendering backend selection.
///
/// - `Auto`: prefer wgpu (Vulkan/Metal/DX12), fall back to OpenGL ES.
/// - `Wgpu`: force the wgpu backend (Vulkan on Android/Windows).
/// - `OpenGl`: force the legacy macroquad/OpenGL ES backend.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RenderBackend {
    #[default]
    Auto,
    Wgpu,
    OpenGl,
}"""
assert old in c, 'pattern not found'
c = c.replace(old, new)

old2 = """    pub res_pack_path: Option<String>,
    pub sample_count: u32,"""
new2 = """    pub res_pack_path: Option<String>,
    pub render_backend: RenderBackend,
    pub sample_count: u32,"""
assert old2 in c, 'pattern2 not found'
c = c.replace(old2, new2)

old3 = """            res_pack_path: None,
            sample_count: 1,"""
new3 = """            res_pack_path: None,
            render_backend: RenderBackend::default(),
            sample_count: 1,"""
assert old3 in c, 'pattern3 not found'
c = c.replace(old3, new3)

with io.open(p, 'w', encoding='utf-8', newline='') as f:
    f.write(c)
print('OK')
