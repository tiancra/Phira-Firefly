use crate::{
    ext::{RectExt, SafeTexture, ScaleType},
    ui::{Ui, PREFER_XCHS_UI},
};
use macroquad::prelude::*;
use std::{
    mem::ManuallyDrop,
    rc::{Rc, Weak},
};

pub const OUT_TIME: f32 = 0.8;
pub const PADDING: f32 = 0.02;

#[derive(Default, Clone)]
#[repr(u8)]
pub enum MessageKind {
    #[default]
    Info,
    Warn,
    Ok,
    Error,
}

impl MessageKind {
    pub fn color(&self) -> Color {
        let xchs = PREFER_XCHS_UI.load(std::sync::atomic::Ordering::Relaxed);
        match self {
            Self::Info => {
                if xchs {
                    // XCHS: 品牌粉色
                    Color::new(0.949, 0.412, 0.580, 1.)
                } else {
                    Color::new(0.16, 0.71, 0.96, 1.)
                }
            }
            Self::Warn => Color::new(1., 0.66, 0.15, 1.),
            Self::Ok => Color::new(0.4, 0.73, 0.42, 1.),
            Self::Error => Color::new(0.96, 0.26, 0.21, 1.),
        }
    }
}

pub struct Message {
    content: String,
    time: f32,
    end_time: f32,
    position: f32,
    target_position: f32,
    last_time: f32,
    width: f32,
    /// XCHS 通知卡片的高度（懒计算）
    height: f32,
    kind: MessageKind,
    handle: Weak<()>,
}

impl Message {
    pub fn new(content: String, time: f32, duration: f32, kind: MessageKind) -> (Self, MessageHandle) {
        let rc = Rc::new(());
        let handle = Rc::downgrade(&rc);
        (
            Self {
                content,
                time,
                end_time: time + duration,
                position: 0.,
                target_position: 0.,
                last_time: time,
                width: 0.,
                height: 0.,
                kind,
                handle,
            },
            MessageHandle(Some(ManuallyDrop::new(rc))),
        )
    }
}

pub struct MessageHandle(Option<ManuallyDrop<Rc<()>>>);
impl MessageHandle {
    pub fn cancel(&mut self) {
        if let Some(rc) = self.0.take() {
            ManuallyDrop::into_inner(rc);
        }
    }
}

pub struct BillBoard {
    messages: Vec<Message>,
    icons: Option<[SafeTexture; 4]>,
}

impl Default for BillBoard {
    fn default() -> Self {
        Self::new()
    }
}

impl BillBoard {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            icons: None,
        }
    }

    pub fn set_icons(&mut self, icons: [SafeTexture; 4]) {
        self.icons = Some(icons);
    }

    pub fn add(&mut self, mut msg: Message) {
        msg.position = self.messages.len() as f32;
        msg.target_position = msg.position;
        self.messages.push(msg);
    }

    pub fn render(&mut self, ui: &mut Ui, t: f32) {
        if PREFER_XCHS_UI.load(std::sync::atomic::Ordering::Relaxed) {
            self.render_xchs(ui, t);
            return;
        }
        let rt = 1. - PADDING;
        let tp = -ui.top + PADDING;
        let h = 0.1;
        let pd = 0.014;
        let rh = h + 0.02;
        let mut pos = 0;
        self.messages.retain_mut(|msg| {
            if msg.end_time > t && msg.handle.strong_count() == 0 {
                msg.end_time = t;
            }
            let rt = if t >= msg.end_time {
                let p = (t - msg.end_time) / OUT_TIME;
                if p > 1. {
                    return false;
                }
                let p = 1. - (1. - p).powi(3);
                rt + msg.width * p
            } else {
                msg.target_position = pos as f32;
                pos += 1;
                if msg.width == 0. {
                    3.
                } else {
                    let p = ((t - msg.time) / OUT_TIME).min(1.);
                    let p = (1. - p).powi(3);
                    rt + msg.width * p
                }
            };
            let p = (0.5_f32).powf((t - msg.last_time) / 0.1);
            msg.position = msg.position * p + msg.target_position * (1. - p);
            msg.last_time = t;
            let tp = tp + msg.position * rh;
            let mut tx = ui
                .text(&msg.content)
                .pos(rt - pd, tp + h / 2.)
                .anchor(1., 0.5)
                .no_baseline()
                .size(0.64)
                .max_width(0.8);
            let r = tx.measure();
            let mut r = Rect::new(r.x - pd - h, tp, r.w + pd * 2. + h, h);
            msg.width = r.w + 0.2;
            tx.ui.fill_rect(r, msg.kind.color());
            if t < msg.end_time {
                tx.ui.fill_rect(
                    Rect::new(r.x, r.bottom() - 0.01, r.w * (1. - (t - msg.time) / (msg.end_time - msg.time)), 0.01),
                    Color::new(1., 1., 1., 0.3),
                );
            }
            r.w = h;
            tx.ui.fill_rect(r, Color::new(1., 1., 1., 0.4));
            if let Some(icons) = self.icons.as_ref() {
                let r = r.feather(-0.02);
                tx.ui.fill_rect(r, (*icons[msg.kind.clone() as u8 as usize], r, ScaleType::Fit));
            }
            tx.draw();
            true
        });
    }

    /// XCHS UI: 右下角的通知卡片——左侧彩色强调条 + 图标 + 多行文字 + 底部剩余时间条，
    /// 从右侧滑入 / 滑出，多条向上堆叠。
    fn render_xchs(&mut self, ui: &mut Ui, t: f32) {
        const NOTIF_W: f32 = 0.58;
        const NOTIF_MIN_H: f32 = 0.115;
        const NOTIF_GAP: f32 = 0.012;
        const ACCENT_W: f32 = 0.007;
        const CORNER: f32 = 0.010;
        const PD: f32 = 0.016;
        const TEXT_SIZE: f32 = 0.50;
        const BG: Color = Color::new(0.13, 0.13, 0.13, 0.96);

        for msg in &mut self.messages {
            if msg.end_time > t && msg.handle.strong_count() == 0 {
                msg.end_time = t;
            }
        }
        self.messages.retain(|msg| t < msg.end_time || (t - msg.end_time) / OUT_TIME <= 1.);

        let icon_size = NOTIF_MIN_H - PD * 2.;
        let text_x_off = ACCENT_W + PD + icon_size + PD;
        let text_max_w = NOTIF_W - text_x_off - PD;

        for msg in self.messages.iter_mut() {
            if msg.height == 0. {
                let text_h = ui
                    .text(&msg.content)
                    .pos(0., 0.)
                    .anchor(0., 0.)
                    .no_baseline()
                    .size(TEXT_SIZE)
                    .max_width(text_max_w)
                    .multiline()
                    .measure()
                    .h;
                let content_h = text_h.max(icon_size);
                msg.height = (content_h + PD * 2.).max(NOTIF_MIN_H);
                msg.width = NOTIF_W;
            }
        }

        let mut y_accum = 0f32;
        for msg in self.messages.iter_mut() {
            if t < msg.end_time {
                msg.target_position = y_accum;
                y_accum += msg.height + NOTIF_GAP;
            }
        }

        let right = 1. - PADDING;
        let bottom = ui.top - PADDING;

        for msg in self.messages.iter_mut() {
            let smooth = (0.5_f32).powf((t - msg.last_time) / 0.1);
            msg.position = msg.position * smooth + msg.target_position * (1. - smooth);
            msg.last_time = t;

            let h = msg.height;
            let slide_p = if t >= msg.end_time {
                let p = (t - msg.end_time) / OUT_TIME;
                1. - (1. - p.min(1.)).powi(3)
            } else if msg.width == 0. {
                1.
            } else {
                let p = ((t - msg.time) / OUT_TIME).min(1.);
                (1. - p).powi(3)
            };

            let nx = right - NOTIF_W + NOTIF_W * slide_p;
            let ny = bottom - h - msg.position;
            let nr = Rect::new(nx, ny, NOTIF_W, h);
            let accent_color = msg.kind.color();

            ui.fill_path(&nr.rounded(CORNER), BG);
            ui.fill_path(&Rect::new(nr.x, nr.y, ACCENT_W + CORNER, h).rounded(CORNER), accent_color);
            ui.fill_rect(Rect::new(nr.x + CORNER, nr.y, ACCENT_W, h), accent_color);

            if t < msg.end_time {
                let remaining = 1. - (t - msg.time) / (msg.end_time - msg.time);
                ui.fill_rect(
                    Rect::new(nr.x, nr.bottom() - 0.004, nr.w * remaining, 0.004),
                    Color::new(1., 1., 1., 0.25),
                );
            }

            let icon_x = nr.x + ACCENT_W + PD;
            let icon_r = Rect::new(icon_x, nr.y + PD, icon_size, icon_size);
            if let Some(icons) = self.icons.as_ref() {
                ui.fill_rect(icon_r, (*icons[msg.kind.clone() as u8 as usize], icon_r, ScaleType::Fit));
            } else {
                ui.fill_rect(icon_r.feather(-icon_size * 0.3), accent_color);
            }

            ui.text(&msg.content)
                .pos(icon_x + icon_size + PD, nr.y + PD)
                .anchor(0., 0.)
                .no_baseline()
                .size(TEXT_SIZE)
                .max_width(text_max_w)
                .multiline()
                .color(WHITE)
                .draw();
        }
    }
}
