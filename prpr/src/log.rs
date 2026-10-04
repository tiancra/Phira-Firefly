//! Logging utilities.

use colored::Colorize;
use macroquad::prelude::{Color, WHITE};
use miniquad::{debug, error, info, trace, warn};
use once_cell::sync::Lazy;
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};
use tracing::{field::Visit, Level, Subscriber};
use tracing_subscriber::{prelude::*, EnvFilter, Layer};

use crate::{ext::semi_black, ui::Ui};

/// 游戏内日志叠加层最多保留的日志条数。
const OVERLAY_CAPACITY: usize = 200;

/// 叠加层每帧最多显示的日志条数（实际行数受屏幕高度限制，取二者较小值）。
const OVERLAY_VIEW_ROWS: usize = 50;

/// 叠加层单行高度（UI 归一化坐标系），需容纳当前字号并留出行间空隙。
const OVERLAY_LINE_HEIGHT: f32 = 0.04;

/// 叠加层字号。
const OVERLAY_FONT_SIZE: f32 = 0.24;

/// 叠加层顶部/底部边距。
const OVERLAY_MARGIN: f32 = 0.02;

/// 「游戏内叠加日志」启用标志，由上层主循环每帧从配置同步。
pub static LOG_OVERLAY_ENABLED: AtomicBool = AtomicBool::new(false);

/// 叠加层日志的级别，用于按级别着色。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl OverlayLevel {
    pub fn label(self) -> &'static str {
        match self {
            Self::Trace => "TRACE",
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }

    pub fn color(self) -> Color {
        match self {
            Self::Trace => Color::new(0.6, 0.6, 0.6, 1.),
            Self::Debug => Color::new(1., 0.5, 1., 1.),
            Self::Info => WHITE,
            Self::Warn => Color::new(1., 0.85, 0.3, 1.),
            Self::Error => Color::new(1., 0.35, 0.35, 1.),
        }
    }
}

/// 叠加层中的一条日志。文本为不含 ANSI 转义码的纯文本。
#[derive(Clone, Debug)]
pub struct OverlayEntry {
    pub level: OverlayLevel,
    pub text: String,
}

static OVERLAY_BUFFER: Lazy<Mutex<VecDeque<OverlayEntry>>> = Lazy::new(|| Mutex::new(VecDeque::with_capacity(OVERLAY_CAPACITY + 1)));

/// 追加一条日志到叠加层缓冲，超出容量时丢弃最旧的一条。
pub fn push_overlay(entry: OverlayEntry) {
    if let Ok(mut buf) = OVERLAY_BUFFER.lock() {
        if buf.len() >= OVERLAY_CAPACITY {
            buf.pop_front();
        }
        buf.push_back(entry);
    }
}

/// 取出最近的 `max` 条日志（由旧到新）。
pub fn overlay_entries(max: usize) -> Vec<OverlayEntry> {
    let Ok(buf) = OVERLAY_BUFFER.lock() else {
        return Vec::new();
    };
    let start = buf.len().saturating_sub(max);
    buf.iter().skip(start).cloned().collect()
}

/// 清空叠加层缓冲。
pub fn clear_overlay() {
    if let Ok(mut buf) = OVERLAY_BUFFER.lock() {
        buf.clear();
    }
}

/// 从事件中抽取 message / target / 其他字段，供控制台层与叠加层共用。
#[derive(Default)]
struct EventVisitor {
    message: Option<String>,
    target: Option<String>,
    fields: Vec<(&'static str, String)>,
}

impl Visit for EventVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        } else if !field.name().starts_with("log.") {
            self.fields.push((field.name(), value.to_string()));
        } else if field.name() == "log.target" {
            self.target = Some(value.to_string());
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        let val = format!("{value:?}");
        if field.name() == "message" {
            self.message = Some(val);
        } else if !field.name().starts_with("log.") {
            self.fields.push((field.name(), val));
        }
    }
}

struct CustomLayer;

impl<S> Layer<S> for CustomLayer
where
    S: Subscriber,
{
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        let mut v = EventVisitor::default();
        event.record(&mut v);

        let meta = event.metadata();
        let target = v.target.as_deref().unwrap_or_else(|| meta.target());
        if target.starts_with("jni::") && meta.level() >= &Level::INFO {
            return;
        }

        #[cfg(not(target_os = "android"))]
        let mut msg = format!("{:.6?} ", chrono::Utc::now()).bright_black().to_string()
            + &match *meta.level() {
                Level::TRACE => "TRACE".bright_black(),
                Level::DEBUG => "DEBUG".magenta(),
                Level::INFO => " INFO".green(),
                Level::WARN => " WARN".yellow(),
                Level::ERROR => "ERROR".red(),
            }
            .to_string()
            + " ";

        #[cfg(target_os = "android")]
        let mut msg = String::new();

        msg += &target.bright_black().to_string();
        if !v.fields.is_empty() {
            msg += &"{".bold().to_string();
            for (name, val) in &v.fields {
                use std::fmt::Write;
                let _ = write!(msg, "{}={val} ", name.italic());
            }
            if !v.fields.is_empty() {
                msg.pop();
            }
            msg += &"}".bold().to_string();
        }
        if let Some(message) = v.message {
            msg += ": ";
            msg += &message;
        }

        match *meta.level() {
            Level::TRACE => trace!("{}", msg),
            Level::DEBUG => debug!("{}", msg),
            Level::INFO => info!("{}", msg),
            Level::WARN => warn!("{}", msg),
            Level::ERROR => error!("{}", msg),
        }
    }
}

/// 把日志同时写入内存环形缓冲，供「游戏内叠加日志」读取。
/// 缓冲中的文本为纯文本（不含 ANSI 颜色转义码），可安全地渲染到屏幕上。
struct OverlayLayer;

impl<S> Layer<S> for OverlayLayer
where
    S: Subscriber,
{
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        let mut v = EventVisitor::default();
        event.record(&mut v);

        let meta = event.metadata();
        let target = v.target.as_deref().unwrap_or_else(|| meta.target());
        if target.starts_with("jni::") && meta.level() >= &Level::INFO {
            return;
        }

        let level = match *meta.level() {
            Level::TRACE => OverlayLevel::Trace,
            Level::DEBUG => OverlayLevel::Debug,
            Level::INFO => OverlayLevel::Info,
            Level::WARN => OverlayLevel::Warn,
            Level::ERROR => OverlayLevel::Error,
        };

        let mut text = format!(
            "{} {:>5} {}: ",
            chrono::Local::now().format("%H:%M:%S%.3f"),
            level.label(),
            target,
        );
        if let Some(message) = &v.message {
            text += message;
        }
        if !v.fields.is_empty() {
            use std::fmt::Write;
            if v.message.is_some() {
                text += " ";
            }
            text.push('{');
            for (i, (name, val)) in v.fields.iter().enumerate() {
                if i != 0 {
                    text += ", ";
                }
                let _ = write!(text, "{name}={val}");
            }
            text.push('}');
        }

        push_overlay(OverlayEntry { level, text });
    }
}

pub fn register() {
    let filter = if std::env::var("RUST_LOG").is_ok() {
        EnvFilter::from_default_env()
    } else {
        EnvFilter::try_new("hyper=info,rustls=info,debug").unwrap()
    };
    tracing_subscriber::registry()
        .with(CustomLayer)
        .with(OverlayLayer)
        .with(filter)
        .init();
}

/// 渲染游戏内日志叠加层：全窗口半透明黑底铺底，日志从上到下排列（旧→新），
/// 最新一行始终保持在屏幕内（固定从顶部排布最近若干条，自动滚动跟随尾部）。
/// 本函数仅负责绘制，不处理任何输入事件，叠加层天然可穿透。
/// 由上层主循环在所有场景之上调用，全平台生效。
pub fn render_overlay(ui: &mut Ui) {
    if !LOG_OVERLAY_ENABLED.load(Ordering::Relaxed) {
        return;
    }

    let screen = ui.screen_rect();
    let left = screen.left() + OVERLAY_MARGIN;
    let top = screen.top() + OVERLAY_MARGIN;
    let width = screen.w - OVERLAY_MARGIN * 2.;

    // 可用可视高度内最多能放几行，保证最后一行始终在屏幕内
    let avail_h = screen.h - OVERLAY_MARGIN * 2.;
    let max_rows = (avail_h / OVERLAY_LINE_HEIGHT).floor().max(1.) as usize;
    let rows = OVERLAY_VIEW_ROWS.min(max_rows);

    let entries = overlay_entries(rows);
    if entries.is_empty() {
        return;
    }

    // 全窗口半透明黑底铺底（覆盖整个游戏窗口，透明度较低以免遮挡画面）
    ui.abs_scope(|ui| {
        ui.fill_rect(screen, semi_black(0.3));
    });

    // 日志从上到下显示，旧→新；超出可视高度时从顶部挤出旧行，最新行始终在屏幕内
    for (i, entry) in entries.iter().enumerate() {
        let y = top + (i as f32 + 0.8) * OVERLAY_LINE_HEIGHT;
        ui.text(entry.text.as_str())
            .pos(left, y)
            .anchor(0., 0.)
            .size(OVERLAY_FONT_SIZE)
            .color(entry.level.color())
            .max_width(width)
            .draw();
    }
}
