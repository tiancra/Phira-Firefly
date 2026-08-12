use macroquad::prelude::*;
use macroquad::text::{draw_text_ex, measure_text, TextParams};
use std::sync::atomic::{AtomicBool, Ordering};
use std::{sync::Mutex, time::SystemTime};
use tracing::error;

/// 崩溃界面是否展开显示 panic 详情（点击/触摸屏幕切换）。
static SHOW_CRASH_DETAIL: AtomicBool = AtomicBool::new(false);

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
const ERR_UNKNOWN: &str = "0010";

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
    // 追加写入而非覆盖：保留每次崩溃记录，避免后续崩溃覆盖掉之前的日志
    let log = format!(
        "========== 崩溃记录 ==========\n[{}]\n错误代码: {}\n错误描述: {}\n崩溃信息: {}\n\n",
        info.timestamp, info.code, info.message, info.panic_info
    );
    let path = CRASH_LOG_PATH.lock().unwrap();
    let path = if path.is_empty() { "crash.log".to_owned() } else { path.clone() };
    use std::io::Write;
    match std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        Ok(mut file) => {
            let _ = file.write_all(log.as_bytes());
        }
        Err(_) => {
            // 打开失败（如目录不可写）时退回覆盖写入
            let _ = std::fs::write(&path, log);
        }
    }
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

        // 捕获调用栈并随日志保存，便于在无崩溃界面文字的情况下定位崩溃点
        let backtrace = std::backtrace::Backtrace::force_capture();
        let log_detail = format!("{}\n\n调用栈:\n{}", detailed, backtrace);

        let (code, message) = classify_panic(&detailed);
        let crash_info = CrashInfo {
            code: code.to_string(),
            message: message.to_string(),
            panic_info: log_detail,
            timestamp: format!("{:?}", SystemTime::now()),
        };
        write_crash_log(&crash_info);

        let mut guard = CRASH_INFO.lock().unwrap();
        *guard = Some(crash_info);
    }));
}

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

/// 重置被崩溃污染的 GL 状态。
///
/// Android 上游玩中崩溃时，GL 状态会残留游戏的相机矩阵 / 离屏 FBO / 视口 / 裁剪区域。
/// 若不重置，后续 clear 与绘制会进入离屏纹理或被错误投影变换，最终屏幕一片空白。
/// 因此每次绘制崩溃界面（以及崩溃后的黑屏阶段）前都必须先调用本函数恢复默认渲染状态。
pub fn reset_gl_state() {
    set_default_camera();
    unsafe { get_internal_gl() }.quad_gl.viewport(None);
    unsafe { get_internal_gl() }.quad_gl.scissor(None);
}

pub fn render_crash_screen(logo: Option<Texture2D>, font: Option<macroquad::text::Font>) {
    reset_gl_state();
    clear_background(WHITE);

    // font 为 None（如 font.ttf 缺失）时回退到宏内置字体（TextParams::default 的 Font(0)）
    let font = font.unwrap_or_default();

    let sw = screen_width();
    let sh = screen_height();

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

    // 即使 CRASH_INFO 尚未写入（极端情况），也必须保证崩溃界面有文字，
    // 避免只绘制 logo 后提前 return 导致"有图无字"。
    let (code, message, panic_info) = {
        let guard = CRASH_INFO.lock().unwrap();
        match guard.as_ref() {
            Some(i) => (i.code.clone(), i.message.clone(), i.panic_info.clone()),
            None => {
                error!("CRASH_INFO is empty when rendering crash screen, using fallback text");
                (
                    "0010".to_owned(),
                    "无法预计的游戏程序失败".to_owned(),
                    String::new(),
                )
            }
        }
    };

    let version_text = format!("PHIRA-FIREFLY {}", env!("CARGO_PKG_VERSION"));
    let version_size = sh * 0.035;
    let version_dims = measure_text(&version_text, Some(font), version_size as u16, 1.0);
    draw_text_ex(
        &version_text,
        (sw - version_dims.width) / 2.0,
        sh * 0.55,
        TextParams {
            font,
            font_size: version_size as u16,
            font_scale: 1.0,
            color: Color::new(0.0, 0.0, 0.0, 1.0),
            ..Default::default()
        },
    );

    let t = get_time();
    let depth = ((t * 8.0).sin() as f32 * 0.5 + 0.5) * 8.0;
    let g = depth / 8.0;
    let red_color = Color::new(1.0, g, 0.0, 1.0);

    let title = format!("ERROR {}", code);
    let error_size = sh * 0.050;
    let error_dims = measure_text(&title, Some(font), error_size as u16, 1.0);
    draw_text_ex(
        &title,
        (sw - error_dims.width) / 2.0,
        sh * 0.65,
        TextParams {
            font,
            font_size: error_size as u16,
            font_scale: 1.0,
            color: red_color,
            ..Default::default()
        },
    );

    let message_size = sh * 0.025;
    let max_text_width = sw * 0.85;
    let line_height = message_size * 1.3;
    let mut cur_y = sh * 0.72;
    for line in wrap_text(&message, font, message_size as u16, max_text_width) {
        let dims = measure_text(&line, Some(font), message_size as u16, 1.0);
        draw_text_ex(
            &line,
            (sw - dims.width) / 2.0,
            cur_y,
            TextParams {
                font,
                font_size: message_size as u16,
                font_scale: 1.0,
                color: red_color,
                ..Default::default()
            },
        );
        cur_y += line_height;
    }

    // 点击/触摸屏幕切换显示真实崩溃详情（panic 消息 + 调用栈），便于无电脑时直接在手机上查看
    if is_mouse_button_pressed(MouseButton::Left)
        || is_mouse_button_pressed(MouseButton::Right)
        || is_key_pressed(KeyCode::Space)
    {
        SHOW_CRASH_DETAIL.fetch_xor(true, Ordering::Relaxed);
    }

    if SHOW_CRASH_DETAIL.load(Ordering::Relaxed) {
        if !panic_info.is_empty() {
            let panic_size = sh * 0.018;
            let panic_height = panic_size * 1.2;
            cur_y += sh * 0.012;
            let panic_lines = wrap_text(&panic_info, font, panic_size as u16, max_text_width);
            for line in panic_lines.iter().take(14) {
                let dims = measure_text(line, Some(font), panic_size as u16, 1.0);
                draw_text_ex(
                    line,
                    (sw - dims.width) / 2.0,
                    cur_y,
                    TextParams {
                        font,
                        font_size: panic_size as u16,
                        font_scale: 1.0,
                        color: Color::new(0.3, 0.3, 0.3, 1.0),
                        ..Default::default()
                    },
                );
                cur_y += panic_height;
            }
        }
    } else {
        // 默认干净界面，仅显示一行轻提示
        let hint = "点击屏幕查看崩溃详情";
        let hint_size = sh * 0.022;
        let hint_dims = measure_text(hint, Some(font), hint_size as u16, 1.0);
        draw_text_ex(
            hint,
            (sw - hint_dims.width) / 2.0,
            sh * 0.90,
            TextParams {
                font,
                font_size: hint_size as u16,
                font_scale: 1.0,
                color: Color::new(0.55, 0.55, 0.55, 1.0),
                ..Default::default()
            },
        );
    }
}

fn wrap_text(text: &str, font: macroquad::text::Font, font_size: u16, max_width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut current_line = String::new();
        for ch in paragraph.chars() {
            let test = format!("{}{}", current_line, ch);
            let w = measure_text(&test, Some(font), font_size, 1.0).width;
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
