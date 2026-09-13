//! Render target types.

use crate::{RenderTargetHandle, TextureHandle};

/// Null render target for testing.
pub struct NullRenderTarget {
    width: u32,
    height: u32,
}

impl NullRenderTarget {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
}

impl RenderTargetHandle for NullRenderTarget {
    fn width(&self) -> u32 { self.width }
    fn height(&self) -> u32 { self.height }
    fn texture(&self) -> &dyn TextureHandle {
        static DUMMY: crate::texture::NullTexture = crate::texture::NullTexture::new(1, 1, crate::TextureFormat::RGBA8);
        &DUMMY
    }
    fn raw_handle(&self) -> u64 { 0 }
}
