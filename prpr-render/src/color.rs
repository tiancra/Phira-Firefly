//! RGBA color type, compatible with macroquad's Color.

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const WHITE: Self = Self { r: 1., g: 1., b: 1., a: 1. };
    pub const BLACK: Self = Self { r: 0., g: 0., b: 0., a: 1. };
    pub const TRANSPARENT: Self = Self { r: 0., g: 0., b: 0., a: 0. };
    pub const RED: Self = Self { r: 1., g: 0., b: 0., a: 1. };
    pub const GREEN: Self = Self { r: 0., g: 1., b: 0., a: 1. };
    pub const BLUE: Self = Self { r: 0., g: 0., b: 1., a: 1. };

    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub fn from_rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: r as f32 / 255.,
            g: g as f32 / 255.,
            b: b as f32 / 255.,
            a: a as f32 / 255.,
        }
    }

    pub fn from_hex_rgb(hex: u32) -> Self {
        Self {
            r: ((hex >> 16) & 0xff) as f32 / 255.,
            g: ((hex >> 8) & 0xff) as f32 / 255.,
            b: (hex & 0xff) as f32 / 255.,
            a: 1.,
        }
    }

    pub fn from_hex_argb(hex: u32) -> Self {
        Self {
            a: ((hex >> 24) & 0xff) as f32 / 255.,
            r: ((hex >> 16) & 0xff) as f32 / 255.,
            g: ((hex >> 8) & 0xff) as f32 / 255.,
            b: (hex & 0xff) as f32 / 255.,
        }
    }

    pub fn to_rgba8(&self) -> [u8; 4] {
        [
            (self.r * 255.) as u8,
            (self.g * 255.) as u8,
            (self.b * 255.) as u8,
            (self.a * 255.) as u8,
        ]
    }
}

impl From<[f32; 4]> for Color {
    fn from(c: [f32; 4]) -> Self {
        Self { r: c[0], g: c[1], b: c[2], a: c[3] }
    }
}
