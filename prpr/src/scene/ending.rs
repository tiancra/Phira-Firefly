prpr_l10n::tl_file!("ending");

use super::{draw_background, game::SimpleRecord, loading::UploadFn, NextScene, Scene};
use crate::{
    config::{Config, Mods},
    core::{BOLD_FONT, PGR_FONT},
    ext::{create_audio_manger, draw_parallelogram, draw_parallelogram_ex, draw_text_aligned, rect_shadow, semi_black, semi_white, RectExt, SafeTexture, ScaleType, PARALLELOGRAM_SLOPE},
    info::ChartInfo,
    judge::{icon_index, icon_index_xcsim, Judge, PlayResult},
    scene::show_message,
    task::Task,
    time::TimeManager,
    ui::{button_hit, clip_sector, DRectButton, Dialog, MessageHandle, RectButton, Ui, PREFER_ALT_UI},
};
use anyhow::Result;
use macroquad::prelude::*;
use sasa::{AudioClip, AudioManager, Music, MusicParams};
use serde::Deserialize;
use std::{cell::RefCell, ops::DerefMut};

#[derive(Deserialize)]
pub struct RecordUpdateState {
    pub best: bool,
    pub improvement: u32,
    pub gain_exp: f32,
    pub new_rks: Option<f32>,
}

pub struct EndingScene {
    background: SafeTexture,
    illustration: SafeTexture,
    player: SafeTexture,
    icons: [SafeTexture; 8],
    icon_retry: SafeTexture,
    icon_proceed: SafeTexture,
    mod_icons: [SafeTexture; 7],
    target: Option<RenderTarget>,
    audio: AudioManager,
    bgm: Music,

    info: ChartInfo,
    result: PlayResult,
    player_name: String,
    player_rks: Option<f32>,
    autoplay: bool,
    use_keyboard: bool,
    speed: f32,
    mods: Mods,
    next: u8, // 0 -> none, 1 -> pop, 2 -> exit
    update_state: Option<RecordUpdateState>,
    rated: bool,

    upload_fn: Option<UploadFn>,
    upload_task: Option<(Task<Result<RecordUpdateState>>, MessageHandle)>,
    record_data: Option<Vec<u8>>,
    best_record: Option<SimpleRecord>,

    btn_retry: DRectButton,
    btn_proceed: DRectButton,
    btn_detail: RectButton,
    detail_mode: bool,

    tr_start: f32,

    avg_fps: Option<f32>,
}

impl EndingScene {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        background: SafeTexture,
        illustration: SafeTexture,
        player: SafeTexture,
        icons: [SafeTexture; 8],
        icon_retry: SafeTexture,
        icon_proceed: SafeTexture,
        mod_icons: [SafeTexture; 7],
        info: ChartInfo,
        result: PlayResult,
        config: &Config,
        bgm: AudioClip,
        upload_fn: Option<UploadFn>,
        player_rks: Option<f32>,
        historic_best: u32,
        record_data: Option<Vec<u8>>,
        best_record: Option<SimpleRecord>,
        avg_fps: Option<f32>,
    ) -> Result<Self> {
        let mut audio = create_audio_manger(config)?;
        let bgm = audio.create_music(
            bgm,
            MusicParams {
                amplifier: config.volume_music,
                loop_mix_time: 0.,
                ..Default::default()
            },
        )?;
        let upload_task = upload_fn
            .as_ref()
            .and_then(|f| record_data.clone().map(|data| (f(data), show_message(tl!("uploading")).handle())));
        Ok(Self {
            background,
            illustration,
            player,
            icons,
            icon_retry,
            icon_proceed,
            mod_icons,
            target: None,
            audio,
            bgm,
            update_state: if upload_task.is_some() {
                None
            } else {
                let (best, improvement) = if result.score > historic_best {
                    (true, result.score - historic_best)
                } else {
                    (false, 0)
                };
                Some(RecordUpdateState {
                    best,
                    improvement,
                    gain_exp: 0.,
                    new_rks: None,
                })
            },
            rated: upload_task.is_some(),

            info,
            result,
            player_name: config.player_name.clone(),
            player_rks,
            autoplay: config.autoplay(),
            use_keyboard: config.use_keyboard,
            speed: config.speed,
            mods: config.mods,
            next: 0,

            upload_fn,
            upload_task,
            record_data,
            best_record,
            detail_mode: false,

            btn_retry: DRectButton::new(),
            btn_proceed: DRectButton::new(),
            btn_detail: RectButton::new(),

            tr_start: f32::NAN,

            avg_fps,
        })
    }
}

thread_local! {
    static RE_UPLOAD: RefCell<bool> = RefCell::default();
}

/// 与 xcsim 的 `ext::draw_illustration` 等价：按 0.076 网格换算尺寸，居中用平行四边形阴影绘制。
fn draw_illustration(tex: Texture2D, x: f32, y: f32, w: f32, h: f32, color: Color) -> Rect {
    let r = illustration_rect(x, y, w, h);
    let tex_ratio = tex.width() / tex.height();
    let rect_ratio = r.w / r.h;
    let tex_rect = if tex_ratio > rect_ratio {
        let new_w = rect_ratio / tex_ratio;
        Rect::new((1. - new_w) / 2., 0., new_w, 1.)
    } else {
        let new_h = tex_ratio / rect_ratio;
        Rect::new(0., (1. - new_h) / 2., 1., new_h)
    };
    draw_parallelogram(r, Some((tex, tex_rect)), color, true);
    r
}

fn illustration_rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    let scale = 0.076;
    let w = scale * 13. * w;
    let h = scale * 7. * h;
    Rect::new(x - w / 2., y - h / 2., w, h)
}

/// 按纹理原始宽高比在矩形内等比缩小（contain），避免 `ScaleType::Fit` 将非方形图拉伸变形。
fn contain_rect(tex: &Texture2D, r: Rect) -> Rect {
    let (tw, th) = (tex.width() as f32, tex.height() as f32);
    if tw <= 0. || th <= 0. {
        return r;
    }
    let ratio = tw / th;
    let (w, h) = if r.w / r.h > ratio {
        (r.h * ratio, r.h)
    } else {
        (r.w, r.w / ratio)
    };
    Rect::new(r.x + (r.w - w) / 2., r.y + (r.h - h) / 2., w, h)
}

impl EndingScene {
    /// 结算页：布局照抄 Phira-Vrenxz/prpr/src/scene/ending.rs，
    /// 并补回 Firefly 的特性（Mod 图标、RKS/新 RKS、详情展开、上传状态、退场过渡）。
    fn render_xchs(&mut self, tm: &mut TimeManager, ui: &mut Ui) -> Result<()> {
        fn ran(t: f32, l: f32, r: f32) -> f32 {
            ((t - l) / (r - l)).clamp(0., 1.)
        }

        let mut cam = ui.camera();
        let asp = -cam.zoom.y;
        let top = 1. / asp;
        let t = tm.now() as f32;
        cam.render_target = self.target;
        let sr = ui.screen_rect();
        set_camera(&cam);
        draw_background(*self.background, ui.viewport);

        let xcsim = self.result.xcsim;
        let score = self.result.score;
        let accuracy = self.result.accuracy as f32;
        let max_combo = self.result.max_combo;
        let num_of_notes = self.result.num_of_notes;
        let counts = self.result.counts;
        let early = self.result.early;
        let late = self.result.late;
        let shiny = self.result.shiny_perfect;
        let early_kind = self.result.early_kind;
        let late_kind = self.result.late_kind;
        let detail_mode = self.detail_mode;
        let grade = if xcsim {
            icon_index_xcsim(score)
        } else {
            icon_index(score, max_combo == num_of_notes)
        };

        let slope = PARALLELOGRAM_SLOPE;
        let dx = 0.06;
        let c = Color::new(0., 0., 0., 0.6);
        let cb = Color::new(0., 0., 0., 1.);

        let status = {
            let spd = if (self.speed - 1.).abs() <= 1e-4 {
                String::new()
            } else {
                format!(" {:.2}x", self.speed)
            };
            // 不带括号：Phira-Firefly / Phira-Firefly 1.50x / Phira-Firefly AUTOPLAY 1.50x
            if self.autoplay {
                format!("Phira-Firefly AUTOPLAY{spd}")
            } else if !self.rated {
                format!("Phira-Firefly{spd}")
            } else if let Some(state) = &self.update_state {
                if state.best {
                    format!("Phira-Firefly{spd}  NEW BEST +{:07}", state.improvement)
                } else {
                    format!("Phira-Firefly{spd}")
                }
            } else {
                "Uploading…".to_owned()
            }
        };
        let score_text = if xcsim { format!("{score:08}") } else { format!("{score:07}") };

        // 公共几何
        let r = illustration_rect(-0.38, 0., 1., 1.2);
        let main = Rect::new(r.right() - 0.05, r.y, r.w * 0.84, r.h / 2.);
        let d = r.h / 16.;
        let s1 = Rect::new(main.x - d * 4. * slope, main.bottom() + d, main.w - d * 5. * slope, d * 3.);
        let s2 = Rect::new(s1.x - d * 4. * slope, s1.bottom() + d, s1.w, s1.h);

        // ---- 曲绘 + 底部渐变 + 曲名/难度 ----
        ui.with_gl(Mat4::from_translation(vec3((1. - ran(t, 0.1, 1.3)).powi(3) * 2., 0., 0.)), |ui| {
            draw_illustration(*self.illustration, -0.38, 0., 1., 1.2, WHITE);
            let ratio = 0.2;
            draw_parallelogram_ex(
                Rect::new(r.x, r.y + r.h * (1. - ratio), r.w - r.h * (1. - ratio) * slope, r.h * ratio),
                None,
                Color::default(),
                Color::new(0., 0., 0., 0.7),
                true,
            );
            let rr = draw_text_aligned(ui, &self.info.level, r.right() - r.h / 7. * 13. * 0.13 - 0.01, r.bottom() - top / 20., (1., 1.), 0.34, WHITE);
            // 曲名：优先大号字，放不下就换小号并裁切
            let p = (r.x + 0.04, r.bottom() - top / 20.);
            let mw = rr.x - 0.02 - p.0;
            let mut text = ui.text(&self.info.name).pos(p.0, p.1).anchor(0., 1.).size(0.7);
            if text.measure().w <= mw {
                text.draw();
            } else {
                drop(text);
                ui.text(&self.info.name).pos(p.0, p.1).anchor(0., 1.).size(0.5).max_width(mw).draw();
            }
        });

        // ---- 成绩板：状态 + 分数 + 评级图标 ----
        ui.with_gl(Mat4::from_translation(vec3((1. - ran(t, 0.2, 1.3)).powi(3) * 2., 0., 0.)), |ui| {
            draw_parallelogram(main, None, c, true);
            let r2 = draw_text_aligned(ui, &status, main.x + dx, main.bottom() - 0.035, (0., 1.), 0.34, WHITE);
            draw_text_aligned(ui, &score_text, r2.x, r2.y - 0.023, (0., 1.), 1., WHITE);
            let ps = ran(t, 1.4, 1.9).powi(2);
            let s = main.h * 0.67;
            // 评级图标：相对 Vrenxz 原位置往左上挪一点
            let ct = (main.right() - main.h * slope - s / 2. - 0.03, r2.bottom() + 0.02 - s / 2. - 0.02);
            let s = s + s * (1. - ps) * 0.3;
            let g_icon = &self.icons[grade];
            let (tw, th) = (g_icon.width() as f32, g_icon.height() as f32);
            let (dw, dh) = if tw > 0. && th > 0. && tw / th > 1. {
                (s, s * th / tw)
            } else if tw > 0. && th > 0. {
                (s * tw / th, s)
            } else {
                (s, s)
            };
            draw_texture_ex(
                **g_icon,
                ct.0 - dw / 2.,
                ct.1 - dh / 2.,
                Color::new(1., 1., 1., ps),
                DrawTextureParams {
                    dest_size: Some(vec2(dw, dh)),
                    ..Default::default()
                },
            );
        });

        // ---- 最大连击 + 准度 ----
        ui.with_gl(Mat4::from_translation(vec3((1. - ran(t, 0.4, 1.5)).powi(3) * 2., 0., 0.)), |ui| {
            draw_parallelogram(s1, None, c, true);
            let dy = 0.025;
            let r3 = draw_text_aligned(ui, "Max Combo", s1.x + dx, s1.bottom() - dy, (0., 1.), 0.34, WHITE);
            draw_text_aligned(ui, &max_combo.to_string(), r3.x, r3.y - 0.01, (0., 1.), 0.7, WHITE);
            let r4 = draw_text_aligned(ui, "Accuracy", s1.right() - dx, s1.bottom() - dy, (1., 1.), 0.34, WHITE);
            draw_text_aligned(ui, &format!("{:.2}%", accuracy * 100.), r4.right(), r4.y - 0.01, (1., 1.), 0.7, WHITE);
        });

        // ---- 判定统计 + Early/Late（Perfect+ 只有 XC-SIM 谱面才有） ----
        ui.with_gl(Mat4::from_translation(vec3((1. - ran(t, 0.5, 1.7)).powi(3) * 2., 0., 0.)), |ui| {
            draw_parallelogram(s2, None, c, true);
            let dy1 = 0.025;
            let dy2 = 0.015;
            let bg = 0.57;
            let sm = 0.26;
            let draw_count = |ui: &mut Ui, ratio: f32, id: usize, name: &str, count: u32| {
                let r5 = draw_text_aligned(ui, name, s2.x + s2.w * ratio, s2.bottom() - dy1, (0.5, 1.), sm, WHITE);
                // Firefly 特性：详情模式下（非 XC-SIM）显示每个判定区的 Early/Late 细分
                if detail_mode && !xcsim && id < 3 {
                    let r = draw_text_aligned(ui, &format!("-{}", early_kind[id]), r5.center().x - 0.008, r5.y - dy2, (1., 1.), sm, Color::new(0.63, 0.83, 0.98, 1.));
                    draw_text_aligned(ui, &format!("+{}", late_kind[id]), r.right() + 0.016, r5.y - dy2, (0., 1.), sm, Color::new(1., 0.67, 0.57, 1.));
                } else {
                    draw_text_aligned(ui, &count.to_string(), r5.center().x, r5.y - dy2, (0.5, 1.), bg, WHITE);
                }
            };
            if xcsim {
                draw_count(ui, 0.127, 0, "Perfect+", shiny);
                draw_count(ui, 0.325, 1, "Perfect", counts[0].saturating_sub(shiny));
                draw_count(ui, 0.46, 2, "Good", counts[1]);
                draw_count(ui, 0.595, 3, "Bad", counts[2]);
                draw_count(ui, 0.73, 4, "Miss", counts[3]);
            } else {
                draw_count(ui, 0.14, 0, "Perfect", counts[0]);
                draw_count(ui, 0.33, 1, "Good", counts[1]);
                draw_count(ui, 0.46, 2, "Bad", counts[2]);
                draw_count(ui, 0.59, 3, "Miss", counts[3]);
            }

            let sm1 = 0.3;
            let l1 = s2.x + s2.w * if xcsim { 0.82 } else { 0.72 };
            let rt = s2.x + s2.w * 0.94;
            let cy = s2.center().y;
            let r6 = draw_text_aligned(ui, "Early", l1, cy - dy2 / 2., (0., 1.), sm1, WHITE);
            draw_text_aligned(ui, &early.to_string(), rt, r6.bottom(), (1., 1.), sm1, WHITE);
            let r7 = draw_text_aligned(ui, "Late", l1, cy + dy2 / 2., (0., 0.), sm1, WHITE);
            draw_text_aligned(ui, &late.to_string(), rt, r7.y, (1., 0.), sm1, WHITE);
        });

        // ---- 左下重试 / 右下继续（斜切板 + 触摸检测，与 Phira-Vrenxz 一致） ----
        fn touched(rect: Rect) -> bool {
            Judge::get_touches().iter().any(|touch| touch.phase == TouchPhase::Ended && rect.contains(touch.position))
        }
        let dy3 = 0.006;
        let bw = 0.17;
        let bh = 0.1;
        let p = (1. - ran(t, 2., 2.7)).powi(2);
        let s3 = 0.05;
        let hs = bh * 0.3;
        let params = DrawTextureParams {
            dest_size: Some(vec2(hs * 2., hs * 2.)),
            ..Default::default()
        };
        let r_retry = Rect::new(-1. - bh * slope, -top + dy3, bw, bh);
        ui.with_gl(Mat4::from_translation(vec3(-p * 0.17, 0., 0.)), |ui| {
            draw_parallelogram(r_retry, None, cb, true);
            draw_parallelogram(Rect::new(r_retry.x + r_retry.w * (1. - s3), r_retry.y, r_retry.w * s3, r_retry.h), None, WHITE, false);
            let ct = r_retry.center();
            draw_texture_ex(*self.icon_retry, ct.x - hs, ct.y - hs, WHITE, params.clone());
        });
        let r_proceed = Rect::new(1. + bh * slope - bw, top - dy3 - bh, bw, bh);
        ui.with_gl(Mat4::from_translation(vec3(p * 0.17, 0., 0.)), |ui| {
            draw_parallelogram(r_proceed, None, cb, true);
            draw_parallelogram(Rect::new(r_proceed.x, r_proceed.y, r_proceed.w * s3, r_proceed.h), None, WHITE, false);
            let ct = r_proceed.center();
            draw_texture_ex(*self.icon_proceed, ct.x - hs, ct.y - hs, WHITE, params);
        });
        // 进场动画结束后才能点，避免动画过程中误触
        if p <= 0. {
            if touched(r_retry) {
                button_hit();
                if self.upload_task.is_some() {
                    show_message(tl!("still-uploading"));
                } else {
                    self.tr_start = t;
                    self.next = 1;
                }
            } else if touched(r_proceed) {
                button_hit();
                if self.upload_task.is_some() {
                    show_message(tl!("still-uploading"));
                } else {
                    self.tr_start = t;
                    self.next = 2;
                }
            }
        }

        // ---- 玩家名牌（右上）+ RKS/新 RKS ----
        let alpha = ran(t, 1.5, 1.9);
        let main1 = Rect::new(1. - 0.28, -top + dy3 * 2.5, 0.35, 0.1);
        draw_parallelogram(main1, None, Color::new(0., 0., 0., 0.6 * alpha), false);
        let sub = Rect::new(1. - 0.13, main1.center().y + 0.01, 0.12, 0.03);
        let color = Color::new(1., 1., 1., alpha);
        draw_parallelogram(sub, None, color, false);
        let rks_text = if let Some(new_rks) = self.update_state.as_ref().and_then(|it| it.new_rks) {
            format!("{new_rks:.2}")
        } else if let Some(rks) = &self.player_rks {
            format!("{rks:.2}")
        } else {
            String::new()
        };
        draw_text_aligned(ui, &rks_text, sub.center().x, sub.center().y, (0.5, 0.5), 0.37, Color::new(0., 0., 0., alpha));
        let r10 = draw_illustration(*self.player, 1. - 0.21, main1.center().y, 0.12 / (0.076 * 7.), 0.12 / (0.076 * 7.), color);
        let mut text2 = ui.text(&self.player_name).pos(r10.x - 0.015, r10.center().y - 0.002).anchor(1., 0.5).size(0.54).color(color);
        let text_rect = text2.measure();
        draw_parallelogram(
            Rect::new(text_rect.x - main1.h * slope - 0.02, main1.y, r10.x - text_rect.x + main1.h * slope * 2. + 0.021, main1.h),
            None,
            Color::new(0., 0., 0., 0.6 * alpha),
            false,
        );
        text2.draw();

        // ---- Firefly 特性：Mod 图标行（左上） ----
        let active_mod_indices: Vec<usize> = [
            (Mods::FLIP_X, 0),
            (Mods::FADE_OUT, 1),
            (Mods::FADE_IN, 2),
            (Mods::NIGHTCORE, 3),
            (Mods::RAINBOW, 4),
            (Mods::AUTOPLAY, 5),
            (Mods::NO_SHADER, 6),
        ]
        .into_iter()
        .filter(|(m, _)| self.mods.contains(*m))
        .map(|(_, idx)| idx)
        .collect();
        if !active_mod_indices.is_empty() {
            let mh = 0.055;
            let mw = mh * 1.25;
            let my = -top + 0.03;
            let mut mx = -0.96;
            for &idx in &active_mod_indices {
                draw_parallelogram(Rect::new(mx, my, mw, mh), None, Color::new(0., 0., 0., 0.6 * alpha), false);
                let isz = mh * 0.78;
                let ir = Rect::new(mx + (mw - isz) / 2. + mh * slope * 0.5, my + (mh - isz) / 2., isz, isz);
                ui.fill_rect(ir, (*self.mod_icons[idx], ir, ScaleType::Fit, semi_white(0.9 * alpha)));
                mx += mw + 0.012;
            }
        }

        // ---- Firefly 特性：退场过渡 ----
        if !self.tr_start.is_nan() {
            let tp = ((t - self.tr_start) / 0.5).min(1.);
            if tp >= 1. {
                self.tr_start = f32::NAN;
            }
            let tp = 1. - (1. - tp).powi(3);
            let mut tr = sr;
            tr.y -= tr.h * (1. - tp);
            rect_shadow(tr, 0.01, 0.5);
            let (tex, ta) = if self.next == 1 { (*self.background, 0.3) } else { (*self.illustration, 0.55) };
            ui.fill_rect(tr, (tex, tr));
            ui.fill_rect(tr, semi_black(ta));
        }

        Ok(())
    }
}

impl Scene for EndingScene {
    fn enter(&mut self, tm: &mut TimeManager, target: Option<RenderTarget>) -> Result<()> {
        tm.reset();
        tm.seek_to(-0.4);
        self.target = target;
        Ok(())
    }

    fn pause(&mut self, tm: &mut TimeManager) -> Result<()> {
        self.bgm.pause()?;
        tm.pause();
        Ok(())
    }

    fn resume(&mut self, tm: &mut TimeManager) -> Result<()> {
        self.bgm.play()?;
        tm.resume();
        Ok(())
    }

    fn touch(&mut self, tm: &mut TimeManager, touch: &Touch) -> Result<bool> {
        let t = tm.now() as f32;
        if self.btn_retry.touch(touch, t) {
            if self.upload_task.is_some() {
                show_message(tl!("still-uploading"));
            } else {
                self.tr_start = t;
                self.next = 1;
            }
            return Ok(true);
        }
        if self.btn_proceed.touch(touch, t) {
            if self.upload_task.is_some() {
                show_message(tl!("still-uploading"));
            } else {
                self.tr_start = t;
                self.next = 2;
            }
            return Ok(true);
        }
        if self.btn_detail.touch(touch) {
            button_hit();
            self.detail_mode = !self.detail_mode;
            return Ok(true);
        }
        Ok(false)
    }

    fn update(&mut self, tm: &mut TimeManager) -> Result<()> {
        self.audio.recover_if_needed()?;
        if tm.now() >= 0. && self.target.is_none() && self.bgm.paused() {
            self.bgm.play()?;
        }
        if RE_UPLOAD.with(|it| std::mem::replace(it.borrow_mut().deref_mut(), false)) && self.upload_task.is_none() {
            self.upload_task = self
                .record_data
                .clone()
                .map(|data| ((self.upload_fn.as_ref().unwrap())(data), show_message(tl!("uploading")).handle()));
        }
        if let Some((task, handle)) = &mut self.upload_task {
            if let Some(result) = task.take() {
                handle.cancel();
                match result {
                    Err(err) => {
                        let error = format!("{:?}", err.context(tl!("upload-failed")));
                        Dialog::plain(tl!("upload-failed"), error)
                            .buttons(vec![tl!("upload-cancel").to_string(), tl!("upload-retry").to_string()])
                            .listener(move |_dialog, pos| {
                                if pos == 1 {
                                    RE_UPLOAD.with(|it| *it.borrow_mut() = true);
                                }
                                false
                            })
                            .show();
                    }
                    Ok(state) => {
                        self.update_state = Some(state);
                        show_message(tl!("uploaded")).ok();
                    }
                }
                self.upload_task = None;
            }
        }
        Ok(())
    }

    fn render(&mut self, tm: &mut TimeManager, ui: &mut Ui) -> Result<()> {
        if PREFER_ALT_UI.load(std::sync::atomic::Ordering::Relaxed) {
            return self.render_xchs(tm, ui);
        }
        let mut cam = ui.camera();
        let asp = -cam.zoom.y;
        let top = 1. / asp;
        let t = tm.now() as f32;
        cam.render_target = self.target;
        let sr = ui.screen_rect();
        set_camera(&cam);
        draw_background(*self.background, ui.viewport);

        fn ran(t: f32, l: f32, r: f32) -> f32 {
            ((t - l) / (r - l)).clamp(0., 1.)
        }

        let ct = vec2(-0.55, 1.2);
        let start = vec2(1.25, 0.9) - ct;
        let end = vec2(-0.15, -0.7) - ct;
        let angle_start = start.y.atan2(start.x) * 0.4;
        let angle_end = end.y.atan2(end.x);
        let center_angle = 1.8;

        let p = ran(t, 0.1, 1.8);
        let p = 1. - (1. - p).powi(3);
        let sector_start = p * (angle_end - angle_start - center_angle) + angle_start;
        let project_y = ct.y + (1. - ct.x) * (sector_start + center_angle).sin();

        let pf = ran(t, 2., 2.4);

        if project_y < top {
            let c = ui.background();
            let y = -top + 0.12;
            let br = Rect::new(-1., y, 2., 0.34);
            ui.fill_rect(br, (c, (-1., y), Color { a: 0.1, ..c }, (1., y + 0.3)));

            let r = ui
                .text(tl!("detail"))
                .pos(1. - 0.02, br.bottom() + 0.02)
                .anchor(1., 0.)
                .size(0.5)
                .color(if self.detail_mode { semi_white(0.4) } else { WHITE })
                .draw_using(&BOLD_FONT);
            self.btn_detail.set(ui, r.feather(0.02));

            let res = &self.result;

            let y = y - 0.07;
            ui.fill_rect(Rect::new(-1., y, 2., 0.07), Color { a: 0.3, ..c });
            let r = ui
                .text(&self.info.name)
                .pos(-0.53 + (1.2 - y) / 1.9 * 0.4, y + 0.012)
                .color(semi_white(0.6))
                .max_width(0.8)
                .size(0.56)
                .draw();
            ui.text(&self.info.level)
                .pos(0.97, r.y)
                .anchor(1., 0.)
                .size(0.56)
                .color(semi_white(0.7))
                .draw();

            let icon = &self.icons[if res.xcsim {
                icon_index_xcsim(res.score)
            } else {
                icon_index(res.score, res.max_combo == res.num_of_notes)
            }];
            let p = ran(t, 1.7, 2.4).powi(2);
            let r = Rect::new(0.75, br.center().y, 0., 0.).feather(0.13 + (1. - p) * 0.05);
            let r = contain_rect(icon, r);
            ui.fill_rect(r, (**icon, r, ScaleType::Fit, semi_white(p)));

            let y = y + 0.16;
            let lf = -0.48 + (1.2 - y) / 1.9 * 0.4;
            let mut x = lf;
            let p = ran(t, 0.9, 2.6);
            // XC-SIM 分数为 8 位，官方/本地为 7 位。
            let ndigits = if res.xcsim { 8 } else { 7 };
            let mut digits = Vec::with_capacity(ndigits);
            let mut s = res.score;
            for _ in 0..ndigits {
                digits.push(s % 10);
                s /= 10;
            }
            digits.reverse();
            let s = 1.5;
            let sr = ui.text("0").size(s).measure_using(&PGR_FONT);
            let h = sr.h;
            ui.scissor(Rect::new(-1., y, 2., h + 0.01), |ui| {
                for (i, d) in digits.into_iter().enumerate() {
                    let p = (p * (1. + (0.16 * (ndigits - 1 - i) as f32).powi(2))).min(1.);
                    let p = 1. - (1. - p).powi(3);
                    let mut p = d as f32 + (1. - p) * 7.;
                    if p > 10. {
                        p -= 10.;
                    }
                    let up = p as u32;
                    let dw = (up + 1) % 10;
                    let o = -h * (p - up as f32);
                    ui.text(up.to_string())
                        .pos(x + sr.w / 2., y + o)
                        .anchor(0.5, 0.)
                        .size(s)
                        .draw_using(&PGR_FONT);
                    ui.text(dw.to_string())
                        .pos(x + sr.w / 2., y + h + o)
                        .anchor(0.5, 0.)
                        .size(s)
                        .draw_using(&PGR_FONT);
                    x += sr.w;
                }
            });

            if let Some(s) = &self.update_state {
                if s.best {
                    ui.text(format!("{}  {:+07}", tl!("new-best"), s.improvement))
                        .pos(x - 0.01, y - 0.016)
                        .anchor(1., 1.)
                        .color(semi_white(pf))
                        .size(0.5)
                        .draw_using(&BOLD_FONT);
                }
            }

            let cl = semi_white(0.6);
            let ct = semi_white(0.8);
            let cs = semi_white(0.4);
            let s = 0.5;

            let r = ui
                .text(tl!("accuracy"))
                .pos(lf - 0.017, y + h + 0.03)
                .color(cl)
                .size(s)
                .draw_using(&BOLD_FONT);
            let r = ui
                .text(format!("{:.2}%", res.accuracy * 100.))
                .pos(r.right() + 0.02, r.y)
                .color(ct)
                .size(s)
                .draw_using(&BOLD_FONT);

            let r = ui.text("|").pos(r.right() + 0.03, r.y).color(cs).size(s).draw();

            let r = ui.text(tl!("error")).pos(r.right() + 0.03, r.y).color(cl).size(s).draw_using(&BOLD_FONT);
            let r = ui
                .text(format!("±{}ms", (res.std * 1000.).round() as i32))
                .pos(r.right() + 0.02, r.y)
                .size(s)
                .color(ct)
                .draw_using(&BOLD_FONT);

            if let Some(avg_fps) = self.avg_fps {
                let r = ui.text("|").pos(r.right() + 0.03, r.y).color(cs).size(s).draw();
                let r = ui.text("AVG FPS").pos(r.right() + 0.03, r.y).color(cl).size(s).draw_using(&BOLD_FONT);
                ui.text(format!("{:.1}", avg_fps))
                    .pos(r.right() + 0.02, r.y)
                    .size(s)
                    .color(ct)
                    .draw_using(&BOLD_FONT);
            }

            let mut y = -top + 0.4 + ui.top * 0.3;
            let tp = y;
            let mut x = -0.26 + (1.2 - y) / 1.9 * 0.4;
            let lf = x;
            let s = 0.64;
            // XC-SIM：从上到下为 Perfect+ / Perfect / Good / Bad / Miss；官方/本地为 Perfect / Good / Bad / Miss。
            let titles: &[&str] = if res.xcsim {
                &["PERFECT+", "PERFECT", "GOOD", "BAD", "MISS"]
            } else {
                &["PERFECT", "GOOD", "BAD", "MISS"]
            };
            for (id, title) in titles.iter().copied().enumerate() {
                ui.text(title)
                    .pos(x, y)
                    .anchor(1., 0.)
                    .color(semi_white(0.6))
                    .size(s)
                    .draw_using(&BOLD_FONT);
                let count = if res.xcsim {
                    match id {
                        0 => res.shiny_perfect,
                        1 => res.counts[0] - res.shiny_perfect,
                        2 => res.counts[1],
                        3 => res.counts[2],
                        _ => res.counts[3],
                    }
                } else {
                    res.counts[id]
                };
                let xchs = PREFER_ALT_UI.load(std::sync::atomic::Ordering::Relaxed);
                let r = if !res.xcsim && self.detail_mode && id != 3 {
                    let r = ui
                        .text(format!("-{}", res.early_kind[id]))
                        .pos(x + 0.03, y)
                        .size(s)
                        .color(if xchs { Color::new(0.984, 0.973, 0.886, 1.) } else { Color::from_hex_rgb(0x81d4fa) })
                        .draw_using(&BOLD_FONT);
                    ui.text(format!("+{}", res.late_kind[id]))
                        .pos(r.right() + 0.01, y)
                        .size(s)
                        .color(if xchs { Color::new(1.0, 0.58, 0.706, 1.) } else { Color::from_hex_rgb(0xffab91) })
                        .draw_using(&BOLD_FONT)
                } else {
                    ui.text(count.to_string()).pos(x + 0.06, y).size(s).draw_using(&BOLD_FONT)
                };
                let dy = r.h + 0.03;
                y += dy;
                x -= dy / 1.9 * 0.4;
            }

            let p = ran(t, 0.8, 1.8);
            let p = 1. - (1. - p).powi(3);
            let mut y = tp;
            let mut x = lf + 0.42;
            let r = ui
                .text(tl!("max-combo"))
                .pos(x, y)
                .anchor(1., 0.)
                .color(semi_white(0.6))
                .size(s)
                .draw_using(&BOLD_FONT);
            let mut r = Rect::new(r.right() + 0.03, r.y + 0.004, 0.45, r.h);
            let draw_par = |ui: &mut Ui, r: Rect, p: f32, c: Color| {
                let sl = 1.9 / 0.4;
                let w = p * r.w;
                let d = r.h / sl;
                let mut b = ui.builder(c);
                b.add(r.x, r.bottom());
                if w < d {
                    b.add(r.x + w, r.bottom());
                    b.add(r.x + w, r.bottom() - w * sl);
                    b.triangle(0, 1, 2);
                } else {
                    b.add(r.x + d, r.y);
                    b.add(r.x + w, r.y);
                    b.add(r.x + w.min(r.w - d), r.bottom());
                    b.triangle(0, 1, 2);
                    b.triangle(0, 2, 3);
                    if w + d > r.right() {
                        b.add(r.x + w, r.y + (r.w - w) * sl);
                        b.triangle(2, 3, 4);
                    }
                }
                b.commit();
            };
            draw_par(ui, r, 1., semi_black(0.4));
            let ct = r.center();
            let combo = (res.max_combo as f32 * p).round() as u32;
            let text = format!("{combo} / {}", res.num_of_notes);
            ui.text(&text)
                .pos(ct.x, ct.y)
                .anchor(0.5, 0.5)
                .no_baseline()
                .size(0.4)
                .draw_using(&BOLD_FONT);
            let p = combo as f32 / res.num_of_notes as f32;
            draw_par(ui, r, p, WHITE);
            r.w *= p;
            ui.scissor(r, |ui| {
                ui.text(text)
                    .pos(ct.x, ct.y)
                    .anchor(0.5, 0.5)
                    .no_baseline()
                    .size(0.4)
                    .color(BLACK)
                    .draw_using(&BOLD_FONT);
            });

            let dy = r.h + 0.03;
            y += dy;
            x -= dy / 1.9 * 0.4;

            let r = ui
                .text(tl!("rks-delta"))
                .pos(x, y)
                .anchor(1., 0.)
                .color(semi_white(0.6))
                .size(s)
                .draw_using(&BOLD_FONT);
            let text = if let Some((new_rks, now)) = self.update_state.as_ref().and_then(|it| it.new_rks).zip(self.player_rks) {
                let delta = new_rks - now;
                if delta.abs() > 1e-5 {
                    format!("{:+.2}", delta)
                } else {
                    "-".to_owned()
                }
            } else {
                "-".to_owned()
            };
            ui.text(text).pos(r.right() + 0.03, y).size(s).draw_using(&BOLD_FONT);

            let mut r = Rect::new(0.96, ui.top - 0.04, 0.25, 0.1);
            r.x -= r.w;
            r.y -= r.h;
            self.btn_proceed.render_shadow(ui, r, t, |ui, path| {
                ui.fill_path(&path, if PREFER_ALT_UI.load(std::sync::atomic::Ordering::Relaxed) { Color::new(0.949, 0.412, 0.580, 1.) } else { Color::from_hex_rgb(0x3f51b5) });
                let ir = Rect::new(r.x + 0.05, r.center().y, 0., 0.).feather(0.03);
                ui.fill_rect(ir, (*self.icon_proceed, ir));
                ui.text(tl!("proceed"))
                    .pos((ir.right() + r.right() - 0.01) / 2., r.center().y)
                    .anchor(0.5, 0.5)
                    .no_baseline()
                    .size(0.44)
                    .draw_using(&BOLD_FONT);
            });

            r.x -= r.w + 0.02;
            self.btn_retry.render_shadow(ui, r, t, |ui, path| {
                ui.fill_path(&path, if PREFER_ALT_UI.load(std::sync::atomic::Ordering::Relaxed) { Color::new(0.243, 0.165, 0.255, 1.) } else { Color::from_hex_rgb(0x78909c) });
                let ir = Rect::new(r.x + 0.05, r.center().y, 0., 0.).feather(0.03);
                ui.fill_rect(ir, (*self.icon_retry, ir));
                ui.text(tl!("retry"))
                    .pos((ir.right() + r.right() - 0.01) / 2., r.center().y)
                    .anchor(0.5, 0.5)
                    .no_baseline()
                    .size(0.44)
                    .draw_using(&BOLD_FONT);
            });

            let spd = if (self.speed - 1.).abs() <= 1e-4 {
                String::new()
            } else {
                format!("{:.2}x", self.speed)
            };
            let status_text = if !self.rated && !self.autoplay && !self.use_keyboard {
                if spd.is_empty() {
                    "UNRATED".to_string()
                } else {
                    format!("UNRATED {spd}")
                }
            } else {
                spd
            };
            let status_text = status_text.trim();
            // mod_icons order: FLIP_X, FADE_OUT, FADE_IN, NIGHTCORE, RAINBOW
            let active_mod_indices: Vec<usize> = [
                (Mods::FLIP_X, 0),
                (Mods::FADE_OUT, 1),
                (Mods::FADE_IN, 2),
                (Mods::NIGHTCORE, 3),
                (Mods::RAINBOW, 4),
                (Mods::AUTOPLAY, 5),
                (Mods::NO_SHADER, 6),
            ]
            .into_iter()
            .filter(|(m, _)| self.mods.contains(*m))
            .map(|(_, idx)| idx)
            .collect();
            let ty = br.bottom();
            let base_x = -0.55 + (1.2 - ty) / 1.9 * 0.4;
            let skew_factor = 0.4 / 1.9;
            let has_text = !status_text.is_empty();
            let has_icons = !active_mod_indices.is_empty();
            if has_text || has_icons {
                let text_size = 0.5;
                let skew_height_ratio = skew_factor;
                let mut current_x = base_x;
                let para_h = 0.04;
                if has_text {
                    let mut text = ui
                        .text(status_text)
                        .pos(current_x + 0.02, ty)
                        .anchor(0., 0.5)
                        .no_baseline()
                        .color(semi_black(0.6))
                        .size(text_size);
                    let tr = text.measure_using(&BOLD_FONT);
                    let r = Rect::new(-1., tr.y, tr.right() + 1.03, tr.h);
                    let mut b = text.ui.builder(WHITE);
                    b.add(-1., tr.y);
                    b.add(r.right(), tr.y);
                    b.add(r.right() - tr.h * skew_height_ratio, tr.bottom());
                    b.add(-1., tr.bottom());
                    b.triangle(0, 1, 2);
                    b.triangle(0, 2, 3);
                    b.commit();

                    text.draw_using(&BOLD_FONT);
                    current_x = tr.right() + 0.04;
                }
                for &mod_idx in &active_mod_indices {
                    let icon_size = para_h * 0.9;
                    let para_w = para_h + 0.02;
                    let skew_offset = para_h * skew_height_ratio;
                    let para_left = current_x;
                    let para_right = current_x + para_w;
                    let para_top = ty - para_h / 2.;
                    let para_bottom = ty + para_h / 2.;
                    let mut b = ui.builder(WHITE);
                    b.add(para_left + skew_offset, para_top);
                    b.add(para_right + skew_offset, para_top);
                    b.add(para_right, para_bottom);
                    b.add(para_left, para_bottom);
                    b.triangle(0, 1, 2);
                    b.triangle(0, 2, 3);
                    b.commit();
                    let icon_x = current_x + (para_w - icon_size) / 2. + skew_offset / 2.;
                    let icon_y = ty - icon_size / 2.;
                    let icon_rect = Rect::new(icon_x, icon_y, icon_size, icon_size);
                    ui.fill_rect(icon_rect, (*self.mod_icons[mod_idx], icon_rect, ScaleType::Fit, semi_black(0.6)));
                    current_x = para_right + 0.02;
                }
            }
        }
        clip_sector(ui, ct, sector_start, sector_start + center_angle, |ui| {
            ui.fill_rect(sr, (*self.illustration, sr));
        });
        let sector_start = (p * 1.4 - 0.3).max(0.) * (angle_end - angle_start - center_angle) + angle_start;
        clip_sector(ui, ct, sector_start, sector_start + center_angle * 0.5, |ui| {
            ui.fill_rect(sr, (*self.illustration, sr.feather(0.15)));
        });

        ui.alpha(pf, |ui| {
            let s = 0.05;
            let pad = 0.02;
            let mw = 0.4;
            let w = s * 2. + pad + ui.text(&self.player_name).size(0.6).measure().w.min(mw) + 0.02;
            let r = Rect::new(-0.96, -top + 0.04, w, s * 2.);
            ui.fill_path(&r.feather(0.01).rounded(s + 0.01), semi_black(0.6));
            ui.avatar(r.x + s, r.y + s, s, t, Ok(Some(self.player.clone())));
            let lf = r.x + s * 2. + pad;
            ui.text(&self.player_name).pos(lf, r.y + s).anchor(0., 1.).max_width(mw).size(0.6).draw();
            ui.text(if let Some(new_rks) = self.update_state.as_ref().and_then(|it| it.new_rks) {
                format!("{new_rks:.2}")
            } else if let Some(rks) = &self.player_rks {
                format!("{rks:.2}")
            } else {
                String::new()
            })
            .pos(lf, r.y + s + 0.008)
            .size(0.4)
            .color(semi_white(0.7))
            .draw();
        });

        if !self.tr_start.is_nan() {
            let p = ((t - self.tr_start) / 0.5).min(1.);
            if p >= 1. {
                self.tr_start = f32::NAN;
            }
            let p = 1. - (1. - p).powi(3);
            let mut r = sr;
            r.y -= r.h * (1. - p);
            rect_shadow(r, 0.01, 0.5);
            let (tex, alpha) = if self.next == 1 {
                (&self.background, 0.3)
            } else {
                (&self.illustration, 0.55)
            };
            ui.fill_rect(r, (**tex, r));
            ui.fill_rect(r, semi_black(alpha));
        }

        Ok(())
    }

    fn next_scene(&mut self, _tm: &mut TimeManager) -> NextScene {
        if !self.tr_start.is_nan() {
            return NextScene::None;
        }
        if self.next != 0 {
            let _ = self.bgm.pause();
        }
        match self.next {
            0 => NextScene::None,
            1 => NextScene::Pop,
            2 => {
                if let Some(rec) = &self.best_record {
                    NextScene::PopNWithResult(2, Box::new(rec.clone()))
                } else {
                    NextScene::PopN(2)
                }
            }
            _ => unreachable!(),
        }
    }
}

