use macroquad::prelude::*;
use macroquad::text::{draw_text, measure_text};
use prpr::{
    ext::draw_text_aligned,
    ui::{TextPainter, Ui},
};
use std::{sync::Mutex, time::SystemTime};

pub struct CrashInfo {
    pub code: String,
    pub message: String,
    pub panic_info: String,
    pub timestamp: String,
}

pub static CRASH_INFO: Mutex<Option<CrashInfo>> = Mutex::new(None);
pub static CRASH_LOG_PATH: Mutex<String> = Mutex::new(String::new());

const ERR_RENDER: &str = "0001";
const ERR_AUDIO: &str = "0002";
const ERR_RESOURCE: &str = "0003";
const ERR_MEMORY: &str = "0004";
const ERR_NETWORK: &str = "0005";
const ERR_UNKNOWN: &str = "0999";

pub fn set_log_path(path: &str) {
    if let Ok(mut guard) = CRASH_LOG_PATH.lock() {
        *guard = path.to_owned();
    }
}

pub fn classify_panic(info: &str) -> (&'static str, &'static str) {
    let lower = info.to_lowercase();
    if lower.contains("texture")
        || lower.contains("render")
        || lower.contains("opengl")
        || lower.contains("gpu")
        || lower.contains("shader")
        || lower.contains("gl")
        || lower.contains("draw")
    {
        (ERR_RENDER, "游戏渲染引擎发生严重错误，图形系统无法正常工作。建议更新显卡驱动或降低画质设置后重试。")
    } else if lower.contains("audio") || lower.contains("sound") || lower.contains("music") || lower.contains("playback") || lower.contains("sfx") {
        (ERR_AUDIO, "音频系统出现崩溃，无法正常播放音乐或音效。请检查音频设备是否被其他程序占用。")
    } else if lower.contains("load")
        || lower.contains("file")
        || lower.contains("assets")
        || lower.contains("resource")
        || lower.contains("missing")
        || lower.contains("not found")
        || lower.contains("不存在")
    {
        (ERR_RESOURCE, "游戏资源加载失败，可能由于文件损坏或缺失。建议验证游戏文件完整性后重试。")
    } else if lower.contains("memory") || lower.contains("alloc") || lower.contains("out of memory") || lower.contains("failed to allocate") {
        (ERR_MEMORY, "系统内存不足，无法继续运行游戏。请关闭其他程序释放内存后重试。")
    } else if lower.contains("network")
        || lower.contains("connection")
        || lower.contains("http")
        || lower.contains("request")
        || lower.contains("timeout")
        || lower.contains("dns")
    {
        (ERR_NETWORK, "网络连接异常，无法与服务器通信。请检查网络连接后重试。")
    } else {
        (ERR_UNKNOWN, "无法预计的游戏程序失败")
    }
}

pub fn write_crash_log(info: &CrashInfo) {
    let log = format!("[{}]\n错误代码: {}\n错误描述: {}\n崩溃信息: {}\n", info.timestamp, info.code, info.message, info.panic_info);
    let path = CRASH_LOG_PATH.lock().unwrap();
    let path = if path.is_empty() { "crash.log".to_owned() } else { path.clone() };
    let _ = std::fs::write(&path, log);
}

pub fn set_panic_hook() {
    std::panic::set_hook(Box::new(|info: &std::panic::PanicHookInfo| {
        let panic_msg = if let Some(s) = info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Unknown panic".to_string()
        };

        let detailed = if let Some(location) = info.location() {
            format!("{} at {}:{}", panic_msg, location.file(), location.line())
        } else {
            panic_msg
        };

        let (code, message) = classify_panic(&detailed);
        let crash_info = CrashInfo {
            code: code.to_string(),
            message: message.to_string(),
            panic_info: detailed,
            timestamp: format!("{:?}", SystemTime::now()),
        };
        write_crash_log(&crash_info);

        let mut guard = CRASH_INFO.lock().unwrap();
        *guard = Some(crash_info);
    }));
}

/// 手动设置非 panic 导致的崩溃信息（如启动时资源加载失败）
pub fn set_error(error_msg: &str) {
    let (code, message) = classify_panic(error_msg);
    let crash_info = CrashInfo {
        code: code.to_string(),
        message: message.to_string(),
        panic_info: error_msg.to_string(),
        timestamp: format!("{:?}", SystemTime::now()),
    };
    write_crash_log(&crash_info);
    if let Ok(mut guard) = CRASH_INFO.lock() {
        *guard = Some(crash_info);
    }
}

pub fn render_crash_screen(logo: Option<Texture2D>, painter: &mut TextPainter) {
    clear_background(WHITE);

    let mut ui = Ui::new(painter, None);
    let top = ui.top;

    push_camera_state();
    set_camera(&ui.camera());

    let info = CRASH_INFO.lock().unwrap();
    let Some(info) = info.as_ref() else {
        pop_camera_state();
        return;
    };

    // Logo（最上方，保持比例，不裁切）
    let _logo_bottom = if let Some(logo) = logo {
        let logo_w = logo.width();
        let logo_h = logo.height();
        let max_ndc_w = 1.2;
        let max_ndc_h = top * 0.90;
        let logo_ndc_w = logo_w / (screen_width() / 2.0);
        let logo_ndc_h = logo_h / (screen_width() / 2.0);
        let scale = (max_ndc_w / logo_ndc_w).min(max_ndc_h / logo_ndc_h).min(1.0);
        let ndc_w = logo_ndc_w * scale;
        let ndc_h = logo_ndc_h * scale;
        let x = -ndc_w / 2.0;
        let y = -top * 0.5 - ndc_h / 2.0;
        draw_texture_ex(
            logo,
            x,
            y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(ndc_w, ndc_h)),
                ..Default::default()
            },
        );
        Some(y + ndc_h)
    } else {
        None
    };

    // 文字组固定在屏幕下半部分的1/4处，组内间隔小
    let version_y = top * 0.40;
    let error_y = version_y + 0.07;
    let message_y = error_y + 0.09;

    // PHIRA-FIREFLY {version}
    let version = env!("CARGO_PKG_VERSION");
    let version_text = format!("PHIRA-FIREFLY {}", version);
    draw_text_aligned(&mut ui, &version_text, 0.0, version_y, (0.5, 0.5), 0.5, Color::new(0.0, 0.0, 0.0, 1.0));

    // 红色呼吸效果（从 #F00 到 #F80）
    let t = get_time();
    let depth = ((t * 8.0).sin() as f32 * 0.5 + 0.5) * 8.0;
    let g = depth / 8.0;
    let red_color = Color::new(1.0, g, 0.0, 1.0);

    // ERROR 错误代码
    let title = format!("ERROR {}", info.code);
    draw_text_aligned(&mut ui, &title, 0.0, error_y, (0.5, 0.5), 0.72, red_color);

    // 错误信息（换行）
    ui.text(&info.message)
        .pos(0.0, message_y)
        .anchor(0.5, 0.5)
        .size(0.38)
        .color(red_color)
        .multiline()
        .max_width(1.6)
        .draw();

    pop_camera_state();
}

/// 当 TextPainter/字体不可用时使用的备用崩溃界面渲染（使用 macroquad 默认字体）
pub fn render_crash_screen_fallback(logo: Option<Texture2D>) {
    clear_background(WHITE);

    let sw = screen_width();
    let sh = screen_height();

    // Logo（最上方，保持比例，不裁切）
    if let Some(logo) = logo {
        let logo_w = logo.width();
        let logo_h = logo.height();
        let max_w = sw * 0.60;
        let max_h = sh * 0.35;
        let scale = (max_w / logo_w).min(max_h / logo_h).min(1.0);
        let w = logo_w * scale;
        let h = logo_h * scale;
        let x = (sw - w) / 2.0;
        let y = sh * 0.25 - h / 2.0;
        draw_texture_ex(
            logo,
            x,
            y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(w, h)),
                ..Default::default()
            },
        );
    }

    let info = CRASH_INFO.lock().unwrap();
    let Some(info) = info.as_ref() else {
        return;
    };

    // PHIRA-FIREFLY {version}
    let version_text = format!("PHIRA-FIREFLY {}", env!("CARGO_PKG_VERSION"));
    let version_size = sh * 0.035;
    let version_dims = measure_text(&version_text, None, version_size as u16, 1.0);
    draw_text(&version_text, (sw - version_dims.width) / 2.0, sh * 0.55, version_size, Color::new(0.0, 0.0, 0.0, 1.0));

    // 红色呼吸效果
    let t = get_time();
    let depth = ((t * 8.0).sin() as f32 * 0.5 + 0.5) * 8.0;
    let g = depth / 8.0;
    let red_color = Color::new(1.0, g, 0.0, 1.0);

    // ERROR 错误代码
    let title = format!("ERROR {}", info.code);
    let error_size = sh * 0.050;
    let error_dims = measure_text(&title, None, error_size as u16, 1.0);
    draw_text(&title, (sw - error_dims.width) / 2.0, sh * 0.65, error_size, red_color);

    // 错误信息（自动换行）
    let message_size = sh * 0.025;
    let max_text_width = sw * 0.85;
    let line_height = message_size * 1.3;
    let mut cur_y = sh * 0.72;
    for line in wrap_text(&info.message, message_size as u16, max_text_width) {
        let dims = measure_text(&line, None, message_size as u16, 1.0);
        draw_text(&line, (sw - dims.width) / 2.0, cur_y, message_size, red_color);
        cur_y += line_height;
    }
}

fn wrap_text(text: &str, font_size: u16, max_width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut current_line = String::new();
        for ch in paragraph.chars() {
            let test = format!("{}{}", current_line, ch);
            let w = measure_text(&test, None, font_size, 1.0).width;
            if w > max_width && !current_line.is_empty() {
                lines.push(current_line);
                current_line = String::new();
            }
            current_line.push(ch);
        }
        if !current_line.is_empty() {
            lines.push(current_line);
        }
    }
    lines
}
