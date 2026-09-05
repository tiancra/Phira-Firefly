//! Vertex buffer types.

use crate::{Vertex, VertexBufferHandle};

/// Null vertex buffer for testing.
pub struct NullVertexBuffer;

impl VertexBufferHandle for NullVertexBuffer {
    fn set_data(&self, _vertices: &[Vertex], _indices: &[u16]) {}
    fn raw_handle(&self) -> u64 { 0 }
}
