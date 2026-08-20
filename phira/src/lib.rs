prpr_l10n::tl_file!("common" ttl crate::);

#[rustfmt::skip]
#[cfg(closed)]
mod inner;

mod anim;
mod charts_view;
mod client;
mod crash;
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
use scene::BootScene;
use std::{
    collections::VecDeque,
    sync::{mpsc, Mutex},
};
use tracing::{error, info};

#[cfg(target_os = "android")]
use jni::{
    objects::{JClass, JString},
    sys::jint,
    EnvUnowned, Outcome,
};

#[cfg(target_os = "android")]
#[no_mangle]
pub unsafe extern "C" fn Java_quad_1native_QuadNative_preprocessInput(
    _: *mut std::ffi::c_void,
    _: *const std::ffi::c_void,
    #[allow(dead_code)] motionEvent: miniquad::native::ndk_sys::AInputEvent,
    #[allow(dead_code)] f: miniquad::native::ndk_sys::jfloat,
    #[allow(dead_code)] f2: miniquad::native::ndk_sys::jfloat,
    #[allow(dead_code)] z: miniquad::native::ndk_sys::jboolean,
    #[allow(dead_code)] z2: miniquad::native::ndk_sys::jboolean,
) {
}

static MESSAGES_TX: Mutex<Option<mpsc::Sender<bool>>> = Mutex::new(None);
static AA_TX: Mutex<Option<mpsc::Sender<i32>>> = Mutex::new(None);
static DATA_PATH: Mutex<Option<String>> = Mutex::new(None);
static CACHE_DIR: Mutex<Option<String>> = Mutex::new(None);
#[cfg(target_os = "android")]
static STARTUP_ARGS: Mutex<Option<(Option<String>, Option<String>, Option<String>)>> = Mutex::new(None);
pub static mut DATA: Option<Data> = None;

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
    let locale = get_data()
        .language
        .as_ref()
        .and_then(|it| if it == "zh-LZH" { "lzh".parse().ok() } else { it.parse().ok() });
    set_prefered_locale(locale);
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
        let base = DATA_PATH.lock().unwrap().as_ref().map(|it| it.to_string());
        #[cfg(target_os = "android")]
        let base = base.or_else(|| {
            std::env::var("TMPDIR")
                .ok()
                .map(|tmpdir| tmpdir.trim_end_matches("/cache").to_string() + "/files")
        });
        let base = base.unwrap_or_else(|| ".".to_string());
        // 绝对路径（如 CACHE_DIR）直接使用，避免被错误拼到 DATA_PATH 下
        let s = if std::path::Path::new(s).is_absolute() {
            s.to_string()
        } else {
            format!("{}/{}", base, s)
        };
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

/// 加载崩溃界面字体。优先使用 font.ttf，缺失时回退到随包自带的其他 TTF 字体，
/// 确保即使主字体缺失，崩溃界面也能以非位图字体正常显示。
async fn load_crash_font() -> Option<macroquad::text::Font> {
    for name in ["font.ttf", "bold.ttf", "halva.ttf", "phigros.ttf"] {
        if let Ok(bytes) = load_file(name).await {
            if let Ok(font) = macroquad::text::load_ttf_font_from_bytes(&bytes) {
                info!("crash font loaded: {name}");
                return Some(font);
            }
        }
    }
    None
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
    crash::set_log_path(&format!("{dir}/crash.log"));
    let mut data: Data = std::fs::read_to_string(format!("{dir}/data.json"))
        .map_err(anyhow::Error::new)
        .and_then(|s| Ok(serde_json::from_str(&s)?))
        .unwrap_or_default();
    data.init().await?;
    set_data(data);
    sync_data();
    save_data()?;

    #[cfg(target_os = "windows")]
    {
        macroquad::window::set_fullscreen(get_data().config.fullscreen_mode);
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
    let mut crash_logo: Option<Texture2D> = match load_texture("crashlogo.png").await {
        Ok(tex) => {
            info!("crash logo loaded: {}x{}", tex.width(), tex.height());
            Some(tex)
        }
        Err(e) => {
            error!("failed to load crash logo via load_texture: {}", e);
            match load_file("crashlogo.png").await {
                Ok(bytes) => {
                    let image = Image::from_file_with_format(&bytes, Some(ImageFormat::Png));
                    info!("crash logo loaded via load_file: {}x{}", image.width, image.height);
                    Some(Texture2D::from_image(&image))
                }
                Err(e2) => {
                    error!("failed to load crash logo via load_file: {}", e2);
                    None
                }
            }
        }
    };
    let mut crash_font: Option<macroquad::text::Font> = load_crash_font().await;

    let mut main = Some(Main::new(Box::new(BootScene::new(font.clone()).await?), TimeManager::default(), None).await?);

    #[cfg(target_os = "android")]
    {
        // 处理来自深链接（phira://）的多人启动参数
        if let Some((join, create, server)) = STARTUP_ARGS.lock().unwrap().take() {
            use crate::scene::MP_PANEL;
            let join_room: Option<phira_mp_common::RoomId> = join.and_then(|s| s.try_into().ok());
            let create_room: Option<phira_mp_common::RoomId> = create.and_then(|s| s.try_into().ok());
            MP_PANEL.with(|it| {
                if let Some(panel) = it.borrow_mut().as_mut() {
                    panel.handle_startup_args(join_room, create_room, server);
                }
            });
        }
    }

    #[cfg(target_os = "windows")]
    {
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
    }

    let tm = TimeManager::default();
    let mut fps_time = -1;

    const FPS_BUF_SIZE: usize = 60;
    let mut fps_times = VecDeque::<f32>::with_capacity(FPS_BUF_SIZE);
    let mut last_frame_start = f32::NAN;
    let mut fps_time_sum = 0.;

    let mut exit_time = f64::INFINITY;
    let mut crashed = false;
    let mut crash_start_time = 0.0;

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

        if crashed {
            // 先恢复默认渲染状态，避免崩溃残留的 FBO/相机/视口把黑屏与崩溃界面渲染到离屏纹理（Android 白屏根因）
            crash::reset_gl_state();
            let elapsed = get_time() - crash_start_time;
            if elapsed < 1.5 {
                // 先黑屏 1.5 秒
                clear_background(BLACK);
            } else {
                // 再显示白色崩溃界面
                crash::render_crash_screen(crash_logo, crash_font);
            }
            next_frame().await;
            continue;
        }

        // 处理深链接（phira://）启动参数，app 运行中也生效
        #[cfg(target_os = "android")]
        if let Some((join, create, server)) = STARTUP_ARGS.lock().unwrap().take() {
            use crate::scene::MP_PANEL;
            let join_room: Option<phira_mp_common::RoomId> = join.and_then(|s| s.try_into().ok());
            let create_room: Option<phira_mp_common::RoomId> = create.and_then(|s| s.try_into().ok());
            MP_PANEL.with(|it| {
                if let Some(panel) = it.borrow_mut().as_mut() {
                    panel.handle_startup_args(join_room, create_room, server);
                }
            });
        }

        let frame_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<()> {
            let m = main.as_mut().unwrap();
            m.update()?;
            m.render(&mut painter)?;
            if let Ok(paused) = rx.try_recv() {
                if paused {
                    m.pause()?;
                } else {
                    m.resume()?;
                }
            }
            Ok(())
        }));

        match frame_result {
            Ok(Ok(())) => {}
            Ok(Err(err)) => {
                error!("uncaught error: {err:?}");
                show_error(err);
            }
            Err(payload) => {
                crashed = true;
                crash_start_time = get_time();
                // 兜底：直接从 panic payload 提取崩溃详情写入 CRASH_INFO + crash.log，
                // 避免 panic 钩子因故未填充时崩溃界面拿不到任何信息。
                crash::capture_panic(payload);
                crash_logo = load_texture("crashlogo.png").await.ok();
                if crash_logo.is_none() {
                    crash_logo = load_file("crashlogo.png").await.ok().map(|bytes| {
                        let image = Image::from_file_with_format(&bytes, Some(ImageFormat::Png));
                        Texture2D::from_image(&image)
                    });
                }
                // 优先复用启动时已加载的崩溃字体（其图集在崩溃前已正常上传 GPU）。
                // 仅当其为空时才重新加载，避免崩溃后 load_file 失败回退到可能失效的默认位图字体。
                if crash_font.is_none() {
                    crash_font = load_crash_font().await;
                }
                if let Some(m) = main.take() {
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        drop(m);
                    }));
                }
                clear_background(WHITE);
            }
        }

        if !crashed && main.as_ref().map(|m| m.should_exit()).unwrap_or(false) {
            break 'app;
        }

        if !crashed {
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

#[cfg(not(target_os = "android"))]
fn load_icon_from_png() -> Option<miniquad::conf::Icon> {
    let bytes = include_bytes!("../../assets/icon.png");
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let small = image::imageops::resize(&img, 16, 16, image::imageops::FilterType::Lanczos3).into_raw();
    let medium = image::imageops::resize(&img, 32, 32, image::imageops::FilterType::Lanczos3).into_raw();
    let big = image::imageops::resize(&img, 64, 64, image::imageops::FilterType::Lanczos3).into_raw();
    Some(miniquad::conf::Icon {
        small: small.try_into().ok()?,
        medium: medium.try_into().ok()?,
        big: big.try_into().ok()?,
    })
}

#[cfg(target_os = "android")]
fn load_icon_from_png() -> Option<miniquad::conf::Icon> {
    None
}

fn build_global_window_conf() -> Conf {
    let mut conf = build_conf();
    conf.window_title = "Phira-Firefly".to_owned();
    conf.icon = load_icon_from_png().or_else(|| {
        Some(miniquad::conf::Icon {
            small: *include_bytes!("../icon/small"),
            medium: *include_bytes!("../icon/medium"),
            big: *include_bytes!("../icon/big"),
        })
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
    crash::set_panic_hook();
    macroquad::Window::from_config(build_global_window_conf(), async {
        if let Err(err) = the_main().await {
            error!(?err, "global error");
            crash::set_error(&err.to_string());

            // 尝试加载崩溃界面资源
            let mut crash_logo = load_texture("crashlogo.png").await.ok();
            if crash_logo.is_none() {
                crash_logo = load_file("crashlogo.png").await.ok().map(|bytes| {
                    let image = Image::from_file_with_format(&bytes, Some(ImageFormat::Png));
                    Texture2D::from_image(&image)
                });
            }
            let crash_font = load_crash_font().await;

            // 崩溃渲染循环：先黑屏再显示崩溃界面
            let crash_start_time = get_time();
            loop {
                crash::reset_gl_state();
                let elapsed = get_time() - crash_start_time;
                if elapsed < 1.5 {
                    clear_background(BLACK);
                } else {
                    crash::render_crash_screen(crash_logo, crash_font);
                }
                next_frame().await;
            }
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
pub extern "C" fn Java_quad_1native_QuadNative_setDataPath(mut env: EnvUnowned, _class: JClass, path: JString) {
    let s = match env.with_env_no_catch(|env| path.try_to_string(env)).into_outcome() {
        Outcome::Ok(s) => s,
        _ => String::new(),
    };
    *DATA_PATH.lock().unwrap() = Some(s);
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn Java_quad_1native_QuadNative_setTempDir(mut env: EnvUnowned, _class: JClass, path: JString) {
    let path = match env.with_env_no_catch(|env| path.try_to_string(env)).into_outcome() {
        Outcome::Ok(s) => s,
        _ => String::new(),
    };
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
pub extern "C" fn Java_quad_1native_QuadNative_setChosenFile(mut env: EnvUnowned, _class: JClass, file: JString) {
    use prpr::scene::CHOSEN_FILE;
    let file = match env.with_env_no_catch(|env| file.try_to_string(env)).into_outcome() {
        Outcome::Ok(s) => s,
        _ => String::new(),
    };
    CHOSEN_FILE.lock().unwrap().1 = Some(file);
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
pub extern "C" fn Java_quad_1native_QuadNative_markAutoImport(_env: EnvUnowned, _class: JClass) {
    use prpr::scene::CHOSEN_FILE;

    // 自动识别：zip 内含 click.png 视为资源包，否则视为谱面
    CHOSEN_FILE.lock().unwrap().0 = Some("_import_auto".to_owned());
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn Java_quad_1native_QuadNative_setInputText(mut env: EnvUnowned, _class: JClass, text: JString) {
    use prpr::scene::INPUT_TEXT;
    let text = match env.with_env_no_catch(|env| text.try_to_string(env)).into_outcome() {
        Outcome::Ok(s) => s,
        _ => String::new(),
    };
    INPUT_TEXT.lock().unwrap().1 = Some(text);
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn Java_quad_1native_QuadNative_setStartupArgs(mut env: EnvUnowned, _class: JClass, join: JString, create: JString, server: JString) {
    let join = if join.is_null() {
        None
    } else {
        match env.with_env_no_catch(|e| join.try_to_string(e)).into_outcome() {
            Outcome::Ok(v) => Some(v),
            _ => None,
        }
    };
    let create = if create.is_null() {
        None
    } else {
        match env.with_env_no_catch(|e| create.try_to_string(e)).into_outcome() {
            Outcome::Ok(v) => Some(v),
            _ => None,
        }
    };
    let server = if server.is_null() {
        None
    } else {
        match env.with_env_no_catch(|e| server.try_to_string(e)).into_outcome() {
            Outcome::Ok(v) => Some(v),
            _ => None,
        }
    };
    *STARTUP_ARGS.lock().unwrap() = Some((join, create, server));
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
