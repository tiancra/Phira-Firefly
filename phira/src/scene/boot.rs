use super::MainScene;
use crate::get_data;
use anyhow::Result;
use macroquad::prelude::*;
use prpr::{
    core::BOLD_FONT,
    ext::{semi_black, semi_white, SafeTexture, ScaleType},
    scene::{NextScene, Scene},
    time::TimeManager,
    ui::{FontArc, Ui, UI_AUDIO},
};
use sasa::{AudioClip, Music, MusicParams};
use std::cell::RefCell;
use tracing::warn;

/// 时间（与 `TimeManager::now()` 同基）记录主界面划入动画开始时刻。
/// 由 BootScene 在点击“点击屏幕开始”时写入，HomePage 据此计算 1 秒划入进度。
thread_local! {
    static BOOT_ENTRY_TIME: RefCell<Option<f64>> = const { RefCell::new(None) };
}

pub fn set_boot_entry_time(t: f64) {
    BOOT_ENTRY_TIME.with(|it| *it.borrow_mut() = Some(t));
}

/// 主界面划入进度：0..=1（Out Sine），未启动时返回 1（不动画）。
pub fn boot_entry_progress(t: f32, dur: f32) -> f32 {
    BOOT_ENTRY_TIME.with(|it| match *it.borrow() {
        Some(start) => ((t - start as f32) / dur).clamp(0., 1.),
        None => 1.,
    })
}

#[inline]
fn out_sine(p: f32) -> f32 {
    (p.clamp(0., 1.) * std::f32::consts::PI / 2.).sin()
}

// 时间轴（秒，自场景进入起）
const SPLASH_FADE_IN: f32 = 1.0; // 渐显 splash
const SPLASH_HOLD: f32 = 3.0; // 保持 splash
const SPLASH_FADE_OUT: f32 = 1.0; // 渐隐 splash
const WARN_FADE_IN: f32 = 1.0; // 渐显警告文字
const WARN_HOLD: f32 = 5.0; // 保持警告文字
const WARN_FADE_OUT: f32 = 1.0; // 渐隐警告文字
const BOOT_FADE_IN: f32 = 1.0; // 渐显背景 + 遮罩 + boot
const BOOT_READY: f32 = SPLASH_FADE_IN + SPLASH_HOLD + SPLASH_FADE_OUT + WARN_FADE_IN + WARN_HOLD + WARN_FADE_OUT + BOOT_FADE_IN; // 可点击
const BOOT_CLICK_FADE_OUT: f32 = 0.8; // 点击后 boot/遮罩渐隐时长
const BGM_FADE_TIME: f64 = 3.0; // 主界面 BGM 渐入时长
const SPLASH_MUSIC_FADE_OUT: f64 = 3.0; // 点击后 splash.mp3 渐隐时长

const WARNING_TEXT: &str = "开始游戏前，请仔细阅读以下内容，并确认你已了解相关注意事项。

本游戏包含快速移动、闪烁画面、动态特效、高密度节奏以及较强的音乐与音效表现，部分内容可能带来较高的视觉与听觉刺激。请根据自身情况选择合适的难度，并将设备音量调整至舒适范围。

长时间连续游玩可能造成视觉疲劳或注意力下降。建议合理安排游戏时间，并适当休息。若出现头晕、恶心、头痛、视线模糊或其他明显不适，请立即停止游戏并充分休息，不要为了成绩或排名勉强继续。";

pub struct BootScene {
    splash: SafeTexture,
    background: SafeTexture,
    boot: SafeTexture,
    splash_music: Option<Music>,
    main_scene: Option<MainScene>,

    started: f64,
    started_set: bool,
    boot_music_started: bool,
    clicked: bool,
    click_time: f64,
}

impl BootScene {
    pub async fn new(font: FontArc) -> Result<Self> {
        let main_scene = MainScene::new(font).await?;
        let splash: SafeTexture = load_texture("splash.png").await?.into();
        let background: SafeTexture = load_texture("background.jpg").await?.into();
        let boot: SafeTexture = load_texture("boot.png").await?.into();
        let splash_music = match AudioClip::new(load_file("splash.mp3").await?) {
            Ok(clip) => UI_AUDIO
                .with(|it| {
                    it.borrow_mut().create_music(
                        clip,
                        MusicParams {
                            amplifier: get_data().config.volume_bgm,
                            loop_mix_time: 0.,
                            ..Default::default()
                        },
                    )
                })
                .ok(),
            Err(err) => {
                warn!("failed to load splash music: {:?}", err);
                None
            }
        };
        Ok(Self {
            splash,
            background,
            boot,
            splash_music,
            main_scene: Some(main_scene),
            started: 0.,
            started_set: false,
            boot_music_started: false,
            clicked: false,
            click_time: 0.,
        })
    }

    fn elapsed(&self, tm: &TimeManager) -> f32 {
        (tm.now() - self.started) as f32
    }

    fn since_click(&self, tm: &TimeManager) -> f32 {
        if self.clicked {
            (tm.now() - self.click_time) as f32
        } else {
            0.
        }
    }

    fn draw_tex_centered(&self, ui: &mut Ui, tex: &SafeTexture, alpha: f32, size: f32) {
        let w = tex.width() as f32;
        let h = tex.height() as f32;
        let aspect = if h > 0. { w / h } else { 1. };
        let rw = size;
        let rh = size / aspect;
        let r = Rect::new(-rw / 2., -rh / 2., rw, rh);
        ui.alpha(alpha, |ui| {
            ui.fill_rect(r, (**tex, r, ScaleType::Fit));
        });
    }

    fn render_warning(&self, ui: &mut Ui, alpha: f32) {
        let screen = ui.screen_rect();
        let title_size = 0.6;
        let body_size = 0.4;
        let gap = 0.08;
        let max_width = 1.7;

        // 测量正文在基准字号下的高度（世界坐标），用于动态适配屏幕比例
        let body_h = ui
            .text(WARNING_TEXT)
            .pos(0., 0.)
            .anchor(0.5, 0.5)
            .size(body_size)
            .max_width(max_width)
            .multiline()
            .h_center()
            .measure()
            .h;

        // 警告块总高（标题 + 间隔 + 正文），按屏幕高度（2*top，随屏幕比例变化）缩放，
        // 使不同屏幕比例下文字都能完整显示。坐标系中屏幕宽度固定为 2，高度 = 2 * height/width，
        // 因此横屏（top 小）可用高度小，需缩小字号避免文字超出屏幕。
        let block_h = title_size + gap + body_h;
        let usable = screen.h * 0.86; // 上下各留 7% 边距
        let scale = (usable / block_h).min(1.0);

        // 标题与正文整体垂直居中于屏幕
        let title_y = (-block_h * 0.5 + title_size * 0.5) * scale;
        let body_y = (block_h * 0.5 - body_h * 0.5) * scale;

        ui.text("警告：游戏前详阅")
            .pos(0., title_y)
            .anchor(0.5, 0.5)
            .size(title_size * scale)
            .color(semi_white(0.95 * alpha))
            .no_baseline()
            .draw_using(&BOLD_FONT);
        ui.text(WARNING_TEXT)
            .pos(0., body_y)
            .anchor(0.5, 0.5)
            .size(body_size * scale)
            .max_width(max_width)
            .multiline()
            .h_center()
            .color(semi_white(0.9 * alpha))
            .draw();
    }

    fn on_click(&mut self, tm: &TimeManager) {
        if self.clicked {
            return;
        }
        self.clicked = true;
        self.click_time = tm.now();
        set_boot_entry_time(tm.now());
        // 点击瞬间：主界面 BGM 在 3 秒内渐入
        if let Some(main) = self.main_scene.as_mut() {
            main.start_boot_bgm(BGM_FADE_TIME);
        }
        // splash.mp3 在 3 秒内音量渐小并停止
        if let Some(m) = self.splash_music.as_mut() {
            let _ = m.fade_out(SPLASH_MUSIC_FADE_OUT);
        }
    }

    /// 跳过前面的黑屏/splash/警告阶段，直接进入 LOGO（背景 + boot）阶段。
    fn skip_to_logo(&mut self, tm: &TimeManager) {
        self.started = tm.now() - BOOT_READY as f64;
        // 让 update() 立即开始播放 splash.mp3
        self.boot_music_started = false;
    }
}

impl Scene for BootScene {
    fn enter(&mut self, _tm: &mut TimeManager, _target: Option<RenderTarget>) -> Result<()> {
        // started 在首次 update 时才记录，避免把 Main::new 里加载图标等耗时计入序列
        self.started_set = false;
        self.boot_music_started = false;
        self.clicked = false;
        Ok(())
    }

    fn touch(&mut self, tm: &mut TimeManager, touch: &Touch) -> Result<bool> {
        if self.clicked {
            return Ok(true);
        }
        if matches!(touch.phase, TouchPhase::Started) {
            if self.elapsed(tm) >= BOOT_READY {
                // LOGO 阶段：点击进入游戏
                self.on_click(tm);
            } else {
                // 黑屏/splash/警告阶段：点击直接跳到 LOGO 阶段
                self.skip_to_logo(tm);
            }
        }
        Ok(true)
    }

    fn update(&mut self, tm: &mut TimeManager) -> Result<()> {
        // 首次进入：以当前时刻作为序列起点
        if !self.started_set {
            self.started_set = true;
            self.started = tm.now();
        }
        // 背景/boot 渐显时开始播放 splash.mp3
        if !self.boot_music_started && self.elapsed(tm) >= BOOT_READY - BOOT_FADE_IN {
            self.boot_music_started = true;
            if let Some(m) = self.splash_music.as_mut() {
                let _ = m.fade_in(1.0);
            }
        }
        Ok(())
    }

    fn render(&mut self, tm: &mut TimeManager, ui: &mut Ui) -> Result<()> {
        set_camera(&ui.camera());
        let elapsed = self.elapsed(tm);
        let screen = ui.screen_rect();

        // 黑色背景
        ui.fill_rect(screen, BLACK);

        // splash 渐显/保持/渐隐
        if elapsed < SPLASH_FADE_IN + SPLASH_HOLD + SPLASH_FADE_OUT {
            let alpha = if elapsed < SPLASH_FADE_IN {
                out_sine(elapsed / SPLASH_FADE_IN)
            } else if elapsed < SPLASH_FADE_IN + SPLASH_HOLD {
                1.0
            } else {
                1.0 - out_sine((elapsed - SPLASH_FADE_IN - SPLASH_HOLD) / SPLASH_FADE_OUT)
            };
            self.draw_tex_centered(ui, &self.splash, alpha, 0.8);
        }

        // 警告文字
        if elapsed >= SPLASH_FADE_IN + SPLASH_HOLD + SPLASH_FADE_OUT
            && elapsed < SPLASH_FADE_IN + SPLASH_HOLD + SPLASH_FADE_OUT + WARN_FADE_IN + WARN_HOLD + WARN_FADE_OUT
        {
            let base = SPLASH_FADE_IN + SPLASH_HOLD + SPLASH_FADE_OUT;
            let alpha = if elapsed < base + WARN_FADE_IN {
                out_sine((elapsed - base) / WARN_FADE_IN)
            } else if elapsed < base + WARN_FADE_IN + WARN_HOLD {
                1.0
            } else {
                1.0 - out_sine((elapsed - base - WARN_FADE_IN - WARN_HOLD) / WARN_FADE_OUT)
            };
            self.render_warning(ui, alpha);
        }

        // 背景 + 模糊遮罩 + boot + 点击提示
        let bg_visible = elapsed >= BOOT_READY - BOOT_FADE_IN || self.clicked;
        if bg_visible {
            // 进入时背景与遮罩一起渐显；点击后 background 保持不动
            let enter_p = ((elapsed - (BOOT_READY - BOOT_FADE_IN)) / BOOT_FADE_IN).clamp(0., 1.);
            let bg_alpha = if self.clicked { 1.0 } else { out_sine(enter_p) };
            ui.alpha(bg_alpha, |ui| {
                ui.fill_rect(screen, (*self.background, screen));
            });

            // 遮罩 / boot / 文字（点击后渐隐），直接用颜色 alpha 控制透明度
            let overlay_alpha = if self.clicked {
                (1.0 - out_sine((self.since_click(tm) / BOOT_CLICK_FADE_OUT).min(1.0))).max(0.0)
            } else {
                out_sine(enter_p)
            };
            // 模糊遮罩（半透明暗色遮罩）
            ui.fill_rect(screen, semi_black(0.5 * overlay_alpha));
            // boot.png（小一点，居中偏上），固定大小
            self.draw_tex_centered(ui, &self.boot, overlay_alpha, 0.7);
            // 点击提示
            ui.text("点击屏幕开始")
                .pos(0., 0.5)
                .anchor(0.5, 0.5)
                .size(0.5)
                .color(semi_white(0.9 * overlay_alpha))
                .draw();
        }

        Ok(())
    }

    fn next_scene(&mut self, tm: &mut TimeManager) -> NextScene {
        if self.clicked && self.since_click(tm) >= BOOT_CLICK_FADE_OUT {
            if let Some(main) = self.main_scene.take() {
                return NextScene::Replace(Box::new(main));
            }
        }
        NextScene::None
    }
}
