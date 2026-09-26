prpr_l10n::tl_file!("chapter");

use crate::{
    anim::Anim,
    data::BriefChartInfo,
    dir,
    icons::Icons,
    load_res_tex,
    page::{ChartItem, ChartType, Illustration, SFader},
    resource::rtl,
};
use anyhow::Result;
use macroquad::prelude::*;
use prpr::{
    config::Mods,
    core::BOLD_FONT,
    ext::{semi_black, semi_white, RectExt, SafeTexture},
    info::ChartInfo,
    scene::{NextScene, Scene},
    time::TimeManager,
    ui::{button_hit, DRectButton, RectButton, Scroll, Ui},
};
use serde::Deserialize;
use std::{borrow::Cow, sync::Arc};
use tap::Tap;

use super::{SongScene, ASSET_CHART_INFO};

#[repr(usize)]
#[derive(Clone, Copy)]
enum Difficulty {
    Easy,
    Hard,
    Extreme,
}
impl Difficulty {
    pub fn name(&self) -> Cow<'static, str> {
        match self {
            Self::Easy => tl!("diff-easy"),
            Self::Hard => tl!("diff-hard"),
            Self::Extreme => tl!("diff-extreme"),
        }
    }

    pub fn color(&self) -> Color {
        Color::from_hex_rgb(match self {
            Self::Easy => 0x16a34a,
            Self::Hard => 0xf97316,
            Self::Extreme => 0xdc2626,
        })
    }
}

#[derive(Deserialize)]
struct LevelInfo {
    level: String,
    charter: String,
    difficulty: f32,
}

#[derive(Deserialize)]
struct SongInfo {
    name: String,
    intro: String,
    composer: String,
    illustrator: String,
    levels: Vec<LevelInfo>,
}

struct ChartInstance {
    id: String,
    info: SongInfo,
    illu: SafeTexture,
    btn: DRectButton,
}

pub struct ChapterScene {
    id: String,

    icons: Arc<Icons>,
    rank_icons: [SafeTexture; 8],
    cover: SafeTexture,

    btn_back: RectButton,

    next_scene: Option<NextScene>,

    first_in: bool,

    diff: Difficulty,
    diff_btn: DRectButton,
    diff_btn_color: Anim<Color>,

    sf: SFader,

    scroll: Scroll,
    charts: Vec<ChartInstance>,
}

impl ChapterScene {
    const WIDTH: f32 = 0.5;
    const HEIGHT: f32 = 0.3;
    const PAD: f32 = 0.05;

    pub async fn new(id: String, icons: Arc<Icons>, rank_icons: [SafeTexture; 8], cover: SafeTexture) -> Result<Self> {
        let songs = match id.as_str() {
            "c1" => vec!["snow", "jumping23"],
            _ => vec![],
        };
        let mut charts = Vec::with_capacity(songs.len());
        for song in songs {
            let info = serde_yaml::from_slice(&load_file(&format!("res/song/{song}/info.yml")).await?)?;
            let illu = load_res_tex(&format!("res/song/{song}/cover")).await;
            charts.push(ChartInstance {
                id: song.to_owned(),
                info,
                illu,
                btn: DRectButton::new(),
            });
        }
        Ok(Self {
            id,

            icons,
            rank_icons,
            cover,
            btn_back: RectButton::new(),

            next_scene: None,

            first_in: true,

            diff: Difficulty::Hard,
            diff_btn: DRectButton::new(),
            diff_btn_color: Anim::new(Difficulty::Hard.color()),

            sf: SFader::new(),

            scroll: Scroll::new().tap_mut(|it| it.y_scroller.step = Self::HEIGHT + Self::PAD),
            charts,
        })
    }
}

impl Scene for ChapterScene {
    fn enter(&mut self, tm: &mut TimeManager, _target: Option<RenderTarget>) -> Result<()> {
        if self.first_in {
            self.first_in = false;
            tm.reset();
        }
        Ok(())
    }

    fn touch(&mut self, tm: &mut TimeManager, touch: &Touch) -> Result<bool> {
        let t = tm.now() as f32;
        if self.btn_back.touch(touch) {
            button_hit();
            self.next_scene = Some(NextScene::Pop);
            return Ok(true);
        }
        if self.diff_btn.touch(touch, t) {
            button_hit();
            self.diff = match self.diff {
                Difficulty::Easy => Difficulty::Hard,
                Difficulty::Hard => Difficulty::Extreme,
                Difficulty::Extreme => Difficulty::Easy,
            };
            self.diff_btn_color.goto(self.diff.color(), t, 0.4);
            return Ok(true);
        }
        if self.scroll.touch(touch, t) {
            return Ok(true);
        }
        for chart in &mut self.charts {
            if chart.btn.touch(touch, t) {
                button_hit();
                let info = &chart.info;
                let level = &info.levels[self.diff as usize];
                let local_path = format!(
                    ":{}:{}",
                    chart.id,
                    match self.diff {
                        Difficulty::Easy => "ez",
                        Difficulty::Hard => "hd",
                        Difficulty::Extreme => "ex",
                    }
                );
                let item = ChartItem {
                    info: BriefChartInfo {
                        id: None,
                        uploader: None,
                        name: info.name.clone(),
                        level: level.level.clone(),
                        difficulty: level.difficulty,
                        intro: info.intro.clone(),
                        charter: level.charter.clone(),
                        composer: info.composer.clone(),
                        illustrator: info.illustrator.clone(),
                        created: None,
                        updated: None,
                        chart_updated: None,
                        has_unlock: false,
                    },
                    illu: Illustration::from_done(chart.illu.clone()),
                    local_path: Some(local_path.clone()),
                    chart_type: ChartType::Integrated,
                };
                let info = &item.info;
                let dir = format!("{}/{}", dir::charts()?, item.local_path.as_ref().unwrap().replace(':', "_"));
                let path = std::path::Path::new(&dir);
                if !path.exists() {
                    std::fs::create_dir_all(path)?;
                }
                let dir = prpr::dir::Dir::new(dir)?;
                *ASSET_CHART_INFO.lock().unwrap() = Some(ChartInfo {
                    id: None,
                    uploader: None,

                    name: info.name.clone(),
                    difficulty: info.difficulty,
                    level: info.level.clone(),
                    charter: info.charter.clone(),
                    composer: info.composer.clone(),
                    illustrator: info.illustrator.clone(),

                    chart: ":chart".to_owned(),
                    format: None,
                    music: ":music".to_owned(),
                    illustration: ":illu".to_owned(),
                    unlock_video: None,

                    dynamic_background: 0,

                    preview_start: 0.,
                    preview_end: None,
                    aspect_ratio: 16. / 9.,
                    background_dim: 0.6,
                    line_length: 6.,
                    offset: dir
                        .read("offset")
                        .map(|d| {
                            f32::from_be_bytes(
                                d.get(0..4)
                                    .map(|first4| {
                                        let mut result = <[u8; 4]>::default();
                                        result.copy_from_slice(first4);
                                        result
                                    })
                                    .unwrap_or_default(),
                            )
                        })
                        .unwrap_or_default(),
                    tip: None,
                    tags: Vec::new(),

                    intro: info.intro.clone(),

                    hold_partial_cover: true,
                    note_uniform_scale: false,
                    force_aspect_ratio: false,
                    use_rpe_170_speed: Some(false),
                    use_attach_ui_fix: Some(true),

                    created: None,
                    updated: None,
                    chart_updated: None,
                });
                self.sf
                    .goto(t, SongScene::new(item, Some(local_path), Arc::clone(&self.icons), self.rank_icons.clone(), Mods::empty()));
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn update(&mut self, tm: &mut TimeManager) -> Result<()> {
        let t = tm.now() as f32;
        self.scroll.update(t);
        Ok(())
    }

    fn render(&mut self, tm: &mut TimeManager, ui: &mut Ui) -> Result<()> {
        if prpr::ui::PREFER_ALT_UI.load(std::sync::atomic::Ordering::Relaxed) {
            return self.render_a1(tm, ui);
        }
        set_camera(&ui.camera());
        let t = tm.now() as f32;

        let r = ui.screen_rect();
        ui.fill_rect(r, (*self.cover, r));
        ui.fill_rect(r, semi_black(0.3));
        let r = ui.back_rect();
        ui.fill_rect(r, (*self.icons.back, r));
        self.btn_back.set(ui, r);

        use crate::resource::L10N_LOCAL;

        let title = rtl!(format!("chap-{}", self.id)).into_owned();
        let intro = rtl!(format!("chap-{}-intro", self.id));

        let p = (t / 0.4).min(1.);
        let p = 1. - (1. - p).powi(3);

        let r = Rect::new(-0.83, -0.35, 0.6, 0.12);
        ui.scissor(r, |ui| {
            ui.text(title).pos(r.x, r.y + (1. - p) * 0.1).size(1.4).draw_using(&BOLD_FONT);
        });
        ui.text(intro)
            .pos(r.x, r.bottom() + 0.02)
            .size(0.44)
            .max_width(0.74)
            .multiline()
            .color(semi_white(p))
            .draw();

        let r = Rect::new(r.x, 0.3, 0.24, 0.1);
        self.diff_btn.render_shadow(ui, r, t, |ui, path| {
            let ct = r.center();
            ui.fill_path(&path, self.diff_btn_color.now(t));
            ui.text(self.diff.name())
                .pos(ct.x, ct.y)
                .anchor(0.5, 0.5)
                .no_baseline()
                .size(0.6)
                .draw_using(&BOLD_FONT);
        });

        let r = Rect::new(0.2, -ui.top, 0.6, ui.top * 2.);
        self.scroll.size((r.w, r.h));
        ui.scope(|ui| {
            ui.dx(r.x);
            ui.dy(r.y);
            self.scroll.render(ui, |ui| {
                ui.dx(r.w / 2.);
                ui.dy(ui.top);
                let mut y = 0.;
                let step = Self::HEIGHT + Self::PAD;
                for chart in &mut self.charts {
                    let r = Rect::new(-Self::WIDTH / 2., y - Self::HEIGHT / 2., Self::WIDTH, Self::HEIGHT);
                    chart.btn.render_shadow(ui, r, t, |ui, path| {
                        ui.fill_path(&path, (*chart.illu, r));
                        ui.fill_path(&path, semi_black(0.4));
                        let mut t = ui
                            .text(&chart.info.levels[self.diff as usize].level)
                            .pos(r.right() - 0.016, r.y + 0.016)
                            .max_width(r.w * 2. / 3.)
                            .anchor(1., 0.)
                            .size(0.52)
                            .color(WHITE);
                        let ms = t.measure();
                        t.ui.fill_path(&ms.feather(0.008).rounded(0.01), Color { a: 0.7, ..t.ui.background() });
                        t.draw();

                        ui.text(&chart.info.name)
                            .pos(r.x + 0.01, r.bottom() - 0.02)
                            .max_width(r.w)
                            .anchor(0., 1.)
                            .size(0.6)
                            .color(WHITE)
                            .draw();
                    });
                    y += step;
                }

                (Self::WIDTH, step * (self.charts.len() - 1) as f32 + ui.top * 2.)
            });
        });

        self.sf.render(ui, t);

        Ok(())
    }

    fn next_scene(&mut self, tm: &mut TimeManager) -> NextScene {
        self.next_scene.take().or_else(|| self.sf.next_scene(tm.now() as f32)).unwrap_or_default()
    }
}

impl ChapterScene {
    /// XCHS 皮肤：暗梅色面板 + 粉色描边，页眉沿用各页统一的 XCHS 配方。
    fn render_a1(&mut self, tm: &mut TimeManager, ui: &mut Ui) -> Result<()> {
        set_camera(&ui.camera());
        let t = tm.now() as f32;
        let itop = -ui.top;

        use crate::resource::L10N_LOCAL;

        let accent = Color::new(1.0, 0.58, 0.706, 1.0);
        let accent_deep = Color::new(0.949, 0.412, 0.580, 1.0);
        let cream = Color::new(0.984, 0.973, 0.886, 1.0);
        let muted = Color::new(1.0, 0.776, 0.847, 0.62);
        let border = Color::new(1.0, 0.776, 0.847, 0.45);
        let card = Color::new(0.145, 0.098, 0.157, 0.98);
        let card_lit = Color::new(0.165, 0.110, 0.180, 1.0);

        // 背景：原封面 + 暗梅色蒙版 + 顶部淡淡的 ♡
        let sr = ui.screen_rect();
        ui.fill_rect(sr, (*self.cover, sr));
        ui.fill_rect(sr, Color::new(0.075, 0.045, 0.09, 0.72));
        for i in 0..7 {
            ui.text("\u{2661}")
                .pos(-0.9 + i as f32 * 0.3, itop + 0.22)
                .anchor(0.5, 0.5)
                .no_baseline()
                .size(0.62)
                .color(Color::new(1.0, 0.776, 0.847, if i % 2 == 0 { 0.05 } else { 0.032 }))
                .draw();
        }
        ui.fill_rect(Rect::new(-1., -itop - 0.08, 2., 0.08), Color::new(0.09, 0.05, 0.08, 0.45));

        // 页眉（不画原来的返回图标贴图，本页自绘）
        let br = ui.back_rect();
        ui.fill_path(&br.feather(-0.004).rounded(0.02), Color::new(1.0, 0.58, 0.706, 0.14));
        ui.text("\u{2190}")
            .pos(br.center().x, br.center().y)
            .anchor(0.5, 0.5)
            .no_baseline()
            .size(0.5)
            .color(accent)
            .draw();
        self.btn_back.set(ui, br);

        let title = rtl!(format!("chap-{}", self.id)).into_owned();
        let intro = rtl!(format!("chap-{}-intro", self.id)).into_owned();
        let title_x = br.right() + 0.04;
        let tr = ui
            .text(title)
            .pos(title_x, itop + 0.155 * 0.38)
            .anchor(0., 0.5)
            .no_baseline()
            .size(0.9)
            .max_width(0.78)
            .color(cream)
            .draw();
        ui.text("\u{2665}")
            .pos(tr.right() + 0.028, tr.center().y)
            .anchor(0., 0.5)
            .no_baseline()
            .size(0.5)
            .color(accent)
            .draw();
        ui.fill_path(&Rect::new(title_x, itop + 0.155 - 0.014, 0.13, 0.006).rounded(0.003), accent);
        ui.fill_rect(Rect::new(-1., itop + 0.155, 2., 0.002), Color::new(1.0, 0.58, 0.706, 0.20));

        // 左栏：章节简介 + 难度选择
        let top = itop + 0.19;
        let ch = -itop - 0.10 - top;
        let panel = Rect::new(-0.94, top, 0.62, ch);
        ui.fill_path(&panel.feather(0.008).rounded(0.028), Color::new(1.0, 0.58, 0.706, 0.07));
        ui.fill_path(&panel.rounded(0.028), card_lit);
        ui.stroke_path(&panel.rounded(0.028), 0.0035, border);
        let ix = panel.x + 0.035;
        let iw = panel.w - 0.07;
        ui.text("\u{2665}  CHAPTER")
            .pos(ix, panel.y + 0.05)
            .anchor(0., 0.5)
            .no_baseline()
            .size(0.3)
            .color(accent)
            .draw();
        ui.fill_path(&Rect::new(ix, panel.y + 0.078, 0.11, 0.005).rounded(0.0025), accent_deep);
        ui.text(format!("\u{2661}  {} CHARTS", self.charts.len()))
            .pos(ix, panel.y + 0.108)
            .anchor(0., 0.5)
            .no_baseline()
            .size(0.26)
            .color(muted)
            .draw();
        ui.text(intro)
            .pos(ix, panel.y + 0.15)
            .size(0.33)
            .max_width(iw)
            .multiline()
            .color(muted)
            .draw();

        let dc = self.diff_btn_color.now(t);
        let dr = Rect::new(ix, panel.bottom() - 0.255, iw, 0.14);
        self.diff_btn.build(ui, t, dr, |ui, path| {
            ui.fill_path(&dr.feather(0.01).rounded(0.02), Color { a: 0.16, ..dc });
            ui.fill_path(&path, Color::new(0.37, 0.185, 0.265, 0.99));
            ui.stroke_path(&path, 0.0035, Color { a: 0.8, ..dc });
            ui.fill_path(&Rect::new(dr.x, dr.bottom() - 0.006, dr.w, 0.006).rounded(0.003), dc);
            ui.text(self.diff.name())
                .pos(dr.center().x, dr.center().y - 0.004)
                .anchor(0.5, 0.5)
                .no_baseline()
                .size(0.5)
                .color(cream)
                .draw();
        });
        ui.text("\u{2665}  DIFFICULTY")
            .pos(ix, panel.bottom() - 0.29)
            .anchor(0., 0.5)
            .no_baseline()
            .size(0.26)
            .color(muted)
            .draw();
        let seg_w = (iw - 0.016) / 3.;
        for i in 0..3 {
            let seg = Rect::new(ix + i as f32 * (seg_w + 0.008), panel.bottom() - 0.075, seg_w, 0.008);
            ui.fill_path(
                &seg.rounded(0.004),
                if i == self.diff as usize { accent } else { Color::new(1.0, 0.776, 0.847, 0.16) },
            );
        }

        // 右栏：谱面卡片列表
        let list = Rect::new(panel.right() + 0.06, top + 0.09, 0.94 - panel.right() - 0.06, ch - 0.09);
        ui.text("\u{2665}  SONGS")
            .pos(list.x, top + 0.035)
            .anchor(0., 0.5)
            .no_baseline()
            .size(0.3)
            .color(accent)
            .draw();
        ui.fill_path(&Rect::new(list.x, top + 0.062, 0.09, 0.005).rounded(0.0025), accent_deep);

        let pad = 0.028;
        let row_h = 0.24;
        let di = self.diff as usize;
        self.scroll.size((list.w, list.h));
        ui.scope(|ui| {
            ui.dx(list.x);
            ui.dy(list.y);
            self.scroll.render(ui, |ui| {
                if self.charts.is_empty() {
                    ui.text("\u{2661}  NO CHART")
                        .pos(list.w / 2., 0.16)
                        .anchor(0.5, 0.)
                        .no_baseline()
                        .size(0.42)
                        .color(muted)
                        .draw();
                }
                let mut y = 0.;
                for chart in &mut self.charts {
                    let r = Rect::new(0., y, list.w - 0.02, row_h);
                    chart.btn.build(ui, t, r, |ui, path| {
                        ui.fill_path(&r.feather(0.008).rounded(0.024), Color::new(1.0, 0.58, 0.706, 0.09));
                        ui.fill_path(&path, card);
                        ui.stroke_path(&path, 0.0035, border);
                        let th = Rect::new(r.x + 0.012, r.y + 0.012, row_h - 0.024, row_h - 0.024);
                        ui.fill_path(&th.rounded(0.018), (*chart.illu, th));
                        ui.fill_path(&th.rounded(0.018), semi_black(0.12));
                        ui.stroke_path(&th.rounded(0.018), 0.0025, Color::new(1.0, 0.776, 0.847, 0.30));
                        let tx = th.right() + 0.026;
                        ui.fill_path(&Rect::new(tx - 0.013, r.y + 0.05, 0.005, r.h - 0.10).rounded(0.0025), accent_deep);
                        let lv = &chart.info.levels[di];
                        let mut lvl = ui
                            .text(format!("\u{2665} {}", lv.level))
                            .pos(r.right() - 0.016, r.y + 0.014)
                            .anchor(1., 0.)
                            .no_baseline()
                            .size(0.34)
                            .max_width(r.w * 0.4)
                            .color(cream);
                        let ms = lvl.measure();
                        lvl.ui.fill_path(&ms.feather(0.012).rounded(0.01), accent_deep);
                        lvl.draw();
                        ui.text(&chart.info.name)
                            .pos(tx, r.y + 0.068)
                            .anchor(0., 0.5)
                            .no_baseline()
                            .size(0.46)
                            .max_width(r.w - (tx - r.x) - 0.02)
                            .color(cream)
                            .draw();
                        ui.text(format!("\u{2665}  {}", lv.charter))
                            .pos(tx, r.bottom() - 0.055)
                            .anchor(0., 0.5)
                            .no_baseline()
                            .size(0.30)
                            .max_width(r.w - (tx - r.x) - 0.02)
                            .color(muted)
                            .draw();
                    });
                    y += row_h + pad;
                }
                (list.w, (self.charts.len() as f32 * (row_h + pad)).max(list.h))
            });
        });

        self.sf.render(ui, t);

        Ok(())
    }
}
