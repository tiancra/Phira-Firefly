//! 首次启动引导场景。
//! 首次启动点击“点击屏幕开始”后，不直接进入主界面，而是进入本场景：
//! 黑色遮罩上从右侧滑入引导内容（欢迎 → 选择语言 → [内测验证] → 设置延迟 → 登录 → 新手教程 → 设置完成），
//! 完成后 Replace 为预加载好的 MainScene，并重新触发主界面入场动画。

prpr_l10n::tl_file!("onboarding" obtl);

use super::MainScene;
use crate::{
    client::{Client, LoginParams, User, UserManager},
    get_data, get_data_mut, save_data,
    popup::ChooseButton,
    scene::{
        dispatch_tos_task, load_tos_and_policy, TutorialLoadingScene, BGM_VOLUME_UPDATED, JUST_ACCEPTED_TOS,
        JUST_LOADED_TOS, TERMS,
    },
    sync_data,
};
use anyhow::{Context, Result};
use inputbox::{InputBox, InputMode};
use macroquad::prelude::*;
use prpr::{
    config::DynamicBackgroundMode,
    core::{ParticleEmitter, ResourcePack, NOTE_WIDTH_RATIO_BASE, BOLD_FONT},
    ext::{create_audio_manger, open_url, poll_future, semi_black, semi_white, LocalTask, RectExt, SafeTexture, ScaleType},
    scene::{request_input, return_input, show_error, show_message, take_input, NextScene, Scene},
    task::Task,
    time::TimeManager,
    ui::{button_hit, DRectButton, Dialog, Slider, Scroll, Ui},
};
use prpr_l10n::{LANGS, LANG_NAMES};
use sasa::{AudioClip, AudioManager, Music, MusicParams, PlaySfxParams, Sfx};
use std::{
    borrow::Cow,
    cell::RefCell,
    net::ToSocketAddrs,
    sync::atomic::Ordering,
};

/// 内测资格验证服务地址（exam 后端，正式环境）。
pub const EXAM_API_URL: &str = "https://pf.tianstudio.top";

// 主界面预加载传送槽：BootScene 创建 MainScene 后放入，引导结束时取出 Replace。
thread_local! {
    pub static PENDING_MAIN: RefCell<Option<MainScene>> = const { RefCell::new(None) };
}

const TRANS_TIME: f32 = 0.35; // 页间右滑动画时长
const SLIDE_W: f32 = 1.0; // 滑入距离（屏幕宽 2）
const FADE_OUT_TIME: f32 = 0.6; // 完成页点击后整体渐隐时长

#[inline]
fn ease_out_cubic(p: f32) -> f32 {
    let p = p.clamp(0., 1.);
    1. - (1. - p).powi(3)
}

/// 含 intest feature 时共有 10 页，否则 9 页（跳过内测验证）。
#[inline]
fn step_count() -> usize {
    if cfg!(feature = "intest") { 10 } else { 9 }
}
#[inline]
fn offset_step() -> usize {
    if cfg!(feature = "intest") { 3 } else { 2 }
}
#[inline]
fn audio_step() -> usize {
    if cfg!(feature = "intest") { 4 } else { 3 }
}
#[inline]
fn pref_step() -> usize {
    if cfg!(feature = "intest") { 5 } else { 4 }
}
#[inline]
fn login_step() -> usize {
    if cfg!(feature = "intest") { 6 } else { 5 }
}
#[inline]
fn tos_step() -> usize {
    if cfg!(feature = "intest") { 7 } else { 6 }
}
#[inline]
fn tutorial_step() -> usize {
    if cfg!(feature = "intest") { 8 } else { 7 }
}
#[derive(Clone, Copy)]
enum NextAction {
    Login,
    Register,
}

pub struct OnboardingScene {
    next_scene: Option<NextScene>,

    /// 当前页 & 转场
    step: usize,
    pending_step: usize,
    trans_from: usize,
    trans_dir: i32, // +1 前进（新页从右滑入），-1 后退（新页从左滑入）
    trans_time: Option<f32>,
    entered: bool,
    started: f32,

    /// 完成页点击“进入游戏”后的整体渐隐开始时刻
    fading_out: Option<f32>,

    /// step 0 欢迎
    btn_start: DRectButton,

    /// step 1 选择语言
    lang_scroll: Scroll,
    lang_btns: Vec<DRectButton>,
    lang_selected: usize,
    btn_lang_next: DRectButton,

    /// step 2（intest）内测验证
    input_qq: DRectButton,
    input_key: DRectButton,
    t_qq: String,
    t_key: String,
    btn_apply: DRectButton,
    btn_verify: DRectButton,
    verify_task: Option<Task<Result<bool>>>,

    /// step 3 设置延迟
    offset: Option<OffsetCali>,
    offset_task: LocalTask<Result<OffsetCali>>,
    btn_offset_next: DRectButton,

    /// step 4 音频设置（音乐/音效/BGM 音量，立即应用）
    music_slider: Slider,
    sfx_slider: Slider,
    bgm_slider: Slider,
    btn_audio_next: DRectButton,

    /// step 5 偏好设置（全屏/多人/服务器/动态背景/低画质，立即应用）
    pref_scroll: Scroll,
    fullscreen_btn: DRectButton,
    mp_btn: DRectButton,
    mp_addr_btn: DRectButton,
    dynamic_bg_btn: ChooseButton,
    lowq_btn: DRectButton,
    btn_pref_next: DRectButton,

    /// step 6 登录
    t_email: String,
    t_pwd: String,
    input_email: DRectButton,
    input_pwd: DRectButton,
    btn_to_reg: DRectButton,
    btn_login: DRectButton,
    in_reg: bool,
    t_reg_email: String,
    t_reg_name: String,
    t_reg_pwd: String,
    input_reg_email: DRectButton,
    input_reg_name: DRectButton,
    input_reg_pwd: DRectButton,
    btn_to_login: DRectButton,
    btn_reg: DRectButton,
    login_task: Option<(&'static str, Task<Result<Option<User>>>)>,
    after_accept_tos: Option<NextAction>,
    skip_login: bool,

    /// step 5 服务条款与隐私政策（登录成功后的独立页）
    tos_scroll: Scroll,
    tos_page: usize,
    tos_pages: Option<Vec<String>>,
    tos_modified: Option<String>,
    btn_tos_deny: DRectButton,
    btn_tos_prev: DRectButton,
    btn_tos_next: DRectButton,

    /// step 8 新手教程
    btn_skip: DRectButton,
    btn_tutorial: DRectButton,
    tutorial_started: bool,

    /// step 9 设置完成
    btn_enter: DRectButton,
}

impl OnboardingScene {
    pub fn new() -> Result<Self> {
        // MainScene 由 BootScene 在点击后放入 PENDING_MAIN，结束时再取出；
        // 第 4 页（设置延迟）的音频资源在进入该页时才懒加载
        let lang_selected = get_data()
            .language
            .as_ref()
            .and_then(|it| LANGS.iter().position(|lang| *lang == it))
            .unwrap_or_default();
        Ok(Self {
            next_scene: None,

            step: 0,
            pending_step: 0,
            trans_from: usize::MAX,
            trans_dir: 1,
            trans_time: None,
            entered: false,
            started: 0.,

            fading_out: None,

            btn_start: DRectButton::new(),

            lang_scroll: Scroll::new(),
            lang_btns: (0..LANGS.len()).map(|_| DRectButton::new()).collect(),
            lang_selected,
            btn_lang_next: DRectButton::new(),

            input_qq: DRectButton::new().with_delta(-0.002),
            input_key: DRectButton::new().with_delta(-0.002),
            t_qq: String::new(),
            t_key: String::new(),
            btn_apply: DRectButton::new(),
            btn_verify: DRectButton::new(),
            verify_task: None,

            offset: None,
            offset_task: None,
            btn_offset_next: DRectButton::new(),

            music_slider: Slider::new(0.0..2.0, 0.05),
            sfx_slider: Slider::new(0.0..2.0, 0.05),
            bgm_slider: Slider::new(0.0..2.0, 0.05),
            btn_audio_next: DRectButton::new(),

            pref_scroll: Scroll::new(),
            fullscreen_btn: DRectButton::new(),
            mp_btn: DRectButton::new(),
            mp_addr_btn: DRectButton::new(),
            dynamic_bg_btn: ChooseButton::new()
                .with_options(vec![
                    obtl!("dynamic-bg-off").to_string(),
                    obtl!("dynamic-bg-static").to_string(),
                    obtl!("dynamic-bg-dynamic").to_string(),
                ])
                .with_selected(get_data().config.dynamic_background.as_u8() as usize),
            lowq_btn: DRectButton::new(),
            btn_pref_next: DRectButton::new(),

            t_email: String::new(),
            t_pwd: String::new(),
            input_email: DRectButton::new().with_delta(-0.002),
            input_pwd: DRectButton::new().with_delta(-0.002),
            btn_to_reg: DRectButton::new(),
            btn_login: DRectButton::new(),
            in_reg: false,
            t_reg_email: String::new(),
            t_reg_name: String::new(),
            t_reg_pwd: String::new(),
            input_reg_email: DRectButton::new().with_delta(-0.002),
            input_reg_name: DRectButton::new().with_delta(-0.002),
            input_reg_pwd: DRectButton::new().with_delta(-0.002),
            btn_to_login: DRectButton::new(),
            btn_reg: DRectButton::new(),
            login_task: None,
            after_accept_tos: None,
            skip_login: false,

            tos_scroll: Scroll::new(),
            tos_page: 0,
            tos_pages: None,
            tos_modified: None,
            btn_tos_deny: DRectButton::new(),
            btn_tos_prev: DRectButton::new(),
            btn_tos_next: DRectButton::new(),

            btn_skip: DRectButton::new(),
            btn_tutorial: DRectButton::new(),
            tutorial_started: false,

            btn_enter: DRectButton::new(),
        })
    }

    /// 暂停/恢复主页背景音乐（通过预加载的 PENDING_MAIN 访问 MainScene）。
    fn set_main_bgm_paused(paused: bool) {
        PENDING_MAIN.with(|it| {
            if let Some(main) = it.borrow_mut().as_mut() {
                main.set_bgm_paused(paused);
            }
        });
    }

    /// 开始向右滑动画并切换到下一页；自动跳过登录页（已登录）与服务条款页（已同意）。
    fn begin_trans(&mut self, t: f32) {
        // 离开当前页的清理
        if self.step == offset_step() {
            if let Some(o) = &mut self.offset {
                o.pause();
            }
            let _ = save_data();
        }
        let mut target = self.step + 1;
        loop {
            if target == login_step() && self.skip_login {
                target += 1;
                continue;
            }
            if target == tos_step() && get_data().terms_modified.is_some() {
                target += 1;
                continue;
            }
            break;
        }
        if target >= step_count() {
            return;
        }
        self.begin_trans_to(t, target);
    }

    /// 切换到指定页（支持前进/后退，带滑动动画）
    fn begin_trans_to(&mut self, t: f32, target: usize) {
        if target >= step_count() {
            return;
        }
        self.trans_dir = if target > self.step { 1 } else { -1 };
        self.trans_from = self.step;
        self.pending_step = target;
        self.trans_time = Some(t);
    }

    fn start_verify(&mut self) {
        if self.verify_task.is_some() {
            return;
        }
        let qq = self.t_qq.trim().to_string();
        let key = self.t_key.trim().to_string();
        self.verify_task = Some(Task::new(async move {
            let resp = crate::client::basic_client_builder()
                .build()?
                .post(format!("{EXAM_API_URL}/api/exam/verify"))
                .json(&serde_json::json!({ "qq": qq, "key": key }))
                .send()
                .await?;
            Ok(resp.status().is_success())
        }));
    }

    fn login_action(&mut self, email: String, pwd: String) {
        self.login_task = Some(("login", Task::new(async move {
            Client::login(LoginParams::Password {
                email: &email,
                password: &pwd,
            })
            .await?;
            Ok(Some(Client::get_me().await?))
        })));
    }

    fn register(&mut self) -> Option<Cow<'static, str>> {
        let email = self.t_reg_email.clone();
        let name = self.t_reg_name.clone();
        let pwd = self.t_reg_pwd.clone();
        if let Some(error) = validate_username(&name) {
            show_message(error).error();
        }
        if !EMAIL_REGEX.is_match(&email) {
            return Some(crate::login::tl!("illegal-email"));
        }
        if !(8..=32).contains(&pwd.len()) {
            return Some(crate::login::tl!("pwd-length-req"));
        }
        self.login_task = Some(("register", Task::new(async move {
            Client::register(&email, &name, &pwd).await?;
            Ok(None)
        })));
        None
    }

    fn handle_input(&mut self) -> Result<()> {
        if let Some((id, text)) = take_input() {
            match id.as_str() {
                "ob_qq" => self.t_qq = text,
                "ob_key" => self.t_key = text,
                "ob_email" => self.t_email = text,
                "ob_pwd" => self.t_pwd = text,
                "ob_reg_email" => self.t_reg_email = text,
                "ob_reg_name" => self.t_reg_name = text,
                "ob_reg_pwd" => self.t_reg_pwd = text,
                "ob_mp_addr" => {
                    // 与设置页一致：校验“主机:端口”，无效报错，有效应用
                    if let Err(err) = text.to_socket_addrs() {
                        show_error(anyhow::Error::new(err).context(obtl!("mp-addr-invalid")));
                    } else {
                        get_data_mut().config.mp_address = text;
                        let _ = save_data();
                    }
                }
                _ => return_input(id, text),
            }
        }
        Ok(())
    }

    fn render_title(&self, ui: &mut Ui, title: &str) {
        let top = ui.top;
        let mut dt = ui.text(title)
            .pos(0.55, -top + 0.14)
            .anchor(0.5, 0.5)
            .size(0.7)
            .color(WHITE)
            .no_baseline();
        dt.draw_using(&BOLD_FONT);
        if self.step == 0 {
            // [workaround] 隐藏的占位文字：维持文字渲染管线状态，避免同帧后续按钮文字不显示
            ui.text("·")
                .pos(0.0, 0.0)
                .anchor(0.5, 0.5)
                .size(0.7)
                .color(Color::new(1., 1., 1., 0.))
                .no_baseline()
                .draw_using(&BOLD_FONT);
        }
    }

    fn bottom_button(&self, ui: &Ui) -> f32 {
        ui.screen_rect().bottom() - 0.17
    }

    fn render_page(&mut self, ui: &mut Ui, t: f32, step: usize) {
        let title = match step {
            0 => obtl!("welcome"),
            1 => obtl!("choose-language"),
            2 => obtl!("verify-intest"),
            3 => obtl!("set-offset"),
            4 => obtl!("audio-setup"),
            5 => obtl!("pref-setup"),
            6 => obtl!("login-phira"),
            7 => Cow::Owned(ttl!("tos-and-policy").into_owned()),
            8 => obtl!("tutorial"),
            9 => obtl!("setup-done"),
            _ => Cow::Borrowed(""),
        };
        self.render_title(ui, &title);
        match step {
            0 => self.render_welcome(ui, t),
            1 => self.render_lang(ui, t),
            2 => self.render_intest(ui, t),
            3 => self.render_offset(ui, t),
            4 => self.render_audio(ui, t),
            5 => self.render_pref(ui, t),
            6 => self.render_login(ui, t),
            7 => self.render_tos(ui, t),
            8 => self.render_tutorial(ui, t),
            9 => self.render_done(ui, t),
            _ => {}
        }
    }

    fn render_welcome(&mut self, ui: &mut Ui, t: f32) {
        let y = self.bottom_button(ui);
        let r = Rect::new(0.25, y, 0.6, 0.12);
        self.btn_start.render_text(ui, r, t, obtl!("start-setup"), 0.6, true);
        // [workaround] 隐藏的占位文字：维持文字渲染管线状态，避免"开始设置"不显示
        let c = Color::new(1., 1., 1., 0.);
        for ch in ["·", "…", "，"] {
            ui.text(ch)
                .pos(r.center().x, r.center().y)
                .anchor(0.5, 0.5)
                .size(0.5)
                .color(c)
                .no_baseline()
                .draw();
        }
    }

    fn render_lang(&mut self, ui: &mut Ui, t: f32) {
        let top = ui.top;
        // 容器从标题下方延伸到底部按钮上方（间距 0.05）
        let r = Rect::new(0.15, -top + 0.32, 0.8, top * 2. - 0.54);
        self.lang_scroll.size((r.w, r.h));
        ui.scope(|ui| {
            ui.dx(r.x);
            ui.dy(r.y);
            self.lang_scroll.render(ui, |ui| {
                let mut h = 0.;
                for (i, (btn, name)) in self.lang_btns.iter_mut().zip(LANG_NAMES.iter()).enumerate() {
                    let rr = Rect::new(0., h, r.w, 0.11);
                    let chosen = i == self.lang_selected;
                    btn.render_text(ui, rr, t, *name, 0.6, chosen);
                    h += 0.12;
                }
                (r.w, h)
            });
        });
        let y = self.bottom_button(ui);
        self.btn_lang_next.render_text(ui, Rect::new(0.25, y, 0.6, 0.12), t, obtl!("next"), 0.6, true);
    }

    fn render_intest(&mut self, ui: &mut Ui, t: f32) {
        let h = 0.1;
        let mut r = Rect::new(0.15, -0.15, 0.8, h);
        self.input_qq.render_input(ui, r, t, &self.t_qq, obtl!("qq"), 0.6);
        r.y += h + 0.05;
        self.input_key.render_input(ui, r, t, &self.t_key, obtl!("intest-key"), 0.6);

        let y = self.bottom_button(ui);
        let bw = 0.38;
        let bh = 0.12;
        let pad = 0.06;
        let x0 = 0.55 - bw - pad / 2.;
        self.btn_apply.render_text(ui, Rect::new(x0, y, bw, bh), t, obtl!("apply"), 0.6, false);
        self.btn_verify.render_text(ui, Rect::new(x0 + bw + pad, y, bw, bh), t, obtl!("next"), 0.6, true);
        if self.verify_task.is_some() {
            ui.full_loading_simple(t);
        }
    }

    fn render_offset(&mut self, ui: &mut Ui, t: f32) {
        if let Some(o) = &mut self.offset {
            o.render(ui, t);
        } else {
            ui.full_loading_simple(t);
        }
        let y = self.bottom_button(ui);
        self.btn_offset_next
            .render_text(ui, Rect::new(0.25, y, 0.6, 0.12), t, obtl!("next"), 0.6, true);
    }

    /// step 4 音频设置：音乐/音效/BGM 音量（与设置界面一致，立即应用）
    fn render_audio(&mut self, ui: &mut Ui, t: f32) {
        let data = get_data();
        let config = &data.config;
        let mut y = -0.24;
        let row_h = 0.17;
        let ctrl_w = 0.3;
        let row = |ui: &mut Ui, y: f32, label: Cow<'_, str>, slider: &mut Slider, value: f32| {
            ui.text(label)
                .pos(0.18, y)
                .anchor(0., 0.5)
                .size(0.6)
                .color(semi_white(0.9))
                .draw();
            slider.render(ui, Rect::new(0.7, y - 0.065, ctrl_w, 0.13), t, value, format!("{value:.2}"));
        };
        row(ui, y, obtl!("item-music"), &mut self.music_slider, config.volume_music);
        y += row_h;
        row(ui, y, obtl!("item-sfx"), &mut self.sfx_slider, config.volume_sfx);
        y += row_h;
        row(ui, y, obtl!("item-bgm"), &mut self.bgm_slider, config.volume_bgm);
        let yb = self.bottom_button(ui);
        self.btn_audio_next
            .render_text(ui, Rect::new(0.25, yb, 0.6, 0.12), t, obtl!("next"), 0.6, true);
    }

    /// step 5 偏好设置：全屏/多人/服务器/动态背景/低画质（与设置界面一致，立即应用，可滚动）
    fn render_pref(&mut self, ui: &mut Ui, t: f32) {
        let top = ui.top;
        let r = Rect::new(0.15, -top + 0.32, 0.8, top * 2. - 0.54);
        self.pref_scroll.size((r.w, r.h));
        let data = get_data();
        let config = &data.config;
        ui.scope(|ui| {
            ui.dx(r.x);
            ui.dy(r.y);
            self.pref_scroll.render(ui, |ui| {
                let mut h = 0.;
                let row_h = 0.22;
                let label_x = 0.03;
                let ctrl_x = 0.55;
                let ctrl_w = 0.24;
                let ctrl_h = 0.11;
                let row = |ui: &mut Ui, y: f32, label: Cow<'_, str>, btn: &mut DRectButton, text: Cow<'_, str>, on: bool| {
                    ui.text(label)
                        .pos(label_x, y + row_h / 2.)
                        .anchor(0., 0.5)
                        .size(0.6)
                        .color(semi_white(0.9))
                        .draw();
                    btn.render_text(ui, Rect::new(ctrl_x, y + (row_h - ctrl_h) / 2., ctrl_w, ctrl_h), t, text, 0.6, on);
                };
                row(ui, h, obtl!("item-fullscreen"), &mut self.fullscreen_btn,
                    if config.fullscreen_mode { ttl!("switch-on") } else { ttl!("switch-off") }, config.fullscreen_mode);
                h += row_h;
                row(ui, h, obtl!("item-mp"), &mut self.mp_btn,
                    if config.mp_enabled { ttl!("switch-on") } else { ttl!("switch-off") }, config.mp_enabled);
                h += row_h;
                row(ui, h, obtl!("item-mp-addr"), &mut self.mp_addr_btn,
                    Cow::Owned(config.mp_address.clone()), false);
                h += row_h;
                ui.text(obtl!("item-dynamic-bg"))
                    .pos(label_x, h + row_h / 2.)
                    .anchor(0., 0.5)
                    .size(0.6)
                    .color(semi_white(0.9))
                    .draw();
                self.dynamic_bg_btn.render(ui, Rect::new(ctrl_x, h + (row_h - ctrl_h) / 2., ctrl_w, ctrl_h), t);
                h += row_h;
                row(ui, h, obtl!("item-lowq"), &mut self.lowq_btn,
                    if config.sample_count == 1 { ttl!("switch-on") } else { ttl!("switch-off") }, config.sample_count == 1);
                h += row_h;
                (r.w, h)
            });
        });
        // 顶层渲染动态背景下拉弹窗
        self.dynamic_bg_btn.render_top(ui, t, 1.);
        let yb = self.bottom_button(ui);
        self.btn_pref_next
            .render_text(ui, Rect::new(0.25, yb, 0.6, 0.12), t, obtl!("next"), 0.6, true);
    }

    fn render_login(&mut self, ui: &mut Ui, t: f32) {
        let y = self.bottom_button(ui);
        let bw = 0.38;
        let bh = 0.12;
        let pad = 0.06;
        let x0 = 0.55 - bw - pad / 2.;
        if self.in_reg {
            let h = 0.1;
            let mut r = Rect::new(0.15, -0.24, 0.8, h);
            self.input_reg_email.render_input(ui, r, t, &self.t_reg_email, obtl!("email"), 0.6);
            r.y += h + 0.04;
            self.input_reg_name.render_input(ui, r, t, &self.t_reg_name, obtl!("username"), 0.6);
            r.y += h + 0.04;
            self.input_reg_pwd.render_input(ui, r, t, "*".repeat(self.t_reg_pwd.len()), obtl!("password"), 0.6);
            self.btn_to_login.render_text(ui, Rect::new(x0, y, bw, bh), t, obtl!("back-login"), 0.6, false);
            self.btn_reg.render_text(ui, Rect::new(x0 + bw + pad, y, bw, bh), t, obtl!("register"), 0.6, true);
        } else {
            let h = 0.1;
            let mut r = Rect::new(0.15, -0.15, 0.8, h);
            self.input_email.render_input(ui, r, t, &self.t_email, obtl!("account"), 0.6);
            r.y += h + 0.05;
            self.input_pwd.render_input(ui, r, t, "*".repeat(self.t_pwd.len()), obtl!("password"), 0.6);
            self.btn_to_reg.render_text(ui, Rect::new(x0, y, bw, bh), t, obtl!("register"), 0.6, false);
            self.btn_login.render_text(ui, Rect::new(x0 + bw + pad, y, bw, bh), t, obtl!("next"), 0.6, true);
        }
        if self.login_task.is_some() {
            ui.full_loading_simple(t);
        }
    }

    /// 构建服务条款分页（从 TERMS 加载），每页 50 行（与主界面弹窗一致）
    fn ensure_tos_pages(&mut self) {
        if self.tos_pages.is_none() {
            if let Some(Some((text, modified))) = TERMS.get() {
                let content = format!("{}\n\n{}", ttl!("tos-and-policy-desc"), text);
                let lines: Vec<&str> = content.split('\n').collect();
                let pages: Vec<String> = lines.chunks(50).map(|it| it.join("\n")).collect();
                self.tos_pages = Some(pages);
                self.tos_modified = Some(modified.clone());
            }
        }
    }

    fn render_tos(&mut self, ui: &mut Ui, t: f32) {
        self.ensure_tos_pages();
        let top = ui.top;
        // 容器从标题下方延伸到底部按钮上方（间距 0.05）
        let r = Rect::new(0.15, -top + 0.30, 0.8, top * 2. - 0.54);
        let page_text = self.tos_pages.as_ref().and_then(|p| p.get(self.tos_page)).cloned().unwrap_or_default();
        self.tos_scroll.size((r.w, r.h));
        ui.scope(|ui| {
            ui.dx(r.x);
            ui.dy(r.y);
            self.tos_scroll.render(ui, |ui| {
                if page_text.is_empty() {
                    ui.text(ttl!("loading_tos_policy"))
                        .pos(0., 0.)
                        .anchor(0., 0.)
                        .size(0.6)
                        .color(semi_white(0.9))
                        .draw();
                    (r.w, r.h)
                } else {
                    let lines = page_text.split('\n').count();
                    ui.text(page_text)
                        .pos(0., 0.)
                        .anchor(0., 0.)
                        .size(0.6)
                        .color(semi_white(0.9))
                        .max_width(r.w)
                        .multiline()
                        .draw();
                    (r.w, lines as f32 * 0.084)
                }
            });
        });
        // 底部按钮：拒绝 / [上一页（非第一页才显示）] / 下一页（最后一页已看完时变“同意”）
        let y = self.bottom_button(ui);
        let bw = 0.22;
        let bh = 0.12;
        let pad = 0.035;
        let is_first = self.tos_page == 0;
        let total_w = if is_first { bw * 2. + pad } else { bw * 3. + pad * 2. };
        let x0 = 0.55 - total_w / 2.;
        self.btn_tos_deny.render_text(ui, Rect::new(x0, y, bw, bh), t, ttl!("tos-deny"), 0.6, false);
        let prev_x = if is_first { 0. } else { x0 + bw + pad };
        if !is_first {
            self.btn_tos_prev.render_text(ui, Rect::new(prev_x, y, bw, bh), t, ttl!("tos-prev-page"), 0.6, false);
        }
        let pages_len = self.tos_pages.as_ref().map_or(0, |p| p.len());
        let last_page = self.tos_page + 1 >= pages_len;
        let at_bottom = self.tos_scroll.y_scroller.offset >= self.tos_scroll.y_scroller.max_offset() - 0.01;
        let next_text = if last_page && at_bottom { ttl!("tos-accept") } else { ttl!("tos-next-page") };
        let next_x = if is_first { x0 + bw + pad } else { x0 + (bw + pad) * 2. };
        self.btn_tos_next.render_text(ui, Rect::new(next_x, y, bw, bh), t, next_text, 0.6, true);
    }

    fn render_tutorial(&mut self, ui: &mut Ui, t: f32) {
        ui.text(obtl!("tutorial-content"))
            .pos(0.55, 0.)
            .anchor(0.5, 0.5)
            .size(0.6)
            .max_width(0.8)
            .multiline()
            .h_center()
            .color(semi_white(0.9))
            .draw();
        let y = self.bottom_button(ui);
        let bw = 0.38;
        let bh = 0.12;
        let pad = 0.06;
        let x0 = 0.55 - bw - pad / 2.;
        self.btn_skip.render_text(ui, Rect::new(x0, y, bw, bh), t, obtl!("skip"), 0.6, false);
        self.btn_tutorial.render_text(ui, Rect::new(x0 + bw + pad, y, bw, bh), t, obtl!("tutorial-enter"), 0.6, true);
    }

    fn render_done(&mut self, ui: &mut Ui, t: f32) {
        let y = self.bottom_button(ui);
        self.btn_enter.render_text(ui, Rect::new(0.25, y, 0.6, 0.12), t, obtl!("enter-game"), 0.6, true);
    }

    fn touch_page(&mut self, touch: &Touch, t: f32) -> Result<bool> {
        match self.step {
            0 => {
                if self.btn_start.touch(touch, t) {
                    button_hit();
                    self.begin_trans(t);
                }
            }
            1 => {
                if self.lang_scroll.touch(touch, t) {
                    return Ok(true);
                }
                // 只响应语言列表可视区内的点击，避免列表溢出部分（在可视区外但 pts 仍在）
                // 与底部“下一步”按钮重叠造成穿透
                if self.lang_scroll.contains(touch) {
                    for (i, btn) in self.lang_btns.iter_mut().enumerate() {
                        if btn.touch(touch, t) {
                            button_hit();
                            self.lang_selected = i;
                            get_data_mut().language = Some(LANGS[i].to_owned());
                            sync_data();
                            let _ = save_data();
                            return Ok(true);
                        }
                    }
                }
                if self.btn_lang_next.touch(touch, t) {
                    button_hit();
                    self.begin_trans(t);
                }
            }
            2 => {
                if self.input_qq.touch(touch, t) {
                    request_input("ob_qq", InputBox::new().default_text(&self.t_qq));
                    return Ok(true);
                }
                if self.input_key.touch(touch, t) {
                    request_input("ob_key", InputBox::new().default_text(&self.t_key));
                    return Ok(true);
                }
                if self.btn_apply.touch(touch, t) {
                    button_hit();
                    let _ = open_url("https://pf.tianstudio.top/exam");
                    return Ok(true);
                }
                if self.btn_verify.touch(touch, t) {
                    button_hit();
                    self.start_verify();
                    return Ok(true);
                }
            }
            3 => {
                if let Some(o) = &mut self.offset {
                    if o.touch(touch, t) {
                        return Ok(true);
                    }
                }
                if self.btn_offset_next.touch(touch, t) {
                    button_hit();
                    self.begin_trans(t);
                }
            }
            4 => {
                let data = get_data_mut();
                let config = &mut data.config;
                if self.music_slider.touch(touch, t, &mut config.volume_music).is_some() {
                    return Ok(true);
                }
                if self.sfx_slider.touch(touch, t, &mut config.volume_sfx).is_some() {
                    return Ok(true);
                }
                let old = config.volume_bgm;
                if self.bgm_slider.touch(touch, t, &mut config.volume_bgm).is_some() {
                    if (config.volume_bgm - old).abs() > 0.001 {
                        BGM_VOLUME_UPDATED.store(true, Ordering::Relaxed);
                    }
                    return Ok(true);
                }
                if self.btn_audio_next.touch(touch, t) {
                    button_hit();
                    self.begin_trans(t);
                }
            }
            5 => {
                if self.pref_scroll.touch(touch, t) {
                    return Ok(true);
                }
                if self.dynamic_bg_btn.top_touch(touch, t) {
                    return Ok(true);
                }
                // 只响应可视区内的点击，避免与底部“下一步”按钮穿透
                if self.pref_scroll.contains(touch) {
                    let data = get_data_mut();
                    let config = &mut data.config;
                    if self.fullscreen_btn.touch(touch, t) {
                        button_hit();
                        config.fullscreen_mode ^= true;
                        macroquad::window::set_fullscreen(config.fullscreen_mode);
                        let _ = save_data();
                        return Ok(true);
                    }
                    if self.mp_btn.touch(touch, t) {
                        button_hit();
                        config.mp_enabled ^= true;
                        let _ = save_data();
                        return Ok(true);
                    }
                    if self.mp_addr_btn.touch(touch, t) {
                        request_input("ob_mp_addr", InputBox::new().default_text(&config.mp_address));
                        return Ok(true);
                    }
                    if self.dynamic_bg_btn.touch(touch, t) {
                        return Ok(false);
                    }
                    if self.lowq_btn.touch(touch, t) {
                        button_hit();
                        config.sample_count = if config.sample_count == 1 { 2 } else { 1 };
                        let _ = save_data();
                        return Ok(true);
                    }
                }
                if self.btn_pref_next.touch(touch, t) {
                    button_hit();
                    self.begin_trans(t);
                }
            }
            6 => {
                if self.in_reg {
                    if self.input_reg_email.touch(touch, t) {
                        request_input("ob_reg_email", InputBox::new().default_text(&self.t_reg_email));
                        return Ok(true);
                    }
                    if self.input_reg_name.touch(touch, t) {
                        request_input("ob_reg_name", InputBox::new().default_text(&self.t_reg_name));
                        return Ok(true);
                    }
                    if self.input_reg_pwd.touch(touch, t) {
                        request_input("ob_reg_pwd", InputBox::new().default_text(&self.t_reg_pwd).mode(InputMode::Password));
                        return Ok(true);
                    }
                    if self.btn_to_login.touch(touch, t) {
                        button_hit();
                        self.in_reg = false;
                        return Ok(true);
                    }
                    if self.btn_reg.touch(touch, t) {
                        if let Some(error) = self.register() {
                            show_message(error).error();
                        }
                        return Ok(true);
                    }
                } else {
                    if self.input_email.touch(touch, t) {
                        request_input("ob_email", InputBox::new().default_text(&self.t_email));
                        return Ok(true);
                    }
                    if self.input_pwd.touch(touch, t) {
                        request_input("ob_pwd", InputBox::new().default_text(&self.t_pwd).mode(InputMode::Password));
                        return Ok(true);
                    }
                    if self.btn_to_reg.touch(touch, t) {
                        button_hit();
                        self.in_reg = true;
                        return Ok(true);
                    }
                    if self.btn_login.touch(touch, t) {
                        let email = self.t_email.clone();
                        let pwd = self.t_pwd.clone();
                        self.login_action(email, pwd);
                        return Ok(true);
                    }
                }
            }
            7 => {
                if self.tos_scroll.touch(touch, t) {
                    return Ok(true);
                }
                if self.btn_tos_deny.touch(touch, t) {
                    button_hit();
                    show_message(ttl!("warn-deny-tos-policy")).warn();
                    return Ok(true);
                }
                if self.tos_page > 0 && self.btn_tos_prev.touch(touch, t) {
                    button_hit();
                    self.tos_page -= 1;
                    self.tos_scroll.set_offset(0., 0.);
                    self.tos_scroll.y_scroller.goto = None;
                    return Ok(true);
                }
                if self.btn_tos_next.touch(touch, t) {
                    button_hit();
                    let pages_len = self.tos_pages.as_ref().map_or(0, |p| p.len());
                    if pages_len == 0 {
                        return Ok(true);
                    }
                    let at_bottom = self.tos_scroll.y_scroller.offset >= self.tos_scroll.y_scroller.max_offset() - 0.01;
                    if !at_bottom {
                        // 未看完当前页：滑动正文到底
                        self.tos_scroll.y_scroller.goto = Some(self.tos_scroll.y_scroller.max_offset());
                    } else if self.tos_page < pages_len - 1 {
                        // 当前页看完：翻到下一段正文
                        self.tos_page += 1;
                        self.tos_scroll.set_offset(0., 0.);
                        self.tos_scroll.y_scroller.goto = None;
                    } else {
                        // 全部看完：同意条款并切到下一页
                        if let Some(m) = &self.tos_modified {
                            get_data_mut().terms_modified = Some(m.clone());
                            let _ = save_data();
                        }
                        self.begin_trans(t);
                    }
                    return Ok(true);
                }
            }
            8 => {
                if self.btn_skip.touch(touch, t) {
                    button_hit();
                    self.begin_trans(t);
                }
                if self.btn_tutorial.touch(touch, t) {
                    button_hit();
                    match TutorialLoadingScene::new() {
                        Ok(scene) => {
                            self.tutorial_started = true;
                            Self::set_main_bgm_paused(true);
                            self.next_scene = Some(NextScene::Overlay(Box::new(scene)));
                        }
                        Err(err) => show_error(err),
                    }
                    return Ok(true);
                }
            }
            9 => {
                if self.btn_enter.touch(touch, t) {
                    button_hit();
                    get_data_mut().onboarding_done = true;
                    let _ = save_data();
                    // 重置主界面入场动画计时，使 Replace 后从头渐显
                    crate::scene::boot::set_boot_entry_time(t as f64);
                    self.fading_out = Some(t);
                }
            }
            _ => {}
        }
        Ok(true)
    }
}

impl Scene for OnboardingScene {
    fn enter(&mut self, tm: &mut TimeManager, _target: Option<RenderTarget>) -> Result<()> {
        if !self.entered {
            self.entered = true;
            self.started = tm.now() as f32;
            self.skip_login = get_data().me.is_some();
            // 首个页面从右侧滑入
            self.trans_from = usize::MAX;
            self.pending_step = 0;
            self.trans_time = Some(self.started);
        } else if self.tutorial_started && self.step == tutorial_step() {
            // 新手教程结束返回本场景：自动进入下一页（设置完成），恢复主页 BGM
            self.tutorial_started = false;
            Self::set_main_bgm_paused(false);
            self.begin_trans(tm.now() as f32);
        }
        Ok(())
    }

    fn touch(&mut self, tm: &mut TimeManager, touch: &Touch) -> Result<bool> {
        if self.trans_time.is_some() || self.fading_out.is_some() {
            return Ok(true);
        }
        self.touch_page(touch, tm.now() as f32)
    }

    fn update(&mut self, tm: &mut TimeManager) -> Result<()> {
        let t = tm.now() as f32;

        // 转场完成推进
        if let Some(st) = self.trans_time {
            if t - st >= TRANS_TIME {
                self.trans_time = None;
                self.step = self.pending_step;
                if self.step == offset_step() && self.offset.is_none() && self.offset_task.is_none() {
                    self.offset_task = Some(Box::pin(OffsetCali::new()));
                }
            }
        }
        // 懒加载第 4 页的延迟校准资源
        if let Some(task) = &mut self.offset_task {
            if let Some(res) = poll_future(task.as_mut()) {
                match res {
                    Ok(o) => {
                        self.offset = Some(o);
                        if self.step == offset_step() {
                            if let Some(o) = &mut self.offset {
                                o.enter();
                            }
                        }
                    }
                    Err(err) => show_error(err.context("Failed to load offset cali")),
                }
                self.offset_task = None;
            }
        }

        self.handle_input()?;

        // 内测验证结果
        if let Some(task) = &mut self.verify_task {
            if let Some(res) = task.take() {
                match res {
                    Ok(true) => self.begin_trans(t),
                    Ok(false) => Dialog::plain(obtl!("verify-intest"), obtl!("intest-invalid")).show(),
                    Err(err) => show_error(err.context("failed to verify intest")),
                }
                self.verify_task = None;
            }
        }

        // 登录 / 注册结果（与主页登录一致）
        dispatch_tos_task();
        if JUST_ACCEPTED_TOS.fetch_and(false, Ordering::Relaxed) {
            match self.after_accept_tos {
                Some(NextAction::Login) => {
                    let email = self.t_email.clone();
                    let pwd = self.t_pwd.clone();
                    self.login_action(email, pwd);
                }
                Some(NextAction::Register) => {
                    if let Some(error) = self.register() {
                        show_message(error).error();
                    }
                }
                None => {}
            }
            self.after_accept_tos = None;
        }
        if let Some((action, task)) = &mut self.login_task {
            if let Some(res) = task.take() {
                match res {
                    Err(err) => show_error(err.context(crate::login::tl!("action-failed", "action" => *action))),
                    Ok(user) => {
                        if let Some(user) = user {
                            UserManager::request(user.id);
                            get_data_mut().me = Some(user);
                            save_data()?;
                        }
                        self.t_pwd.clear();
                        show_message(crate::login::tl!("action-success", "action" => *action)).ok();
                        if *action == "register" {
                            Dialog::simple(crate::login::tl!("email-sent")).show();
                            self.t_reg_email.clear();
                            self.t_reg_name.clear();
                            self.t_reg_pwd.clear();
                            self.in_reg = false;
                        }
                        if *action == "login" {
                            self.begin_trans(t);
                        }
                    }
                }
                self.login_task = None;
            }
        }

        // 完成页渐隐结束后切换到主界面
        if let Some(ft) = self.fading_out {
            if t - ft >= FADE_OUT_TIME {
                let main = PENDING_MAIN.with(|it| it.borrow_mut().take());
                if let Some(main) = main {
                    self.next_scene = Some(NextScene::Replace(Box::new(main)));
                }
                self.fading_out = None;
            }
        }

        if self.step == offset_step() {
            if let Some(o) = &mut self.offset {
                o.update();
            }
        }
        if self.step == 1 {
            self.lang_scroll.update(t);
        }
        if self.step == pref_step() {
            self.pref_scroll.update(t);
            self.dynamic_bg_btn.update(t);
            if self.dynamic_bg_btn.changed() {
                let data = get_data_mut();
                data.config.dynamic_background = match self.dynamic_bg_btn.selected() {
                    0 => DynamicBackgroundMode::Off,
                    1 => DynamicBackgroundMode::StaticBrightness,
                    _ => DynamicBackgroundMode::DynamicBrightness,
                };
                let _ = save_data();
            }
        }
        if self.step == tos_step() {
            // 进入服务条款页后触发条款加载（若尚未加载）
            if TERMS.get().is_none() {
                load_tos_and_policy(true, true);
            }
            self.tos_scroll.update(t);
        }
        // 引导内已通过服务条款页处理条款确认，消费 JUST_LOADED_TOS，
        // 避免进入主界面后再弹一次条款确认框
        JUST_LOADED_TOS.fetch_and(false, Ordering::Relaxed);
        Ok(())
    }

    fn render(&mut self, tm: &mut TimeManager, ui: &mut Ui) -> Result<()> {
        set_camera(&ui.camera());
        let t = tm.now() as f32;
        let screen = ui.screen_rect();

        // 背景 + 半透明黑色遮罩（与 boot 阶段视觉一致，保留遮罩并在其上显示引导内容）
        if let Some(bg) = crate::scene::TEX_BACKGROUND.with(|it| it.borrow().clone()) {
            ui.fill_rect(screen, (*bg, screen));
        }
        ui.fill_rect(screen, semi_black(0.5));

        // 完成页点击后整体渐隐
        let alpha = if let Some(ft) = self.fading_out {
            1. - ((t - ft) / FADE_OUT_TIME).clamp(0., 1.)
        } else {
            1.
        };
        if alpha <= 0. {
            return Ok(());
        }

        ui.alpha(alpha, |ui| {
            if let Some(st) = self.trans_time {
                let p = (t - st) / TRANS_TIME;
                let e = ease_out_cubic(p);
                let dir = self.trans_dir as f32;
                // 旧页滑出 + 渐隐（前进：向左滑出；后退：向右滑出）
                if self.trans_from != usize::MAX {
                    ui.alpha(1. - e, |ui| {
                        ui.scope(|ui| {
                            ui.dx(-e * SLIDE_W * dir);
                            self.render_page(ui, t, self.trans_from);
                        });
                    });
                }
                // 新页滑入 + 渐显（前进：从右滑入；后退：从左滑入）
                ui.alpha(e, |ui| {
                    ui.scope(|ui| {
                        ui.dx((1. - e) * SLIDE_W * dir);
                        self.render_page(ui, t, self.pending_step);
                    });
                });
            } else {
                self.render_page(ui, t, self.step);
            }
        });

        Ok(())
    }

    fn next_scene(&mut self, _tm: &mut TimeManager) -> NextScene {
        self.next_scene.take().unwrap_or_default()
    }

    fn pause(&mut self, _tm: &mut TimeManager) -> Result<()> {
        if self.step == offset_step() {
            if let Some(o) = &mut self.offset {
                o.pause();
            }
        }
        Ok(())
    }

    fn resume(&mut self, _tm: &mut TimeManager) -> Result<()> {
        // 教程 overlay 关闭返回时恢复主页 BGM
        Self::set_main_bgm_paused(false);
        if self.step == offset_step() {
            if let Some(o) = &mut self.offset {
                o.resume();
            }
        }
        Ok(())
    }
}

/// 设置延迟页：搬自 OffsetPage 的主要部分（滑块 + 判定线试听），
/// 不含左上角标题/返回按钮与右下角试玩按钮。
struct OffsetCali {
    _audio: AudioManager,
    cali: Music,
    cali_hit: Sfx,

    tm: TimeManager,
    cali_last: bool,

    click: SafeTexture,
    _hit_fx: SafeTexture,
    emitter: ParticleEmitter,
    color: Color,

    slider: Slider,

    touched: bool,
    touch: Option<(f32, f32)>,
}

impl OffsetCali {
    const FADE_TIME: f32 = 0.8;

    pub async fn new() -> Result<Self> {
        let mut audio = create_audio_manger(&get_data().config)?;
        let cali = audio.create_music(
            AudioClip::new(load_file("cali.ogg").await?)?,
            MusicParams {
                loop_mix_time: 0.,
                ..Default::default()
            },
        )?;
        let cali_hit = audio.create_sfx(AudioClip::new(load_file("cali_hit.ogg").await?)?, None)?;

        let mut tm = TimeManager::new(1., true);
        tm.force = 3e-2;

        let respack = ResourcePack::from_path(get_data().config.res_pack_path.as_ref())
            .await
            .context("Failed to load resource pack")?;
        let click = respack.note_style.click.clone();
        let emitter = ParticleEmitter::new(&respack, get_data().config.note_scale, respack.info.hide_particles)?;
        Ok(Self {
            _audio: audio,
            cali,
            cali_hit,

            tm,
            cali_last: false,

            click,
            _hit_fx: respack.hit_fx,
            emitter,
            color: respack.info.fx_perfect(),

            slider: Slider::new(-500.0..500.0, 5.),

            touched: false,
            touch: None,
        })
    }

    fn enter(&mut self) {
        let _ = self.cali.seek_to(0.);
        let _ = self.cali.play();
        self.tm.reset();
    }

    fn pause(&mut self) {
        self.tm.pause();
        let _ = self.cali.pause();
    }

    fn resume(&mut self) {
        self.tm.resume();
        let _ = self.cali.play();
    }

    fn touch(&mut self, touch: &Touch, t: f32) -> bool {
        let config = &mut get_data_mut().config;
        let mut offset = config.offset * 1000.;
        if self.slider.touch(touch, t, &mut offset).is_some() {
            config.offset = offset / 1000.;
            return true;
        }
        if touch.phase == TouchPhase::Started && touch.position.x < 0. {
            self.touched = true;
        }
        false
    }

    fn update(&mut self) {
        if !self.cali.paused() {
            let pos = self.cali.position();
            let now = self.tm.now();
            if now > 2. {
                self.tm.seek_to(now - 2.);
                self.tm.dont_wait();
            }
            let now = self.tm.now();
            if now - pos >= -1. {
                self.tm.update(pos);
            }
        }
    }

    fn render(&mut self, ui: &mut Ui, t: f32) {
        let lf = -0.92;
        let mut r = ui.content_rect();
        r.y += 0.3;
        r.h -= 0.3;
        r.w += r.x - lf;
        r.x = lf;
        // 不绘制圆角矩形面板/边框（用户不需要）

        let ct = (-0.4, r.bottom() - 0.12);
        let hw = 0.4;
        let hh = 0.005;
        ui.fill_rect(Rect::new(ct.0 - hw, ct.1 - hh, hw * 2., hh * 2.), WHITE);

        let ot = t;

        let config = &get_data().config;
        let mut t = self.tm.now() as f32 - config.offset;
        if t < 0. {
            t += 2.;
        }
        if t >= 2. {
            t -= 2.;
        }
        let ny = ct.1 + (t - 1.) * 0.6;
        if self.touched {
            self.touch = Some((ot, ny));
            self.touched = false;
        }
        if t <= 1. {
            let w = NOTE_WIDTH_RATIO_BASE as f32 * config.note_scale * 2.;
            let h = w * self.click.height() / self.click.width();
            let r = Rect::new(ct.0 - w / 2., ny, w, h);
            ui.fill_rect(r, (*self.click, r, ScaleType::Fit));
            self.cali_last = true;
        } else {
            if self.cali_last {
                let g = ui.to_global(ct);
                self.emitter.emit_at(vec2(g.0, g.1), 0., self.color);
                let _ = self.cali_hit.play(PlaySfxParams::default());
            }
            self.cali_last = false;
        }

        if let Some((time, pos)) = &self.touch {
            let p = (ot - time) / Self::FADE_TIME;
            if p > 1. {
                self.touch = None;
            } else {
                let p = p.max(0.);
                let c = Color {
                    a: (if p <= 0.5 { 1. } else { (1. - p) * 2. }) * self.color.a,
                    ..self.color
                };
                ui.fill_rect(Rect::new(ct.0 - hw, pos - hh, hw * 2., hh * 2.), c);
            }
        }

        let offset = config.offset * 1000.;
        self.slider
            .render(ui, Rect::new(0.46, -0.1, 0.45, 0.2), ot, offset, format!("{offset:.0}ms"));

        self.emitter.draw(get_frame_time());
    }
}

static EMAIL_REGEX: once_cell::sync::Lazy<regex::Regex> = once_cell::sync::Lazy::new(|| {
    regex::Regex::new(
        r"\A[a-z0-9!#$%&'*+/=?^_‘{|}~-]+(?:\.[a-z0-9!#$%&'*+/=?^_‘{|}~-]+)*@(?:[a-z0-9](?:[a-z0-9-]*[a-z0-9])?\.)+[a-z0-9](?:[a-z0-9-]*[a-z0-9])?\z",
    )
    .unwrap()
});

fn validate_username(username: &str) -> Option<Cow<'static, str>> {
    if !(4..=12).contains(&username.chars().count()) {
        return Some(crate::login::tl!("name-length-req"));
    }
    if username.chars().any(|it| it != '_' && it != '-' && !it.is_alphanumeric()) {
        return Some(crate::login::tl!("name-has-illegal-char"));
    }
    None
}


#[cfg(test)]
mod tests {
    use super::L10N_LOCAL;

    /// 验证 onboarding bundle 能解析全部键（若任一 .ftl 语法错误，Lazy 初始化时直接 panic）。
    #[test]
    fn onboarding_l10n_resolves() {
        let keys = [
            "welcome", "start-setup", "choose-language", "next", "verify-intest", "qq",
            "intest-key", "apply", "intest-invalid", "set-offset", "login-phira",
            "account", "password", "register", "back-login", "email", "username",
            "tutorial", "tutorial-content", "skip", "setup-done", "enter-game",
            "illegal-email", "pwd-length-req", "name-length-req", "name-has-illegal-char",
            "email-sent",
        ];
        for key in keys {
            let s = super::obtl!(key);
            assert_ne!(s, key, "key not translated: {key}");
            assert!(!s.is_empty(), "empty translation for {key}");
        }
        let s = super::obtl!("action-success", "action" => "login");
        assert_ne!(s, "action-success");
        let s = super::obtl!("action-failed", "action" => "register");
        assert_ne!(s, "action-failed");
    }
}
