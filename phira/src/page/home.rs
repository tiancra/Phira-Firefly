prpr_l10n::tl_file!("home");

use super::{
    load_font_with_cksum, set_bold_font, EventPage, LibraryPage, MessagePage, NextPage, Page, ResPackPage, SFader, SettingsPage, SharedState,
    BOLD_FONT_CKSUM,
};
use crate::{
    anim::Anim,
    client::{recv_raw, Character, Client, LoginParams, User, UserManager},
    dir, get_data, get_data_mut,
    icons::Icons,
    login::Login,
    save_data,
    scene::{check_read_tos_and_policy, ProfileScene, TutorialLoadingScene, JUST_LOADED_TOS},
    sync_data,
    threed::ThreeD,
};
use ::rand::{random, thread_rng, Rng};
use anyhow::{bail, Context, Result};
use image::DynamicImage;
use macroquad::prelude::*;
use prpr::{
    core::BOLD_FONT,
    ext::{open_url, screen_aspect, semi_black, semi_white, RectExt, SafeTexture, ScaleType},
    info::ChartInfo,
    scene::{show_error, NextScene},
    task::Task,
    ui::{button_hit_large, clip_rounded_rect, ClipType, Dialog, DRectButton, FontArc, PREFER_ALT_UI, RectButton, Scroll, Ui},
};
use prpr_l10n::LANG_IDENTS;
use reqwest::StatusCode;
use serde::Deserialize;
use std::{
    borrow::Cow,
    sync::{atomic::Ordering, Arc},
};
use tap::Tap;
use tracing::{info, warn};

const BOARD_SWITCH_TIME: f32 = 4.;
const BOARD_TRANSIT_TIME: f32 = 1.2;

/// 主界面划入：返回 (偏移, 渐显 alpha)，dir 决定方向（1 秒 Out Sine）。
#[inline]
fn entry_anim(t: f32, dir: f32) -> (f32, f32) {
    let p = crate::scene::boot::boot_entry_progress(t, 1.0);
    let eased = (p * std::f32::consts::PI / 2.).sin();
    ((1. - eased) * dir * 1.0, eased)
}

type BoldFontUpdateTask = Task<Result<Option<(FontArc, String)>>>;

pub struct HomePage {
    icons: Arc<Icons>,

    btn_play: DRectButton,
    btn_event: DRectButton,
    btn_respack: DRectButton,
    btn_replay: DRectButton,
    btn_msg: DRectButton,
    btn_settings: DRectButton,
    btn_user: DRectButton,

    next_page: Option<NextPage>,

    login: Login,
    update_task: Option<Task<Result<User>>>,

    need_back: bool,
    sf: SFader,

    board_task: Option<Task<Result<Option<(DynamicImage, String, String, String)>>>>,
    board_last_time: f32,
    board_last: Option<String>,
    board_tex_last: Option<SafeTexture>,
    board_tex: Option<SafeTexture>,
    board_dir: bool,
    board_name: Option<String>,
    board_composer: Option<String>,
    board_level: Option<String>,

    has_new_task: Option<Task<Result<bool>>>,
    has_new: bool,

    btn_about: RectButton,
    btn_feedback: RectButton,
    btn_changelog: RectButton,
    btn_exit: RectButton,
    btn_more: RectButton,
    credits_scroll: Scroll,

    check_bold_font_update_task: Option<BoldFontUpdateTask>,

    btn_play_3d: ThreeD,
    btn_other_3d: ThreeD,

    character: Character,
    char_appear_p: Anim<f32>,
    char_last_illu: Option<String>,
    char_last_user_id: Option<i32>,
    char_fetch_task: Option<Task<Result<Character>>>,
    char_illu: Option<SafeTexture>,
    char_illu_task: Option<Task<Result<DynamicImage>>>,
    // progress of character screen
    char_screen_p: Anim<f32>,
    char_btn: RectButton,
    char_text_start: f32,
    char_cached_size: f32,
    char_scroll: Scroll,
    char_edit_btn: RectButton,

    #[cfg(feature = "aa")]
    beian_btn: RectButton,
}

impl HomePage {
    pub async fn new() -> Result<Self> {
        let update_task = if get_data().config.offline_mode {
            None
        } else if let Some(u) = &get_data().me {
            UserManager::request(u.id);
            Some(Task::new(async {
                Client::login(LoginParams::RefreshToken {
                    token: &get_data().tokens.as_ref().unwrap().1,
                })
                .await?;
                Client::get_me().await
            }))
        } else {
            None
        };

        let mut res = Self {
            icons: Arc::new(Icons::new().await?),

            btn_play: DRectButton::new().with_delta(-0.01).no_sound(),
            btn_event: DRectButton::new().with_elevation(0.002).no_sound(),
            btn_respack: DRectButton::new().with_elevation(0.002).no_sound(),
            btn_replay: DRectButton::new().with_radius(0.008).with_delta(-0.003).with_elevation(0.002),
            btn_msg: DRectButton::new().with_radius(0.008).with_delta(-0.003).with_elevation(0.002),
            btn_settings: DRectButton::new().with_radius(0.008).with_delta(-0.003).with_elevation(0.002),
            btn_user: DRectButton::new().with_delta(-0.003),

            next_page: None,

            login: Login::new(),
            update_task,

            need_back: false,
            sf: SFader::new(),

            board_task: None,
            board_last_time: f32::NEG_INFINITY,
            board_last: None,
            board_tex_last: None,
            board_tex: None,
            board_dir: false,
            board_name: None,
            board_composer: None,
            board_level: None,

            has_new_task: None,
            has_new: false,

            btn_about: RectButton::new(),
            btn_feedback: RectButton::new(),
            btn_changelog: RectButton::new(),
            btn_exit: RectButton::new(),
            btn_more: RectButton::new(),
            credits_scroll: Scroll::new().use_clip(ClipType::Clip),

            check_bold_font_update_task: {
                let cksum = BOLD_FONT_CKSUM.with(|it| it.borrow().clone());
                Some(Task::new(async move {
                    let resp = Client::get("/font-bold")
                        .query(&[("cksum", cksum)])
                        .query(&[("new_bold_font", "true")])
                        .send()
                        .await?;
                    if resp.status() == StatusCode::NOT_MODIFIED {
                        info!("bold font not modified");
                        return Ok(None);
                    }
                    if !resp.status().is_success() {
                        let status = resp.status().as_str().to_owned();
                        let text = resp.text().await.context("failed to receive text")?;
                        if let Ok(what) = serde_json::from_str::<serde_json::Value>(&text) {
                            if let Some(detail) = what["error"].as_str() {
                                bail!("request failed ({status}): {detail}");
                            }
                        }
                        bail!("request failed ({status}): {text}");
                    }
                    info!("downloading new bold font");
                    let bytes = resp.bytes().await?;
                    std::fs::write(dir::bold_font_path()?, &bytes).context("failed to save font")?;
                    Ok(Some(load_font_with_cksum(bytes.to_vec())?))
                }))
            },

            btn_play_3d: ThreeD::new(),
            btn_other_3d: ThreeD::new().tap_mut(|it| {
                it.anchor = vec2(0.2, -0.2);
                it.angle = 0.14;
                it.sync();
            }),

            character: get_data().character.clone().unwrap_or_default(),
            char_appear_p: Anim::new(0.),
            char_last_illu: None,
            char_last_user_id: None,
            char_fetch_task: None,
            char_illu: None,
            char_illu_task: None,
            char_screen_p: Anim::new(0.),
            char_btn: RectButton::new(),
            char_text_start: 0.,
            char_cached_size: 0.,
            char_scroll: Scroll::new().use_clip(ClipType::Clip),
            char_edit_btn: RectButton::new(),

            #[cfg(feature = "aa")]
            beian_btn: RectButton::new(),
        };
        res.load_char_illu();

        Ok(res)
    }
}

impl HomePage {
    fn load_char_illu(&mut self) {
        let key = if self.character.illust == "@" {
            format!("@{}", self.character.id)
        } else {
            self.character.illust.clone()
        };
        if self.char_last_illu.as_ref() == Some(&key) {
            return;
        }
        self.char_last_illu = Some(key);

        self.char_appear_p.set(0.);

        #[cfg(closed)]
        if self.character.illust == "@" {
            let id = self.character.id.clone();
            self.char_illu_task =
                Some(Task::new(
                    async move { Ok(image::load_from_memory(&crate::inner::resolve_data(load_file(&format!("res/{id}.char")).await?))?) },
                ));
        } else {
            let file = crate::page::File {
                url: self.character.illust.clone(),
            };
            self.char_illu_task =
                Some(Task::new(async move { Ok(image::load_from_memory(&crate::inner::resolve_data(file.fetch().await?.to_vec()))?) }));
        }
    }

    fn fetch_has_new(&mut self) {
        if get_data().config.offline_mode || get_data().me.is_none() || get_data().tokens.is_none() {
            self.has_new_task = None;
            self.has_new = false;
            return;
        }
        let time = get_data().message_check_time.unwrap_or_default();
        self.has_new_task = Some(Task::new(async move {
            #[derive(Deserialize)]
            struct Resp {
                has: bool,
            }
            let resp: Resp = recv_raw(Client::get("/message/has_new").query(&[("checked", time)]))
                .await?
                .json()
                .await?;
            Ok(resp.has)
        }));
    }

    fn render_not_char(&mut self, ui: &mut Ui, s: &mut SharedState) {
        let t = s.t;
        // 主界面右侧按钮组从右侧边缘外滑入并渐显
        let (off, alpha) = entry_anim(t, 1.);
        let old_alpha = ui.alpha;
        ui.alpha = old_alpha * alpha;
        ui.dx(off);

        let pad = 0.04;
        // play button
        let r = Rect::new(0., -0.33, 0.83, 0.45);
        let mat = self.btn_play_3d.now(ui, r, t);
        let top = ui.with_gl(mat, |ui| {
            s.render_fader(ui, |ui| {
                let top = r.bottom() + 0.02;
                let rad = self.btn_play.config.radius;
                self.btn_play.render_shadow(ui, r, t, |ui, path| {
                    ui.fill_path(&path, semi_black(0.4));
                    if let Some(cur) = &self.board_tex {
                        let p = (t - self.board_last_time) / BOARD_TRANSIT_TIME;
                        if p > 1. {
                            self.board_tex_last = None;
                            ui.fill_path(&path, (**cur, r));
                        } else if let Some(last) = &self.board_tex_last {
                            let (cur, last) = if self.board_dir { (last, cur) } else { (cur, last) };
                            let p = 1. - (1. - p).powi(3);
                            let p = if self.board_dir { 1. - p } else { p };
                            clip_rounded_rect(ui, r, rad, |ui| {
                                let mut nr = r;
                                nr.h = r.h * (1. - p);
                                ui.fill_rect(nr, (**last, nr));

                                nr.h = r.h * p;
                                nr.y = r.bottom() - nr.h;
                                ui.fill_rect(nr, (**cur, nr));
                            });
                        } else {
                            ui.fill_path(&path, (**cur, r, ScaleType::CropCenter, semi_white(p)));
                        }
                    }
                    ui.fill_path(&path, (semi_black(0.7), (r.x, r.y), Color::default(), (r.x + 0.6, r.y)));
                    ui.text(tl!("play")).pos(r.x + pad, r.y + pad).draw();
                    let r = Rect::new(r.x + 0.02, r.bottom() - 0.18, 0.17, 0.17);
                    ui.fill_rect(r, (*self.icons.play, r, ScaleType::Fit, semi_white(0.6)));
                });
                top + 0.03
            })
        });

        let text_and_icon = |s: &mut SharedState, ui: &mut Ui, r: Rect, btn: &mut DRectButton, text, icon| {
            let ow = r.w;
            s.render_fader(ui, |ui| {
                btn.render_shadow(ui, r, t, |ui, path| {
                    ui.fill_path(&path, semi_black(0.4));
                    let ir = Rect::new(r.x + 0.02, r.bottom() - 0.08, 0.14, 0.14);
                    ui.text(text).pos(r.x + 0.026, r.y + 0.026).size(0.7 * r.w / ow).draw();
                    ui.fill_rect(
                        {
                            let mut ir = ir;
                            ir.h = ir.h.min(r.bottom() - ir.y);
                            ir
                        },
                        (icon, ir, ScaleType::Fit, semi_white(0.4)),
                    );
                });
            });
        };

        let mat = self.btn_other_3d.now(ui, Rect::new(0., top - 0.4, 0.83, 0.23), t);
        ui.with_gl(mat, |ui| {
            let r = Rect::new(0., top, 0.38, 0.23);
            text_and_icon(s, ui, r, &mut self.btn_event, tl!("event"), *self.icons.medal);

            let r = Rect::new(r.right() + 0.02, top, 0.29, 0.23);
            text_and_icon(s, ui, r, &mut self.btn_respack, tl!("respack"), *self.icons.respack);

            let lf = r.right() + 0.02;

            s.render_fader(ui, |ui| {
                let r = Rect::new(lf, top, 0.11, 0.07);
                self.btn_msg.render_shadow(ui, r, t, |ui, path| {
                    ui.fill_path(&path, semi_black(0.4));
                    let r = r.feather(-0.005);
                    ui.fill_rect(r, (*self.icons.msg, r, ScaleType::Fit));
                    if self.has_new {
                        let pad = 0.006;
                        ui.fill_circle(r.right() - pad, r.y + pad, 0.008, RED);
                    }
                });

                let r = Rect::new(lf, top + 0.08, 0.11, 0.07);
                self.btn_replay.render_shadow(ui, r, t, |ui, path| {
                    ui.fill_path(&path, semi_black(0.4));
                    let r = r.feather(-0.005);
                    ui.fill_rect(r, (*self.icons.play, r, ScaleType::Fit));
                });

                let r = Rect::new(lf, top + 0.16, 0.11, 0.07);
                self.btn_settings.render_shadow(ui, r, t, |ui, path| {
                    ui.fill_path(&path, semi_black(0.4));
                    let r = r.feather(-0.005);
                    ui.fill_rect(r, (*self.icons.settings, r, ScaleType::Fit));
                });
            });
        });
        ui.dx(-off);
        ui.alpha = old_alpha;
    }

    /// XCHS-style home layout, ported faithfully from XCHS page_app/home_page.rs.
    fn render_a1(&mut self, ui: &mut Ui, s: &mut SharedState) -> Result<()> {
        let t = s.t;
        let top = ui.top;

        fn draw_soft_text(ui: &mut Ui, text: &str, x: f32, y: f32, anchor: (f32, f32), size: f32, color: Color) -> Rect {
            const OFFS: [(f32, f32); 8] = [
                (-0.004, 0.), (0.004, 0.), (0., -0.004), (0., 0.004),
                (-0.003, -0.003), (0.003, -0.003), (-0.003, 0.003), (0.003, 0.003),
            ];
            let shadow = Color::new(0., 0., 0., 0.18 * color.a);
            for (dx, dy) in OFFS {
                ui.text(text).pos(x + dx, y + dy).anchor(anchor.0, anchor.1).no_baseline().size(size).color(shadow).draw();
            }
            ui.text(text).pos(x, y).anchor(anchor.0, anchor.1).no_baseline().size(size).color(color).draw()
        }

        fn fill_vgrad(ui: &mut Ui, r: Rect, max_alpha: f32, rgb: (f32, f32, f32)) {
            const N: i32 = 14;
            for i in 0..N {
                let p0 = i as f32 / N as f32;
                let p1 = (i + 1) as f32 / N as f32;
                let a = max_alpha * p1 * p1;
                ui.fill_rect(
                    Rect::new(r.x, r.y + r.h * p0, r.w, r.h * (p1 - p0) + 0.0015),
                    Color::new(rgb.0, rgb.1, rgb.2, a),
                );
            }
        }

        fn draw_clock(ui: &mut Ui, x: f32, y: f32, r: f32, color: Color) {
            ui.stroke_circle(x, y, r, 0.006, color);
            ui.fill_rect(Rect::new(x - 0.003, y - r * 0.62, 0.006, r * 0.62), color);
            ui.fill_rect(Rect::new(x - 0.002, y - 0.003, r * 0.55, 0.006), color);
        }

        let c_title = Color::new(0.97, 0.98, 1.0, 1.);
        let c_sub = Color::new(0.82, 0.86, 0.92, 0.85);
        let c_pink = Color::new(1.000, 0.580, 0.706, 1.0);

        // backdrop: plum tint + scattered hearts
        s.render_fader(ui, |ui| {
            ui.fill_rect(ui.screen_rect(), Color::new(0.16, 0.08, 0.13, 0.55));
            let sr = ui.screen_rect();
            for i in 0..7 {
                let x = sr.x + 0.12 + i as f32 * 0.3;
                ui.text("\u{2665}")
                    .pos(x, sr.y + 0.06)
                    .anchor(0.5, 0.5)
                    .no_baseline()
                    .size(0.5)
                    .color(Color::new(1.0, 0.776, 0.847, 0.10))
                    .draw();
            }
        });

        // left vertical nav cards
        let nav_x = -0.95;
        let nw = 0.46;
        let ic_play = self.icons.play.clone();
        let ic_event = self.icons.medal.clone();
        let ic_respack = self.icons.respack.clone();
        let ic_msg = self.icons.msg.clone();
        let ic_settings = self.icons.settings.clone();
        let has_new = self.has_new;

        fn nav_card(
            ui: &mut Ui, r: Rect, t: f32, btn: &mut DRectButton,
            icon: SafeTexture, title: &str, subtitle: &str, active: bool, dot: bool,
        ) {
            let bg = if active {
                Color::new(0.176, 0.216, 0.314, 0.60)
            } else {
                Color::new(0.106, 0.129, 0.196, 0.52)
            };
            btn.config.radius = if active { 0.024 } else { 0.02 };
            btn.render_shadow(ui, r, t, |ui, path| {
                ui.fill_path(&path, bg);
                let isz = if active { 0.085 } else { 0.062 };
                let ir = Rect::new(r.x + 0.045, r.center().y - isz / 2., isz, isz);
                ui.fill_rect(ir, (*icon, ir, ScaleType::Fit, Color::new(0.97, 0.98, 1.0, 1.)));
                if dot {
                    ui.fill_circle(ir.right() - 0.002, ir.y + 0.006, 0.009, RED);
                }
                let tx = r.x + 0.045 + isz + 0.05;
                let tsz = if active { 0.62 } else { 0.46 };
                draw_soft_text(ui, title, tx, r.center().y - 0.02, (0., 0.5), tsz, Color::new(0.97, 0.98, 1.0, 1.));
                draw_soft_text(ui, subtitle, tx, r.center().y + 0.045, (0., 0.5), 0.32, Color::new(0.82, 0.86, 0.92, 0.85));
            });
        }

        s.render_fader(ui, |ui| {
            nav_card(ui, Rect::new(nav_x, -0.42, nw, 0.215), t, &mut self.btn_play, ic_play.clone(), "游玩", "START", true, false);
            nav_card(ui, Rect::new(nav_x, -0.175, nw, 0.12), t, &mut self.btn_event, ic_event.clone(), "活动", "EVENT", false, false);
            nav_card(ui, Rect::new(nav_x, -0.035, nw, 0.12), t, &mut self.btn_respack, ic_respack.clone(), "皮肤", "RESPACK", false, false);
            nav_card(ui, Rect::new(nav_x, 0.105, nw, 0.12), t, &mut self.btn_replay, ic_play.clone(), "回放", "REPLAY", false, false);
            nav_card(ui, Rect::new(nav_x, 0.245, nw, 0.12), t, &mut self.btn_msg, ic_msg.clone(), "消息", "MESSAGE", false, has_new);
            nav_card(ui, Rect::new(nav_x, 0.385, nw, 0.12), t, &mut self.btn_settings, ic_settings.clone(), "设置", "SETTINGS", false, false);
        });

        // right big board with song info
        let board_r = Rect::new(-0.47, -top + 0.165, 0.84, 2. * top - 0.325);
        s.render_fader(ui, |ui| {
            ui.fill_path(&board_r.rounded(0.03), Color::new(0.05, 0.06, 0.10, 1.));
            if let Some(tex) = &self.board_tex {
                ui.fill_path(&board_r.rounded(0.03), (**tex, board_r, ScaleType::CropCenter, Color::new(1., 1., 1., 1.)));
            }
            let scrim = Rect::new(board_r.x + 0.03, board_r.bottom() - 0.32, board_r.w - 0.06, 0.32);
            fill_vgrad(ui, scrim, 0.82, (0.02, 0.03, 0.06));
            let name = self.board_name.clone().unwrap_or_else(|| "Phira".to_string());
            let composer = self.board_composer.clone().unwrap_or_default();
            let level = self.board_level.clone().unwrap_or_default();
            draw_soft_text(ui, &name, board_r.x + 0.045, board_r.bottom() - 0.075, (0., 1.), 0.5, c_title);
            if !composer.is_empty() {
                draw_soft_text(ui, &composer, board_r.x + 0.05, board_r.bottom() - 0.04, (0., 1.), 0.34, c_sub);
            }
            if !level.is_empty() {
                draw_soft_text(ui, &level, board_r.right() - 0.04, board_r.bottom() - 0.06, (1., 1.), 0.5, c_title);
            }
        });

        // credits panel
        let credits_r = Rect::new(0.385, board_r.y, 0.55, board_r.h);
        s.render_fader(ui, |ui| {
            ui.fill_path(&credits_r.rounded(0.014), Color::new(0.063, 0.078, 0.125, 0.50));
            let more = ui.text("更多").pos(credits_r.right() - 0.045, credits_r.y + 0.06).anchor(1., 0.5).no_baseline().size(0.4).color(c_sub).draw();
            self.btn_more.set(ui, more.feather(0.012));
            self.credits_scroll.size((credits_r.w - 0.09, credits_r.h - 0.14));
            ui.scope(|ui| {
                ui.dx(credits_r.x + 0.045);
                ui.dy(credits_r.y + 0.11);
                self.credits_scroll.render(ui, |ui| {
                    ui.text("制作组").pos(0., 0.).no_baseline().size(0.52).color(c_title).draw();
                    ui.text("小天是个小男娘").pos(0., 0.085).no_baseline().multiline().max_width(credits_r.w - 0.09).size(0.4).color(c_sub).draw();
                    (credits_r.w - 0.09, 0.3)
                });
            });
        });

        // top-left user chip + center title
        s.render_fader(ui, |ui| {
            let rad = 0.05;
            let cx = -0.90;
            let cy = -top + 0.085;
            self.btn_user.config.radius = rad;
            let r = Rect::new(cx, cy, 0., 0.).feather(rad);
            self.btn_user.build(ui, t, r, |ui, _| {
                ui.avatar(cx, cy, r.w / 2., t,
                    get_data().me.as_ref()
                        .map(|me| UserManager::opt_avatar(me.id, &self.icons.user))
                        .unwrap_or(Err(self.icons.user.clone())));
            });
            let tx = cx + rad + 0.025;
            let uid;
            if let Some(me) = &get_data().me {
                uid = me.id;
                draw_soft_text(ui, &me.name, tx, cy - 0.02, (0., 0.5), 0.5, c_title);
                draw_soft_text(ui, &format!("RKS {:.2}", me.rks), tx, cy + 0.02, (0., 0.5), 0.4, c_pink);
            } else {
                uid = 0;
                draw_soft_text(ui, "未登录", tx, cy, (0., 0.5), 0.5, c_title);
            }
            let chip = Rect::new(tx, cy + 0.04, 0.09, 0.028);
            ui.fill_path(&chip.rounded(0.007), Color::new(1., 1., 1., 0.16));
            ui.text(format!("UID {}", uid)).pos(chip.center().x, chip.center().y).anchor(0.5, 0.5).no_baseline().size(0.3).color(c_sub).draw();
        });

        // bottom link bar
        s.render_fader(ui, |ui| {
            let y = top - 0.11;
            let ic = 0.045;
            let r = Rect::new(-0.94, y - ic / 2., ic, ic);
            ui.fill_rect(r, (*self.icons.info, r, ScaleType::Fit, c_sub));
            let txt = draw_soft_text(ui, "关于", -0.94 + ic + 0.02, y, (0., 0.5), 0.55, c_sub);
            self.btn_about.set(ui, Rect::new(-0.94, y - 0.035, txt.right() + 0.94, 0.07));

            let r = Rect::new(-0.70, y - ic / 2., ic, ic);
            ui.fill_rect(r, (*self.icons.msg, r, ScaleType::Fit, c_sub));
            let txt = draw_soft_text(ui, "反馈", -0.70 + ic + 0.02, y, (0., 0.5), 0.55, c_sub);
            self.btn_feedback.set(ui, Rect::new(-0.70, y - 0.035, txt.right() + 0.70, 0.07));

            draw_clock(ui, -0.42 + ic / 2., y, ic / 2., c_sub);
            let txt = draw_soft_text(ui, "日志", -0.42 + ic + 0.02, y, (0., 0.5), 0.55, c_sub);
            self.btn_changelog.set(ui, Rect::new(-0.42, y - 0.035, txt.right() + 0.42, 0.07));

            let txt = draw_soft_text(ui, "退出", 0.95, y, (1., 0.5), 0.62, c_sub);
            self.btn_exit.set(ui, txt.feather(0.02));

            draw_soft_text(ui, "Phira-Firefly", 0., top - 0.035, (0.5, 0.5), 0.62, c_pink);
        });

        self.login.render(ui, t);
        self.sf.render(ui, t);
        Ok(())
    }

}

impl Page for HomePage {
    fn label(&self) -> Cow<'static, str> {
        "PHIRA".into()
    }

    fn enter(&mut self, s: &mut SharedState) -> Result<()> {
        if self.need_back {
            self.sf.enter(s.t);
            self.need_back = false;
        }
        self.fetch_has_new();
        Ok(())
    }

    fn touch(&mut self, touch: &Touch, s: &mut SharedState) -> Result<bool> {
        if self.sf.transiting() {
            return Ok(true);
        }
        let t = s.t;
        let rt = s.rt;
        if self.login.touch(touch, s.t) {
            return Ok(true);
        }
        if self.char_screen_p.now(rt) < 1e-2 {
            self.btn_play_3d.touch(touch, t);
            if self.btn_play.touch(touch, t) {
                button_hit_large();
                self.next_page = Some(NextPage::Overlay(Box::new(LibraryPage::new(Arc::clone(&self.icons), s.icons.clone())?)));
                return Ok(true);
            }
            if self.btn_event.touch(touch, t) {
                button_hit_large();
                if get_data().me.is_none() {
                    self.login.enter(t);
                } else {
                    self.next_page = Some(NextPage::Overlay(Box::new(EventPage::new(Arc::clone(&self.icons), s.icons.clone()))));
                }
                return Ok(true);
            }
            if self.btn_respack.touch(touch, t) {
                button_hit_large();
                self.next_page = Some(NextPage::Overlay(Box::new(ResPackPage::new(Arc::clone(&self.icons))?)));
                return Ok(true);
            }
            if self.btn_msg.touch(touch, t) {
                self.next_page = Some(NextPage::Overlay(Box::new(MessagePage::new(Arc::clone(&self.icons), s.icons.clone()))));
                return Ok(true);
            }
            if self.btn_replay.touch(touch, t) {
                button_hit_large();
                self.next_page = Some(NextPage::Overlay(Box::new(super::ReplayListPage::new(Arc::clone(&self.icons), s.icons.clone())?)));
                return Ok(true);
            }
            if self.btn_settings.touch(touch, t) {
                self.next_page = Some(NextPage::Overlay(Box::new(SettingsPage::new(self.icons.icon.clone(), self.icons.lang.clone()))));
                return Ok(true);
            }
        } else {
            if self.char_scroll.touch(touch, t) {
                return Ok(true);
            }
            if self.char_edit_btn.touch(touch) {
                let _ = open_url("https://phira.moe/settings/account");
            }
        }
        if self.btn_more.touch(touch) {
            Dialog::plain("制作组", "Phira-Firefly").show();
            return Ok(true);
        }
        if self.btn_about.touch(touch) {
            Dialog::plain("关于", concat!("Phira-Firefly ", env!("CARGO_PKG_VERSION"))).show();
            return Ok(true);
        }
        if self.btn_feedback.touch(touch) {
            Dialog::plain("反馈", "请前往 QQ 群或 GitHub 反馈").show();
            return Ok(true);
        }
        if self.btn_changelog.touch(touch) {
            Dialog::plain("更新日志", "查看版本更新内容").show();
            return Ok(true);
        }
        if self.btn_exit.touch(touch) {
            Dialog::plain("退出", "确定要退出吗？")
                .buttons(vec!["取消".to_string(), "退出".to_string()])
                .listener(|_, id| {
                    if id == 1 {
                        std::process::exit(0);
                    }
                    false
                })
                .show();
            return Ok(true);
        }
        if self.btn_user.touch(touch, t) {
            if let Some(me) = &get_data().me {
                self.need_back = true;
                self.sf.goto(t, ProfileScene::new(me.id, self.icons.user.clone(), s.icons.clone()));
            } else {
                self.login.enter(t);
            }
            return Ok(true);
        }
        #[cfg(feature = "aa")]
        if self.beian_btn.touch(touch) {
            let _ = open_url("https://beian.miit.gov.cn/#/home");
            return Ok(true);
        }
        if self.char_btn.touch(touch) {
            if !self.char_screen_p.transiting(rt) {
                let to = if self.char_screen_p.now(rt) < 0.5 {
                    self.char_text_start = rt;
                    1.
                } else {
                    0.
                };
                self.char_screen_p.goto(to, rt, 0.5);
            }
            return Ok(true);
        }
        Ok(false)
    }

    fn update(&mut self, s: &mut SharedState) -> Result<()> {
        let t = s.t;
        self.login.update(t)?;
        if self.login.take_start_tutorial() {
            self.need_back = true;
            match TutorialLoadingScene::new() {
                Ok(scene) => self.sf.goto(t, scene),
                Err(err) => show_error(err),
            }
        }
        let current_user = Some(get_data().me.as_ref().map_or(-1, |it| it.id));
        self.char_scroll.update(t);
        if self.char_last_user_id != current_user {
            let locale = get_data().language.clone().unwrap_or(LANG_IDENTS[0].to_string());
            self.char_last_user_id = current_user;
            if get_data().config.offline_mode || get_data().me.is_none() || get_data().tokens.is_none() {
                self.char_fetch_task = None;
            } else {
                self.char_fetch_task =
                    Some(Task::new(async move { Ok(recv_raw(Client::get("/me/char").query(&[("locale", locale)])).await?.json().await?) }));
            }
        }
        if let Some(task) = &mut self.update_task {
            if let Some(res) = task.take() {
                match res {
                    Err(err) => {
                        if format!("{err:?}").contains("invalid token") {
                            get_data_mut().me = None;
                            get_data_mut().tokens = None;
                            let _ = save_data();
                            sync_data();
                        }
                        show_error(err.context(tl!("failed-to-update") + "\n" + tl!("note-try-login-again")));
                    }
                    Ok(val) => {
                        get_data_mut().me = Some(val);
                        save_data()?;
                    }
                }
                self.update_task = None;
            }
        }
        if self.board_task.is_none() && t - self.board_last_time > BOARD_SWITCH_TIME {
            let charts = &get_data().charts;
            let last_index = self
                .board_last
                .as_ref()
                .and_then(|path| charts.iter().position(|it| &it.local_path == path));
            if charts.is_empty() || (charts.len() == 1 && last_index.is_some()) {
                self.board_task = Some(Task::new(async move { Ok(None) }));
            } else {
                let mut index = thread_rng().gen_range(0..(charts.len() - last_index.is_some() as usize));
                if last_index.is_some_and(|it| it <= index) {
                    index += 1;
                }
                let path = charts[index].local_path.clone();
                let dir = prpr::dir::Dir::new(format!("{}/{}", dir::charts()?, path))?;
                self.board_last = Some(path);
                self.board_task = Some(Task::new(async move {
                    let info: ChartInfo = serde_yaml::from_reader(dir.open("info.yml")?)?;
                    let bytes = dir.read(info.illustration)?;
                    Ok(Some((image::load_from_memory(&bytes)?, info.name, info.composer, info.level)))
                }));
            }
        }
        if let Some(task) = &mut self.board_task {
            if let Some(res) = task.take() {
                match res {
                    Err(err) => {
                        warn!(?err, "failed to load illustration for board");
                    }
                    Ok(res) => {
                        if let Some((image, name, composer, level)) = res {
                            let tex: SafeTexture = image.into();
                            self.board_tex_last = self.board_tex.replace(tex);
                            self.board_dir = random();
                            self.board_name = Some(name);
                            self.board_composer = Some(composer);
                            self.board_level = Some(level);
                        }
                    }
                }
                self.board_last_time = t;
                self.board_task = None;
            }
        }
        if let Some(task) = &mut self.has_new_task {
            if let Some(res) = task.take() {
                match res {
                    Err(err) => {
                        warn!("fail to load has new {:?}", err);
                    }
                    Ok(has) => {
                        self.has_new = has;
                    }
                }
                self.has_new_task = None;
            }
        }
        if let Some(task) = &mut self.check_bold_font_update_task {
            if let Some(res) = task.take() {
                match res {
                    Err(err) => {
                        warn!("fail to check bold font update {:?}", err);
                    }
                    Ok(None) => {}
                    Ok(Some(parsed)) => {
                        info!(cksum = parsed.1, "new bold font");
                        set_bold_font(parsed);
                    }
                }
                self.check_bold_font_update_task = None;
            }
        }
        if let Some(task) = &mut self.char_illu_task {
            if let Some(res) = task.take() {
                match res {
                    Err(err) => {
                        warn!(?err, "fail to load char illu");
                    }
                    Ok(image) => {
                        self.char_appear_p.goto(1., t, 0.5);
                        let tex: SafeTexture = image.into();
                        self.char_illu = Some(tex.with_mipmap());
                    }
                }
                self.char_illu_task = None;
            }
        }
        if let Some(task) = &mut self.char_fetch_task {
            if let Some(res) = task.take() {
                match res {
                    Err(err) => {
                        warn!("fail to load char");
                    }
                    Ok(char) => {
                        info!(?char, "char loaded");
                        self.character = char;
                        get_data_mut().character = Some(self.character.clone());
                        let _ = save_data();
                        self.char_cached_size = 0.;
                        self.load_char_illu();
                    }
                }
                self.char_fetch_task = None;
            }
        }
        if JUST_LOADED_TOS.fetch_and(false, Ordering::Relaxed) {
            check_read_tos_and_policy(true, true);
        }

        Ok(())
    }

    fn render(&mut self, ui: &mut Ui, s: &mut SharedState) -> Result<()> {
        if PREFER_ALT_UI.load(Ordering::Relaxed) {
            return self.render_a1(ui, s);
        }
        let t = s.t;
        let rt = s.rt;

        let cp = self.char_screen_p.now(rt);
        let (off, alpha) = entry_anim(t, -1.);
        let old_alpha = ui.alpha;
        ui.alpha = old_alpha * alpha;
        ui.dx(off);
        s.render_fader(ui, |ui| {
            let r = Rect::new(-1. + 0.14 * cp, -ui.top + 0.12, 1., 1.7);
            if let Some(illu) = &self.char_illu {
                let p = self.char_appear_p.now(t);
                let (ox, oy, ow, oh) = self.character.illu_adjust;
                let r = Rect::new(r.x + ox, r.y + (1. - p) * 0.05 + oy, r.w + ow, r.h + oh);
                ui.fill_rect(ui.screen_rect(), (**illu, r, ScaleType::CropCenter, semi_white(p)));
            }
            self.char_btn.set(ui, r);

            if cp > 1e-5 {
                let height = 0.8 - ((screen_aspect() - 16. / 9.) * 0.2).min(0.2);
                let r = Rect::new(0.16, (-height - height * cp) / 4., 0.6, height);
                let mat = ThreeD::build(vec2(0., 0.), r, 0.12);
                let gl = unsafe { get_internal_gl() }.quad_gl;
                gl.push_model_matrix(mat);

                ui.alpha(cp, |ui| {
                    let mut r = Rect::new(r.x, r.y + 0.14, r.w, r.h - 0.14);
                    ui.fill_rect(r, semi_black(0.3));
                    ui.fill_rect(Rect::new(r.x, r.y, 0.01, r.h), WHITE);
                    let mut t = ui.text(tl!("change-char")).pos(r.x + 0.01, r.bottom() + 0.015).size(0.3);
                    let ir = t.measure().feather(0.007);
                    t.ui.fill_rect(ir, semi_black(0.2));
                    self.char_edit_btn.set(t.ui, ir);
                    t.draw();
                    let pad = 0.01;

                    let mut t = ui
                        .text(self.character.name_en())
                        .pos(r.right() - pad, r.bottom() - pad)
                        .anchor(1., 1.)
                        .color(semi_white(0.2));
                    if self.char_cached_size < 1e-6 {
                        let mut initial = 2.;
                        loop {
                            t = t.size(initial);
                            if t.measure().w < r.w * 0.7 {
                                break;
                            }
                            initial *= 0.95;
                        }
                        self.char_cached_size = initial;
                    } else {
                        t = t.size(self.char_cached_size);
                    }
                    t.draw();

                    r.x += 0.01;
                    r.w -= 0.01;

                    self.char_scroll.size((r.w, r.h));
                    ui.scope(|ui| {
                        ui.dx(r.x);
                        ui.dy(r.y);
                        let ow = r.w;
                        self.char_scroll.render(ui, |ui| {
                            let r = Rect::new(0., 0., r.w, r.h);
                            let r = r.feather(-0.03);
                            let r = ui.text(&self.character.intro).pos(r.x, r.y).max_width(r.w).multiline().size(0.4).draw();
                            (ow, r.h + 0.1)
                        });
                    });
                });

                let r = Rect::new(r.x, r.y, 0.4, 0.12);

                ui.alpha(cp, |ui| {
                    let r = ui
                        .text(&self.character.name)
                        .pos(r.x + (1. - cp) * 0.12 + 0.01, r.center().y)
                        .anchor(0., 0.5)
                        .size(self.character.name_size.unwrap_or(1.4))
                        .draw_using(&BOLD_FONT);

                    let off = if self.character.baseline { 0. } else { 0.01 };
                    ui.text(format!("Artist: {}", self.character.artist))
                        .pos(r.right() + (1. - cp) * 0.1 + 0.02, r.bottom() + off - 0.03)
                        .anchor(0., 1.)
                        .size(0.34)
                        .color(semi_white(0.7))
                        .draw();
                    ui.text(format!("Designer: {}", self.character.designer))
                        .pos(r.right() + (1. - cp) * 0.1 + 0.016, r.bottom() + off)
                        .anchor(0., 1.)
                        .size(0.34)
                        .color(semi_white(0.7))
                        .draw();
                });

                gl.pop_model_matrix();
            }
        });
        ui.dx(-off);
        ui.alpha = old_alpha;

        ui.alpha(1. - cp, |ui| {
            self.render_not_char(ui, s);
        });

        s.fader.roll_back();
        let (off, alpha) = entry_anim(t, 1.);
        let old_alpha = ui.alpha;
        ui.alpha = old_alpha * alpha;
        ui.dx(off);
        s.render_fader(ui, |ui| {
            let rad = 0.05;
            let ct = (0.92, -ui.top + 0.08);
            self.btn_user.config.radius = rad;
            let r = Rect::new(ct.0, ct.1, 0., 0.).feather(rad);
            self.btn_user.build(ui, t, r, |ui, _| {
                ui.avatar(
                    ct.0,
                    ct.1,
                    r.w / 2.,
                    t,
                    get_data()
                        .me
                        .as_ref()
                        .map(|user| UserManager::opt_avatar(user.id, &self.icons.user))
                        .unwrap_or(Err(self.icons.user.clone())),
                );
            });
            let rt = ct.0 - rad - 0.02;
            if let Some(me) = &get_data().me {
                ui.text(&me.name).pos(rt, r.center().y + 0.002).anchor(1., 1.).size(0.6).draw();
                ui.text(format!("RKS {:.2}", me.rks))
                    .pos(rt, r.center().y + 0.008)
                    .anchor(1., 0.)
                    .size(0.4)
                    .color(semi_white(0.6))
                    .draw();
            } else {
                ui.text(tl!("not-logged-in"))
                    .pos(rt, r.center().y)
                    .anchor(1., 0.5)
                    .no_baseline()
                    .size(0.6)
                    .draw();
            }

            #[cfg(feature = "aa")]
            {
                let r = ui.screen_rect();
                let r = ui
                    .text("备案号：闽ICP备18008307号-64A")
                    .pos(r.x + 0.02, r.bottom() - 0.03)
                    .size(0.5)
                    .anchor(0., 1.)
                    .draw();
                self.beian_btn.set(ui, r);
            }
        });
        ui.dx(-off);
        ui.alpha = old_alpha;

        self.login.render(ui, t);
        self.sf.render(ui, t);

        Ok(())
    }



    fn next_page(&mut self) -> NextPage {
        self.next_page.take().unwrap_or_default()
    }

    fn next_scene(&mut self, s: &mut SharedState) -> NextScene {
        self.sf.next_scene(s.t).unwrap_or_default()
    }
}
