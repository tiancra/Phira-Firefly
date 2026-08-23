//! Internal test build watermark and dialog support.
//! Only compiled when the `intest` feature is enabled.

use macroquad::prelude::*;
use std::sync::Mutex;

static INTEST_FONT: Mutex<Option<Font>> = Mutex::new(None);

const WATERMARK_TEXT: &str = concat!(
    "Phira-Firefly ",
    env!("CARGO_PKG_VERSION"),
    "\n仅供内部测试使用，严禁对外传播"
);

/// Load the font used for watermark rendering and register the render hook.
/// Called once at startup.
pub async fn init() {
    for name in ["font.ttf", "bold.ttf", "phigros.ttf"] {
        if let Ok(bytes) = crate::load_file(name).await {
            if let Ok(font) = load_ttf_font_from_bytes(&bytes) {
                *INTEST_FONT.lock().unwrap() = Some(font);
                break;
            }
        }
    }
    // Register the hook so watermark renders inside Main::render's top-level pass.
    prpr::scene::set_extra_top_render(Some(render_watermark));
}

/// Render the tiled diagonal watermark. Called via the Main::render hook.
/// At this point push_camera_state() has saved the UI camera; we switch to
/// the default camera for pixel-coordinate text, and pop_camera_state()
/// will restore everything afterwards.
fn render_watermark() {
    let font_guard = INTEST_FONT.lock().unwrap();
    let Some(font_ref) = font_guard.as_ref() else {
        return;
    };
    let font = font_ref.clone();

    set_default_camera();
    unsafe {
        get_internal_gl().flush();
    }

    let sw = screen_width();
    let sh = screen_height();
    let rotation = 30.0_f32.to_radians();
    let font_size = (sw * 0.022).max(12.0);
    let color = Color::new(1.0, 1.0, 1.0, 0.12);

    let step_x = sw * 0.28;
    let step_y = sh * 0.32;
    let margin = sw.max(sh);

    let params = TextParams {
        font,
        font_size: font_size as u16,
        font_scale: 1.0,
        font_scale_aspect: 1.0,
        rotation,
        color,
    };

    let mut y = -margin;
    while y < sh + margin {
        let mut x = -margin;
        while x < sw + margin {
            draw_text_ex(WATERMARK_TEXT, x, y, params);
            x += step_x;
        }
        y += step_y;
    }

    unsafe {
        get_internal_gl().flush();
    }
}
