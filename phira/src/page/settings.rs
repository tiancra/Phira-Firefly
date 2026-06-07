prpr_l10n::tl_file!("settings");

use super::{NextPage, OffsetPage, Page, SharedState};
use crate::{
    dir, get_data, get_data_mut,
    icons::Icons,
    popup::{ChooseButton, Popup},
    save_data,
    scene::{BGM_VOLUME_UPDATED, MainScene},
    sync_data,
    tabs::{Tabs, TitleFn},
};
use anyhow::Result;
use bytesize::ByteSize;
use inputbox::InputBox;
use macroquad::prelude::*;
use once_cell::sync::Lazy;
use prpr::{
    core::{BOLD_FONT, Tweenable},
    ext::{open_url, poll_future, semi_black, semi_white, LocalTask, RectExt, SafeTexture, ScaleType},
    scene::{request_input, return_input, show_error, show_message, take_input, request_file},
    task::Task,
    ui::{button_hit, button_hit_large, DRectButton, Dialog, RectButton, Scroll, Slider, Ui, PREFER_REDUCED_MOTION},
};
use prpr_l10n::{LanguageIdentifier, LANG_IDENTS, LANG_NAMES};
use reqwest::Url;
use serde::Deserialize;
use std::{borrow::Cow, fs, io, net::ToSocketAddrs, path::PathBuf, sync::atomic::Ordering, sync::Arc};

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
    Themes,
    About,
}

pub struct SettingsPage {
    list_general: GeneralList,
    list_audio: AudioList,
    list_chart: ChartList,
    list_debug: DebugList,
    list_themes: ThemesList,

    tabs: Tabs<SettingListType>,

    scroll: Scroll,
    save_time: f32,

    icon: SafeTexture,
}

impl SettingsPage {
    const SAVE_TIME: f32 = 0.5;

    pub fn new(icon: SafeTexture, icon_lang: SafeTexture, icons: Arc<Icons>) -> Self {
        Self {
            list_general: GeneralList::new(icon_lang),
            list_audio: AudioList::new(),
            list_chart: ChartList::new(),
            list_debug: DebugList::new(),
            list_themes: ThemesList::new(icons),

            tabs: Tabs::new([
                (SettingListType::General, || tl!("general")),
                (SettingListType::Audio, || tl!("audio")),
                (SettingListType::Chart, || tl!("chart")),
                (SettingListType::Debug, || tl!("debug")),
                (SettingListType::Themes, || tl!("themes")),
                (SettingListType::About, || tl!("about")),
            ] as [(SettingListType, TitleFn); 6]),

            scroll: Scroll::new(),
            save_time: f32::INFINITY,

            icon,
        }
    }
}

impl Page for SettingsPage {
    fn label(&self) -> Cow<'static, str> {
        tl!("label")
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
        
        if match self.tabs.selected() {
            SettingListType::General => self.list_general.top_touch(touch, t),
            SettingListType::Audio => self.list_audio.top_touch(touch, t),
            SettingListType::Chart => self.list_chart.top_touch(touch, t),
            SettingListType::Debug => self.list_debug.top_touch(touch, t),
            SettingListType::Themes => self.list_themes.top_touch(touch, t),
            SettingListType::About => false,
        } {
            return Ok(true);
        }

        if self.tabs.touch(touch, s.rt) {
            return Ok(true);
        }

        // 先处理主题列表的触摸（在 scroll 之前）
        if let Some(p) = match self.tabs.selected() {
            SettingListType::General => self.list_general.touch(touch, t)?,
            SettingListType::Audio => self.list_audio.touch(touch, t)?,
            SettingListType::Chart => self.list_chart.touch(touch, t)?,
            SettingListType::Debug => self.list_debug.touch(touch, t)?,
            SettingListType::Themes => self.list_themes.touch(touch, t)?,
            SettingListType::About => None,
        } {
            if p {
                self.save_time = t;
            }
            self.scroll.y_scroller.halt();
            return Ok(true);
        }

        if self.scroll.touch(touch, t) {
            return Ok(true);
        }
        Ok(false)
    }

    fn update(&mut self, s: &mut SharedState) -> Result<()> {
        let t = s.t;
        
        // 如果切换到了 Themes 选项卡，刷新主题列表
        if matches!(self.tabs.selected(), SettingListType::Themes) {
            // 简单检查是否需要刷新（比如刚导入了主题）
            if let Some(_theme_info) = MainScene::take_imported_theme() {
                self.list_themes.refresh()?;
            }
        }
        
        let changed = match self.tabs.selected() {
            SettingListType::General => self.list_general.update(t)?,
            SettingListType::Audio => self.list_audio.update(t)?,
            SettingListType::Chart => self.list_chart.update(t)?,
            SettingListType::Debug => self.list_debug.update(t)?,
            SettingListType::Themes => self.list_themes.update(t)?,
            SettingListType::About => false,
        };
        self.scroll.update(t);
        if changed {
            self.save_time = t;
        }
        if t > self.save_time + Self::SAVE_TIME {
            save_data()?;
            self.save_time = f32::INFINITY;
        }
        Ok(())
    }

    fn render(&mut self, ui: &mut Ui, s: &mut SharedState) -> Result<()> {
        let t = s.t;
        let rt = s.rt;

        let content_r = ui.content_rect();
        
        // 如果主题正在展开或已经展开，则直接渲染主题展开页面
        if matches!(self.tabs.selected(), SettingListType::Themes) && (self.list_themes.expanded_index.is_some() || self.list_themes.transit.is_some()) {
            // 只渲染设置页面的背景内容，不渲染侧边栏
            s.fader.render(ui, s.t, |ui| {
                let r = content_r.feather(-0.01);
                self.scroll.size((r.w, r.h));
                ui.scope(|ui| {
                    ui.dx(r.x);
                    ui.dy(r.y);
                    self.scroll.render(ui, |_ui| {
                        // 不渲染任何内容，只是保持结构
                        (r.w, r.h)
                    });
                });
            });
            
            // 然后渲染主题展开页面（全屏）
            let full_screen_r = ui.screen_rect();
            self.list_themes.render(ui, full_screen_r, t);
            
            // 渲染顶部过渡动画
            self.list_themes.render_top(ui, t);
            
            return Ok(());
        }
        
        // 正常渲染设置页面
        s.fader.render(ui, s.t, |ui| {
            let r = content_r;
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
                        SettingListType::Themes => self.list_themes.render(ui, r, t),
                        SettingListType::About => render_about(ui, r, &self.icon),
                    });
                });
                Ok(())
            })
        })?;

        // 渲染主题列表的顶部过渡动画
        if matches!(self.tabs.selected(), SettingListType::Themes) {
            self.list_themes.render_top(ui, t);
        }

        Ok(())
    }

    fn next_page(&mut self) -> NextPage {
        if matches!(self.tabs.selected(), SettingListType::Audio) {
            return self.list_audio.next_page().unwrap_or_default();
        }
        NextPage::None
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

    #[cfg(all(any(target_os = "windows", target_os = "linux"), not(target_env = "ohos")))]
    fullscreen_btn: DRectButton,

    cache_btn: DRectButton,
    offline_btn: DRectButton,
    server_status_btn: DRectButton,
    mp_btn: DRectButton,
    mp_addr_btn: DRectButton,
    #[cfg(not(target_env = "ohos"))]
    lowq_btn: DRectButton,
    prefer_reduced_motion_btn: DRectButton,
    insecure_btn: DRectButton,
    enable_anys_btn: DRectButton,
    anys_gateway_btn: DRectButton,

    cache_size: Option<u64>,
    cache_task: Option<Task<Result<u64>>>,
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
                        .and_then(|it| it.parse::<LanguageIdentifier>().ok())
                        .and_then(|ident| LANG_IDENTS.iter().position(|it| *it == ident))
                        .unwrap_or_default(),
                ),

            #[cfg(all(any(target_os = "windows", target_os = "linux"), not(target_env = "ohos")))]
            fullscreen_btn: DRectButton::new(),

            cache_btn: DRectButton::new(),
            offline_btn: DRectButton::new(),
            server_status_btn: DRectButton::new(),
            mp_btn: DRectButton::new(),
            mp_addr_btn: DRectButton::new(),
            #[cfg(not(target_env = "ohos"))]
            lowq_btn: DRectButton::new(),
            prefer_reduced_motion_btn: DRectButton::new(),
            insecure_btn: DRectButton::new(),
            enable_anys_btn: DRectButton::new(),
            anys_gateway_btn: DRectButton::new(),

            cache_size: None,
            cache_task: None,
        };
        let _ = this.update_cache_size();
        this
    }

    pub fn top_touch(&mut self, touch: &Touch, t: f32) -> bool {
        if self.lang_btn.top_touch(touch, t) {
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
        if self.mp_addr_btn.touch(touch, t) {
            request_input("mp_addr", InputBox::new().default_text(&config.mp_address));
            return Ok(Some(true));
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
        if self.insecure_btn.touch(touch, t) {
            data.accept_invalid_cert ^= true;
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
        Ok(None)
    }

    pub fn update(&mut self, t: f32) -> Result<bool> {
        self.lang_btn.update(t);
        let data = get_data_mut();
        if self.lang_btn.changed() {
            data.language = Some(LANG_IDENTS[self.lang_btn.selected()].to_string());
            sync_data();
            return Ok(true);
        }
        if let Some((id, text)) = take_input() {
            if id == "mp_addr" {
                if let Err(err) = text.to_socket_addrs() {
                    show_error(anyhow::Error::new(err).context(tl!("item-mp-addr-invalid")));
                    return Ok(false);
                } else {
                    data.config.mp_address = text;
                    return Ok(true);
                }
            } else if id == "anys_gateway" {
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
            self.mp_addr_btn.render_text(ui, rr, t, &config.mp_address, 0.4, false);
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
            render_title(ui, tl!("item-insecure"), Some(tl!("item-insecure-sub")));
            render_switch(ui, rr, t, &mut self.insecure_btn, data.accept_invalid_cert);
        }
        item! {
            render_title(ui, tl!("item-enable-anys"), Some(tl!("item-enable-anys-sub")));
            render_switch(ui, rr, t, &mut self.enable_anys_btn, data.enable_anys);
        }
        item! {
            render_title(ui, tl!("item-anys-gateway"), Some(tl!("item-anys-gateway-sub")));
            self.anys_gateway_btn.render_text(ui, rr, t, &data.anys_gateway, 0.4, false);
        }
        self.lang_btn.render_top(ui, t, 1.);
        (w, h)
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
    dc_pause_btn: DRectButton,
    dhint_btn: DRectButton,
    opt_btn: DRectButton,
    use_keyboard_btn: DRectButton,
    speed_slider: Slider,
    size_slider: Slider,
}

impl ChartList {
    pub fn new() -> Self {
        Self {
            show_acc_btn: DRectButton::new(),
            ap_fc_indicator_btn: DRectButton::new(),
            show_avg_fps_btn: DRectButton::new(),
            dc_pause_btn: DRectButton::new(),
            dhint_btn: DRectButton::new(),
            opt_btn: DRectButton::new(),
            use_keyboard_btn: DRectButton::new(),
            speed_slider: Slider::new(0.5..2., 0.05),
            size_slider: Slider::new(0.8..1.2, 0.005),
        }
    }

    pub fn top_touch(&mut self, _touch: &Touch, _t: f32) -> bool {
        false
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
        if let wt @ Some(_) = self.speed_slider.touch(touch, t, &mut config.speed) {
            return Ok(wt);
        }
        if let wt @ Some(_) = self.size_slider.touch(touch, t, &mut config.note_scale) {
            return Ok(wt);
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
            render_title(ui, tl!("item-show-acc"), None);
            render_switch(ui, rr, t, &mut self.show_acc_btn, config.show_acc);
        }
        item! {
            render_title(ui, tl!("item-ap-fc-indicator"), Some(tl!("item-ap-fc-indicator-sub")));
            render_switch(ui, rr, t, &mut self.ap_fc_indicator_btn, config.ap_fc_indicator);
        }
        item! {
            render_title(ui, tl!("item-show-avg-fps"), Some(tl!("item-show-avg-fps-sub")));
            render_switch(ui, rr, t, &mut self.show_avg_fps_btn, config.show_avg_fps);
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
        item! {
            render_title(ui, tl!("item-speed"), None);
            self.speed_slider.render(ui, rr, t, config.speed, format!("{:.2}", config.speed));
        }
        item! {
            render_title(ui, tl!("item-note-size"), None);
            self.size_slider.render(ui, rr, t, config.note_scale, format!("{:.3}", config.note_scale));
        }
        (w, h)
    }
}

struct DebugList {
    chart_debug_btn: DRectButton,
    touch_debug_btn: DRectButton,
}

impl DebugList {
    pub fn new() -> Self {
        Self {
            chart_debug_btn: DRectButton::new(),
            touch_debug_btn: DRectButton::new(),
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
        (w, h)
    }
}

#[derive(Clone)]
struct ThemeItem {
    name: String,
    version: Option<String>,
    description: Option<String>,
    cover: Option<SafeTexture>,
    btn: DRectButton,
    // 主题文件夹路径（None 表示默认主题）
    folder_path: Option<PathBuf>,
}

fn transit_time() -> Option<f32> {
    if get_data().prefer_reduced_motion {
        None
    } else {
        Some(0.4)
    }
}

struct TransitState {
    id: u32,
    rect: Option<Rect>,
    theme: ThemeItem,
    start_time: f32,
    back: bool,
    done: bool,
}

struct ThemesList {
    import_btn: DRectButton,
    themes: Vec<ThemeItem>,
    // 过渡状态
    transit: Option<TransitState>,
    back_fade_in: Option<(u32, f32)>,
    // 展开状态
    expanded_index: Option<usize>,
    back_btn: RectButton,
    apply_btn: DRectButton,
    more_btn: DRectButton,
    more_menu: Popup,
    need_show_more_menu: bool,
    // 当前应用的主题索引（0为默认主题）
    applied_index: usize,
    // 图标
    icons: Arc<Icons>,
}

impl ThemesList {
    pub fn new(icons: Arc<Icons>) -> Self {
        let mut themes = Vec::new();
        
        // 从配置文件读取当前应用的主题ID（文件夹名称）
        let saved_theme_id = get_data().theme.clone();
        let mut applied_index = 0;
        
        // 扫描 data/themes 目录下的所有主题文件夹
        if let Ok(root) = dir::root() {
            let themes_dir = format!("{}/themes", root);
            if let Ok(entries) = std::fs::read_dir(&themes_dir) {
                // 简单解析 json，获取 name, version, description
                #[derive(Deserialize)]
                struct ThemeManifest {
                    name: String,
                    version: Option<String>,
                    description: Option<String>,
                }
                
                for entry in entries {
                    if let Ok(entry) = entry {
                        let path = entry.path();
                        if path.is_dir() {
                            let folder_name = entry.file_name().to_string_lossy().to_string();
                            // 跳过current目录
                            if folder_name == "current" {
                                continue;
                            }
                            let manifest_path = path.join("manifest.json");
                            if let Ok(manifest_content) = std::fs::read_to_string(&manifest_path) {
                                if let Ok(manifest) = serde_json::from_str::<ThemeManifest>(&manifest_content) {
                                    let cover_path = path.join("cover.jpg");
                                    let cover = if cover_path.exists() {
                                        if let Ok(bytes) = std::fs::read(&cover_path) {
                                            if let Ok(img) = image::load_from_memory(&bytes) {
                                                Some(SafeTexture::from(img))
                                            } else {
                                                None
                                            }
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    };
                                    
                                    // 检查是否是当前应用的主题
                                    if folder_name == saved_theme_id {
                                        applied_index = themes.len();
                                    }
                                    
                                    themes.push(ThemeItem {
                                        name: manifest.name,
                                        version: manifest.version,
                                        description: manifest.description,
                                        cover,
                                        btn: DRectButton::new(),
                                        folder_path: Some(path),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
        
        // 如果没有找到任何主题，添加默认主题
        if themes.is_empty() {
            let default_cover = Self::load_default_cover();
            themes.push(ThemeItem {
                name: tl!("default-theme").to_string(),
                version: None,
                description: None,
                cover: default_cover,
                btn: DRectButton::new(),
                folder_path: None,
            });
        }

        Self {
            import_btn: DRectButton::new(),
            themes,
            transit: None,
            back_fade_in: None,
            expanded_index: None,
            back_btn: RectButton::new(),
            apply_btn: DRectButton::new(),
            more_btn: DRectButton::new(),
            more_menu: Popup::new(),
            need_show_more_menu: false,
            applied_index,
            icons,
        }
    }

    fn load_default_cover() -> Option<SafeTexture> {
        let Ok(root) = dir::root() else {
            return None;
        };
        let path = format!("{}/themes/Default/cover.jpg", root);
        let Ok(bytes) = std::fs::read(&path) else {
            return None;
        };
        let Ok(img) = image::load_from_memory(&bytes) else {
            return None;
        };
        Some(img.into())
    }
    
    pub fn refresh(&mut self) -> Result<()> {
        let mut themes = Vec::new();
        
        // 从配置文件读取当前应用的主题ID（文件夹名称）
        let saved_theme_id = get_data().theme.clone();
        let mut applied_index = 0;
        
        // 扫描 data/themes 目录下的所有主题文件夹
        if let Ok(root) = dir::root() {
            let themes_dir = format!("{}/themes", root);
            if let Ok(entries) = std::fs::read_dir(&themes_dir) {
                // 简单解析 json，获取 name, version, description
                #[derive(Deserialize)]
                struct ThemeManifest {
                    name: String,
                    version: Option<String>,
                    description: Option<String>,
                }
                
                for entry in entries {
                    if let Ok(entry) = entry {
                        let path = entry.path();
                        if path.is_dir() {
                            let folder_name = entry.file_name().to_string_lossy().to_string();
                            // 跳过current目录
                            if folder_name == "current" {
                                continue;
                            }
                            let manifest_path = path.join("manifest.json");
                            if let Ok(manifest_content) = std::fs::read_to_string(&manifest_path) {
                                if let Ok(manifest) = serde_json::from_str::<ThemeManifest>(&manifest_content) {
                                    let cover_path = path.join("cover.jpg");
                                    let cover = if cover_path.exists() {
                                        if let Ok(bytes) = std::fs::read(&cover_path) {
                                            if let Ok(img) = image::load_from_memory(&bytes) {
                                                Some(SafeTexture::from(img))
                                            } else {
                                                None
                                            }
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    };
                                    
                                    // 检查是否是当前应用的主题
                                    if folder_name == saved_theme_id {
                                        applied_index = themes.len();
                                    }
                                    
                                    themes.push(ThemeItem {
                                        name: manifest.name,
                                        version: manifest.version,
                                        description: manifest.description,
                                        cover,
                                        btn: DRectButton::new(),
                                        folder_path: Some(path),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
        
        // 如果没有找到任何主题，添加默认主题
        if themes.is_empty() {
            let default_cover = Self::load_default_cover();
            themes.push(ThemeItem {
                name: tl!("default-theme").to_string(),
                version: None,
                description: None,
                cover: default_cover,
                btn: DRectButton::new(),
                folder_path: None,
            });
        }
        
        // 更新 applied_index
        self.applied_index = applied_index;
        
        self.themes = themes;
        self.expanded_index = None;
        
        Ok(())
    }
    
    // 删除主题
    fn delete_theme(&mut self, index: usize) -> Result<()> {
        if let Some(theme) = self.themes.get(index) {
            // 如果是默认主题（文件夹ID为"Default"），不删除
            let is_default_theme = theme.folder_path.as_ref()
                .map(|p| p.file_name().unwrap_or_default().to_string_lossy() == "Default")
                .unwrap_or(false);
            if is_default_theme {
                return Ok(());
            }
            
            // 删除主题文件夹
            if let Some(folder_path) = &theme.folder_path {
                if let Err(e) = std::fs::remove_dir_all(folder_path) {
                    eprintln!("Failed to delete theme folder: {:?}", e);
                }
            }
            
            // 如果删除的是当前应用的主题，重置应用默认主题
            if index == self.applied_index {
                self.applied_index = 0;
                // 保存到配置文件（使用文件夹名称"Default"）
                get_data_mut().theme = "Default".to_string();
                // 应用默认主题（清空current目录）
                let _ = crate::apply_theme("Default");
            } else if index < self.applied_index {
                // 如果删除的是当前应用主题之前的主题，调整索引
                self.applied_index -= 1;
                // 获取新的应用主题的文件夹名称并保存
                let new_theme_folder = self.themes[self.applied_index].folder_path.as_ref()
                    .map(|p| p.file_name().unwrap_or_default().to_string_lossy().to_string())
                    .unwrap_or_else(|| "Default".to_string());
                get_data_mut().theme = new_theme_folder.clone();
                // 应用新的主题
                let _ = crate::apply_theme(&new_theme_folder);
            }
            
            // 从列表中移除
            self.themes.remove(index);
            self.expanded_index = None;
            
            // 同步保存配置
            let _ = crate::save_data();
        }
        Ok(())
    }

    pub fn top_touch(&mut self, _touch: &Touch, _t: f32) -> bool {
        false
    }

    pub fn touch(&mut self, touch: &Touch, t: f32) -> Result<Option<bool>> {
        // 处理展开页面的触摸（包括过渡期间）
        if let Some(index) = self.expanded_index {
            let theme = &self.themes[index];
            
            // 处理更多菜单
            if self.more_menu.showing() {
                let was_showing = self.more_menu.showing();
                let selected_before = self.more_menu.selected();
                self.more_menu.touch(touch, t);
                let selected_after = self.more_menu.selected();
                
                // 如果菜单选项被选中（selected 从 usize::MAX 变成了有效的索引）
                if selected_before == usize::MAX && selected_after != usize::MAX {
                    button_hit();
                    let selected = selected_after;
                    // 根据是否是默认主题来判断选项的含义
                    // 默认主题的文件夹ID为"Default"
                    let is_default_theme = theme.folder_path.as_ref()
                        .map(|p| p.file_name().unwrap_or_default().to_string_lossy() == "Default")
                        .unwrap_or(false);
                    
                    if is_default_theme {
                        // 默认主题只有"关于"选项（选项0）
                        if selected == 0 {
                            // 显示关于信息
                            let version = theme.version.clone().unwrap_or_else(|| "未知".to_string());
                            let description = theme.description.clone().unwrap_or_else(|| "无描述".to_string());
                            Dialog::plain(
                                tl!("about"),
                                tl!("theme-about-content", 
                                    "name" => theme.name.clone(),
                                    "version" => version,
                                    "desc" => description
                                ),
                            )
                            .listener(|_dialog, pos| pos == -2)
                            .show();
                        }
                    } else {
                        // 其他主题有"删除"（选项0）和"关于"（选项1）
                        if selected == 0 {
                            // 删除选项
                            // 删除主题文件夹
                            let _ = self.delete_theme(index);
                            // 刷新列表
                            let _ = self.refresh();
                        } else if selected == 1 {
                            // 关于选项
                            // 显示关于信息
                            let version = theme.version.clone().unwrap_or_else(|| "未知".to_string());
                            let description = theme.description.clone().unwrap_or_else(|| "无描述".to_string());
                            Dialog::plain(
                                tl!("about"),
                                tl!("theme-about-content", 
                                    "name" => theme.name.clone(),
                                    "version" => version,
                                    "desc" => description
                                ),
                            )
                            .listener(|_dialog, pos| pos == -2)
                            .show();
                        }
                    }
                }
                
                if was_showing && !self.more_menu.showing() {
                    // 菜单刚刚关闭，重置选中状态
                    self.more_menu.set_selected(usize::MAX);
                }
                
                return Ok(Some(false));
            }
            
            // 返回按钮（即使在过渡中也可以点击）
            if self.back_btn.touch(touch) {
                button_hit();
                // 如果正在过渡到展开状态，取消过渡
                if self.transit.is_some() {
                    self.transit = None;
                } else {
                    // 初始化返回过渡
                    let theme = self.themes[index].clone();
                    self.transit = Some(TransitState {
                        id: index as u32,
                        rect: None,
                        theme,
                        start_time: t,
                        back: true,
                        done: false,
                    });
                }
                return Ok(Some(false));
            }
            
            // 应用按钮
            if self.apply_btn.touch(touch, t) {
                button_hit();
                // 应用主题
                self.applied_index = index;
                // 获取主题的文件夹名称
                let theme_folder = self.themes[index].folder_path.as_ref()
                    .map(|p| p.file_name().unwrap_or_default().to_string_lossy().to_string())
                    .unwrap_or_else(|| "Default".to_string());
                // 保存到配置文件（保存文件夹名称而不是索引）
                get_data_mut().theme = theme_folder.clone();
                // 同步保存配置（确保配置已保存）
                let _ = crate::save_data();
                // 应用主题（复制文件到current目录）
                let _ = crate::apply_theme(&theme_folder);
                
                // 弹窗提示用户需要重启应用
                Dialog::plain(
                    tl!("theme-apply"),
                    tl!("theme-need-restart"),
                )
                .buttons(vec![
                    tl!("cancel").into_owned(),
                    tl!("confirm").into_owned(),
                ])
                .listener(|_dialog, pos| {
                    if pos == 1 {
                        // 用户点击确定，关闭应用
                        std::process::exit(0);
                    }
                    false
                })
                .show();
                
                return Ok(Some(false));
            }
            
            // 更多按钮
            if self.more_btn.touch(touch, t) {
                button_hit();
                let theme = &self.themes[index];
                // 根据是否是默认主题设置不同的菜单选项
                // 默认主题的文件夹ID为"Default"
                let is_default_theme = theme.folder_path.as_ref()
                    .map(|p| p.file_name().unwrap_or_default().to_string_lossy() == "Default")
                    .unwrap_or(false);
                if is_default_theme {
                    // 默认主题只显示"关于"选项
                    self.more_menu.set_options(vec![
                        tl!("about").into_owned(),
                    ]);
                } else {
                    // 其他主题显示"删除"和"关于"选项
                    self.more_menu.set_options(vec![
                        tl!("theme-delete").into_owned(),
                        tl!("about").into_owned(),
                    ]);
                }
                self.need_show_more_menu = true;
                return Ok(Some(false));
            }
            
            return Ok(Some(false));
        }
        
        // 如果正在过渡中，不处理卡片点击
        if self.transit.is_some() {
            return Ok(Some(false));
        }
        
        // 处理导入按钮
        if self.import_btn.touch(touch, t) {
            request_file("_import_theme");
            return Ok(Some(false));
        }
        
        // 处理主题卡片点击
        for (id, theme) in &mut self.themes.iter_mut().enumerate() {
            if theme.btn.touch(touch, t) {
                button_hit_large();
                self.transit = Some(TransitState {
                    id: id as u32,
                    rect: None,
                    theme: theme.clone(),
                    start_time: t,
                    back: false,
                    done: false,
                });
                return Ok(Some(false));
            }
        }
        Ok(None)
    }

    pub fn update(&mut self, t: f32) -> Result<bool> {
        // 处理过渡状态
        if let Some(transit) = &mut self.transit {
            if t > transit.start_time + transit_time().unwrap_or_default() {
                if transit.back {
                    self.back_fade_in = Some((transit.id, t));
                    self.transit = None;
                    self.expanded_index = None;
                } else {
                    transit.done = true;
                    self.expanded_index = Some(transit.id as usize);
                    self.transit = None;
                }
            }
        }
        
        // 更新更多菜单
        self.more_menu.update(t);
        
        Ok(false)
    }

    pub fn render(&mut self, ui: &mut Ui, r: Rect, t: f32) -> (f32, f32) {
        let w = r.w;
        let pad = 0.013;
        let bottom_pad = 0.03;  // 底部元素使用更大的间距
        
        // 渲染展开页面
        if let Some(index) = self.expanded_index {
            let theme = &self.themes[index];
            let is_applied = index == self.applied_index;
            
            // 渲染背景（封面图片全屏）
            if let Some(cover) = &theme.cover {
                ui.fill_rect(r, (**cover, r));
            } else {
                ui.fill_rect(r, semi_black(0.3));
            }
            ui.fill_rect(r, semi_black(0.4));
            
            // 覆盖顶部标题区域（隐藏设置标题）
            let header_r = Rect::new(r.x, r.y, r.w, 0.15);
            ui.fill_rect(header_r, semi_black(0.8));
            
            // 左上角返回按钮（参考活动页面）
            let back_r = ui.back_rect();
            ui.fill_rect(back_r, (*self.icons.back, back_r, ScaleType::Fit, semi_white(1.)));
            self.back_btn.set(ui, back_r);
            
            // 左下角显示主题名称（标题）
            ui.text(&theme.name)
                .pos(r.x + bottom_pad, r.bottom() - bottom_pad)
                .anchor(0., 1.)
                .size(0.8)
                .draw();
            
            // 右上角更多按钮（使用DRectButton控件，和其他按钮保持一致）
            let more_btn_r = Rect::new(r.right() - pad - 0.06, r.y + pad, 0.06, 0.06);
            self.more_btn.render_shadow(ui, more_btn_r, t, |ui, path| {
                ui.fill_path(&path, semi_black(0.4));
                // 绘制三个点
                let dot_size = 0.01;
                let dot_spacing = 0.015;
                let dots_x = more_btn_r.center().x;
                let dots_y = more_btn_r.center().y;
                for i in 0..3 {
                    let offset_x = (i as f32 - 1.) * dot_spacing;
                    let dot_r = Rect::new(dots_x + offset_x - dot_size/2., dots_y - dot_size/2., dot_size, dot_size);
                    ui.fill_circle(dot_r.center().x, dot_r.center().y, dot_size/2., WHITE);
                }
            });
            
            // 显示更多菜单（只在状态改变时显示一次）
            if self.need_show_more_menu {
                // 根据是否是默认主题设置不同的菜单选项
                // 默认主题的文件夹ID为"Default"
                let is_default_theme = theme.folder_path.as_ref()
                    .map(|p| p.file_name().unwrap_or_default().to_string_lossy() == "Default")
                    .unwrap_or(false);
                
                if is_default_theme {
                    // 默认主题只显示"关于"选项
                    self.more_menu.set_options(vec![
                        tl!("about").into_owned(),
                    ]);
                } else {
                    // 其他主题显示"删除"和"关于"选项
                    self.more_menu.set_options(vec![
                        tl!("theme-delete").into_owned(),
                        tl!("about").into_owned(),
                    ]);
                }
                self.more_menu.set_auto_adjust(Some(ui.screen_rect().nonuniform_feather(-0.03, -0.05)));
                self.more_menu.show(ui, t, Rect::new(more_btn_r.x - 0.15, more_btn_r.bottom() + 0.02, 0.3, 0.2));
                self.need_show_more_menu = false; // 重置状态，避免重复显示
            }
            
            // 右下角应用按钮
            let apply_btn_w = 0.4;
            let apply_btn_h = 0.1;
            let apply_btn_r = Rect::new(r.right() - bottom_pad - apply_btn_w, r.bottom() - bottom_pad - apply_btn_h, apply_btn_w, apply_btn_h);
            let ct = apply_btn_r.center();
            
            if is_applied {
                // 已应用的主题，按钮变灰，显示"已应用"
                self.apply_btn.render_shadow(ui, apply_btn_r, t, |ui, path| {
                    ui.fill_path(&path, semi_black(0.4));
                    ui.text(tl!("theme-applied"))
                        .pos(ct.x, ct.y)
                        .anchor(0.5, 0.5)
                        .no_baseline()
                        .size(0.8)
                        .max_width(apply_btn_r.w)
                        .color(semi_white(0.7))
                        .draw();
                });
            } else {
                // 未应用的主题，显示"应用"按钮
                self.apply_btn.render_shadow(ui, apply_btn_r, t, |ui, path| {
                    ui.fill_path(&path, ui.background());
                    ui.text(tl!("theme-apply"))
                        .pos(ct.x, ct.y)
                        .anchor(0.5, 0.5)
                        .no_baseline()
                        .size(0.8)
                        .max_width(apply_btn_r.w)
                        .draw();
                });
            }
            
            // 渲染更多菜单
            self.more_menu.render(ui, t, 1.);
            
            return (w, r.h);
        }
        
        let row_num = 4;
        let row_height = 0.3;
        let cw = w / row_num as f32;
        let ch = row_height;
        
        // 渲染主题卡片
        let content_h = ((self.themes.len() + row_num as usize - 1) / row_num as usize) as f32 * ch;
        ui.hgrids(w, ch, row_num, self.themes.len() as u32, |ui, id| {
            let theme = &mut self.themes[id as usize];
            let rect = Rect::new(pad, pad, cw - pad * 2., ch - pad * 2.);
            
            // 记录卡片位置，用于过渡动画
            if let Some(transit) = &mut self.transit {
                if transit.id == id {
                    transit.rect = Some(ui.rect_to_global(rect));
                }
            }
            
            let mut c = WHITE;
            theme.btn.render_shadow(ui, rect, t, |ui, path| {
                ui.fill_path(&path, semi_black(0.5));
                if let Some(cover) = &theme.cover {
                    ui.fill_path(&path, (**cover, rect.feather(-0.01)));
                } else {
                    ui.fill_path(&path, semi_black(0.3));
                }
                ui.fill_path(&path, (semi_black(0.3), (0., 0.), semi_black(0.6), (0., ch)));
                
                // 处理back_fade_in效果
                if let Some((that_id, start_time)) = &self.back_fade_in {
                    if id == *that_id {
                        let p = ((t - start_time) / 0.2).max(0.);
                        if p > 1. || get_data().prefer_reduced_motion {
                            self.back_fade_in = None;
                        } else {
                            ui.fill_path(&path, semi_black(0.55 * (1. - p)));
                            c.a = p;
                        }
                    }
                }
                
                ui.text(&theme.name)
                    .pos(rect.x + 0.01, rect.bottom() - 0.02)
                    .max_width(rect.w)
                    .anchor(0., 1.)
                    .size(0.6 * rect.w / cw)
                    .color(c)
                    .draw();
            });
        });
        
        // 渲染导入主题按钮（在主题卡片下方居中显示）
        let btn_h = 0.08;
        let btn_y = content_h + 0.02;
        let btn_r = Rect::new(pad, btn_y, w - pad * 2., btn_h);
        self.import_btn.render_text(ui, btn_r, t, tl!("import-theme"), 0.5, false);

        (w, btn_y + btn_h)
    }

    pub fn render_top(&mut self, ui: &mut Ui, t: f32) {
        if let Some(transit) = &self.transit {
            if let Some(fr) = transit.rect {
                let p = transit_time().map_or(1., |tt| ((t - transit.start_time) / tt).clamp(0., 1.));
                let p = (1. - p).powi(4);
                let p = if transit.back { p } else { 1. - p };
                let r = Rect::tween(&fr, &ui.screen_rect(), p);
                let path = r.rounded(0.02 * (1. - p));
                if let Some(cover) = &transit.theme.cover {
                    ui.fill_path(&path, (**cover, r.feather(0.01 * (1. - p))));
                }
                ui.fill_path(&path, semi_black(0.55));
            }
        }
    }
}
