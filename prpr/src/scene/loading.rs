use super::{draw_background, ending::RecordUpdateState, game::GameMode, GameScene, NextScene, Scene};
use crate::{
    config::{Config, DynamicBackgroundMode},
    core::{DynamicBackground, Resource, BOLD_FONT},
    ext::{draw_parallelogram, draw_text_aligned, get_viewport, poll_future, semi_black, semi_white, LocalTask, RectExt, SafeTexture},
    fs::FileSystem,
    info::ChartInfo,
    judge::Judge,
    scene::game::SimpleRecord,
    task::Task,
    time::TimeManager,
    ui::{clip_rounded_rect, rounded_rect_shadow, LoadingParams, ShadowConfig, Ui, PREFER_REDUCED_MOTION, PREFER_XCHS_UI},
};
use ::rand::{seq::SliceRandom, thread_rng};
use anyhow::{Context, Result};
use macroquad::prelude::*;
use regex::Regex;
use std::sync::{atomic::Ordering, Arc};
use tracing::warn;

const BEFORE_TIME: f32 = 1.;
const FADE_IN_TIME: f32 = 0.6;

/// 与 xcsim 的 `ext::draw_illustration` 等价：按 0.076 网格把 (w, h) 换算成屏幕尺寸，
/// 以 (x, y) 为中心用平行四边形阴影绘制，并返回绘制矩形。
fn draw_illustration(tex: Texture2D, x: f32, y: f32, w: f32, h: f32, color: Color) -> Rect {
    let scale = 0.076;
    let w = scale * 13. * w;
    let h = scale * 7. * h;
    let r = Rect::new(x - w / 2., y - h / 2., w, h);
    let tex_ratio = tex.width() / tex.height();
    let rect_ratio = w / h;
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

/// 与 xcsim 的 `ext::draw_text_aligned_opt_width` 等价：对齐绘制，超出 `max_width` 时裁切（不缩小字号）。
fn draw_text_aligned_clip(ui: &mut Ui, text: &str, x: f32, y: f32, anchor: (f32, f32), size: f32, color: Color, max_width: f32) -> Rect {
    ui.text(text).pos(x, y).anchor(anchor.0, anchor.1).size(size).color(color).max_width(max_width).draw()
}

pub type UploadFn = Arc<dyn Fn(Vec<u8>) -> Task<Result<RecordUpdateState>>>;
pub type UpdateFn = Box<dyn FnMut(f64, &mut Resource, &mut Judge)>;
pub type SaveFn = Box<dyn Fn(SimpleRecord) -> Result<()>>;

fn transition_time() -> Option<f32> {
    if PREFER_REDUCED_MOTION.load(Ordering::Relaxed) {
        None
    } else {
        Some(1.4)
    }
}

fn wait_time() -> f32 {
    if PREFER_REDUCED_MOTION.load(Ordering::Relaxed) {
        0.
    } else {
        0.4
    }
}

pub struct BasicPlayer {
    pub avatar: Option<SafeTexture>,
    pub id: i32,
    pub rks: f32,
    pub historic_best: u32,
}

pub struct LoadingScene {
    info: ChartInfo,
    background: SafeTexture,
    illustration: SafeTexture,
    pub load_task: LocalTask<Result<GameScene>>,
    next_scene: Option<NextScene>,
    finish_time: f32,
    target: Option<RenderTarget>,
    charter: String,

    theme_color: Color,
    use_black: bool,
}

impl LoadingScene {
    pub async fn load(fs: &mut dyn FileSystem, path: &str) -> Result<(SafeTexture, SafeTexture, Color)> {
        let image = image::load_from_memory(&fs.load_file(path).await?).context("Failed to decode image")?;
        let (w, h) = (image.width(), image.height());
        let size = w as usize * h as usize;

        let mut blurred_rgb = image.to_rgb8();
        let color = color_thief::get_palette(&blurred_rgb, color_thief::ColorFormat::Rgb, 10, 2)?[0];
        let mut vec = unsafe { Vec::from_raw_parts(std::mem::transmute::<*mut u8, *mut [u8; 3]>(blurred_rgb.as_mut_ptr()), size, size) };
        fastblur::gaussian_blur(&mut vec, w as _, h as _, 50.);
        std::mem::forget(vec);
        let mut blurred = Vec::with_capacity(size * 4);
        for input in blurred_rgb.chunks_exact(3) {
            blurred.extend_from_slice(input);
            blurred.push(255);
        }
        Ok((
            Texture2D::from_rgba8(w as _, h as _, &image.into_rgba8()).into(),
            Texture2D::from_image(&Image {
                width: w as _,
                height: h as _,
                bytes: blurred,
            })
            .into(),
            Color::from_rgba(color.r, color.g, color.b, 255),
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        mode: GameMode,
        mut info: ChartInfo,
        config: Config,
        mut fs: Box<dyn FileSystem>,
        player: Option<BasicPlayer>,
        upload_fn: Option<UploadFn>,
        update_fn: Option<UpdateFn>,
        save_fn: Option<SaveFn>,
        xcsim: bool,
        record_save_fn: Option<crate::replay::RecordSaveFn>,
        replay_handoff: Option<crate::replay::ReplayHandoff>,

        preloaded: Option<(SafeTexture, SafeTexture, Color)>,
    ) -> Result<Self> {
        // 加载界面始终使用静态模糊曲绘；动态背景只在游玩界面生效
        let (illustration, background, theme_color) = if let Some((ill, bg, color)) = preloaded {
            (ill, bg, color)
        } else {
            Self::load(fs.as_mut(), &info.illustration).await?
        };

        // 设置项强制覆盖谱面的动态背景模式
        if config.dynamic_background != DynamicBackgroundMode::Off {
            info.dynamic_background = config.dynamic_background.as_u8();
        }

        let dynamic_bg = if info.dynamic_background > 0 {
            match Self::load_dynamic(fs.as_mut(), &info).await {
                Ok(dbg) => Some(dbg),
                Err(err) => {
                    warn!("failed to load dynamic background: {err:?}");
                    None
                }
            }
        } else {
            None
        };
        let use_black = (theme_color.r * 0.299 + theme_color.g * 0.587 + theme_color.b * 0.114) > 186. / 255.;
        if info.tip.is_none() {
            info.tip = Some(crate::config::TIPS.choose(&mut thread_rng()).unwrap().to_owned());
        }
        let future = Box::pin(GameScene::new(
            mode,
            info.clone(),
            config,
            fs,
            player,
            background.clone(),
            illustration.clone(),
            dynamic_bg,
            upload_fn,
            update_fn,
            save_fn,
            xcsim,
            record_save_fn,
            replay_handoff,
        ));
        let charter = Regex::new(r"\[!:[0-9]+:([^:]*)\]").unwrap().replace_all(&info.charter, "$1").to_string();

        Ok(Self {
            info,
            background,
            illustration,
            load_task: Some(future),
            next_scene: None,
            finish_time: f32::INFINITY,
            target: None,
            charter,

            theme_color,
            use_black,
        })
    }

    /// 仅创建动态背景对象；加载界面仍使用静态模糊曲绘。
    async fn load_dynamic(fs: &mut dyn FileSystem, info: &ChartInfo) -> Result<DynamicBackground> {
        let image = image::load_from_memory(&fs.load_file(&info.illustration).await?).context("Failed to decode image")?;
        let vp = get_viewport();
        DynamicBackground::new(info.dynamic_background, &image, info.background_dim, vp)
    }
}

impl Scene for LoadingScene {
    fn enter(&mut self, tm: &mut TimeManager, target: Option<RenderTarget>) -> Result<()> {
        self.target = target;
        tm.reset();
        Ok(())
    }

    fn update(&mut self, tm: &mut TimeManager) -> Result<()> {
        if let Some(future) = self.load_task.as_mut() {
            loop {
                match poll_future(future.as_mut()) {
                    None => {
                        if self.target.is_none() {
                            break;
                        }
                        std::thread::yield_now();
                    }
                    Some(game_scene) => {
                        self.load_task = None;
                        self.next_scene =
                            Some(game_scene.map_or_else(|e| NextScene::PopWithResult(Box::new(e)), |it| NextScene::Replace(Box::new(it))));
                        self.finish_time = tm.now() as f32 + BEFORE_TIME;
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    fn render(&mut self, tm: &mut TimeManager, ui: &mut Ui) -> Result<()> {
        let mut cam = ui.camera();
        let asp = -cam.zoom.y;
        let top = 1. / asp;
        let t = tm.now() as f32;
        cam.render_target = self.target;
        set_camera(&cam);
        draw_background(*self.background, ui.viewport);

        ui.alpha((t / FADE_IN_TIME).min(1.), |ui| {
            if PREFER_XCHS_UI.load(Ordering::Relaxed) {
                // ---- XCHS 加载界面（照抄 xcsim-core/src/scene_core/loading_scene.rs） ----
                let dx = if t > self.finish_time {
                    transition_time().map_or(1., |tt| {
                        let p = ((t - self.finish_time) / tt).min(1.);
                        p.powi(2) * 3. + p.powi(5) * 11.
                    })
                } else {
                    0.
                };
                // 平行四边形/插图是裸 quad_gl 绘制，不吃 ui.dx，必须用模型矩阵推动整块
                let mat = Mat4::from_translation(vec3(dx, 0., 0.));
                ui.with_gl(mat, |ui| {
                    let vo = -top / 10.;
                let voi = -top / 8.5;
                let r = draw_illustration(*self.illustration, 0.380, voi, 1.03, 1.0, WHITE);
                let h1 = r.h / 3.55;
                let main = Rect::new(-0.87, vo - h1 / 2. - top / 10., 0.768, h1);
                draw_parallelogram(main, None, Color::new(0., 0., 0., 0.6), false);
                let p1 = (main.x + main.w * 0.085, main.y + main.h * 0.35 + 0.025);
                let p2 = (main.x + main.w * 0.09, main.y + main.h * 0.74 - 0.0125);
                draw_text_aligned_clip(ui, &self.info.name, p1.0, p1.1, (0., 1.), 0.73, WHITE, main.w * 0.65);
                draw_text_aligned_clip(ui, &self.info.composer, p2.0, p2.1, (0., 0.), 0.363, WHITE, main.w * 0.60);

                // 难度板（白色平行四边形）
                let ext = 0.04;
                let sub = Rect::new(main.x + main.w * 0.724, main.y - main.h * ext, main.w * 0.25, main.h * (1. + ext * 2.));
                let mut ct = sub.center();
                ct.x += sub.w * 0.01;
                ct.y += sub.h * 0.05;
                draw_parallelogram(sub, None, WHITE, true);
                // 等级文本：与 xcsim 一致 —— 大字取「最后一个空白词」里的数字（可带 . 和 ?），
                // 小字取「第一个空白词」即难度类型。"IN Lv.14" → 大字 14 / 小字 IN
                let lv_prefix = self.info.level.split_whitespace().next().unwrap_or("?");
                let lv_num: String = self
                    .info
                    .level
                    .split_whitespace()
                    .last()
                    .and_then(|w| {
                        let i = w.find(|c: char| c.is_ascii_digit() || c == '?')?;
                        Some(w[i..].chars().take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '?').collect::<String>())
                    })
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "?".to_owned());
                draw_text_aligned_clip(ui, &lv_num, ct.x, ct.y + sub.h * 0.05, (0.5, 1.), 0.90, BLACK, main.w * 0.18);
                // 首词自身不含数字时才画（"15.2" 这种没有难度前缀的写法就不会重复显示）
                if !lv_prefix.chars().any(|c| c.is_ascii_digit()) {
                    draw_text_aligned_clip(ui, lv_prefix, ct.x, ct.y + sub.h * 0.09, (0.5, 0.), 0.30, BLACK, main.w * 0.16);
                }

                // 谱师 / 画师
                let w1 = 0.031;
                let h2 = 0.030;
                let tc = draw_text_aligned(ui, "Chart", main.x + main.w / 6.1, main.y + main.h * 1.32, (0., 0.), 0.253, WHITE);
                let t1 = draw_text_aligned_clip(ui, &self.charter, tc.x, tc.y + top / 22., (0., 0.), 0.415, WHITE, 0.58);
                let t2 = draw_text_aligned(ui, "Illustration", t1.x - w1, t1.y + t1.h + h2, (0., 0.), 0.253, WHITE);
                draw_text_aligned_clip(ui, &self.info.illustrator, t2.x - 0.002, t2.y + top / 22., (0., 0.), 0.415, WHITE, 0.58);

                // 底部提示
                let tip = self.info.tip.as_ref().unwrap();
                draw_text_aligned_clip(ui, tip, -0.895, top * 0.88, (0., 1.), 0.47, WHITE, 1.55);

                // 右上 Loading... + 扫光条
                let t3 = draw_text_aligned(ui, "Loading...", 0.865, top * 0.865, (1., 1.), 0.41, WHITE);
                let r1 = Rect::new(t3.x - t3.w * 0.19, t3.y - t3.h * 0.35, t3.w * 1.418, t3.h * 1.7);
                let t4 = ((t - 0.3).max(0.) % 1.4) / 0.6;
                let st = (t4 - 1.).clamp(0., 1.).powi(3);
                let en = 1. - (1. - t4.min(1.)).powi(3);
                ui.fill_rect(Rect::new(r1.x + r1.w * st, r1.y, r1.w * (en - st), r1.h), WHITE);
                draw_text_aligned(ui, "Loading...", 0.865, top * 0.865, (1., 1.), 0.41, BLACK);
                });
                return;
            }

            let r = Rect::default().nonuniform_feather(0.65, top * 0.7);
            let config = ShadowConfig {
                radius: 0.008,
                ..Default::default()
            };
            let bar_height = 0.16;
            let ir = Rect { h: r.h - bar_height, ..r };

            let (main, sub) = Ui::main_sub_colors(self.use_black, 1.);

            rounded_rect_shadow(ui, r, &config);
            clip_rounded_rect(ui, r, config.radius, |ui| {
                ui.fill_rect(r, Color { a: 0.6, ..self.theme_color });
                ui.fill_rect(ir, (*self.illustration, ir));
                ui.fill_rect(ir, (semi_black(0.5), (ir.x, ir.bottom()), Color::default(), (ir.x, ir.y)));
            });

            let ct = ir.bottom() + bar_height / 2.;
            let lf = r.x + 0.04;
            let rt = r.x + r.w * 0.65;
            let mw = rt - lf - 0.02;
            ui.text(&self.info.name)
                .pos(lf, ct)
                .anchor(0., 1.)
                .size(0.7)
                .color(main)
                .max_width(mw)
                .draw();
            ui.text(&self.info.composer)
                .pos(lf, ct + 0.012)
                .anchor(0., 0.)
                .size(0.4)
                .color(sub)
                .max_width(mw)
                .draw();

            let lf = rt + 0.03;
            let dy = bar_height / 6.;
            let size = 0.45;
            ui.text("Chart")
                .pos(lf, ct - dy)
                .anchor(0., 0.5)
                .no_baseline()
                .size(size)
                .color(sub)
                .draw_using(&BOLD_FONT);
            ui.text("Cover")
                .pos(lf, ct + dy)
                .anchor(0., 0.5)
                .no_baseline()
                .size(size)
                .color(sub)
                .draw_using(&BOLD_FONT);

            let lf = lf + 0.12;
            let mw = r.right() - lf - 0.01;
            ui.text(&self.charter)
                .pos(lf, ct - dy)
                .anchor(0., 0.5)
                .no_baseline()
                .size(size)
                .color(main)
                .max_width(mw)
                .draw();
            ui.text(&self.info.illustrator)
                .pos(lf, ct + dy)
                .anchor(0., 0.5)
                .no_baseline()
                .size(size)
                .color(main)
                .max_width(mw)
                .draw();

            let r = 0.07;
            ui.loading(
                1. - r,
                top - r,
                t,
                if t > self.finish_time {
                    let p = ((t - self.finish_time) / 0.4).min(1.);
                    semi_white((1. - p).powi(3))
                } else {
                    WHITE
                },
                LoadingParams {
                    radius: 0.04,
                    width: 0.01,
                    ..Default::default()
                },
            );

            ui.text(self.info.tip.as_ref().unwrap())
                .pos(-0.95, top - 0.05)
                .anchor(0., 1.)
                .size(0.47)
                .color(WHITE)
                .draw();
        });

        Ok(())
    }

    fn next_scene(&mut self, tm: &mut TimeManager) -> NextScene {
        if matches!(self.next_scene, Some(NextScene::PopWithResult(_))) {
            return self.next_scene.take().unwrap();
        }
        if tm.now() as f32 > self.finish_time + transition_time().unwrap_or_default() + wait_time() {
            if let Some(scene) = self.next_scene.take() {
                return scene;
            }
        }
        NextScene::None
    }
}
