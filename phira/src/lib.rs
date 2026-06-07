prpr_l10n::tl_file!("common" ttl crate::);

#[rustfmt::skip]
#[cfg(closed)]
mod inner;

mod anim;
mod charts_view;
mod client;
mod data;
mod icons;
mod images;
mod login;
mod mp;
mod page;
mod popup;
mod rate;
mod resource;
mod scene;
mod tabs;
mod tags;
mod threed;
mod uml;

use anyhow::Result;
use core::f64;
use data::Data;
use macroquad::prelude::*;
use prpr::{
    build_conf,
    core::{init_assets, PGR_FONT},
    ext::SafeTexture,
    log,
    scene::{show_error, show_message},
    time::TimeManager,
    ui::{FontArc, TextPainter},
    Main,
};
use prpr_l10n::{set_prefered_locale, GLOBAL, LANGS};
use scene::MainScene;
use std::{
    collections::VecDeque,
    sync::{mpsc, Mutex},
};
use tracing::{error, info, warn};

#[cfg(target_os = "android")]
use jni::{
    objects::{JClass, JString},
    sys::jint,
    EnvUnowned,
};

static MESSAGES_TX: Mutex<Option<mpsc::Sender<bool>>> = Mutex::new(None);
static AA_TX: Mutex<Option<mpsc::Sender<i32>>> = Mutex::new(None);
static DATA_PATH: Mutex<Option<String>> = Mutex::new(None);
static CACHE_DIR: Mutex<Option<String>> = Mutex::new(None);
pub static mut DATA: Option<Data> = None;
// 当前主题路径
static THEME_PATH: Mutex<Option<String>> = Mutex::new(None);

#[cfg(target_env = "ohos")]
use napi_derive_ohos::napi;

#[cfg(closed)]
pub async fn load_res(name: &str) -> Vec<u8> {
    let bytes = load_file(name).await.unwrap();
    inner::resolve_data(bytes)
}

#[allow(unused)]
pub async fn load_res_tex(name: &str) -> SafeTexture {
    #[cfg(closed)]
    {
        let bytes = load_res(name).await;
        let image = image::load_from_memory(&bytes).unwrap();
        image.into()
    }
    #[cfg(not(closed))]
    prpr::ext::BLACK_TEXTURE.clone()
}

pub fn sync_data() {
    set_prefered_locale(get_data().language.as_ref().and_then(|it| it.parse().ok()));
    if get_data().language.is_none() {
        get_data_mut().language = Some(LANGS[GLOBAL.order.lock().unwrap()[0]].to_owned());
    }
    let _ = client::set_access_token_sync(get_data().tokens.as_ref().map(|it| &*it.0));
}

pub fn set_data(data: Data) {
    unsafe {
        DATA = Some(data);
    }
}

// 设置当前主题路径
pub fn set_theme_path(path: Option<String>) {
    *THEME_PATH.lock().unwrap() = path;
}

// 获取当前主题路径
pub fn get_theme_path() -> Option<String> {
    THEME_PATH.lock().unwrap().clone()
}

// 递归复制文件夹
fn copy_dir_all(src: impl AsRef<std::path::Path>, dst: impl AsRef<std::path::Path>) -> Result<()> {
    let src = src.as_ref();
    let dst = dst.as_ref();
    
    std::fs::create_dir_all(dst)?;
    
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(entry.path(), dst.join(entry.file_name()))?;
        } else {
            std::fs::copy(entry.path(), dst.join(entry.file_name()))?;
        }
    }
    Ok(())
}

// 初始化 current 目录（只复制 assets）
fn init_current_dir() -> Result<()> {
    let dir = dir::root()?;
    let themes_dir = format!("{}/themes", dir);
    let current_dir = format!("{}/current", themes_dir);
    let current_path = std::path::Path::new(&current_dir);
    
    if !current_path.exists() {
        std::fs::create_dir_all(current_path)?;
        
        // 复制 assets 目录的所有文件到 current
        let mut assets_path = None;
<<<<<<< HEAD
        
        // Android: assets在APK内部，无法直接访问，跳过复制
        #[cfg(target_os = "android")]
        {
            // Android上不复制assets，让macroquad的load_file处理
            info!("Android platform: skipping assets copy to current directory");
        }
        
        // iOS: assets在应用包内
        #[cfg(target_os = "ios")]
        {
            use objc2_foundation::{NSBundle, NSBundleMainBundle};
            let bundle = NSBundle::mainBundle();
            if let Some(bundle_path) = bundle.resourcePath() {
                let candidate = std::path::Path::new(bundle_path.to_string()).join("assets");
                if candidate.exists() {
                    assets_path = Some(candidate);
                }
            }
        }
        
        // HarmonyOS: assets在固定路径
        #[cfg(target_env = "ohos")]
        {
            let candidate = std::path::Path::new("/data/storage/el1/bundle/entry/resources/resfile/assets");
            if candidate.exists() {
                assets_path = Some(candidate.to_path_buf());
            }
        }
        
        // 桌面系统: 从可执行文件目录查找
        #[cfg(not(any(target_os = "android", target_os = "ios", target_env = "ohos")))]
=======
        #[cfg(not(target_env = "ohos"))]
>>>>>>> 663e9780b4904a68f23ab7f1b2776d671e58dcb5
        if let Ok(mut exe) = std::env::current_exe() {
            while exe.pop() {
                let candidate = exe.join("assets");
                if candidate.exists() {
                    assets_path = Some(candidate);
                    break;
                }
            }
        }
<<<<<<< HEAD
=======
        #[cfg(target_env = "ohos")]
        {
            let candidate = std::path::Path::new("/data/storage/el1/bundle/entry/resources/resfile/assets");
            if candidate.exists() {
                assets_path = Some(candidate.to_path_buf());
            }
        }
>>>>>>> 663e9780b4904a68f23ab7f1b2776d671e58dcb5
        
        if let Some(assets) = assets_path {
            copy_dir_all(&assets, current_path)?;
        }
    }
    
    Ok(())
}

// 应用主题（复制文件到data/themes/current目录）
pub fn apply_theme(theme_id: &str) -> Result<()> {
    let dir = dir::root()?;
    let themes_dir = format!("{}/themes", dir);
    let current_dir = format!("{}/current", themes_dir);
    let current_path = std::path::Path::new(&current_dir);
    
    // 清空 current 目录
    if current_path.exists() {
        std::fs::remove_dir_all(current_path)?;
    }
    std::fs::create_dir_all(current_path)?;
    
    // 复制 assets 目录的所有文件到 current
    let mut assets_path = None;
<<<<<<< HEAD
    
    // Android: assets在APK内部，无法直接访问，跳过复制
    #[cfg(target_os = "android")]
    {
        // Android上不复制assets，让macroquad的load_file处理
        info!("Android platform: skipping assets copy in apply_theme");
    }
    
    // iOS: assets在应用包内
    #[cfg(target_os = "ios")]
    {
        use objc2_foundation::{NSBundle, NSBundleMainBundle};
        let bundle = NSBundle::mainBundle();
        if let Some(bundle_path) = bundle.resourcePath() {
            let candidate = std::path::Path::new(bundle_path.to_string()).join("assets");
            if candidate.exists() {
                assets_path = Some(candidate);
            }
        }
    }
    
    // HarmonyOS: assets在固定路径
    #[cfg(target_env = "ohos")]
    {
        let candidate = std::path::Path::new("/data/storage/el1/bundle/entry/resources/resfile/assets");
        if candidate.exists() {
            assets_path = Some(candidate.to_path_buf());
        }
    }
    
    // 桌面系统: 从可执行文件目录查找
    #[cfg(not(any(target_os = "android", target_os = "ios", target_env = "ohos")))]
=======
    #[cfg(not(target_env = "ohos"))]
>>>>>>> 663e9780b4904a68f23ab7f1b2776d671e58dcb5
    if let Ok(mut exe) = std::env::current_exe() {
        while exe.pop() {
            let candidate = exe.join("assets");
            if candidate.exists() {
                assets_path = Some(candidate);
                break;
            }
        }
    }
<<<<<<< HEAD
=======
    #[cfg(target_env = "ohos")]
    {
        let candidate = std::path::Path::new("/data/storage/el1/bundle/entry/resources/resfile/assets");
        if candidate.exists() {
            assets_path = Some(candidate.to_path_buf());
        }
    }
>>>>>>> 663e9780b4904a68f23ab7f1b2776d671e58dcb5
    
    if let Some(assets) = assets_path {
        copy_dir_all(&assets, current_path)?;
    }
    
    // 复制主题文件夹的内容到 current（覆盖已存在的文件）
    let theme_dir = format!("{}/{}", themes_dir, theme_id);
    let theme_path = std::path::Path::new(&theme_dir);
    if theme_path.exists() {
        copy_dir_all(theme_path, current_path)?;
    }
    
    // 设置主题路径为 current 目录
    set_theme_path(Some(current_dir));
    
    Ok(())
}

// 加载主题资源
pub async fn load_theme_res(name: &str) -> Option<Vec<u8>> {
    if let Some(theme_path) = get_theme_path() {
        let path = format!("{}/{}", theme_path, name);
        match tokio::fs::read(&path).await {
            Ok(bytes) => return Some(bytes),
            Err(_) => {}
        }
    }
    None
}

// 加载资源（只从 current 目录加载）
pub async fn load_asset(name: &str) -> Vec<u8> {
    if let Some(bytes) = load_theme_res(name).await {
        return bytes;
    }
    warn!("Failed to load asset from theme path: {}", name);
    load_file(name).await.unwrap_or_default()
}

<<<<<<< HEAD
// 加载纹理（优先从 current 目录加载，失败则回退到 assets）
pub async fn load_theme_texture(name: &str) -> Result<prpr::ext::SafeTexture> {
    // 首先尝试从主题目录加载
=======
// 加载纹理（只从 current 目录加载）
pub async fn load_theme_texture(name: &str) -> Result<prpr::ext::SafeTexture> {
>>>>>>> 663e9780b4904a68f23ab7f1b2776d671e58dcb5
    if let Some(theme_path) = get_theme_path() {
        let full_path = format!("{}/{}", theme_path, name);
        info!("Loading texture from: {}", full_path);
        
        // 直接读取文件字节
<<<<<<< HEAD
        if let Ok(bytes) = tokio::fs::read(&full_path).await {
            // 从字节加载图片
            if let Ok(image) = image::load_from_memory(&bytes) {
                // 转换为纹理
                let texture: prpr::ext::SafeTexture = image.into();
                return Ok(texture);
            }
        }
    }
    
    // 如果从主题目录加载失败，回退到 assets（通过 macroquad 的 load_file）
    info!("Falling back to assets for: {}", name);
    let bytes = load_file(name).await
        .map_err(|e| anyhow::anyhow!("Failed to load texture {} from assets: {}", name, e))?;
    
    // 从字节加载图片
    let image = image::load_from_memory(&bytes)
        .map_err(|e| anyhow::anyhow!("Failed to load image from assets {}: {}", name, e))?;
    
    // 转换为纹理
    let texture: prpr::ext::SafeTexture = image.into();
    
    Ok(texture)
=======
        let bytes = tokio::fs::read(&full_path).await
            .map_err(|e| anyhow::anyhow!("Failed to read file {}: {}", full_path, e))?;
        
        // 从字节加载图片
        let image = image::load_from_memory(&bytes)
            .map_err(|e| anyhow::anyhow!("Failed to load image from {}: {}", full_path, e))?;
        
        // 转换为纹理
        let texture: prpr::ext::SafeTexture = image.into();
        
        return Ok(texture);
    }
    Err(anyhow::anyhow!("Theme path not set"))
>>>>>>> 663e9780b4904a68f23ab7f1b2776d671e58dcb5
}

#[allow(static_mut_refs)]
pub fn get_data() -> &'static Data {
    unsafe { DATA.as_ref().unwrap() }
}

#[allow(static_mut_refs)]
pub fn get_data_mut() -> &'static mut Data {
    unsafe { DATA.as_mut().unwrap() }
}

pub fn save_data() -> Result<()> {
    std::fs::write(format!("{}/data.json", dir::root()?), serde_json::to_string(get_data())?)?;
    Ok(())
}

mod dir {
    use anyhow::Result;

    use crate::{CACHE_DIR, DATA_PATH};

    fn ensure(s: &str) -> Result<String> {
        let s = format!("{}/{}", DATA_PATH.lock().unwrap().as_ref().map(|it| it.as_str()).unwrap_or("."), s);
        let path = std::path::Path::new(&s);
        if !path.exists() {
            std::fs::create_dir_all(path)?;
        }
        Ok(s)
    }

    pub fn cache() -> Result<String> {
        if let Some(cache) = &*CACHE_DIR.lock().unwrap() {
            ensure(cache)
        } else {
            ensure("cache")
        }
    }

    pub fn bold_font_path() -> Result<String> {
        Ok(format!("{}/bold.ttf", root()?))
    }

    pub fn cache_image_local() -> Result<String> {
        ensure(&format!("{}/image", cache()?))
    }

    pub fn root() -> Result<String> {
        ensure("data")
    }

    pub fn charts() -> Result<String> {
        ensure("data/charts")
    }

    pub fn collections() -> Result<String> {
        ensure("data/collections")
    }

    pub fn custom_charts() -> Result<String> {
        ensure("data/charts/custom")
    }

    pub fn downloaded_charts() -> Result<String> {
        ensure("data/charts/download")
    }

    pub fn respacks() -> Result<String> {
        ensure("data/respack")
    }
}

async fn the_main() -> Result<()> {
    log::register();
    #[cfg(target_env = "ohos")]
    {
        *DATA_PATH.lock().unwrap() = Some("/data/storage/el2/base".to_owned());
        *CACHE_DIR.lock().unwrap() = Some("/data/storage/el2/base/cache".to_owned());
        prpr::core::DPI_VALUE.store(250, std::sync::atomic::Ordering::Relaxed);
    };

    init_assets();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .unwrap();
    let _guard = rt.enter();

    #[cfg(target_os = "ios")]
    {
        use objc2_foundation::{NSSearchPathDirectory, NSSearchPathDomainMask, NSSearchPathForDirectoriesInDomains};

        let directories = NSSearchPathForDirectoriesInDomains(NSSearchPathDirectory::LibraryDirectory, NSSearchPathDomainMask::UserDomainMask, true);
        let path = directories.firstObject().unwrap().to_string();
        *DATA_PATH.lock().unwrap() = Some(path);
        *CACHE_DIR.lock().unwrap() = Some("Caches".to_owned());
    }

    let dir = dir::root()?;
    let mut data: Data = std::fs::read_to_string(format!("{dir}/data.json"))
        .map_err(anyhow::Error::new)
        .and_then(|s| Ok(serde_json::from_str(&s)?))
        .unwrap_or_default();
    data.init().await?;
    set_data(data);
    sync_data();
    save_data()?;
    
    // 初始化 current 目录（确保它存在并包含 assets 文件）
    let _ = init_current_dir();
    
    // 应用配置的主题
    let theme_id = get_data().theme.clone();
    let _ = apply_theme(&theme_id);
    
    // 确保主题路径始终设置为 current 目录，即使 apply_theme 失败
    if let Ok(dir) = dir::root() {
        let current_dir = format!("{}/themes/current", dir);
        set_theme_path(Some(current_dir));
    }

    let rx = {
        let (tx, rx) = mpsc::channel();
        *MESSAGES_TX.lock().unwrap() = Some(tx);
        rx
    };

    let aa_rx = {
        let (tx, rx) = mpsc::channel();
        *AA_TX.lock().unwrap() = Some(tx);
        rx
    };

    unsafe { get_internal_gl() }
        .quad_context
        .display_mut()
        .set_pause_resume_listener(on_pause_resume);

    if let Some(me) = &get_data().me {
        anti_addiction_action("startup", Some(format!("phira-{}", me.id)));
    }

    let pgr_font = FontArc::try_from_vec(load_file("phigros.ttf").await?)?;
    PGR_FONT.with(move |it| *it.borrow_mut() = Some(TextPainter::new(pgr_font, None)));

    let font = FontArc::try_from_vec(load_file("font.ttf").await?)?;
    let mut painter = TextPainter::new(font.clone(), None);

    let mut main = Main::new(Box::new(MainScene::new(font).await?), TimeManager::default(), None).await?;

    // 处理启动参数
    let mut join_room: Option<phira_mp_common::RoomId> = None;
    let mut create_room: Option<phira_mp_common::RoomId> = None;
    let mut mp_address: Option<String> = None;
    
    for arg in std::env::args() {
        let arg = arg.trim();
        
        // 处理多种格式的启动参数
        let processed_arg = if arg.starts_with("phira://") {
            // 处理 URI 格式：phira://room/join/123&server=...
            arg.strip_prefix("phira://").unwrap_or(arg)
        } else if arg.starts_with("room/") {
            // 处理没有前导斜杠的格式：room/join/123&server=...
            arg
        } else if arg.starts_with("/room/") {
            // 处理有前导斜杠的格式：/room/join/123&server=...
            arg.strip_prefix("/").unwrap_or(arg)
        } else {
            continue;
        };
        
        if processed_arg.starts_with("room/join/") {
            let mut parts = processed_arg.split("&");
            if let Some(room_part) = parts.next() {
                if let Some(room_id) = room_part.strip_prefix("room/join/") {
                    if let Ok(id) = room_id.to_string().try_into() {
                        join_room = Some(id);
                    }
                }
            }
            // 解析服务器地址
            for part in parts {
                if part.starts_with("server=") {
                    if let Some(address) = part.strip_prefix("server=") {
                        mp_address = Some(address.to_string());
                    }
                }
            }
        } else if processed_arg.starts_with("room/create/") {
            let mut parts = processed_arg.split("&");
            if let Some(room_part) = parts.next() {
                if let Some(room_id) = room_part.strip_prefix("room/create/") {
                    if let Ok(id) = room_id.to_string().try_into() {
                        create_room = Some(id);
                    }
                }
            }
            // 解析服务器地址
            for part in parts {
                if part.starts_with("server=") {
                    if let Some(address) = part.strip_prefix("server=") {
                        mp_address = Some(address.to_string());
                    }
                }
            }
        }
    }
    
    // 处理启动参数
    if join_room.is_some() || create_room.is_some() {
        use crate::scene::MP_PANEL;
        MP_PANEL.with(|it| {
            if let Some(panel) = it.borrow_mut().as_mut() {
                panel.handle_startup_args(join_room, create_room, mp_address);
            }
        });
    }

    let tm = TimeManager::default();
    let mut fps_time = -1;

    const FPS_BUF_SIZE: usize = 60;
    let mut fps_times = VecDeque::<f32>::with_capacity(FPS_BUF_SIZE);
    let mut last_frame_start = f32::NAN;
    let mut fps_time_sum = 0.;

    let mut exit_time = f64::INFINITY;

    'app: loop {
        let frame_start = tm.real_time();
        if !last_frame_start.is_nan() {
            if fps_times.len() == FPS_BUF_SIZE {
                fps_time_sum -= fps_times.pop_front().unwrap();
            }
            let frame_time = frame_start as f32 - last_frame_start;
            fps_times.push_back(frame_time);
            fps_time_sum += frame_time;
        }
        last_frame_start = frame_start as f32;
        let res = || -> Result<()> {
            main.update()?;
            main.render(&mut painter)?;
            if let Ok(paused) = rx.try_recv() {
                if paused {
                    main.pause()?;
                } else {
                    main.resume()?;
                }
            }
            Ok(())
        }();
        if let Err(err) = res {
            error!("uncaught error: {err:?}");
            show_error(err);
        }
        if main.should_exit() {
            break 'app;
        }

        if let Ok(code) = aa_rx.try_recv() {
            info!("anti addiction callback: {code}");
            match code {
                // login success
                500 => {
                    anti_addiction_action("enterGame", None);
                }
                // switch account
                1001 => {
                    anti_addiction_action("exit", None);
                    get_data_mut().me = None;
                    get_data_mut().tokens = None;
                    let _ = save_data();
                    sync_data();
                    use crate::login::L10N_LOCAL;
                    show_message(crate::login::tl!("logged-out")).ok();
                }
                // period restrict
                1030 => {
                    show_and_exit("你当前为未成年账号，已被纳入防沉迷系统。根据国家相关规定，周五、周六、周日及法定节假日 20 点 - 21 点之外为健康保护时段。当前时间段无法游玩，请合理安排时间。");
                    exit_time = frame_start;
                }
                // duration limit
                1050 => {
                    show_and_exit("你当前为未成年账号，已被纳入防沉迷系统。根据国家相关规定，周五、周六、周日及法定节假日 20 点 - 21 点之外为健康保护时段。你已达时间限制，无法继续游戏。");
                    exit_time = frame_start;
                }
                // stopped
                9002 => {
                    show_and_exit("必须实名认证方可进行游戏。");
                    exit_time = frame_start;
                }
                _ => {}
            }
        }

        let t = tm.real_time();

        if t > exit_time + 5. {
            break;
        }

        let fps_now = t as i32;
        if fps_now != fps_time {
            fps_time = fps_now;
            if fps_times.len() == FPS_BUF_SIZE {
                let actual_fps = 1. / (fps_time_sum / FPS_BUF_SIZE as f32);
                let current_fps = 1. / (t - frame_start);
                info!("FPS {} (capped at {})", current_fps as u32, actual_fps as u32);
            }
        }

        next_frame().await;
    }
    Ok(())
}

fn show_and_exit(msg: &str) {
    prpr::ui::Dialog::simple(msg)
        .buttons(vec!["确定".to_owned()])
        .listener(|_, _| std::process::exit(0))
        .show();
}

fn build_global_window_conf() -> Conf {
    let mut conf = build_conf();
    conf.window_title = "Phira".to_owned();
    conf.icon = Some(miniquad::conf::Icon {
        small: *include_bytes!("../icon/small"),
        medium: *include_bytes!("../icon/medium"),
        big: *include_bytes!("../icon/big"),
    });

    #[cfg(target_os = "windows")]
    {
        conf.fullscreen = dir::root()
            .ok()
            .and_then(|r| std::fs::read_to_string(std::path::Path::new(&r).join("data.json")).ok())
            .and_then(|s| serde_json::from_str::<Data>(&s).ok())
            .is_some_and(|d| d.config.fullscreen_mode);
    }

    conf
}

#[no_mangle]
pub extern "C" fn quad_main() {
    macroquad::Window::from_config(build_global_window_conf(), async {
        if let Err(err) = the_main().await {
            error!(?err, "global error");
        }
    });
}

fn on_pause_resume(pause: bool) {
    if let Some(tx) = MESSAGES_TX.lock().unwrap().as_mut() {
        let _ = tx.send(pause);
    }
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn Java_quad_1native_QuadNative_initializeEnvironment(env: EnvUnowned, _class: JClass) {
    unsafe {
        inputbox::backend::Android::initialize_raw(env.as_raw()).unwrap();
    }
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn Java_quad_1native_QuadNative_prprActivityOnPause(_env: EnvUnowned, _class: JClass) {
    anti_addiction_action("leaveGame", None);
    if let Some(tx) = MESSAGES_TX.lock().unwrap().as_mut() {
        let _ = tx.send(true);
    }
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn Java_quad_1native_QuadNative_prprActivityOnResume(_env: EnvUnowned, _class: JClass) {
    anti_addiction_action("enterGame", None);
    if let Some(tx) = MESSAGES_TX.lock().unwrap().as_mut() {
        let _ = tx.send(false);
    }
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn Java_quad_1native_QuadNative_prprActivityOnDestroy(_env: EnvUnowned, _class: JClass) {
    // std::process::exit(0);
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn Java_quad_1native_QuadNative_setDataPath(_env: EnvUnowned, _class: JClass, path: JString) {
    *DATA_PATH.lock().unwrap() = Some(path.to_string());
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn Java_quad_1native_QuadNative_setTempDir(_env: EnvUnowned, _class: JClass, path: JString) {
    let path = path.to_string();
    std::env::set_var("TMPDIR", path.clone());
    *CACHE_DIR.lock().unwrap() = Some(path);
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn Java_quad_1native_QuadNative_setDpi(_env: EnvUnowned, _class: JClass, dpi: jint) {
    prpr::core::DPI_VALUE.store(dpi as _, std::sync::atomic::Ordering::SeqCst);
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn Java_quad_1native_QuadNative_setChosenFile(_env: EnvUnowned, _class: JClass, file: JString) {
    use prpr::scene::CHOSEN_FILE;
    CHOSEN_FILE.lock().unwrap().1 = Some(file.to_string());
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn Java_quad_1native_QuadNative_markImport(_env: EnvUnowned, _class: JClass) {
    use prpr::scene::CHOSEN_FILE;

    CHOSEN_FILE.lock().unwrap().0 = Some("_import".to_owned());
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn Java_quad_1native_QuadNative_markImportRespack(_env: EnvUnowned, _class: JClass) {
    use prpr::scene::CHOSEN_FILE;

    CHOSEN_FILE.lock().unwrap().0 = Some("_import_respack".to_owned());
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn Java_quad_1native_QuadNative_setInputText(_env: EnvUnowned, _class: JClass, text: JString) {
    use prpr::scene::INPUT_TEXT;
    INPUT_TEXT.lock().unwrap().1 = Some(text.to_string());
}

#[cfg(not(all(target_os = "android", feature = "aa")))]
pub fn anti_addiction_action(_action: &str, _arg: Option<String>) {}

#[cfg(all(target_os = "android", feature = "aa"))]
pub fn anti_addiction_action(action: &str, arg: Option<String>) {
    use jni::{jni_sig, jni_str, objects::JObject, vm::JavaVM};

    JavaVM::singleton()
        .unwrap()
        .attach_current_thread(|env| -> jni::errors::Result<()> {
            let ctx = unsafe { JObject::from_raw(env, ndk_context::android_context().context() as _) };
            let action = env.new_string(action)?;
            #[allow(clippy::redundant_closure)]
            let arg = arg
                .as_ref()
                .map(|it| env.new_string(it))
                .transpose()?
                .map_or_else(|| JObject::null(), |s| s.into());
            env.call_method(ctx, jni_str!("antiAddiction"), jni_sig!("(Ljava/lang/String;Ljava/lang/String;)V"), &[(&action).into(), (&arg).into()])?;
            Ok(())
        })
        .unwrap();
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn Java_quad_1native_QuadNative_antiAddictionCallback(_env: EnvUnowned, _class: JClass, #[allow(dead_code)] code: jint) {
    if cfg!(feature = "aa") {
        if let Some(tx) = AA_TX.lock().unwrap().as_mut() {
            let _ = tx.send(code);
        }
    }
}

#[cfg(target_env = "ohos")]
#[napi]
pub fn set_input_text(text: String) {
    use prpr::scene::INPUT_TEXT;
    INPUT_TEXT.lock().unwrap().1 = Some(text);
}

#[cfg(target_env = "ohos")]
#[napi]
pub fn set_chosen_file(file: String) {
    use prpr::scene::CHOSEN_FILE;
    CHOSEN_FILE.lock().unwrap().1 = Some(file);
}

#[cfg(target_env = "ohos")]
#[napi]
pub fn mark_auto_import() {
    use prpr::scene::CHOSEN_FILE;
    CHOSEN_FILE.lock().unwrap().0 = Some("_import_auto".to_owned());
}
