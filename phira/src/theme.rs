prpr_l10n::tl_file!("theme");

use crate::{
    dir, get_data, get_data_mut,
    page::Fader,
    popup::Popup,
    save_data,
    scene::{confirm_delete, TEX_ICON_BACK},
};
use anyhow::{bail, Context, Result};
use macroquad::prelude::*;
use prpr::{
    core::Tweenable,
    ext::{semi_black, semi_white, unzip_into, RectExt, SafeTexture, ScaleType},
    scene::{request_file, NextScene, Scene},
    time::TimeManager,
    ui::{button_hit, button_hit_large, Dialog, DRectButton, PREFER_ALT_UI, RectButton, Scroll, Ui},
};
use serde::Deserialize;
use std::{
    fs::File,
    io::BufReader,
    ops::Range,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use uuid::Uuid;

const THEME_PADDING: f32 = 0.013;
const BACK_FADE_IN_TIME: f32 = 0.2;
const DEFAULT_THEME_ID: &str = "default";
/// 主题导入完成标志：导入成功后由 main.rs 置位，ThemeView::update 轮询后刷新列表。
pub static THEME_IMPORTED: AtomicBool = AtomicBool::new(false);
/// 默认主题封面/配置编译期内嵌回退资源（仅当运行时 assets/theme 目录缺失时使用）。
const DEFAULT_THEME_COVER: &[u8] = include_bytes!("../../assets/theme/default/cover.png");
const DEFAULT_THEME_CONFIG: &str = include_str!("../../assets/theme/default/config.json");

/// 主题资源项（texture/audio 数组元素）。
#[derive(Deserialize)]
struct ThemeResourceItem {
    key: String,
    path: String,
}

/// 主题配置，读取路径：assets/theme/<theme_id>/config.json（或已导入目录 data/themes/<theme_id>/config.json）。
#[derive(Deserialize)]
struct ThemeConfig {
    name: String,
    #[serde(default)]
    description: String,
    cover: String,
    #[serde(default)]
    texture: Vec<ThemeResourceItem>,
    #[serde(default)]
    audio: Vec<ThemeResourceItem>,
}

/// 主题数据模型。
#[derive(Clone)]
pub struct ThemeItem {
    /// 主题 id，即目录名。
    pub id: String,
    /// 主题名（来自 config.json 的 name 字段）。
    pub name: String,
    /// 主题描述（来自 config.json 的 description 字段）。
    pub description: String,
    /// 主题封面（config.json 的 cover 字段指向的图片）。
    pub cover: SafeTexture,
    /// 是否为内置默认主题。
    pub is_default: bool,
}

struct ThemeCard {
    item: ThemeItem,
    btn: DRectButton,
}

struct ThemeTransit {
    id: u32,
    rect: Option<Rect>,
    item: ThemeItem,
    start_time: f32,
    next_scene: Option<NextScene>,
    back: bool,
    done: bool,
}

fn transit_time() -> Option<f32> {
    if get_data().prefer_reduced_motion {
        None
    } else {
        Some(0.4)
    }
}

/// 加载内置默认主题（封面/名字/描述来自运行时 assets/theme/default 目录，用户可直接修改；
/// 该目录缺失时回退到编译期内嵌资源）。
fn load_default_theme() -> Result<ThemeItem> {
    let base = "assets/theme/default";
    let cfg_text = std::fs::read_to_string(format!("{base}/config.json"))
        .unwrap_or_else(|_| DEFAULT_THEME_CONFIG.to_string());
    let cfg: ThemeConfig =
        serde_json::from_str(&cfg_text).context("parse default theme config")?;
    // 封面按 config.json 的 cover 字段读取（兼容 "./cover.jpg" 等相对写法），读不到时回退内嵌资源。
    let cover_name = cfg.cover.trim_start_matches("./").trim_start_matches('/');
    let cover_bytes = match std::fs::read(format!("{base}/{cover_name}")) {
        Ok(bytes) => bytes,
        Err(_) => DEFAULT_THEME_COVER.to_vec(),
    };
    let img = image::load_from_memory(&cover_bytes).context("load default theme cover")?;
    Ok(ThemeItem {
        id: DEFAULT_THEME_ID.into(),
        name: cfg.name,
        description: cfg.description,
        cover: SafeTexture::from(img),
        is_default: true,
    })
}

/// 加载已导入主题（data/themes/<id>/config.json）。
fn load_imported_theme(path: &str, id: &str) -> Option<ThemeItem> {
    let text = std::fs::read_to_string(format!("{path}/config.json")).ok()?;
    let cfg: ThemeConfig = serde_json::from_str(&text).ok()?;
    let bytes = std::fs::read(format!("{path}/{}", cfg.cover)).ok()?;
    let img = image::load_from_memory(&bytes).ok()?;
    Some(ThemeItem {
        id: id.into(),
        name: cfg.name,
        description: cfg.description,
        cover: SafeTexture::from(img),
        is_default: false,
    })
}

/// 从用户选择的 zip 文件导入主题：校验 zip 内 config.json，解压到 data/themes/<uuid>，
/// 解压后再校验一次（config.json 与封面可加载）才算成功；失败时自动清理已创建目录。
pub fn import_theme_from_zip(file: String) -> Result<String> {
    let root = dir::themes()?;
    let tdir = prpr::dir::Dir::new(&root)?;
    let mut dir_id: Option<String> = None;
    let res = (|| -> Result<String> {
        // 先读 zip 内 config.json，确认是合法主题包
        let config: ThemeConfig = {
            let mut zip = zip::ZipArchive::new(BufReader::new(File::open(&file)?))?;
            let x = serde_json::from_reader(zip.by_name("config.json").context("missing config.json")?)
                .context("invalid config.json")?;
            x
        };
        let mut uuid = Uuid::new_v4();
        while tdir.exists(uuid.to_string())? {
            uuid = Uuid::new_v4();
        }
        let id = uuid.to_string();
        tdir.create_dir_all(&id)?;
        let dir = tdir.open_dir(&id)?;
        dir_id = Some(id.clone());
        unzip_into(BufReader::new(File::open(&file)?), &dir, false).context("failed to unzip")?;
        // 解压后再校验一遍：config.json 与封面可加载才视为导入成功
        if load_imported_theme(&format!("{root}/{id}"), &id).is_none() {
            bail!("invalid theme package");
        }
        Ok(config.name)
    })();
    match res {
        Err(err) => {
            if let Some(id) = &dir_id {
                let _ = tdir.remove_dir_all(id);
            }
            Err(err)
        }
        Ok(name) => Ok(name),
    }
}

/// 定位主题资源目录：用户主题优先 data/themes/<id>，其次 assets/theme/<id>；
/// 默认主题只读 assets/theme/default。
fn resolve_theme_dir(id: &str) -> Option<std::path::PathBuf> {
    if id != DEFAULT_THEME_ID {
        if let Ok(themes) = dir::themes() {
            let p = std::path::Path::new(&themes).join(id);
            if p.join("config.json").exists() {
                return Some(p.to_path_buf());
            }
        }
    }
    let p = std::path::Path::new("assets").join("theme").join(id);
    if p.join("config.json").exists() {
        Some(p.to_path_buf())
    } else {
        None
    }
}

/// config.json 纹理资源键 → assets 默认图片路径。
fn default_texture_path(key: &str) -> Option<&'static str> {
    Some(match key {
        "background" => "background.jpg",
        "abstract" => "abstract.jpg",
        "boot" => "boot.png",
        "splash" => "splash.png",
        "player" => "player.jpg",
        "icon" => "icon.png",
        "rank_phi" => "rank/phi.png",
        "rank_fc" => "rank/FC.png",
        "rank_s" => "rank/S.png",
        "rank_a" => "rank/A.png",
        "rank_b" => "rank/B.png",
        "rank_c" => "rank/C.png",
        "rank_f" => "rank/F.png",
        "rank_v" => "rank/V.png",
        "mod_autoplay" => "mod/autoplay.png",
        "mod_fade_in" => "mod/fade_in.png",
        "mod_fade_out" => "mod/fade_out.png",
        "mod_flip_x" => "mod/flip_x.png",
        "mod_nightcore" => "mod/nightcore.png",
        "mod_no_shader" => "mod/no-shader.png",
        "mod_rainbow" => "mod/rainbow.png",
        _ => return None,
    })
}

/// config.json 音频资源键 → assets 默认音频路径。
fn default_audio_path(key: &str) -> Option<&'static str> {
    Some(match key {
        "bgm" => "bgm.mp3",
        "splash" => "splash.mp3",
        "button" => "button.ogg",
        "button_large" => "button_large.ogg",
        "switch" => "switch.ogg",
        "click" => "click.ogg",
        "drag" => "drag.ogg",
        "flick" => "flick.ogg",
        "cali" => "cali.ogg",
        "cali_hit" => "cali_hit.ogg",
        "ending" => "ending.ogg",
        "enterlibrary" => "enterlibrary.ogg",
        "chartpreview" => "chartpreview.ogg",
        "startplaying" => "startplaying.ogg",
        "entersplash" => "entersplash.ogg",
        "trackskip" => "trackskip.ogg",
        "enter" => "enter.ogg",
        "toast_ok" => "toast_ok.ogg",
        "toast_warning" => "toast_warning.ogg",
        "toast_error" => "toast_error.ogg",
        _ => return None,
    })
}

/// 应用当前主题：读取 applied_theme 对应目录的 config.json，
/// 将非空 path 的资源项注册为覆盖，替换默认 assets 资源后再开始游戏加载。
/// 主题缺失/解析失败时清空覆盖表（全部回退默认资源），不阻止启动。
pub fn apply_theme_overrides() {
    use std::collections::HashMap;
    let theme_id = get_data()
        .applied_theme
        .clone()
        .unwrap_or_else(|| DEFAULT_THEME_ID.into());
    let Some(dir) = resolve_theme_dir(&theme_id) else {
        prpr::theme::clear_theme_overrides();
        return;
    };
    let Ok(text) = std::fs::read_to_string(dir.join("config.json")) else {
        prpr::theme::clear_theme_overrides();
        return;
    };
    let Ok(cfg) = serde_json::from_str::<ThemeConfig>(&text) else {
        prpr::theme::clear_theme_overrides();
        return;
    };
    let mut map = HashMap::new();
    for item in &cfg.texture {
        let path = item.path.trim().trim_start_matches("./").to_string();
        if path.is_empty() {
            continue;
        }
        let Some(target) = default_texture_path(&item.key) else { continue };
        let src = dir.join(&path);
        if src.exists() {
            map.insert(target.to_string(), src);
        }
    }
    for item in &cfg.audio {
        let path = item.path.trim().trim_start_matches("./").to_string();
        if path.is_empty() {
            continue;
        }
        let Some(target) = default_audio_path(&item.key) else { continue };
        let src = dir.join(&path);
        if src.exists() {
            map.insert(target.to_string(), src);
        }
    }
    prpr::theme::set_theme_overrides(map);
}

/// 主题列表视图：网格布局与展开动画均参照 ChartsView（charts_view.rs）。
pub struct ThemeView {
    scroll: Scroll,
    fader: Fader,
    items: Vec<ThemeCard>,
    applied: Option<String>,
    transit: Option<ThemeTransit>,
    back_fade_in: Option<(u32, f32)>,
    refresh: bool,
    import_btn: DRectButton,
    /// 由 settings.rs 的 render_a1 置位：加号改由顶部标题栏右侧渲染，内容区不再绘制。
    import_in_header: bool,
}

impl ThemeView {
    pub fn new() -> Self {
        let mut v = Self {
            scroll: Scroll::new(),
            fader: Fader::new().with_distance(0.06),
            items: Vec::new(),
            // data.json 无主题数据（applied_theme 为 None）时，当前应用主题视为默认主题。
            applied: get_data().applied_theme.clone().or(Some(DEFAULT_THEME_ID.into())),
            transit: None,
            back_fade_in: None,
            refresh: true,
            import_btn: DRectButton::new(),
            import_in_header: false,
        };
        v.reload();
        v
    }

    pub fn transiting(&self) -> bool {
        self.transit.is_some()
    }

    pub fn reset_scroll(&mut self) {
        self.scroll.y_scroller.reset();
    }

    /// 重新扫描主题列表（默认主题 + 已导入主题）。
    pub fn reload(&mut self) {
        self.items.clear();
        if let Ok(item) = load_default_theme() {
            self.items.push(ThemeCard {
                item,
                btn: DRectButton::new(),
            });
        }
        if let Ok(root) = dir::themes() {
            if let Ok(rd) = std::fs::read_dir(&root) {
                for entry in rd.flatten() {
                    let id = match entry.file_name().into_string() {
                        Ok(id) => id,
                        Err(_) => continue,
                    };
                    if id == DEFAULT_THEME_ID {
                        continue;
                    }
                    if let Some(item) = load_imported_theme(&format!("{root}/{id}"), &id) {
                        self.items.push(ThemeCard {
                            item,
                            btn: DRectButton::new(),
                        });
                    }
                }
            }
        }
    }

    fn display_range(&self, content_size: (f32, f32)) -> Range<u32> {
        let sy = self.scroll.y_scroller.offset;
        let row_height = 0.32;
        let start_line = (sy / row_height) as u32;
        let end_line = ((sy + content_size.1) / row_height).ceil() as u32;
        (start_line * 3)..((end_line + 1) * 3)
    }

    pub fn touch(&mut self, touch: &Touch, t: f32) -> Result<bool> {
        if self.scroll.touch(touch, t) {
            return Ok(true);
        }
        if self.import_btn.touch(touch, t) {
            button_hit();
            request_file("_import_theme");
            return Ok(true);
        }
        if !self.scroll.contains(touch) {
            return Ok(false);
        }
        for (id, card) in self.items.iter_mut().enumerate() {
            if card.btn.touch(touch, t) {
                button_hit_large();
                let item = card.item.clone();
                let applied = self.applied.as_deref() == Some(&item.id);
                self.transit = Some(ThemeTransit {
                    id: id as u32,
                    rect: None,
                    item,
                    start_time: t,
                    next_scene: Some(NextScene::Overlay(Box::new(ThemeDetailScene::new(card.item.clone(), applied)))),
                    back: false,
                    done: false,
                });
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn update(&mut self, t: f32) -> Result<bool> {
        self.scroll.update(t);
        // 导入完成标志：main.rs 解压成功后置位，这里刷新列表展示新主题。
        if THEME_IMPORTED.swap(false, Ordering::Relaxed) {
            self.refresh = true;
        }
        // data.json 无主题数据（applied_theme 为 None）时，当前应用主题视为默认主题。
        let applied = get_data().applied_theme.clone().or(Some(DEFAULT_THEME_ID.into()));
        if self.applied != applied || self.refresh {
            self.refresh = false;
            self.applied = applied;
            self.reload();
        }
        if let Some(transit) = &mut self.transit {
            if t > transit.start_time + transit_time().unwrap_or_default() {
                if transit.back {
                    self.back_fade_in = Some((transit.id, t));
                    self.transit = None;
                } else {
                    transit.done = true;
                }
            }
        }
        if let Some((_, t0)) = &mut self.back_fade_in {
            if t > *t0 + BACK_FADE_IN_TIME {
                self.back_fade_in = None;
            }
        }
        Ok(false)
    }

    pub fn render(&mut self, ui: &mut Ui, r: Rect, t: f32) -> (f32, f32) {
        let content_size = (r.w, r.h);
        // 调用方（settings.rs 的 render_a1 / render）已把坐标系平移到内容区原点，
        // 这里使用局部矩形，避免 Official UI 下重复偏移导致内容画到屏幕外不显示。
        let rl = Rect::new(0., 0., r.w, r.h);
        let range = self.display_range(content_size);
        if self.items.is_empty() {
            let ct = rl.center();
            ui.text(tl!("list-empty"))
                .pos(ct.x, ct.y)
                .anchor(0.5, 0.5)
                .no_baseline()
                .draw();
            if !self.import_in_header {
                self.render_import_btn_at(ui, Rect::new(rl.w - 0.105, 0.015, 0.09, 0.09), t);
            }
            return (0., 0.);
        }
        self.scroll.size(content_size);
        self.scroll.render(ui, |ui| {
            let cw = rl.w / 3.;
            let ch = 0.32;
            let p = THEME_PADDING;
            let cr = Rect::new(p, p, cw - p * 2., ch - p * 2.);
            self.fader.reset();
            self.fader.for_sub(|f| {
                ui.hgrids(content_size.0, ch, 3, self.items.len() as u32, |ui, id| {
                    if let Some(transit) = &mut self.transit {
                        if transit.id == id {
                            transit.rect = Some(ui.rect_to_global(cr));
                        }
                    }
                    if !range.contains(&id) {
                        return;
                    }
                    f.render(ui, t, |ui| {
                        let card = &mut self.items[id as usize];
                        card.btn.render_shadow(ui, cr, t, |ui, path| {
                            ui.fill_path(&path, (*card.item.cover, cr.feather(0.01), ScaleType::CropCenter));
                            ui.fill_path(&path, semi_black(0.3));
                            ui.fill_path(&path, (semi_black(0.4), (0., 0.), semi_black(0.85), (0., ch)));
                            let applied = self.applied.as_deref() == Some(&card.item.id);
                            if applied {
                                ui.stroke_path(&path, 0.003, Color::new(1.0, 0.58, 0.706, 1.0));
                            }
                            ui.text(&card.item.name)
                                .pos(cr.x + 0.01, cr.bottom() - 0.02)
                                .max_width(cr.w)
                                .anchor(0., 1.)
                                .no_baseline()
                                .size(0.85 * cr.w / cw)
                                .color(WHITE)
                                .draw();
                        });
                    });
                });
            });
            (0., 0.)
        });
        if !self.import_in_header {
            self.render_import_btn_at(ui, Rect::new(rl.w - 0.105, 0.015, 0.09, 0.09), t);
        }
        (0., 0.)
    }

    /// 导入主题按钮：样式同谱面库右上角导入按钮（半透明圆角底 + 加号）。
    /// 位置矩形由调用方决定：默认内容区右上角；settings.rs 的 render_a1 会传入顶部标题栏右侧。
    pub fn render_import_btn_at(&mut self, ui: &mut Ui, r: Rect, t: f32) {
        self.import_btn.render_shadow(ui, r, t, |ui, path| {
            ui.fill_path(&path, semi_black(0.4));
            let cr = r.feather(-0.012);
            ui.text("+")
                .pos(cr.center().x, cr.center().y)
                .anchor(0.5, 0.5)
                .no_baseline()
                .size(0.55)
                .color(WHITE)
                .draw();
        });
    }

    /// 设置加号是否改由设置页顶部标题栏右侧渲染（true 时内容区不再绘制加号）。
    pub fn set_import_in_header(&mut self, v: bool) {
        self.import_in_header = v;
    }

    pub fn render_top(&mut self, ui: &mut Ui, t: f32) {
        if let Some(transit) = &mut self.transit {
            if let Some(fr) = transit.rect {
                // 展开动画完成后 ThemeDetailScene 已接管全屏绘制，这里不再绘制，
                // 否则返回后背景会残留在原地（直到退出设置页才消失）。
                if !transit.back && transit.done && transit.next_scene.is_none() {
                    return;
                }
                let p = transit_time().map_or(1., |tt| ((t - transit.start_time) / tt).clamp(0., 1.));
                let p = (1. - p).powi(4);
                let p = if transit.back { p } else { 1. - p };
                let r = Rect::tween(&fr, &ui.screen_rect(), p);
                let path = r.rounded(0.02 * (1. - p));
                let cover = transit.item.cover.clone();
                ui.fill_path(&path, (*cover, r.feather(0.01 * (1. - p)), ScaleType::CropCenter));
                ui.fill_path(&path, semi_black(0.55));
                return;
            }
        }
        if let Some((_id, t0)) = &self.back_fade_in {
            let p = (t - *t0) / BACK_FADE_IN_TIME;
            if p >= 0. && p <= 1. {
                let rr = ui.screen_rect();
                ui.fill_rect(rr, semi_black(0.4 * (1. - p)));
            }
        }
    }

    pub fn next_scene(&mut self) -> Option<NextScene> {
        if let Some(transit) = &mut self.transit {
            if transit.done {
                return transit.next_scene.take();
            }
        }
        None
    }

    /// 从主题详情场景返回时调用：触发收起动画并刷新列表。
    pub fn on_result(&mut self, t: f32) {
        if let Some(transit) = &mut self.transit {
            transit.start_time = t;
            transit.back = true;
            transit.done = false;
        }
        self.refresh = true;
    }
}

/// 主题详情场景：类似谱面预览界面，但仅保留返回 / 主题名 / 应用按钮与更多菜单。
pub struct ThemeDetailScene {
    item: ThemeItem,
    applied: bool,
    icon_back: SafeTexture,
    back_btn: RectButton,
    apply_btn: DRectButton,
    menu_btn: RectButton,
    menu: Popup,
    need_show_menu: bool,
    confirm_delete: Arc<AtomicBool>,
    next: NextScene,
}

impl ThemeDetailScene {
    pub fn new(item: ThemeItem, applied: bool) -> Self {
        Self {
            item,
            applied,
            icon_back: TEX_ICON_BACK.with(|it| it.borrow().clone().unwrap()),
            back_btn: RectButton::new(),
            apply_btn: DRectButton::new(),
            menu_btn: RectButton::new(),
            menu: Popup::new(),
            need_show_menu: false,
            confirm_delete: Arc::new(AtomicBool::new(false)),
            next: NextScene::None,
        }
    }

    fn menu_options(&self) -> Vec<String> {
        if self.item.is_default {
            vec![tl!("about").into_owned()]
        } else {
            vec![tl!("delete").into_owned(), tl!("about").into_owned()]
        }
    }

    fn show_theme_info(&self) {
        Dialog::plain(tl!("theme-info"), self.item.description.clone())
            .buttons(vec![ttl!("confirm").into_owned()])
            .show();
    }
}

impl Scene for ThemeDetailScene {
    fn touch(&mut self, tm: &mut TimeManager, touch: &Touch) -> Result<bool> {
        let t = tm.now() as f32;
        if self.menu.showing() {
            self.menu.touch(touch, t);
            return Ok(true);
        }
        if self.back_btn.touch(touch) {
            button_hit();
            // 带结果返回，触发设置页 on_result 执行收起动画（Pop 不会回调 on_result，背景会残留）。
            self.next = NextScene::PopWithResult(Box::new(()));
            return Ok(true);
        }
        if !self.applied && self.apply_btn.touch(touch, t) {
            button_hit();
            let theme_id = self.item.id.clone();
            Dialog::plain(tl!("apply"), tl!("apply-restart"))
                .buttons(vec![ttl!("cancel").into_owned(), ttl!("confirm").into_owned()])
                .listener(move |_dialog, id| {
                    if id == 1 {
                        get_data_mut().applied_theme = Some(theme_id.clone());
                        save_data().ok();
                        if let Ok(exe) = std::env::current_exe() {
                            let _ = std::process::Command::new(exe).spawn();
                        }
                        std::process::exit(0);
                    }
                    false
                })
                .show();
            return Ok(true);
        }
        if self.menu_btn.touch(touch) {
            button_hit();
            self.need_show_menu = true;
            return Ok(true);
        }
        Ok(false)
    }

    fn update(&mut self, tm: &mut TimeManager) -> Result<()> {
        let t = tm.now() as f32;
        self.menu.update(t);
        if self.menu.changed() {
            let sel = self.menu.selected();
            self.menu.set_selected(usize::MAX);
            if self.item.is_default {
                if sel == 0 {
                    self.show_theme_info();
                }
            } else {
                match sel {
                    0 => confirm_delete(self.confirm_delete.clone()),
                    1 => self.show_theme_info(),
                    _ => {}
                }
            }
        }
        if self.confirm_delete.swap(false, Ordering::SeqCst) {
            if !self.item.is_default {
                let root = dir::themes()?;
                let path = format!("{root}/{}", self.item.id);
                let _ = std::fs::remove_dir_all(&path)
                    .with_context(|| format!("remove theme dir {path}"));
                if get_data().applied_theme.as_deref() == Some(self.item.id.as_str()) {
                    get_data_mut().applied_theme = None;
                }
                save_data()?;
                self.next = NextScene::PopWithResult(Box::new(()));
            }
        }
        Ok(())
    }

    fn render(&mut self, tm: &mut TimeManager, ui: &mut Ui) -> Result<()> {
        let t = tm.now() as f32;
        let rr = ui.screen_rect();
        ui.fill_rect(rr, (*self.item.cover.clone(), rr, ScaleType::CropCenter));
        ui.fill_rect(rr, semi_black(0.55));

        // 左上：返回按钮（Official UI 用 back.png 图标；Alt UI 与其他页面一致：粉色圆角底 + ←）
        let back_r = ui.back_rect();
        self.back_btn.set(ui, back_r);
        if PREFER_ALT_UI.load(Ordering::Relaxed) {
            ui.fill_path(&back_r.feather(-0.004).rounded(0.02), Color::new(1.0, 0.58, 0.706, 0.14));
            ui.text("\u{2190}")
                .pos(back_r.center().x, back_r.center().y)
                .anchor(0.5, 0.5)
                .no_baseline()
                .size(0.5)
                .color(Color::new(1.0, 0.58, 0.706, 1.0))
                .draw();
        } else {
            ui.fill_rect(back_r, (*self.icon_back, back_r));
        }

        // 右上：更多菜单
        let menu_r = Rect::new(0.86, -ui.top + 0.02, 0.11, 0.1);
        self.menu_btn.set(ui, menu_r);
        ui.fill_rect(menu_r, semi_black(0.5));
        ui.text("\u{22ef}")
            .pos(menu_r.center().x, menu_r.center().y)
            .anchor(0.5, 0.5)
            .no_baseline()
            .size(0.74)
            .color(WHITE)
            .draw();
        if self.need_show_menu {
            self.need_show_menu = false;
            self.menu.set_bottom(true);
            self.menu.set_options(self.menu_options());
            self.menu.set_selected(usize::MAX);
            self.menu.show(ui, t, Rect::new(0.62, -ui.top + 0.12, 0.35, 0.5));
        }

        // 左下：主题名（标题字号调大、整体右移、下移）
        ui.text(&self.item.name)
            .pos(rr.x + 0.055, rr.bottom() - 0.11)
            .anchor(0., 1.)
            .no_baseline()
            .size(0.62)
            .max_width(0.6)
            .color(WHITE)
            .draw();

        // 右下：应用按钮
        let apply_r = Rect::new(0.7, ui.top - 0.16, 0.26, 0.11);
        self.apply_btn.render_shadow(ui, apply_r, t, |ui, path| {
            if self.applied {
                ui.fill_path(&path, semi_white(0.35));
                ui.stroke_path(&path, 0.002, semi_white(0.2));
                ui.text(tl!("applied"))
                    .pos(apply_r.center().x, apply_r.center().y)
                    .anchor(0.5, 0.5)
                    .no_baseline()
                    .size(0.42)
                    .color(semi_white(0.5))
                    .draw();
            } else {
                ui.fill_path(&path, Color::new(1.0, 0.58, 0.706, 1.0));
                ui.text(tl!("apply"))
                    .pos(apply_r.center().x, apply_r.center().y)
                    .anchor(0.5, 0.5)
                    .no_baseline()
                    .size(0.42)
                    .color(WHITE)
                    .draw();
            }
        });

        self.menu.render(ui, t, 1.);
        Ok(())
    }

    fn next_scene(&mut self, _tm: &mut TimeManager) -> NextScene {
        std::mem::replace(&mut self.next, NextScene::None)
    }
}
