//! Core math types for the rendering layer.
//!
//! These are simple value types that mirror macroquad's API surface
//! to minimize migration friction, but are backend-agnostic.

/// 2D vector.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const ZERO: Self = Self { x: 0., y: 0. };
    pub const ONE: Self = Self { x: 1., y: 1. };

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn length(self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    pub fn normalize(self) -> Self {
        let len = self.length();
        if len > 0. { Self { x: self.x / len, y: self.y / len } } else { Self::ZERO }
    }

    pub fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y
    }
}

impl std::ops::Add for Vec2 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self { Self { x: self.x + rhs.x, y: self.y + rhs.y } }
}

impl std::ops::Sub for Vec2 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self { Self { x: self.x - rhs.x, y: self.y - rhs.y } }
}

impl std::ops::Mul<f32> for Vec2 {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self { Self { x: self.x * rhs, y: self.y * rhs } }
}

impl std::ops::Div<f32> for Vec2 {
    type Output = Self;
    fn div(self, rhs: f32) -> Self { Self { x: self.x / rhs, y: self.y / rhs } }
}

/// 3D vector.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Self = Self { x: 0., y: 0., z: 0. };

    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
}

/// 4D vector.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Vec4 {
    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }
}

/// 4x4 matrix, column-major (matches wgpu/OpenGL convention).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat4 {
    pub cols: [f32; 16],
}

impl Mat4 {
    pub const IDENTITY: Self = Self {
        cols: [
            1., 0., 0., 0.,
            0., 1., 0., 0.,
            0., 0., 1., 0.,
            0., 0., 0., 1.,
        ],
    };

    pub const fn identity() -> Self {
        Self::IDENTITY
    }

    pub fn from_cols_array(cols: &[f32; 16]) -> Self {
        Self { cols: *cols }
    }

    pub fn from_translation(x: f32, y: f32, z: f32) -> Self {
        let mut m = Self::IDENTITY;
        m.cols[12] = x;
        m.cols[13] = y;
        m.cols[14] = z;
        m
    }

    pub fn from_scale(x: f32, y: f32, z: f32) -> Self {
        let mut m = Self::IDENTITY;
        m.cols[0] = x;
        m.cols[5] = y;
        m.cols[10] = z;
        m
    }

    /// Orthographic projection matrix.
    pub fn orthographic(left: f32, right: f32, bottom: f32, top: f32, near: f32, far: f32) -> Self {
        let mut m = Self::IDENTITY;
        m.cols[0] = 2. / (right - left);
        m.cols[5] = 2. / (top - bottom);
        m.cols[10] = 1. / (far - near);
        m.cols[12] = -(right + left) / (right - left);
        m.cols[13] = -(top + bottom) / (top - bottom);
        m.cols[14] = -near / (far - near);
        m
    }

    /// Perspective projection (infinite far plane, right-handed).
    pub fn perspective_infinite_rh(fov_y: f32, aspect: f32, near: f32) -> Self {
        let f = 1. / (fov_y / 2.).tan();
        let mut m = Self::IDENTITY;
        m.cols[0] = f / aspect;
        m.cols[5] = f;
        m.cols[10] = -1.;
        m.cols[11] = -1.;
        m.cols[14] = -2. * near;
        m.cols[15] = 0.;
        m
    }
}

impl std::ops::Mul for Mat4 {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        let mut result = [0f32; 16];
        for i in 0..4 {
            for j in 0..4 {
                let mut sum = 0.;
                for k in 0..4 {
                    sum += self.cols[k * 4 + i] * rhs.cols[j * 4 + k];
                }
                result[j * 4 + i] = sum;
            }
        }
        Self { cols: result }
    }
}

/// Vertex for 2D rendering: position, UV, color.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Vertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

impl Vertex {
    pub fn new(x: f32, y: f32, z: f32, u: f32, v: f32, color: super::Color) -> Self {
        Self {
            position: [x, y, z],
            uv: [u, v],
            color: [color.r, color.g, color.b, color.a],
        }
    }
}

unsafe impl bytemuck::Pod for Vertex {}
unsafe impl bytemuck::Zeroable for Vertex {}
