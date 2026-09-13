//! Texture-related types and helpers.

use crate::{Color, TextureFormat, TextureHandle};
use anyhow::Result;
use async_trait::async_trait;

/// A CPU-side image that can be uploaded to a texture.
#[derive(Clone)]
pub struct ImageData {
    pub width: u32,
    pub height: u32,
    pub bytes: Vec<u8>,
    pub format: TextureFormat,
}

impl ImageData {
    pub fn from_rgba8(width: u32, height: u32, bytes: Vec<u8>) -> Self {
        Self {
            width,
            height,
            bytes,
            format: TextureFormat::RGBA8,
        }
    }

    pub fn solid(color: Color, width: u32, height: u32) -> Self {
        let [r, g, b, a] = color.to_rgba8();
        let bytes: Vec<u8> = std::iter::repeat([r, g, b, a])
            .take((width * height) as usize)
            .flatten()
            .collect();
        Self::from_rgba8(width, height, bytes)
    }
}

/// Null texture handle for testing / fallback.
pub struct NullTexture {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) format: TextureFormat,
}

impl NullTexture {
    pub const fn new(width: u32, height: u32, format: TextureFormat) -> Self {
        Self { width, height, format }
    }
}

#[async_trait]
impl TextureHandle for NullTexture {
    fn width(&self) -> u32 { self.width }
    fn height(&self) -> u32 { self.height }
    fn format(&self) -> TextureFormat { self.format }
    async fn set_data(&self, _data: &[u8]) -> Result<()> { Ok(()) }
    fn generate_mipmaps(&self) -> Result<()> { Ok(()) }
    fn set_filter(&self, _min: crate::FilterMode, _mag: crate::FilterMode) {}
    fn set_wrap(&self, _u: crate::WrapMode, _v: crate::WrapMode) {}
    fn raw_handle(&self) -> u64 { 0 }
    fn as_any(&self) -> &dyn std::any::Any { self }
}
