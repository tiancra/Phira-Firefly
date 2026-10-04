prpr_l10n::tl_file!("settings");

use super::{MpServerPage, NextPage, OffsetPage, Page, SFader, SharedState};
use crate::{
    dir, get_data, get_data_mut,
    popup::ChooseButton,
    save_data,
    scene::{TutorialLoadingScene, BGM_VOLUME_UPDATED},
    sync_data,
    tabs::{Tabs, TitleFn},
    theme::ThemeView,
};
use anyhow::Result;
use bytesize::ByteSize;
use inputbox::InputBox;
use macroquad::prelude::*;
use once_cell::sync::Lazy;
use prpr::{
    config::DynamicBackgroundMode,
    core::BOLD_FONT,
    ext::{open_url, poll_future, semi_white, LocalTask, RectExt, SafeTexture},
    scene::{request_input, return_input, show_error, show_message, take_input, NextScene},
    task::Task,
    ui::{Dialog, DRectButton, Scroll, Slider, Ui, PREFER_REDUCED_MOTION},
};
use prpr_l10n::{LANGS, LANG_NAMES};
use reqwest::Url;
use serde::Deserialize;
use std::{borrow::Cow, fs, io, path::PathBuf, sync::atomic::Ordering};

const ITEM_HEIGHT: f32 = 0.15;
const INTERACT_WIDTH: f32 = 0.26;
const STATUS_PAGE: &str = "https://status.phira.cn";

struct NameList(String);
impl<'de> Deserialize<'de> for NameList {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = Vec::<String>::deserialize(deserializer)?;
        Ok(Self(s.join(", ")))
    }
}

#[derive(Deserialize)]
struct LocalizationListRaw {
    #[serde(rename = "en-US")]
    en_us: NameList,
    #[serde(rename = "fr-FR")]
    fr_fr: NameList,
    #[serde(rename = "de-DE")]
    de_de: NameList,
    #[serde(rename = "id-ID")]
    id_id: NameList,
    #[serde(rename = "ja-JP")]
    ja_jp: NameList,
    #[serde(rename = "ko-KR")]
    ko_kr: NameList,
    #[serde(rename = "pl-PL")]
    pl_pl: NameList,
    #[serde(rename = "pt-BR")]
    pt_br: NameList,
    #[serde(rename = "ru-RU")]
    ru_ru: NameList,
    #[serde(rename = "th-TH")]
    th_th: NameList,
    #[serde(rename = "zh-TW")]
    zh_tw: NameList,
    #[serde(rename = "tr-TR")]
    tr_tr: NameList,
    #[serde(rename = "vi-VN")]
    vi_vn: NameList,
}

struct LocalizationList(String);
impl<'de> Deserialize<'de> for LocalizationList {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = LocalizationListRaw::deserialize(deserializer)?;
        Ok(Self(format!(
            "\
English (en-US)\n{}\n
French (fr-FR)\n{}\n
German (de-DE)\n{}\n
Indonesian (id-ID)\n{}\n
Japanese (ja-JP)\n{}\n
Korean (ko-KR)\n{}\n
Polish (pl-PL)\n{}\n
Portuguese (pt-BR)\n{}\n
Russian (ru-RU)\n{}\n
Thai (th-TH)\n{}\n
Traditional Chinese (zh-TW)\n{}\n
Turkish (tr-TR)\n{}\n
Vietnamese (vi-VN)\n{}",
            raw.en_us.0,
            raw.fr_fr.0,
            raw.de_de.0,
            raw.id_id.0,
            raw.ja_jp.0,
            raw.ko_kr.0,
            raw.pl_pl.0,
            raw.pt_br.0,
            raw.ru_ru.0,
            raw.th_th.0,
            raw.zh_tw.0,
            raw.tr_tr.0,
            raw.vi_vn.0
        )))
    }
}

#[derive(Deserialize)]
struct StaffList {
    development: NameList,
    operations: NameList,
    documentation: NameList,
    art: NameList,
    music: NameList,
    audio: NameList,
    community: NameList,
    localization: LocalizationList,
}

static STAFF_LIST: Lazy<StaffList> = Lazy::new(|| {
    let data = include_str!("../../staff.yml");
    serde_yaml::from_str(data).unwrap()
});

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingListType {
    General,
    Audio,
    Chart,
    Debug,
    About,
    Theme,
}

pub struct SettingsPage {
    list_general: GeneralList,
    list_audio: AudioList,
    list_chart: ChartList,
    list_debug: DebugList,
    theme_view: ThemeView,

    tabs: Tabs<SettingListType>,

    scroll: Scroll,
    save_time: f32,

    icon: SafeTexture,

    sf: SFader,
    need_back: bool,

    nb1: [DRectButton; 6],
    tr2: [Rect; 6],
}

impl SettingsPage {
    const SAVE_TIME: f32 = 0.5;

    pub fn new(icon: SafeTexture, icon_lang: SafeTexture) -> Self {
        Self {
            list_general: GeneralList::new(icon_lang),
            list_audio: AudioList::new(),
            list_chart: ChartList::new(),
            list_debug: DebugList::new(),
            theme_view: ThemeView::new(),

            tabs: Tabs::new([
                (SettingListType::General, || tl!("general")),
                (SettingListType::Audio, || tl!("audio")),
                (SettingListType::Chart, || tl!("chart")),
                (SettingListType::Debug, || tl!("debug")),
                (SettingListType::Theme, || tl!("theme")),
                (SettingListType::About, || tl!("about")),
            ] as [(SettingListType, TitleFn); 6]),

            scroll: Scroll::new(),
            save_time: f32::INFINITY,

            icon,

            sf: SFader::new(),
            need_back: false,

            nb1: [(); 6].map(|_| DRectButton::new()),
            tr2: [Rect::new(0., 0., 0., 0.); 6],
        }
    }

    pub fn render_a1(&mut self, ui: &mut Ui, s2: &mut SharedState) -> Result<()> {
        let t = s2.t;
        self.theme_view.set_import_in_header(true);
        let rt1 = s2.rt;
        let top = ui.top;
        let bar_y = -top;
        let total_h = top * 2.;
        const HDR_H: f32 = 0.155;
        const MARGIN: f32 = 0.045;
        const SECTION_HDR_H: f32 = 0.072;
        let full_x = -1.0 + MARGIN;
        let full_w = (1.0 - MARGIN) - full_x;
        let c_bg = Color::new(0.15, 0.075, 0.12, 0.3);
        let c_nav_bg = Color::new(0.15, 0.075, 0.12, 0.45);
        let c_hdr_bg = Color::new(0.15, 0.075, 0.12, 0.7);
        let c_sec_hdr = Color::new(0.176, 0.118, 0.188, 1.);
        let c_sep = Color::new(1., 0.776, 0.847, 0.10);
        let c_accent = Color::new(1.0, 0.58, 0.706, 1.0);
        let c_cream = Color::new(0.984, 0.973, 0.886, 1.0);

        let section_name: Cow<'static, str> = match self.tabs.selected() {
            SettingListType::General => tl!("general"),
            SettingListType::Audio => tl!("audio"),
            SettingListType::Chart => tl!("chart"),
            SettingListType::Debug => tl!("debug"),
            SettingListType::About => tl!("about"),
            SettingListType::Theme => tl!("theme"),
        };
        let section_str = section_name.as_ref();

        s2.render_fader(ui, |ui| {
            // c_hdr_bg drawn by main scene
            let br = ui.back_rect();
            ui.fill_path(&br.feather(-0.004).rounded(0.02), Color::new(1.0, 0.58, 0.706, 0.14));
            ui.text("\u{2190}")
                .pos(br.center().x, br.center().y)
                .anchor(0.5, 0.5).no_baseline().size(0.5)
                .color(c_accent).draw();
            let title_x = br.right() + 0.04;
            let title_y = bar_y + HDR_H * 0.38;
            let st_r = ui.text("Settings")
                .pos(title_x, title_y)
                .anchor(0., 0.5).no_baseline().size(0.9)
                .color(c_cream).draw();
            ui.text("\u{2665}")
                .pos(st_r.right() + 0.028, st_r.center().y)
                .anchor(0., 0.5).no_baseline().size(0.5)
                .color(c_accent).draw();
            ui.text(section_str)
                .pos(title_x + 0.002, title_y + 0.058)
                .anchor(0., 0.5).no_baseline().size(0.40)
                .color(Color::new(1.0, 0.776, 0.847, 0.8)).draw();
            ui.fill_path(&Rect::new(title_x, bar_y + HDR_H - 0.014, 0.13, 0.006).rounded(0.003), c_accent);

            let tab_y = bar_y + HDR_H;
            let card_top = tab_y + 0.012;
            let card_h = (top - MARGIN) - card_top;
            let side_w = 0.52;
            let gap = 0.032;
            let side_rect = Rect::new(full_x, card_top, side_w, card_h);
            ui.fill_path(&side_rect.feather(0.008).rounded(0.035), Color::new(1.0, 0.58, 0.706, 0.10));
            ui.fill_path(&side_rect.rounded(0.03), c_bg);
            let sel = self.tabs.selected_idx();
            let n = self.tabs.len();
            let btn_h = 0.104;
            let btn_gap = 0.02;
            let btns_top = card_top + 0.024;
            for i in 0..n {
                let by = btns_top + i as f32 * (btn_h + btn_gap);
                let tr = Rect::new(full_x + 0.02, by, side_w - 0.04, btn_h);
                self.tr2[i] = tr;
                let active = i == sel;
                if active {
                    ui.fill_path(&tr.feather(0.006).rounded(btn_h * 0.5), Color::new(1.0, 0.58, 0.706, 0.25));
                    ui.fill_path(&tr.rounded(btn_h * 0.5), c_accent);
                } else {
                    ui.fill_path(&tr.rounded(btn_h * 0.5), c_nav_bg);
                    ui.stroke_path(&tr.rounded(btn_h * 0.5), 0.0025, Color::new(1.0, 0.776, 0.847, 0.18));
                }
                ui.text(format!("{} {}", if active { "\u{2665}" } else { "\u{2661}" }, self.tabs.title(i)))
                    .pos(tr.x + 0.036, tr.center().y)
                    .anchor(0., 0.5).no_baseline().size(0.44)
                    .max_width(tr.w - 0.07)
                    .color(if active { WHITE } else { semi_white(0.7) })
                    .draw();
                self.nb1[i].render_shadow(ui, tr, rt1, |_, _| {});
            }
            let panel_x = full_x + side_w + gap;
            let panel_w = full_w - side_w - gap;
            let content_card = Rect::new(panel_x, card_top, panel_w, card_h);
            ui.fill_path(&content_card.feather(0.008).rounded(0.035), Color::new(1.0, 0.58, 0.706, 0.10));
            ui.fill_path(&content_card.rounded(0.03), c_bg);
            let sh_r = Rect::new(panel_x, card_top, panel_w, SECTION_HDR_H);
            ui.fill_path(&Rect::new(panel_x + 0.018, card_top + 0.014, panel_w - 0.036, SECTION_HDR_H - 0.018).rounded(0.018), c_sec_hdr);
            ui.text(section_str)
                .pos(panel_x + 0.04, sh_r.center().y)
                .anchor(0., 0.5).no_baseline().size(0.5)
                .color(c_cream).draw();
            // 主题页导入加号：放在顶部标题栏右侧（与"主题"标题同一行）。
            if let SettingListType::Theme = self.tabs.selected() {
                let ir = Rect::new(sh_r.right() - 0.075, sh_r.center().y - 0.0375, 0.075, 0.075);
                self.theme_view.render_import_btn_at(ui, ir, t);
            }
            ui.fill_path(&Rect::new(panel_x + 0.03, sh_r.bottom() + 0.002, panel_w - 0.06, 0.003).rounded(0.0015), c_sep);
            let list_y = card_top + SECTION_HDR_H + 0.008;
            let list_h = card_h - SECTION_HDR_H - 0.02;
            let list_r = Rect::new(panel_x + 0.012, list_y, panel_w - 0.024, list_h);
            self.scroll.size((panel_w - 0.08, list_h));
            ui.scissor(list_r, |ui| {
                ui.scope(|ui| {
                    ui.dx(panel_x + 0.04);
                    ui.dy(list_y);
                    let render_r = Rect::new(0., 0., panel_w - 0.08, list_h);
                    self.scroll.render(ui, |ui| match self.tabs.selected() {
                        SettingListType::General => self.list_general.render(ui, render_r, t),
                        SettingListType::Audio => self.list_audio.render(ui, render_r, t),
                        SettingListType::Chart => self.list_chart.render(ui, render_r, t),
                        SettingListType::Debug => self.list_debug.render(ui, render_r, t),
                        SettingListType::About => render_about(ui, render_r, &self.icon),
                        SettingListType::Theme => self.theme_view.render(ui, render_r, t),
                    });
                });
            });
            Ok::<(), anyhow::Error>(())
        })?;
        Ok(())
    }
}

impl Page for SettingsPage {
    fn label(&self) -> Cow<'static, str> {
        tl!("label")
    }

    fn enter(&mut self, s: &mut SharedState) -> Result<()> {
        if std::mem::take(&mut self.need_back) {
            self.sf.enter(s.t);
        }
        Ok(())
    }

    fn exit(&mut self) -> Result<()> {
        BGM_VOLUME_UPDATED.store(true, Ordering::Relaxed);
        if self.save_time.is_finite() {
            save_data()?;
        }
        Ok(())
    }

    fn touch(&mut self, touch: &Touch, s: &mut SharedState) -> Result<bool> {
        let t = s.t;
        if prpr::ui::PREFER_ALT_UI.load(std::sync::atomic::Ordering::Relaxed) {
            for (i, nr) in self.tr2.iter().enumerate() {
                if nr.w > 0. && nr.contains(touch.position) {
                    self.tabs.goto(t, i);
                    return Ok(true);
                }
            }
        }
        if match self.tabs.selected() {
            SettingListType::General => self.list_general.top_touch(touch, t),
            SettingListType::Audio => self.list_audio.top_touch(touch, t),
            SettingListType::Chart => self.list_chart.top_touch(touch, t),
            SettingListType::Debug => self.list_debug.top_touch(touch, t),
            SettingListType::About => false,
            SettingListType::Theme => false,
        } {
            return Ok(true);
        }

        if self.tabs.touch(touch, s.rt) {
            return Ok(true);
        }

        if self.scroll.touch(touch, t) {
            return Ok(true);
        }
        if let SettingListType::Theme = self.tabs.selected() {
            if self.theme_view.touch(touch, t)? {
                self.scroll.y_scroller.halt();
                return Ok(true);
            }
        } else if let Some(p) = match self.tabs.selected() {
            SettingListType::General => self.list_general.touch(touch, t)?,
            SettingListType::Audio => self.list_audio.touch(touch, t)?,
            SettingListType::Chart => self.list_chart.touch(touch, t)?,
            SettingListType::Debug => self.list_debug.touch(touch, t)?,
            SettingListType::About => None,
            SettingListType::Theme => None,
        } {
            if p {
                self.save_time = t;
            }
            self.scroll.y_scroller.halt();
            return Ok(true);
        }
        Ok(false)
    }

    fn update(&mut self, s: &mut SharedState) -> Result<()> {
        let t = s.t;
        let changed = match self.tabs.selected() {
            SettingListType::General => self.list_general.update(t)?,
            SettingListType::Audio => self.list_audio.update(t)?,
            SettingListType::Chart => self.list_chart.update(t)?,
            SettingListType::Debug => self.list_debug.update(t)?,
            SettingListType::About => false,
            SettingListType::Theme => self.theme_view.update(t)?,
        };
        self.scroll.update(t);
        if changed {
            self.save_time = t;
        }
        if t > self.save_time + Self::SAVE_TIME {
            save_data()?;
            self.save_time = f32::INFINITY;
        }
        if self.list_general.start_tutorial {
            self.list_general.start_tutorial = false;
            self.need_back = true;
            match TutorialLoadingScene::new() {
                Ok(scene) => self.sf.goto(t, scene),
                Err(err) => show_error(err),
            }
        }
        Ok(())
    }

    fn render(&mut self, ui: &mut Ui, s: &mut SharedState) -> Result<()> {
        if prpr::ui::PREFER_ALT_UI.load(std::sync::atomic::Ordering::Relaxed) {
            return self.render_a1(ui, s);
        }
        let t = s.t;
        let rt = s.rt;

        s.fader.render(ui, s.t, |ui| {
            let r = ui.content_rect();
            self.tabs.render(ui, rt, r, |ui, item| {
                let r = r.feather(-0.01);
                self.scroll.size((r.w, r.h));
                ui.scope(|ui| {
                    ui.dx(r.x);
                    ui.dy(r.y);
                    self.scroll.render(ui, |ui| match item {
                        SettingListType::General => self.list_general.render(ui, r, t),
                        SettingListType::Audio => self.list_audio.render(ui, r, t),
                        SettingListType::Chart => self.list_chart.render(ui, r, t),
                        SettingListType::Debug => self.list_debug.render(ui, r, t),
                        SettingListType::About => render_about(ui, r, &self.icon),
                        SettingListType::Theme => self.theme_view.render(ui, r, t),
                    });
                });

                Ok(())
            })
        })?;
        self.sf.render(ui, s.t);

        Ok(())
    }

    fn next_page(&mut self) -> NextPage {
        match self.tabs.selected() {
            SettingListType::General => self.list_general.next_page().unwrap_or_default(),
            SettingListType::Audio => self.list_audio.next_page().unwrap_or_default(),
            _ => NextPage::None,
        }
    }

    fn next_scene(&mut self, s: &mut SharedState) -> NextScene {
        if let Some(scene) = self.theme_view.next_scene() {
            return scene;
        }
        self.sf.next_scene(s.t).unwrap_or_default()
    }

    fn on_result(&mut self, _result: Box<dyn std::any::Any>, s: &mut SharedState) -> Result<()> {
        self.theme_view.on_result(s.t);
        Ok(())
    }

    fn render_top(&mut self, ui: &mut Ui, s: &mut SharedState) -> Result<()> {
        self.theme_view.render_top(ui, s.t);
        Ok(())
    }
}

fn render_about(ui: &mut Ui, mut r: Rect, icon: &SafeTexture) -> (f32, f32) {
    r.x = 0.;
    r.y = 0.;
    let ow = r.w;
    let r = r.feather(-0.02);

    let ct = r.center();
    let s = 0.1;
    let ir = Rect::new(ct.x - s, r.y + 0.05, s * 2., s * 2.);
    ui.fill_path(&ir.rounded(0.02), (**icon, ir));

    let staff = &*STAFF_LIST;
    let text = tl!(
        "about-content",
        "version" => format!("{} ({})", env!("CARGO_PKG_VERSION"), env!("GIT_HASH")),

        "development" => &staff.development.0,
        "operations" => &staff.operations.0,
        "documentation" => &staff.documentation.0,
        "art" => &staff.art.0,
        "music" => &staff.music.0,
        "audio" => &staff.audio.0,
        "community" => &staff.community.0,
        "localization" => &staff.localization.0
    );
    let (first, text) = text.split_once('\n').unwrap();
    let tr = ui
        .text(first)
        .pos(ct.x, ir.bottom() + 0.03)
        .anchor(0.5, 0.)
        .size(0.6)
        .draw_using(&BOLD_FONT);

    let r = ui
        .text(text.trim())
        .pos(r.x, tr.bottom() + 0.06)
        .size(0.55)
        .multiline()
        .max_width(r.w)
        .h_center()
        .draw();

    (ow, r.bottom() + 0.03)
}

fn render_title<'a>(ui: &mut Ui, title: impl Into<Cow<'a, str>>, subtitle: Option<Cow<'a, str>>) -> f32 {
    const TITLE_SIZE: f32 = 0.6;
    const SUBTITLE_SIZE: f32 = 0.35;
    const LEFT: f32 = 0.06;
    const PAD: f32 = 0.01;
    const SUB_MAX_WIDTH: f32 = 1.4;
    if let Some(subtitle) = subtitle {
        let title = title.into();
        let r1 = ui.text(Cow::clone(&title)).size(TITLE_SIZE).measure();
        let r2 = ui
            .text(Cow::clone(&subtitle))
            .size(SUBTITLE_SIZE)
            .max_width(SUB_MAX_WIDTH)
            .no_baseline()
            .measure();
        let h = r1.h + PAD + r2.h;
        let r1 = ui
            .text(subtitle)
            .pos(LEFT, (ITEM_HEIGHT + h) / 2.)
            .anchor(0., 1.)
            .size(SUBTITLE_SIZE)
            .max_width(SUB_MAX_WIDTH)
            .color(semi_white(0.6))
            .draw()
            .right();
        let r2 = ui
            .text(title)
            .pos(LEFT, (ITEM_HEIGHT - h) / 2.)
            .no_baseline()
            .size(TITLE_SIZE)
            .draw()
            .right();
        r1.max(r2)
    } else {
        ui.text(title.into())
            .pos(LEFT, ITEM_HEIGHT / 2.)
            .anchor(0., 0.5)
            .no_baseline()
            .size(TITLE_SIZE)
            .draw()
            .right()
    }
}

#[inline]
fn render_switch(ui: &mut Ui, r: Rect, t: f32, btn: &mut DRectButton, on: bool) {
    btn.render_text(ui, r, t, if on { ttl!("switch-on") } else { ttl!("switch-off") }, 0.5, on);
}

#[inline]
fn right_rect(w: f32) -> Rect {
    let rh = ITEM_HEIGHT * 2. / 3.;
    Rect::new(w - 0.3, (ITEM_HEIGHT - rh) / 2., INTERACT_WIDTH, rh)
}

struct GeneralList {
    icon_lang: SafeTexture,

    lang_btn: ChooseButton,

    ui_layout_btn: ChooseButton,

    #[cfg(all(any(target_os = "windows", target_os = "linux"), not(target_env = "ohos")))]
    fullscreen_btn: DRectButton,

    cache_btn: DRectButton,
    offline_btn: DRectButton,
    server_status_btn: DRectButton,
    mp_btn: DRectButton,
    mp_server_btn: DRectButton,
    #[cfg(not(target_env = "ohos"))]
    lowq_btn: DRectButton,
    prefer_reduced_motion_btn: DRectButton,
    enable_anys_btn: DRectButton,
    anys_gateway_btn: DRectButton,
    watch_tutorial_btn: DRectButton,

    cache_size: Option<u64>,
    cache_task: Option<Task<Result<u64>>>,
    pub start_tutorial: bool,
    next_page: Option<NextPage>,
}

impl GeneralList {
    pub fn new(icon_lang: SafeTexture) -> Self {
        let mut this = Self {
            icon_lang,

            lang_btn: ChooseButton::new()
                .with_options(LANG_NAMES.iter().map(|s| s.to_string()).collect())
                .with_selected(
                    get_data()
                        .language
                        .as_ref()
                        .and_then(|it| LANGS.iter().position(|lang| *lang == it))
                        .unwrap_or_default(),
                ),

            ui_layout_btn: ChooseButton::new()
                .with_options(vec!["Official".to_string(), "XCHS UI".to_string()])
                .with_selected(get_data().ui_layout),

            #[cfg(all(any(target_os = "windows", target_os = "linux"), not(target_env = "ohos")))]
            fullscreen_btn: DRectButton::new(),

            cache_btn: DRectButton::new(),
            offline_btn: DRectButton::new(),
            server_status_btn: DRectButton::new(),
            mp_btn: DRectButton::new(),
            mp_server_btn: DRectButton::new(),
            #[cfg(not(target_env = "ohos"))]
            lowq_btn: DRectButton::new(),
            prefer_reduced_motion_btn: DRectButton::new(),
            enable_anys_btn: DRectButton::new(),
            anys_gateway_btn: DRectButton::new(),
            watch_tutorial_btn: DRectButton::new(),

            cache_size: None,
            cache_task: None,
            start_tutorial: false,
            next_page: None,
        };
        let _ = this.update_cache_size();
        this
    }

    pub fn top_touch(&mut self, touch: &Touch, t: f32) -> bool {
        if self.lang_btn.top_touch(touch, t) {
            return true;
        }
        if self.ui_layout_btn.top_touch(touch, t) {
            return true;
        }
        false
    }

    fn dir_size(path: impl Into<PathBuf>) -> io::Result<u64> {
        fn inner(mut dir: fs::ReadDir) -> io::Result<u64> {
            dir.try_fold(0, |acc, file| {
                let file = file?;
                let size = match file.metadata()? {
                    data if data.is_dir() => inner(fs::read_dir(file.path())?)?,
                    data => data.len(),
                };
                Ok(acc + size)
            })
        }

        inner(fs::read_dir(path.into())?)
    }

    fn update_cache_size(&mut self) -> Result<()> {
        self.cache_size = None;

        let cache_dir = dir::cache()?;
        self.cache_task = Some(Task::new(async { Ok(Self::dir_size(cache_dir)?) }));
        Ok(())
    }

    pub fn touch(&mut self, touch: &Touch, t: f32) -> Result<Option<bool>> {
        let data = get_data_mut();
        let config = &mut data.config;
        if self.lang_btn.touch(touch, t) {
            return Ok(Some(false));
        }
        if self.ui_layout_btn.touch(touch, t) {
            return Ok(Some(false));
        }

        #[cfg(all(any(target_os = "windows", target_os = "linux"), not(target_env = "ohos")))]
        if self.fullscreen_btn.touch(touch, t) {
            config.fullscreen_mode ^= true;

            macroquad::window::set_fullscreen(config.fullscreen_mode);

            return Ok(Some(true));
        }

        if self.cache_btn.touch(touch, t) {
            fs::remove_dir_all(dir::cache()?)?;
            self.update_cache_size()?;
            show_message(tl!("item-cache-cleared")).ok();
            return Ok(Some(false));
        }
        if self.offline_btn.touch(touch, t) {
            config.offline_mode ^= true;
            return Ok(Some(true));
        }
        if self.server_status_btn.touch(touch, t) {
            let _ = open_url(STATUS_PAGE);
            return Ok(Some(true));
        }
        if self.mp_btn.touch(touch, t) {
            config.mp_enabled ^= true;
            return Ok(Some(true));
        }
        if self.mp_server_btn.touch(touch, t) {
            self.next_page = Some(NextPage::Overlay(Box::new(MpServerPage::new())));
            return Ok(Some(false));
        }
        #[cfg(not(target_env = "ohos"))]
        if self.lowq_btn.touch(touch, t) {
            config.sample_count = if config.sample_count == 1 { 2 } else { 1 };
            return Ok(Some(true));
        }
        if self.prefer_reduced_motion_btn.touch(touch, t) {
            data.prefer_reduced_motion ^= true;
            PREFER_REDUCED_MOTION.store(data.prefer_reduced_motion, Ordering::Relaxed);
            return Ok(Some(true));
        }
        if self.enable_anys_btn.touch(touch, t) {
            data.enable_anys ^= true;
            return Ok(Some(true));
        }
        if self.anys_gateway_btn.touch(touch, t) {
            request_input("anys_gateway", InputBox::new().default_text(&data.anys_gateway));
            return Ok(Some(true));
        }
        if self.watch_tutorial_btn.touch(touch, t) {
            self.start_tutorial = true;
            return Ok(Some(false));
        }
        Ok(None)
    }

    pub fn update(&mut self, t: f32) -> Result<bool> {
        self.lang_btn.update(t);
        self.ui_layout_btn.update(t);
        if self.ui_layout_btn.changed() {
            let selected = self.ui_layout_btn.selected();
            get_data_mut().ui_layout = selected;
            save_data()?;
            Dialog::plain(
                "重启以应用",
                if selected == 1 {
                    "已切换为 XCHS UI 布局，重启游戏后生效。"
                } else {
                    "已切换为 Official UI 布局，重启游戏后生效。"
                },
            )
            .buttons(vec!["确定".to_string()])
            .listener(|_, _| false)
            .show();
        }
        let data = get_data_mut();
        if self.lang_btn.changed() {
            data.language = Some(LANGS[self.lang_btn.selected()].to_owned());
            sync_data();
            return Ok(true);
        }
        if let Some((id, text)) = take_input() {
            if id == "anys_gateway" {
                if let Err(err) = Url::parse(&text) {
                    show_error(anyhow::Error::new(err).context(tl!("item-anys-gateway-invalid")));
                    return Ok(false);
                } else {
                    data.anys_gateway = text.trim_end_matches('/').to_string();
                    return Ok(true);
                }
            } else {
                return_input(id, text);
            }
        }
        if let Some(task) = &mut self.cache_task {
            if let Some(size) = task.take() {
                self.cache_size = size.ok();
                self.cache_task = None;
            }
        }
        Ok(false)
    }

    pub fn render(&mut self, ui: &mut Ui, r: Rect, t: f32) -> (f32, f32) {
        let w = r.w;
        let mut h = 0.;
        macro_rules! item {
            ($($b:tt)*) => {{
                $($b)*
                ui.dy(ITEM_HEIGHT);
                h += ITEM_HEIGHT;
            }}
        }
        let rr = right_rect(w);

        let data = get_data();
        let config = &data.config;
        item! {
            let rt = render_title(ui, tl!("item-lang"), None);
            let w = 0.06;
            let r = Rect::new(rt + 0.01, (ITEM_HEIGHT - w) / 2., w, w);
            ui.fill_rect(r, (*self.icon_lang, r));
            self.lang_btn.render(ui, rr, t);
        }
        item! {
            render_title(ui, "UI布局", Some(Cow::Borrowed("选择界面风格，重启游戏后生效")));
            self.ui_layout_btn.render(ui, rr, t);
        }

        #[cfg(all(any(target_os = "windows", target_os = "linux"), not(target_env = "ohos")))]
        item! {
            render_title(ui, tl!("item-fullscreen"), None);
            render_switch(ui, rr, t, &mut self.fullscreen_btn, config.fullscreen_mode);
        }

        item! {
            render_title(ui, tl!("item-offline"), Some(tl!("item-offline-sub")));
            render_switch(ui, rr, t, &mut self.offline_btn, config.offline_mode);
        }
        item! {
            render_title(ui, tl!("item-server-status"), Some(tl!("item-server-status-sub")));
            self.server_status_btn.render_text(ui, rr, t, tl!("check-status"), 0.5, true);
        }
        item! {
            render_title(ui, tl!("item-mp"), Some(tl!("item-mp-sub")));
            render_switch(ui, rr, t, &mut self.mp_btn, config.mp_enabled);
        }
        item! {
            render_title(ui, tl!("item-mp-addr"), Some(tl!("item-mp-addr-sub")));
            let text = config
                .mp_servers
                .iter()
                .find(|server| server.address == config.mp_address)
                .map(|server| format!("{} - {}", server.name, server.address))
                .unwrap_or_else(|| tl!("mp-server-none").into_owned());
            self.mp_server_btn.render_text(ui, rr, t, text, 0.32, false);
        }
        item! {
            render_title(ui, tl!("item-prefer-reduced-motion"), Some(tl!("item-prefer-reduced-motion-sub")));
            render_switch(ui, rr, t, &mut self.prefer_reduced_motion_btn, data.prefer_reduced_motion);
        }
        #[cfg(not(target_env = "ohos"))]
        item! {
            render_title(ui, tl!("item-lowq"), Some(tl!("item-lowq-sub")));
            render_switch(ui, rr, t, &mut self.lowq_btn, config.sample_count == 1);
        }
        item! {
            let cache_size = if let Some(size) = self.cache_size {
                Cow::Owned(tl!("item-cache-size", "size" => ByteSize(size).to_string()))
            } else {
                tl!("item-cache-size-loading")
            };
            render_title(ui, tl!("item-clear-cache"), Some(cache_size));
            self.cache_btn.render_text(ui, rr, t, tl!("item-clear-cache-btn"), 0.5, true);
        }
        ui.dy(0.04);
        h += 0.04;
        item! {
            render_title(ui, tl!("item-enable-anys"), Some(tl!("item-enable-anys-sub")));
            render_switch(ui, rr, t, &mut self.enable_anys_btn, data.enable_anys);
        }
        item! {
            render_title(ui, tl!("item-anys-gateway"), Some(tl!("item-anys-gateway-sub")));
            self.anys_gateway_btn.render_text(ui, rr, t, &data.anys_gateway, 0.4, false);
        }
        item! {
            render_title(ui, tl!("item-watch-tutorial"), Some(tl!("item-watch-tutorial-sub")));
            self.watch_tutorial_btn.render_text(ui, rr, t, tl!("item-watch-tutorial-btn"), 0.5, true);
        }
        self.lang_btn.render_top(ui, t, 1.);
        self.ui_layout_btn.render_top(ui, t, 1.);
        (w, h)
    }

    pub fn next_page(&mut self) -> Option<NextPage> {
        self.next_page.take()
    }
}

struct AudioList {
    adjust_btn: DRectButton,
    music_slider: Slider,
    sfx_slider: Slider,
    bgm_slider: Slider,
    cali_btn: DRectButton,
    #[cfg(not(target_os = "android"))]
    preferred_sample_rate_btn: DRectButton,
    #[cfg(target_env = "ohos")]
    audio_buffer_size_btn: DRectButton,
    cali_task: LocalTask<Result<OffsetPage>>,
    next_page: Option<NextPage>,
}

impl AudioList {
    pub fn new() -> Self {
        Self {
            adjust_btn: DRectButton::new(),
            music_slider: Slider::new(0.0..2.0, 0.05),
            sfx_slider: Slider::new(0.0..2.0, 0.05),
            bgm_slider: Slider::new(0.0..2.0, 0.05),
            cali_btn: DRectButton::new(),
            #[cfg(not(target_os = "android"))]
            preferred_sample_rate_btn: DRectButton::new(),
            #[cfg(target_env = "ohos")]
            audio_buffer_size_btn: DRectButton::new(),

            cali_task: None,
            next_page: None,
        }
    }

    pub fn top_touch(&mut self, _touch: &Touch, _t: f32) -> bool {
        false
    }

    pub fn touch(&mut self, touch: &Touch, t: f32) -> Result<Option<bool>> {
        let data = get_data_mut();
        let config = &mut data.config;
        if self.adjust_btn.touch(touch, t) {
            config.adjust_time ^= true;
            return Ok(Some(true));
        }
        if let wt @ Some(_) = self.music_slider.touch(touch, t, &mut config.volume_music) {
            return Ok(wt);
        }
        if let wt @ Some(_) = self.sfx_slider.touch(touch, t, &mut config.volume_sfx) {
            return Ok(wt);
        }
        let old = config.volume_bgm;
        if let wt @ Some(_) = self.bgm_slider.touch(touch, t, &mut config.volume_bgm) {
            if (config.volume_bgm - old).abs() > 0.001 {
                BGM_VOLUME_UPDATED.store(true, Ordering::Relaxed);
            }
            return Ok(wt);
        }
        if self.cali_btn.touch(touch, t) {
            self.cali_task = Some(Box::pin(OffsetPage::new()));
            return Ok(Some(false));
        }
        #[cfg(not(target_os = "android"))]
        if self.preferred_sample_rate_btn.touch(touch, t) {
            let options = [None, Some(44100), Some(48000), Some(88200), Some(96000), Some(192000)];
            let current = config.preferred_sample_rate;
            let selected = options.iter().position(|&r| r == current).unwrap_or(0);
            config.preferred_sample_rate = options[(selected + 1) % options.len()];
            return Ok(Some(true));
        }
        #[cfg(target_env = "ohos")]
        if self.audio_buffer_size_btn.touch(touch, t) {
            let options = [128u32, 256u32, 512u32];
            let current = config.audio_buffer_size.unwrap_or(256);
            let selected = options.iter().position(|&r| r == current).unwrap_or(1);
            config.audio_buffer_size = Some(options[(selected + 1) % options.len()]);
            return Ok(Some(true));
        }
        Ok(None)
    }

    pub fn update(&mut self, _t: f32) -> Result<bool> {
        if let Some(task) = &mut self.cali_task {
            if let Some(res) = poll_future(task.as_mut()) {
                match res {
                    Err(err) => show_error(err.context(tl!("load-cali-failed"))),
                    Ok(page) => {
                        self.next_page = Some(NextPage::Overlay(Box::new(page)));
                    }
                }
                self.cali_task = None;
            }
        }
        Ok(false)
    }

    pub fn render(&mut self, ui: &mut Ui, r: Rect, t: f32) -> (f32, f32) {
        let w = r.w;
        let mut h = 0.;
        macro_rules! item {
            ($($b:tt)*) => {{
                $($b)*
                ui.dy(ITEM_HEIGHT);
                h += ITEM_HEIGHT;
            }}
        }
        let rr = right_rect(w);

        let data = get_data();
        let config = &data.config;
        item! {
            render_title(ui, tl!("item-adjust"), Some(tl!("item-adjust-sub")));
            render_switch(ui, rr, t, &mut self.adjust_btn, config.adjust_time);
        }
        item! {
            render_title(ui, tl!("item-music"), None);
            self.music_slider.render(ui, rr, t, config.volume_music, format!("{:.2}", config.volume_music));
        }
        item! {
            render_title(ui, tl!("item-sfx"), None);
            self.sfx_slider.render(ui, rr, t, config.volume_sfx, format!("{:.2}", config.volume_sfx));
        }
        item! {
            render_title(ui, tl!("item-bgm"), None);
            self.bgm_slider.render(ui, rr, t, config.volume_bgm, format!("{:.2}", config.volume_bgm));
        }
        item! {
            render_title(ui, tl!("item-cali"), None);
            self.cali_btn.render_text(ui, rr, t, format!("{:.0}ms", config.offset * 1000.), 0.5, true);
        }
        #[cfg(not(target_os = "android"))]
        item! {
            render_title(ui, tl!("item-preferred-sample-rate"), None);
            let text = if let Some(rate) = config.preferred_sample_rate {
                format!("{} Hz", rate)
            } else {
                tl!("preferred-sample-rate-default").to_string()
            };
            self.preferred_sample_rate_btn.render_text(ui, rr, t, text, 0.5, false);
        }
        #[cfg(target_env = "ohos")]
        item! {
            render_title(ui, tl!("item-audio-buffer-size"), None);
            let buf_size = config.audio_buffer_size.unwrap_or(256);
            self.audio_buffer_size_btn.render_text(ui, rr, t, format!("{}", buf_size), 0.5, false);
        }
        (w, h)
    }

    pub fn next_page(&mut self) -> Option<NextPage> {
        self.next_page.take()
    }
}

struct ChartList {
    show_acc_btn: DRectButton,
    ap_fc_indicator_btn: DRectButton,
    show_avg_fps_btn: DRectButton,
    perf_monitor_btn: DRectButton,
    dc_pause_btn: DRectButton,
    dhint_btn: DRectButton,
    opt_btn: DRectButton,
    use_keyboard_btn: DRectButton,
    #[cfg(not(target_os = "android"))]
    vsync_btn: DRectButton,
    #[cfg(target_os = "windows")]
    smtc_btn: DRectButton,
    dynamic_bg_btn: ChooseButton,
    auto_record_btn: DRectButton,
    speed_slider: Slider,
    size_slider: Slider,
}

impl ChartList {
    pub fn new() -> Self {
        Self {
            show_acc_btn: DRectButton::new(),
            ap_fc_indicator_btn: DRectButton::new(),
            show_avg_fps_btn: DRectButton::new(),
            perf_monitor_btn: DRectButton::new(),
            dc_pause_btn: DRectButton::new(),
            dhint_btn: DRectButton::new(),
            opt_btn: DRectButton::new(),
            use_keyboard_btn: DRectButton::new(),
            #[cfg(not(target_os = "android"))]
            vsync_btn: DRectButton::new(),
            #[cfg(target_os = "windows")]
            smtc_btn: DRectButton::new(),
            dynamic_bg_btn: ChooseButton::new()
                .with_options(vec![
                    tl!("dynamic-bg-off").to_string(),
                    tl!("dynamic-bg-static").to_string(),
                    tl!("dynamic-bg-dynamic").to_string(),
                ])
                .with_selected(get_data().config.dynamic_background.as_u8() as usize),
            auto_record_btn: DRectButton::new(),
            speed_slider: Slider::new(0.5..2., 0.05),
            size_slider: Slider::new(0.8..1.2, 0.005),
        }
    }

    pub fn top_touch(&mut self, touch: &Touch, t: f32) -> bool {
        self.dynamic_bg_btn.top_touch(touch, t)
    }

    pub fn touch(&mut self, touch: &Touch, t: f32) -> Result<Option<bool>> {
        let data = get_data_mut();
        let config = &mut data.config;
        if self.show_acc_btn.touch(touch, t) {
            config.show_acc ^= true;
            return Ok(Some(true));
        }
        if self.ap_fc_indicator_btn.touch(touch, t) {
            config.ap_fc_indicator ^= true;
            return Ok(Some(true));
        }
        if self.show_avg_fps_btn.touch(touch, t) {
            config.show_avg_fps ^= true;
            return Ok(Some(true));
        }
        if self.perf_monitor_btn.touch(touch, t) {
            config.performance_monitor ^= true;
            return Ok(Some(true));
        }
        if self.dc_pause_btn.touch(touch, t) {
            config.double_click_to_pause ^= true;
            return Ok(Some(true));
        }
        if self.dhint_btn.touch(touch, t) {
            config.double_hint ^= true;
            return Ok(Some(true));
        }
        if self.opt_btn.touch(touch, t) {
            config.aggressive ^= true;
            return Ok(Some(true));
        }
        if self.use_keyboard_btn.touch(touch, t) {
            config.use_keyboard ^= true;
            return Ok(Some(true));
        }
        #[cfg(target_os = "windows")]
        if self.smtc_btn.touch(touch, t) {
            config.smtc_integration ^= true;
            return Ok(Some(true));
        }
        #[cfg(not(target_os = "android"))]
        if self.vsync_btn.touch(touch, t) {
            config.vsync ^= true;
            prpr::set_swap_interval(if config.vsync { 1 } else { 0 });
            return Ok(Some(true));
        }
        if self.dynamic_bg_btn.touch(touch, t) {
            return Ok(Some(false));
        }
        if self.auto_record_btn.touch(touch, t) {
            config.auto_record ^= true;
            return Ok(Some(true));
        }
        if let wt @ Some(_) = self.speed_slider.touch(touch, t, &mut config.speed) {
            return Ok(wt);
        }
        if let wt @ Some(_) = self.size_slider.touch(touch, t, &mut config.note_scale) {
            return Ok(wt);
        }
        Ok(None)
    }

    pub fn update(&mut self, t: f32) -> Result<bool> {
        self.dynamic_bg_btn.update(t);
        if self.dynamic_bg_btn.changed() {
            let data = get_data_mut();
            data.config.dynamic_background = match self.dynamic_bg_btn.selected() {
                0 => DynamicBackgroundMode::Off,
                1 => DynamicBackgroundMode::StaticBrightness,
                _ => DynamicBackgroundMode::DynamicBrightness,
            };
            return Ok(true);
        }
        Ok(false)
    }

    pub fn render(&mut self, ui: &mut Ui, r: Rect, t: f32) -> (f32, f32) {
        let w = r.w;
        let mut h = 0.;
        macro_rules! item {
            ($($b:tt)*) => {{
                $($b)*
                ui.dy(ITEM_HEIGHT);
                h += ITEM_HEIGHT;
            }}
        }
        let rr = right_rect(w);

        let data = get_data();
        let config = &data.config;
        item! {
            render_title(ui, tl!("item-show-acc"), None);
            render_switch(ui, rr, t, &mut self.show_acc_btn, config.show_acc);
        }
        item! {
            render_title(ui, tl!("item-ap-fc-indicator"), Some(tl!("item-ap-fc-indicator-sub")));
            render_switch(ui, rr, t, &mut self.ap_fc_indicator_btn, config.ap_fc_indicator);
        }
        item! {
            render_title(ui, tl!("item-dynamic-bg"), Some(tl!("item-dynamic-bg-sub")));
            self.dynamic_bg_btn.render(ui, rr, t);
        }
        item! {
            render_title(ui, tl!("item-show-avg-fps"), Some(tl!("item-show-avg-fps-sub")));
            render_switch(ui, rr, t, &mut self.show_avg_fps_btn, config.show_avg_fps);
        }
        item! {
            render_title(ui, tl!("item-perf-monitor"), Some(tl!("item-perf-monitor-sub")));
            render_switch(ui, rr, t, &mut self.perf_monitor_btn, config.performance_monitor);
        }
        item! {
            render_title(ui, tl!("item-dc-pause"), None);
            render_switch(ui, rr, t, &mut self.dc_pause_btn, config.double_click_to_pause);
        }
        item! {
            render_title(ui, tl!("item-dhint"), Some(tl!("item-dhint-sub")));
            render_switch(ui, rr, t, &mut self.dhint_btn, config.double_hint);
        }
        item! {
            render_title(ui, tl!("item-opt"), Some(tl!("item-opt-sub")));
            render_switch(ui, rr, t, &mut self.opt_btn, config.aggressive);
        }
        item! {
            render_title(ui, tl!("item-use-keyboard"), Some(tl!("item-use-keyboard-sub")));
            render_switch(ui, rr, t, &mut self.use_keyboard_btn, config.use_keyboard);
        }
        #[cfg(target_os = "windows")]
        item! {
            render_title(ui, "兼容系统播放控件", Some(Cow::Borrowed("将游玩中的谱面音乐接入系统播放控件（SMTC），此时你将可以通过系统播放控件控制谱面播放，也可以使用Lyricify等软件显示歌词")));
            render_switch(ui, rr, t, &mut self.smtc_btn, config.smtc_integration);
        }
        #[cfg(not(target_os = "android"))]
        item! {
            render_title(ui, "垂直同步", Some(Cow::Borrowed("关闭此选项可以适当提升游戏帧率和降低输入延迟，但是会导致更高的资源占用，配置较差的设备可能导致画面撕裂")));
            render_switch(ui, rr, t, &mut self.vsync_btn, config.vsync);
        }
        item! {
            render_title(ui, tl!("item-auto-record"), Some(tl!("item-auto-record-sub")));
            render_switch(ui, rr, t, &mut self.auto_record_btn, config.auto_record);
        }
        item! {
            render_title(ui, tl!("item-speed"), None);
            self.speed_slider.render(ui, rr, t, config.speed, format!("{:.2}", config.speed));
        }
        item! {
            render_title(ui, tl!("item-note-size"), None);
            self.size_slider.render(ui, rr, t, config.note_scale, format!("{:.3}", config.note_scale));
        }
        self.dynamic_bg_btn.render_top(ui, t, 1.);
        (w, h)
    }
}

struct DebugList {
    chart_debug_btn: DRectButton,
    touch_debug_btn: DRectButton,
    #[cfg(target_os = "windows")]
    debug_console_btn: DRectButton,
    log_overlay_btn: DRectButton,
    startup_post_btn: DRectButton,
    crash_btn: DRectButton,
}

impl DebugList {
    pub fn new() -> Self {
        Self {
            chart_debug_btn: DRectButton::new(),
            touch_debug_btn: DRectButton::new(),
            #[cfg(target_os = "windows")]
            debug_console_btn: DRectButton::new(),
            log_overlay_btn: DRectButton::new(),
            startup_post_btn: DRectButton::new(),
            crash_btn: DRectButton::new(),
        }
    }

    pub fn top_touch(&mut self, _touch: &Touch, _t: f32) -> bool {
        false
    }

    pub fn touch(&mut self, touch: &Touch, t: f32) -> Result<Option<bool>> {
        let data = get_data_mut();
        let config = &mut data.config;
        if self.chart_debug_btn.touch(touch, t) {
            config.chart_debug ^= true;
            return Ok(Some(true));
        }
        if self.touch_debug_btn.touch(touch, t) {
            config.touch_debug ^= true;
            return Ok(Some(true));
        }
        #[cfg(target_os = "windows")]
        if self.debug_console_btn.touch(touch, t) {
            config.debug_console ^= true;
            return Ok(Some(true));
        }
        if self.log_overlay_btn.touch(touch, t) {
            config.log_overlay ^= true;
            return Ok(Some(true));
        }
        if self.startup_post_btn.touch(touch, t) {
            config.startup_post ^= true;
            return Ok(Some(true));
        }
        if self.crash_btn.touch(touch, t) {
            let reasons = [
                "Simulated render crash: OpenGL context lost",
                "Simulated audio crash: sound playback failed",
                "Simulated resource crash: file not found: assets/test.png",
                "Simulated memory crash: failed to allocate memory",
                "Simulated network crash: connection timeout",
                "Simulated unknown crash: something unexpected happened",
            ];
            let idx = (t * 1000.0) as usize % reasons.len();
            panic!("{}", reasons[idx]);
        }
        Ok(None)
    }

    pub fn update(&mut self, _t: f32) -> Result<bool> {
        Ok(false)
    }

    pub fn render(&mut self, ui: &mut Ui, r: Rect, t: f32) -> (f32, f32) {
        let w = r.w;
        let mut h = 0.;
        macro_rules! item {
            ($($b:tt)*) => {{
                $($b)*
                ui.dy(ITEM_HEIGHT);
                h += ITEM_HEIGHT;
            }}
        }
        let rr = right_rect(w);

        let data = get_data();
        let config = &data.config;
        item! {
            render_title(ui, tl!("item-chart-debug"), Some(tl!("item-chart-debug-sub")));
            render_switch(ui, rr, t, &mut self.chart_debug_btn, config.chart_debug);
        }
        item! {
            render_title(ui, tl!("item-touch-debug"), Some(tl!("item-touch-debug-sub")));
            render_switch(ui, rr, t, &mut self.touch_debug_btn, config.touch_debug);
        }
        #[cfg(target_os = "windows")]
        item! {
            render_title(ui, "启动命令框", Some(std::borrow::Cow::Borrowed("仅 Windows：启用后启动时显示命令框")));
            render_switch(ui, rr, t, &mut self.debug_console_btn, config.debug_console);
        }
        item! {
            render_title(ui, "游戏内日志叠加层", Some(std::borrow::Cow::Borrowed("全平台：启用后在游戏内显示日志叠加层")));
            render_switch(ui, rr, t, &mut self.log_overlay_btn, config.log_overlay);
        }
        item! {
            render_title(ui, "启动时校验游戏文件", Some(std::borrow::Cow::Borrowed("注意：该功能会严重拖慢启动速度")));
            render_switch(ui, rr, t, &mut self.startup_post_btn, config.startup_post);
        }
        item! {
            render_title(ui, "立即崩溃", Some(std::borrow::Cow::Borrowed("点击以随机理由触发崩溃")));
            self.crash_btn.render_text(ui, rr, t, "立即崩溃", 0.5, false);
        }
        (w, h)
    }
}
