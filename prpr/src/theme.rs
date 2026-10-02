//! 主题资源覆盖层。
//!
//! 应用启动时由 phira 侧按当前主题读取 config.json，将非空资源项注册为覆盖表
//! （默认资源路径 → 主题文件绝对路径）。各资源加载点通过本模块的 load_asset_*
//! 函数优先读取主题文件，未命中时回退到默认 assets 资源。

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::RwLock,
};
use anyhow::Result;
use macroquad::prelude::*;

/// 主题覆盖表：默认资源路径（如 "background.jpg"、"rank/phi.png"）→ 主题文件绝对路径。
static THEME_OVERRIDES: RwLock<Option<HashMap<String, PathBuf>>> = RwLock::new(None);

/// 设置主题覆盖表（应用主题时调用；传空表等效于全部回退默认资源）。
pub fn set_theme_overrides(overrides: HashMap<String, PathBuf>) {
    *THEME_OVERRIDES.write().unwrap() = Some(overrides);
}

/// 清空主题覆盖表。
pub fn clear_theme_overrides() {
    *THEME_OVERRIDES.write().unwrap() = None;
}

/// 读取主题覆盖资源字节；未命中或读取失败返回 None。
fn try_load_override(name: &str) -> Option<Vec<u8>> {
    let map = THEME_OVERRIDES.read().ok()?;
    let map = map.as_ref()?;
    let path = map.get(name)?;
    std::fs::read(path).ok()
}

/// 加载资源文件：优先主题覆盖，否则回退默认 assets 资源。
pub async fn load_asset_file(name: &str) -> Result<Vec<u8>> {
    if let Some(bytes) = try_load_override(name) {
        return Ok(bytes);
    }
    Ok(load_file(name).await?)
}

/// 加载纹理：优先主题覆盖，否则回退默认 assets 资源。
pub async fn load_asset_texture(name: &str) -> Result<Texture2D> {
    if let Some(bytes) = try_load_override(name) {
        return Ok(Texture2D::from_file_with_format(&bytes, None));
    }
    Ok(load_texture(name).await?)
}

/// 加载图片（内存 Image）：优先主题覆盖，否则回退默认 assets 资源。
pub async fn load_asset_image(name: &str) -> Result<Image> {
    if let Some(bytes) = try_load_override(name) {
        return Ok(Image::from_file_with_format(&bytes, None));
    }
    Ok(load_image(name).await?)
}
