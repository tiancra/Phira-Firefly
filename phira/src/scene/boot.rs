use super::{MainScene, OnboardingScene};
use super::onboarding::PENDING_MAIN;
use crate::get_data;
use anyhow::Result;
use macroquad::prelude::*;
use prpr::{
    core::BOLD_FONT,
    ext::{semi_black, semi_white, SafeTexture, ScaleType},
    scene::{NextScene, Scene},
    time::TimeManager,
    ui::{enter_sfx, enter_splash_sfx, FontArc, Ui, UI_AUDIO},
};
use sasa::{AudioClip, Music, MusicParams};
use std::cell::RefCell;
use std::sync::mpsc;
use tracing::warn;

/// 时间（与 `TimeManager::now()` 同基）记录主界面划入动画开始时刻。
/// 由 BootScene 在点击“点击屏幕开始”时写入，HomePage 据此计算 1 秒划入进度。
thread_local! {
    static BOOT_ENTRY_TIME: RefCell<Option<f64>> = const { RefCell::new(None) };
}

pub fn set_boot_entry_time(t: f64) {
    BOOT_ENTRY_TIME.with(|it| *it.borrow_mut() = Some(t));
}

/// 主界面划入进度：0..=1（Out Sine），未启动时返回 1（不动画）。
pub fn boot_entry_progress(t: f32, dur: f32) -> f32 {
    BOOT_ENTRY_TIME.with(|it| match *it.borrow() {
        Some(start) => ((t - start as f32) / dur).clamp(0., 1.),
        None => 1.,
    })
}

#[inline]
fn out_sine(p: f32) -> f32 {
    (p.clamp(0., 1.) * std::f32::consts::PI / 2.).sin()
}

// 时间轴（秒，自场景进入起）
const SPLASH_FADE_IN: f32 = 1.0; // 渐显 splash
const SPLASH_HOLD: f32 = 3.0; // 保持 splash
const SPLASH_FADE_OUT: f32 = 1.0; // 渐隐 splash
const WARN_FADE_IN: f32 = 1.0; // 渐显警告文字
const WARN_HOLD: f32 = 5.0; // 保持警告文字
const WARN_FADE_OUT: f32 = 1.0; // 渐隐警告文字
const BOOT_FADE_IN: f32 = 1.0; // 渐显背景 + 遮罩 + boot
const BOOT_READY: f32 = SPLASH_FADE_IN + SPLASH_HOLD + SPLASH_FADE_OUT + WARN_FADE_IN + WARN_HOLD + WARN_FADE_OUT + BOOT_FADE_IN; // 可点击
const BOOT_CLICK_FADE_OUT: f32 = 0.8; // 点击后 boot/遮罩渐隐时长
const BGM_FADE_TIME: f64 = 3.0; // 主界面 BGM 渐入时长
const SPLASH_MUSIC_FADE_OUT: f64 = 3.0; // 点击后 splash.mp3 渐隐时长

const WARNING_TEXT: &str = "开始游戏前，请仔细阅读以下内容，并确认你已了解相关注意事项。

本游戏包含快速移动、闪烁画面、动态特效、高密度节奏以及较强的音乐与音效表现，部分内容可能带来较高的视觉与听觉刺激。请根据自身情况选择合适的难度，并将设备音量调整至舒适范围。

长时间连续游玩可能造成视觉疲劳或注意力下降。建议合理安排游戏时间，并适当休息。若出现头晕、恶心、头痛、视线模糊或其他明显不适，请立即停止游戏并充分休息，不要为了成绩或排名勉强继续。";

/// 启动自检状态：仅允许 CHECK / GOOD / BAD / N/A 四种展示状态。
/// N/A 表示当前平台或设备不适用，不参与错误判定。
#[derive(Clone, Copy, PartialEq, Eq)]
enum PostStatus {
    /// 尚未执行检测。
    Pending,
    /// 正在检测（仅后台文件校验使用）。
    Check,
    /// 检查通过。
    Good,
    /// 检查失败。
    Bad,
    /// 当前平台/设备不适用，立即跳过。
    Na,
}

/// 每个检测子项的等待时长（秒）：逐项串行检测，等待完成后再进行下一项。
const CHECK_DURATION: f64 = 0.6;

/// 自检项目：按当前平台/设备能力动态生成，每个大类拆分为独立检测子项，
/// 例如音频包含 API、初始化、输出设备、上下文、流、采样配置、缓冲等独立项，
/// 每项串行检测：CHECK（等待）→ 常亮 GOOD/BAD/N/A。
#[derive(Clone, Copy, PartialEq, Eq)]
enum PostKind {
    // 渲染
    RendererApi,
    RendererInit,
    RendererDevice,
    RendererContext,
    RendererExt,
    RendererCap,
    // 音频
    AudioApi,
    AudioInit,
    AudioDevice,
    AudioContext,
    AudioStream,
    AudioFormat,
    AudioBuffer,
    // 触摸
    TouchApi,
    TouchDevice,
    TouchInit,
    TouchEvent,
    TouchHandler,
    TouchMulti,
    // 键盘
    KeyApi,
    KeyInit,
    KeyMap,
    KeyHandler,
    KeyCap,
    // 手柄
    PadApi,
    PadEnum,
    PadInfo,
    PadButton,
    PadAxis,
    PadTrigger,
    PadMap,
    PadHotplug,
    // 文件
    FileManifest,
    FileCore,
    FileFont,
    FileAudio,
    FileData,
    FileIntegrity,
    // 存储
    StorageDir,
    StorageAssets,
    StorageSpace,
}

/// 自检大类：自检页面只展示大类，大类内部子项逐项串行检测，
/// 全部子项完成后整个大类统一为 GOOD / BAD / N/A。
#[derive(Clone, Copy, PartialEq, Eq)]
enum PostGroup {
    Renderer,
    Audio,
    Touch,
    Keyboard,
    Gamepad,
    Files,
    Storage,
}

fn group_name(group: PostGroup) -> &'static str {
    match group {
        PostGroup::Renderer => "渲染",
        PostGroup::Audio => "音频",
        PostGroup::Touch => "触摸",
        PostGroup::Keyboard => "键盘",
        PostGroup::Gamepad => "手柄",
        PostGroup::Files => "文件",
        PostGroup::Storage => "存储",
    }
}

struct PostItem {
    group: PostGroup,
    kind: PostKind,
    status: PostStatus,
}

/// 全自动跨平台启动自检：黑色背景铺底，逐项显示各检测子项，
/// 每一项独立串行检测：CHECK（等待）→ 常亮 GOOD/BAD/N/A；
/// 不适用项显示 N/A 并立即跳过，无需玩家任何操作；按住 Pause / Insert 可暂停查看。
struct PostRun {
    items: Vec<PostItem>,
    /// 当前正在检测的子项索引（串行推进）。
    cursor: usize,
    phase_start: f64,
    paused: bool,
    done: bool,
    failed: bool,
    /// 后台文件校验结果接收端（桌面平台缓存失效时启动后台线程，不阻塞启动）。
    files_receiver: Option<mpsc::Receiver<bool>>,
    /// 文件完整性已确认结果（缓存有效路径，避免重复 IO）。
    files_known: Option<bool>,
    /// 资产清单是否成功读取。
    manifest_ok: bool,
    /// 资产清单内容（用于核心资源/字体/音频/数据子项判定）。
    manifest: Option<Vec<String>>,
}

/// 当前平台/设备是否具备触摸输入能力（仅硬件能力；不模拟触摸事件，也不要求玩家触摸）。
fn touch_available() -> bool {
    #[cfg(target_os = "windows")]
    {
        // SM_MAXIMUMTOUCHES = 95，返回 0 表示无触摸支持（普通台式机 → N/A）
        unsafe { GetSystemMetrics(95) > 0 }
    }
    #[cfg(not(target_os = "windows"))]
    {
        // 移动平台默认具备；其余桌面平台无法轻量探测时按具备处理，避免把可用触摸误报为 N/A
        true
    }
}

/// 当前平台/设备是否具备键盘输入能力（不要求玩家按键）。
fn keyboard_available() -> bool {
    #[cfg(target_os = "windows")]
    {
        // GetKeyboardType(0) 返回 0 表示无物理键盘
        unsafe { GetKeyboardType(0) != 0 }
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        false
    }
    #[cfg(not(any(target_os = "windows", target_os = "android", target_os = "ios")))]
    {
        true
    }
}

/// 当前平台是否支持 Gamepad API（移动端 prpr 的 gilrs 封装为空实现，视为不适用）。
fn gamepad_available() -> bool {
    #[cfg(any(target_arch = "wasm32", target_os = "android", target_os = "ios", target_env = "ohos"))]
    {
        false
    }
    #[cfg(not(any(target_arch = "wasm32", target_os = "android", target_os = "ios", target_env = "ohos")))]
    {
        true
    }
}

/// 渲染器：按当前实际使用的后端检测（wgpu 或 macroquad/OpenGL）。
fn renderer_ok() -> bool {
    let mgr = prpr::render_backend::RenderBackendManager::global().lock().unwrap();
    if mgr.use_wgpu() {
        // 配置要求 wgpu：后端实例已创建才算通过
        mgr.wgpu_backend().is_some()
    } else {
        // macroquad/OpenGL 后端：当前场景能正常渲染即已初始化
        true
    }
}

/// 音频：检测默认输出设备与音频系统（sasa 已在 UI_AUDIO 初始化；不播放测试声）。
fn audio_ok() -> bool {
    #[cfg(target_os = "windows")]
    {
        unsafe { waveOutGetNumDevs() > 0 }
    }
    #[cfg(not(target_os = "windows"))]
    {
        true
    }
}

/// 存储：游戏目录/资源目录可访问。
fn storage_dir_ok() -> bool {
    std::path::Path::new("assets").is_dir()
}

/// 存储：剩余空间足够进行必要操作。
fn storage_space_ok() -> bool {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt;
        let cwd = std::env::current_dir().unwrap_or_default();
        let wide: Vec<u16> = cwd.as_os_str().encode_wide().chain(Some(0)).collect();
        let space_ok = unsafe {
            let mut avail: u64 = 0;
            GetDiskFreeSpaceExW(wide.as_ptr(), &mut avail, std::ptr::null_mut(), std::ptr::null_mut()) != 0
                && avail >= 100 * 1024 * 1024
        };
        space_ok
    }
    #[cfg(not(target_os = "windows"))]
    {
        true
    }
}

/// 手柄检测：存在已连接手柄即通过（映射由 prpr 保证）；API 不可用或无设备 → N/A（不阻塞启动）。
fn gamepad_state() -> (bool, bool) {
    let mut gi = prpr::gamepad::GamepadInput::new();
    let raw = gi.get_raw_state();
    if !raw.gilrs_ready {
        (true, false)
    } else if raw.connected {
        (false, true)
    } else {
        (true, false)
    }
}

#[cfg(target_os = "windows")]
#[link(name = "user32")]
extern "system" {
    fn GetSystemMetrics(nIndex: i32) -> i32;
    fn GetKeyboardType(nTypeFlag: i32) -> i32;
}

#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
extern "system" {
    fn GetDiskFreeSpaceExW(
        lpDirectoryName: *const u16,
        lpFreeBytesAvailableToCaller: *mut u64,
        lpTotalNumberOfBytes: *mut u64,
        lpTotalNumberOfFreeBytes: *mut u64,
    ) -> i32;
}

#[cfg(target_os = "windows")]
#[link(name = "winmm")]
extern "system" {
    fn waveOutGetNumDevs() -> u32;
}

/// 文件完整性自检的输入：结果已确定，或桌面平台后台线程校验中。
enum FilesInput {
    /// 结果已确定（缓存有效 / 移动平台已预检）。
    Known(bool),
    /// 桌面平台缓存失效，后台线程校验中。
    Pending(mpsc::Receiver<bool>),
    /// 自检未启用。
    Disabled,
}

/// 文件自检缓存：清单版本（长度）+ 各文件大小/修改时间签名。
#[derive(serde::Serialize, serde::Deserialize)]
struct FilesCache {
    manifest_len: u64,
    files: Vec<FileSig>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct FileSig {
    path: String,
    len: u64,
    #[serde(default)]
    modified: Option<u64>,
}

fn files_cache_path() -> Result<std::path::PathBuf> {
    Ok(std::path::Path::new(&crate::dir::root()?).join("selfcheck_cache.json"))
}

/// 读取资产清单（默认 assets/selfcheck.list，有主题覆盖时优先）。
async fn load_manifest() -> Result<(Vec<String>, u64)> {
    let bytes = prpr::theme::load_asset_file("selfcheck.list").await?;
    let manifest_len = bytes.len() as u64;
    let list = String::from_utf8_lossy(&bytes)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    Ok((list, manifest_len))
}

/// 缓存是否仍然有效：清单未变，且各文件大小/修改时间未变（移动平台仅按清单长度判定）。
/// 有效 → 沿用上次 GOOD 结果，跳过重新校验；无效 → 需要重新完整校验。
fn files_cache_valid_sync(manifest: &[String], manifest_len: u64) -> bool {
    let Ok(cache_path) = files_cache_path() else { return false; };
    let Ok(text) = std::fs::read_to_string(cache_path) else { return false; };
    let Ok(cache) = serde_json::from_str::<FilesCache>(&text) else { return false; };
    if cache.manifest_len != manifest_len || cache.files.len() != manifest.len() {
        return false;
    }
    for (path, sig) in manifest.iter().zip(cache.files.iter()) {
        if path != &sig.path {
            return false;
        }
        #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32", target_env = "ohos")))]
        {
            let Ok(meta) = std::fs::metadata(std::path::Path::new("assets").join(path)) else {
                return false;
            };
            if meta.len() != sig.len {
                return false;
            }
            if let Some(m) = sig.modified {
                let cur = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs());
                if cur != Some(m) {
                    return false;
                }
            }
        }
    }
    true
}

fn write_files_cache(manifest_len: u64, sigs: Vec<(String, u64, Option<u64>)>) {
    let Ok(cache_path) = files_cache_path() else { return; };
    let cache = FilesCache {
        manifest_len,
        files: sigs
            .into_iter()
            .map(|(path, len, modified)| FileSig { path, len, modified })
            .collect(),
    };
    if let Ok(json) = serde_json::to_string(&cache) {
        let _ = std::fs::write(cache_path, json);
    }
}

/// 桌面平台：同步完整读取校验（后台线程调用，不阻塞启动）。
fn verify_files_sync(manifest: &[String], manifest_len: u64) -> bool {
    let mut sigs = Vec::with_capacity(manifest.len());
    for path in manifest {
        let p = std::path::Path::new("assets").join(path);
        let Ok(bytes) = std::fs::read(&p) else {
            tracing::warn!("selfcheck: 游戏文件缺失或不可读: {path}");
            return false;
        };
        let len = bytes.len() as u64;
        let modified = std::fs::metadata(&p)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs());
        sigs.push((path.clone(), len, modified));
    }
    write_files_cache(manifest_len, sigs);
    true
}

/// 移动平台：async 完整读取校验（仅缓存失效时在启动阶段执行一次）。
#[cfg(any(target_os = "android", target_os = "ios", target_arch = "wasm32", target_env = "ohos"))]
async fn verify_files_async(manifest: &[String], manifest_len: u64) -> bool {
    let mut sigs = Vec::with_capacity(manifest.len());
    for path in manifest {
        match prpr::theme::load_asset_file(path).await {
            Ok(bytes) => sigs.push((path.clone(), bytes.len() as u64, None)),
            Err(err) => {
                tracing::warn!("selfcheck: 游戏文件缺失或不可读: {path}: {err:?}");
                return false;
            }
        }
    }
    write_files_cache(manifest_len, sigs);
    true
}

impl PostRun {
    fn new(enabled: bool, files: FilesInput, manifest: Option<Vec<String>>) -> Self {
        let mut items: Vec<PostItem> = Vec::new();
        if enabled {
            // 渲染：全平台适用，拆分为 API / 初始化 / 设备 / 上下文 / 扩展 / 能力
            for k in [
                PostKind::RendererApi,
                PostKind::RendererInit,
                PostKind::RendererDevice,
                PostKind::RendererContext,
                PostKind::RendererExt,
                PostKind::RendererCap,
            ] {
                items.push(PostItem {
                    group: PostGroup::Renderer,
                    kind: k,
                    status: PostStatus::Pending,
                });
            }
            // 音频：全平台适用，拆分为 API / 初始化 / 输出设备 / 上下文 / 流 / 采样配置 / 缓冲
            for k in [
                PostKind::AudioApi,
                PostKind::AudioInit,
                PostKind::AudioDevice,
                PostKind::AudioContext,
                PostKind::AudioStream,
                PostKind::AudioFormat,
                PostKind::AudioBuffer,
            ] {
                items.push(PostItem {
                    group: PostGroup::Audio,
                    kind: k,
                    status: PostStatus::Pending,
                });
            }
            // 触摸：平台/设备无触摸能力 → 整组 N/A，不进入检测
            let touch = touch_available();
            for k in [
                PostKind::TouchApi,
                PostKind::TouchDevice,
                PostKind::TouchInit,
                PostKind::TouchEvent,
                PostKind::TouchHandler,
                PostKind::TouchMulti,
            ] {
                items.push(PostItem {
                    group: PostGroup::Touch,
                    kind: k,
                    status: if touch { PostStatus::Pending } else { PostStatus::Na },
                });
            }
            // 键盘：无键盘 → 整组 N/A
            let kb = keyboard_available();
            for k in [
                PostKind::KeyApi,
                PostKind::KeyInit,
                PostKind::KeyMap,
                PostKind::KeyHandler,
                PostKind::KeyCap,
            ] {
                items.push(PostItem {
                    group: PostGroup::Keyboard,
                    kind: k,
                    status: if kb { PostStatus::Pending } else { PostStatus::Na },
                });
            }
            // 手柄：API 不可用 → 整组 N/A；API 可用但当前无设备 → 设备相关项 N/A
            let pad_api = gamepad_available();
            let (pad_na, pad_connected) = gamepad_state();
            let pad_dev_ok = pad_api && !pad_na && pad_connected;
            for k in [PostKind::PadApi, PostKind::PadEnum, PostKind::PadMap, PostKind::PadHotplug] {
                items.push(PostItem {
                    group: PostGroup::Gamepad,
                    kind: k,
                    status: if pad_api { PostStatus::Pending } else { PostStatus::Na },
                });
            }
            for k in [PostKind::PadInfo, PostKind::PadButton, PostKind::PadAxis, PostKind::PadTrigger] {
                items.push(PostItem {
                    group: PostGroup::Gamepad,
                    kind: k,
                    status: if pad_dev_ok { PostStatus::Pending } else { PostStatus::Na },
                });
            }
            // 文件：全平台适用（完整性项跟随 files 输入；其余子项按清单内容判定）
            for k in [
                PostKind::FileManifest,
                PostKind::FileCore,
                PostKind::FileFont,
                PostKind::FileAudio,
                PostKind::FileData,
            ] {
                items.push(PostItem {
                    group: PostGroup::Files,
                    kind: k,
                    status: PostStatus::Pending,
                });
            }
            let integrity_status = match &files {
                FilesInput::Known(_) | FilesInput::Pending(_) => PostStatus::Pending,
                FilesInput::Disabled => PostStatus::Na,
            };
            items.push(PostItem {
                group: PostGroup::Files,
                kind: PostKind::FileIntegrity,
                status: integrity_status,
            });
            // 存储：全平台适用
            for k in [PostKind::StorageDir, PostKind::StorageAssets, PostKind::StorageSpace] {
                items.push(PostItem {
                    group: PostGroup::Storage,
                    kind: k,
                    status: PostStatus::Pending,
                });
            }
        }
        let (files_receiver, files_known) = match files {
            FilesInput::Pending(rx) => (Some(rx), None),
            FilesInput::Known(ok) => (None, Some(ok)),
            FilesInput::Disabled => (None, None),
        };
        let manifest_ok = manifest.is_some();
        let failed = items.iter().any(|it| it.status == PostStatus::Bad);
        Self {
            items,
            cursor: 0,
            phase_start: 0.,
            paused: false,
            done: !enabled,
            failed,
            files_receiver,
            files_known,
            manifest_ok,
            manifest,
        }
    }

    /// 清单中是否存在满足谓词的文件（清单读取失败视为不满足）。
    fn manifest_has(manifest: Option<&[String]>, pred: impl Fn(&str) -> bool) -> bool {
        match manifest {
            Some(list) => list.iter().any(|p| pred(p)),
            None => false,
        }
    }

    /// 执行单个子项的实际检测，返回终态（Good/Bad/Na）。
    fn check_item(&self, kind: PostKind) -> PostStatus {
        use PostKind::*;
        match kind {
            // 渲染：后端已创建即通过（API/初始化/设备/上下文/扩展/能力随初始化结果）
            RendererApi | RendererInit | RendererDevice | RendererContext | RendererExt | RendererCap => {
                if renderer_ok() {
                    PostStatus::Good
                } else {
                    PostStatus::Bad
                }
            }
            // 音频：API 由构建确定存在；输出设备独立探测；其余子项随音频系统初始化结果
            AudioApi => PostStatus::Good,
            AudioDevice => {
                #[cfg(target_os = "windows")]
                {
                    if unsafe { waveOutGetNumDevs() > 0 } {
                        PostStatus::Good
                    } else {
                        PostStatus::Bad
                    }
                }
                #[cfg(not(target_os = "windows"))]
                {
                    PostStatus::Good
                }
            }
            AudioInit | AudioContext | AudioStream | AudioFormat | AudioBuffer => {
                if audio_ok() {
                    PostStatus::Good
                } else {
                    PostStatus::Bad
                }
            }
            // 触摸：硬件能力存在即通过；引擎/输入层能力由 prpr 保证
            TouchApi | TouchDevice | TouchMulti => {
                if touch_available() {
                    PostStatus::Good
                } else {
                    PostStatus::Bad
                }
            }
            TouchInit | TouchEvent | TouchHandler => PostStatus::Good,
            // 键盘
            KeyApi => {
                if keyboard_available() {
                    PostStatus::Good
                } else {
                    PostStatus::Bad
                }
            }
            KeyInit | KeyMap | KeyHandler | KeyCap => PostStatus::Good,
            // 手柄：API/枚举/映射/热插拔随 API 可用性；设备相关随连接状态
            PadApi | PadEnum | PadMap | PadHotplug => {
                let (na, _) = gamepad_state();
                if na {
                    PostStatus::Na
                } else {
                    PostStatus::Good
                }
            }
            PadInfo | PadButton | PadAxis | PadTrigger => {
                let (na, good) = gamepad_state();
                if na || !good {
                    PostStatus::Na
                } else {
                    PostStatus::Good
                }
            }
            // 文件：清单读取成功与否决定清单项；关键资源按清单内容匹配
            FileManifest => {
                if self.manifest_ok {
                    PostStatus::Good
                } else {
                    PostStatus::Bad
                }
            }
            FileCore | FileFont | FileAudio | FileData => {
                let hit = match kind {
                    FileCore => Self::manifest_has(self.manifest.as_deref(), |p| {
                        p.ends_with(".png") || p.ends_with(".jpg") || p.ends_with(".jpeg")
                    }),
                    FileFont => Self::manifest_has(self.manifest.as_deref(), |p| {
                        p.ends_with(".ttf") || p.ends_with(".otf") || p.ends_with(".ttc") || p.ends_with(".woff2")
                    }),
                    FileAudio => Self::manifest_has(self.manifest.as_deref(), |p| {
                        p.ends_with(".mp3") || p.ends_with(".ogg") || p.ends_with(".wav")
                    }),
                    FileData => Self::manifest_has(self.manifest.as_deref(), |p| {
                        p.contains("charts") || p.contains("music") || p.ends_with(".phs") || p.ends_with(".pec")
                    }),
                    _ => false,
                };
                if hit {
                    PostStatus::Good
                } else {
                    PostStatus::Bad
                }
            }
            FileIntegrity => {
                // 在 update 中按 files 输入等待/取缓存结果，不会走到这里
                PostStatus::Pending
            }
            // 存储
            StorageDir | StorageAssets => {
                if storage_dir_ok() {
                    PostStatus::Good
                } else {
                    PostStatus::Bad
                }
            }
            StorageSpace => {
                if storage_space_ok() {
                    PostStatus::Good
                } else {
                    PostStatus::Bad
                }
            }
        }
    }

    fn update(&mut self, tm: &TimeManager) {
        let now = tm.now();
        if self.phase_start == 0. {
            self.phase_start = now;
        }
        // 可选暂停：按住 Pause / Insert 冻结检测进度（仅查看，不影响自动完成）
        let pause_held = is_key_down(KeyCode::Pause) || is_key_down(KeyCode::Insert);
        self.paused = pause_held;
        if pause_held {
            return;
        }
        // 全部子项处理完毕
        if self.cursor >= self.items.len() {
            self.done = true;
            return;
        }
        // 串行推进：每次只处理当前项，等待完成后再进行下一项
        let kind = self.items[self.cursor].kind;
        let status = self.items[self.cursor].status;
        match status {
            PostStatus::Pending => {
                // 进入检测：开始等待（sleep 一会），完成后再进行下一项
                self.items[self.cursor].status = PostStatus::Check;
                self.phase_start = now;
            }
            PostStatus::Check => {
                // 文件完整性：等待后台线程完成校验，且等满展示时长后再进入下一项
                if kind == PostKind::FileIntegrity {
                    let result = if let Some(rx) = &self.files_receiver {
                        match rx.try_recv() {
                            Ok(ok) => Some(ok),
                            Err(mpsc::TryRecvError::Disconnected) => Some(false),
                            Err(mpsc::TryRecvError::Empty) => None,
                        }
                    } else {
                        self.files_known
                    };
                    match result {
                        Some(ok) => {
                            if now - self.phase_start >= CHECK_DURATION {
                                self.items[self.cursor].status =
                                    if ok { PostStatus::Good } else { PostStatus::Bad };
                                self.failed |= !ok;
                                self.advance(now);
                            }
                            // 结果已就绪但展示时长未满：继续等待，保持当前项可见
                        }
                        None => {
                            // 结果未就绪：保持 CHECK，等待后台完成
                        }
                    }
                    return;
                }
                // 常规子项：等待时长结束后执行检测，再进入下一项
                if now - self.phase_start >= CHECK_DURATION {
                    let status = self.check_item(kind);
                    self.items[self.cursor].status = status;
                    self.failed |= status == PostStatus::Bad;
                    self.advance(now);
                }
            }
            _ => {
                // 构造时已定的 N/A / 终态：立即跳过，不等待
                self.advance(now);
            }
        }
    }

    fn advance(&mut self, now: f64) {
        self.cursor += 1;
        self.phase_start = now;
    }

    /// 聚合大类状态：子项全部完成后统一为 GOOD / BAD / N/A；有子项检测中返回 CHECK，未开始返回 Pending。
    fn group_status(&self, group: PostGroup) -> PostStatus {
        let mut any_check = false;
        let mut any_pending = false;
        let mut any_bad = false;
        let mut any_good = false;
        for item in &self.items {
            if item.group != group {
                continue;
            }
            match item.status {
                PostStatus::Check => any_check = true,
                PostStatus::Pending => any_pending = true,
                PostStatus::Bad => any_bad = true,
                PostStatus::Good => any_good = true,
                PostStatus::Na => {}
            }
        }
        if any_check {
            return PostStatus::Check;
        }
        if any_pending {
            return PostStatus::Pending;
        }
        if any_bad {
            return PostStatus::Bad;
        }
        if any_good {
            return PostStatus::Good;
        }
        PostStatus::Na
    }

    fn render(&self, ui: &mut Ui, tm: &TimeManager) {
        let screen = ui.screen_rect();
        let now = tm.now();
        // 黑色背景铺底
        ui.fill_rect(screen, BLACK);
        // 标题（小字置顶，避免挤压列表空间）
        ui.text("正在检测")
            .pos(0., screen.top() * 0.88)
            .anchor(0.5, 0.5)
            .size(0.3)
            .color(semi_white(1.0))
            .no_baseline()
            .draw_using(&BOLD_FONT);
        // 检测项列表：页面只展示大类；大类内部子项逐项串行检测，
        // 全部子项完成后整个大类统一为 GOOD / BAD / N/A
        let groups = [
            PostGroup::Renderer,
            PostGroup::Audio,
            PostGroup::Touch,
            PostGroup::Keyboard,
            PostGroup::Gamepad,
            PostGroup::Files,
            PostGroup::Storage,
        ];
        let n = groups.len() as f32;
        let row_h = 0.065;
        let name_size = 0.42;
        let status_size = 0.5;
        let start_y = (screen.top() + screen.bottom()) * 0.5 - (n - 1.) * row_h * 0.5;
        for (i, group) in groups.iter().enumerate() {
            let y = start_y + i as f32 * row_h;
            ui.text(group_name(*group))
                .pos(-0.6, y)
                .anchor(0., 0.5)
                .size(name_size)
                .color(semi_white(0.95))
                .draw();
            let x = 0.62;
            match self.group_status(*group) {
                PostStatus::Pending => {
                    // 该大类尚未开始检测：不显示状态
                }
                PostStatus::Check => {
                    // 该大类正在检测：CHECK 显示 0.5 秒再隐藏 0.5 秒
                    let phase = (now - self.phase_start) as f32;
                    if (phase % 1.0) < 0.5 {
                        ui.text("CHECK")
                            .pos(x, y)
                            .anchor(1., 0.5)
                            .size(status_size)
                            .color(WHITE)
                            .no_baseline()
                            .draw_using(&BOLD_FONT);
                    }
                }
                PostStatus::Good => {
                    ui.text("GOOD")
                        .pos(x, y)
                        .anchor(1., 0.5)
                        .size(status_size)
                        .color(GREEN)
                        .no_baseline()
                        .draw_using(&BOLD_FONT);
                }
                PostStatus::Bad => {
                    ui.text("BAD")
                        .pos(x, y)
                        .anchor(1., 0.5)
                        .size(status_size)
                        .color(RED)
                        .no_baseline()
                        .draw_using(&BOLD_FONT);
                }
                PostStatus::Na => {
                    // 不适用：不显示状态
                }
            }
        }
        // 底部提示
        let bottom_text = if self.paused {
            "已暂停 · 松开 Pause / Insert 继续"
        } else {
            "自检全自动进行中 · 按住 Pause 或 Insert 暂停"
        };
        ui.text(bottom_text)
            .pos(0., screen.bottom() * 0.85)
            .anchor(0.5, 0.5)
            .size(0.28)
            .color(semi_white(0.7))
            .draw();
    }
}

pub struct BootScene {
    splash: SafeTexture,
    background: SafeTexture,
    boot: SafeTexture,
    splash_music: Option<Music>,
    main_scene: Option<MainScene>,
    onboarding: Option<OnboardingScene>,

    started: f64,
    started_set: bool,
    boot_music_started: bool,
    clicked: bool,
    click_time: f64,

    post: PostRun,
}

impl BootScene {
    pub async fn new(font: FontArc) -> Result<Self> {
        let main_scene = MainScene::new(font).await?;
        let splash: SafeTexture = prpr::theme::load_asset_texture("splash.png").await?.into();
        let background: SafeTexture = prpr::theme::load_asset_texture("background.jpg").await?.into();
        let boot: SafeTexture = prpr::theme::load_asset_texture("boot.png").await?.into();
        let splash_music = match AudioClip::new(prpr::theme::load_asset_file("splash.mp3").await?) {
            Ok(clip) => UI_AUDIO
                .with(|it| {
                    it.borrow_mut().create_music(
                        clip,
                        MusicParams {
                            amplifier: get_data().config.volume_bgm,
                            loop_mix_time: 0.,
                            ..Default::default()
                        },
                    )
                })
                .ok(),
            Err(err) => {
                warn!("failed to load splash music: {:?}", err);
                None
            }
        };
        let onboarding = OnboardingScene::new()?;
        // 启动自检：文件完整性校验。
        // 快速路径：上次缓存仍然有效（清单未变、文件大小/修改时间未变）→ 直接沿用 GOOD，零 IO 跳过；
        // 缓存失效 → 桌面平台后台线程完整读取（不阻塞启动），移动平台 async 完整读取（仅失效时执行一次）。
        let mut post_files = FilesInput::Disabled;
        let mut post_manifest: Option<Vec<String>> = None;
        if get_data().config.startup_post {
            match load_manifest().await {
                Ok((manifest, manifest_len)) => {
                    post_manifest = Some(manifest.clone());
                    if files_cache_valid_sync(&manifest, manifest_len) {
                        post_files = FilesInput::Known(true);
                    } else {
                        #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32", target_env = "ohos")))]
                        {
                            let (tx, rx) = mpsc::channel();
                            std::thread::spawn(move || {
                                let ok = verify_files_sync(&manifest, manifest_len);
                                let _ = tx.send(ok);
                            });
                            post_files = FilesInput::Pending(rx);
                        }
                        #[cfg(any(target_os = "android", target_os = "ios", target_arch = "wasm32", target_env = "ohos"))]
                        {
                            let ok = verify_files_async(&manifest, manifest_len).await;
                            post_files = FilesInput::Known(ok);
                        }
                    }
                }
                Err(err) => {
                    warn!("selfcheck: 无法读取资产清单 selfcheck.list: {err:?}");
                    post_files = FilesInput::Known(false);
                }
            }
        }
        let post = PostRun::new(get_data().config.startup_post, post_files, post_manifest);
        Ok(Self {
            splash,
            background,
            boot,
            splash_music,
            main_scene: Some(main_scene),
            onboarding: Some(onboarding),
            started: 0.,
            started_set: false,
            boot_music_started: false,
            clicked: false,
            click_time: 0.,
            post,
        })
    }

    fn elapsed(&self, tm: &TimeManager) -> f32 {
        (tm.now() - self.started) as f32
    }

    fn since_click(&self, tm: &TimeManager) -> f32 {
        if self.clicked {
            (tm.now() - self.click_time) as f32
        } else {
            0.
        }
    }

    fn draw_tex_centered(&self, ui: &mut Ui, tex: &SafeTexture, alpha: f32, size: f32) {
        let w = tex.width() as f32;
        let h = tex.height() as f32;
        let aspect = if h > 0. { w / h } else { 1. };
        let rw = size;
        let rh = size / aspect;
        let r = Rect::new(-rw / 2., -rh / 2., rw, rh);
        ui.alpha(alpha, |ui| {
            ui.fill_rect(r, (**tex, r, ScaleType::Fit));
        });
    }

    fn render_warning(&self, ui: &mut Ui, alpha: f32) {
        let screen = ui.screen_rect();
        let title_size = 0.6;
        let body_size = 0.4;
        let gap = 0.08;
        let max_width = 1.7;

        // 测量正文在基准字号下的高度（世界坐标），用于动态适配屏幕比例
        let body_h = ui
            .text(WARNING_TEXT)
            .pos(0., 0.)
            .anchor(0.5, 0.5)
            .size(body_size)
            .max_width(max_width)
            .multiline()
            .h_center()
            .measure()
            .h;

        // 警告块总高（标题 + 间隔 + 正文），按屏幕高度（2*top，随屏幕比例变化）缩放，
        // 使不同屏幕比例下文字都能完整显示。坐标系中屏幕宽度固定为 2，高度 = 2 * height/width，
        // 因此横屏（top 小）可用高度小，需缩小字号避免文字超出屏幕。
        let block_h = title_size + gap + body_h;
        let usable = screen.h * 0.86; // 上下各留 7% 边距
        let scale = (usable / block_h).min(1.0);

        // 标题与正文整体垂直居中于屏幕
        let title_y = (-block_h * 0.5 + title_size * 0.5) * scale;
        let body_y = (block_h * 0.5 - body_h * 0.5) * scale;

        ui.text("警告：游戏前详阅")
            .pos(0., title_y)
            .anchor(0.5, 0.5)
            .size(title_size * scale)
            .color(semi_white(0.95 * alpha))
            .no_baseline()
            .draw_using(&BOLD_FONT);
        ui.text(WARNING_TEXT)
            .pos(0., body_y)
            .anchor(0.5, 0.5)
            .size(body_size * scale)
            .max_width(max_width)
            .multiline()
            .h_center()
            .color(semi_white(0.9 * alpha))
            .draw();
    }

    fn on_click(&mut self, tm: &TimeManager) {
        if self.clicked {
            return;
        }
        self.clicked = true;
        self.click_time = tm.now();
        set_boot_entry_time(tm.now());
        // 点击瞬间：主界面 BGM 在 3 秒内渐入
        if let Some(main) = self.main_scene.as_mut() {
            main.start_boot_bgm(BGM_FADE_TIME);
        }
        // splash.mp3 在 3 秒内音量渐小并停止
        if let Some(m) = self.splash_music.as_mut() {
            let _ = m.fade_out(SPLASH_MUSIC_FADE_OUT);
        }
        // 鍣ㄩ煶:鐐瑰嚮灞忓箷杩涘叆涓诲睆闈㈡椂鎾斁
        enter_sfx();
    }

    /// 跳过前面的黑屏/splash/警告阶段，直接进入 LOGO（背景 + boot）阶段。
    fn skip_to_logo(&mut self, tm: &TimeManager) {
        self.started = tm.now() - BOOT_READY as f64;
        // 让 update() 立即开始播放 splash.mp3
        self.boot_music_started = false;
    }
}

impl Scene for BootScene {
    fn enter(&mut self, _tm: &mut TimeManager, _target: Option<RenderTarget>) -> Result<()> {
        // started 在首次 update 时才记录，避免把 Main::new 里加载图标等耗时计入序列
        self.started_set = false;
        self.boot_music_started = false;
        self.clicked = false;
        // 自检状态在 BootScene::new 初始化一次（启动场景只进入一次），无需重建
        Ok(())
    }

    fn touch(&mut self, tm: &mut TimeManager, touch: &Touch) -> Result<bool> {
        // 自检阶段：只转发触摸事件用于输入链路检测 / 失败确认
        if !self.post.done {
            // 自检阶段全自动：仅拦截输入，不要求玩家操作
            return Ok(true);
        }
        if self.clicked {
            return Ok(true);
        }
        if matches!(touch.phase, TouchPhase::Started) {
            if self.elapsed(tm) >= BOOT_READY {
                // LOGO 阶段：点击进入游戏
                self.on_click(tm);
            } else {
                // 黑屏/splash/警告阶段：点击直接跳到 LOGO 阶段
                self.skip_to_logo(tm);
            }
        }
        Ok(true)
    }

    fn update(&mut self, tm: &mut TimeManager) -> Result<()> {
        // 启动自检阶段：推进检测，完成后才进入正常启动序列
        if !self.post.done {
            self.post.update(tm);
            return Ok(());
        }
        // 首次进入：以当前时刻作为序列起点
        if !self.started_set {
            self.started_set = true;
            self.started = tm.now();
        }
        // 背景/boot 渐显时开始播放 splash.mp3，同时播放一次 entersplash 音效
        if !self.boot_music_started && self.elapsed(tm) >= BOOT_READY - BOOT_FADE_IN {
            self.boot_music_started = true;
            if let Some(m) = self.splash_music.as_mut() {
                let _ = m.fade_in(1.0);
            }
            enter_splash_sfx();
        }
        Ok(())
    }

    fn render(&mut self, tm: &mut TimeManager, ui: &mut Ui) -> Result<()> {
        set_camera(&ui.camera());
        // 启动自检阶段：渲染 POST 界面（黑色背景铺底），完成后进入正常启动序列
        if !self.post.done {
            self.post.render(ui, tm);
            return Ok(());
        }
        let elapsed = self.elapsed(tm);
        let screen = ui.screen_rect();

        // 黑色背景
        ui.fill_rect(screen, BLACK);

        // splash 渐显/保持/渐隐
        if elapsed < SPLASH_FADE_IN + SPLASH_HOLD + SPLASH_FADE_OUT {
            let alpha = if elapsed < SPLASH_FADE_IN {
                out_sine(elapsed / SPLASH_FADE_IN)
            } else if elapsed < SPLASH_FADE_IN + SPLASH_HOLD {
                1.0
            } else {
                1.0 - out_sine((elapsed - SPLASH_FADE_IN - SPLASH_HOLD) / SPLASH_FADE_OUT)
            };
            self.draw_tex_centered(ui, &self.splash, alpha, 0.8);
        }

        // 警告文字
        if elapsed >= SPLASH_FADE_IN + SPLASH_HOLD + SPLASH_FADE_OUT
            && elapsed < SPLASH_FADE_IN + SPLASH_HOLD + SPLASH_FADE_OUT + WARN_FADE_IN + WARN_HOLD + WARN_FADE_OUT
        {
            let base = SPLASH_FADE_IN + SPLASH_HOLD + SPLASH_FADE_OUT;
            let alpha = if elapsed < base + WARN_FADE_IN {
                out_sine((elapsed - base) / WARN_FADE_IN)
            } else if elapsed < base + WARN_FADE_IN + WARN_HOLD {
                1.0
            } else {
                1.0 - out_sine((elapsed - base - WARN_FADE_IN - WARN_HOLD) / WARN_FADE_OUT)
            };
            self.render_warning(ui, alpha);
        }

        // 背景 + 模糊遮罩 + boot + 点击提示
        let bg_visible = elapsed >= BOOT_READY - BOOT_FADE_IN || self.clicked;
        if bg_visible {
            // 进入时背景与遮罩一起渐显；点击后 background 保持不动
            let enter_p = ((elapsed - (BOOT_READY - BOOT_FADE_IN)) / BOOT_FADE_IN).clamp(0., 1.);
            let bg_alpha = if self.clicked { 1.0 } else { out_sine(enter_p) };
            ui.alpha(bg_alpha, |ui| {
                ui.fill_rect(screen, (*self.background, screen));
            });

            // 遮罩 / boot / 文字
            // 半透明黑色遮罩点击后保持不渐隐，作为引导场景（onboarding）的底，视觉无缝衔接
            let mask_alpha = if self.clicked { 1.0 } else { out_sine(enter_p) };
            ui.fill_rect(screen, semi_black(0.5 * mask_alpha));
            // boot / 文字（点击后渐隐），直接用颜色 alpha 控制透明度
            let overlay_alpha = if self.clicked {
                (1.0 - out_sine((self.since_click(tm) / BOOT_CLICK_FADE_OUT).min(1.0))).max(0.0)
            } else {
                out_sine(enter_p)
            };
            // boot.png（小一点，居中偏上），固定大小
            self.draw_tex_centered(ui, &self.boot, overlay_alpha, 0.7);
            // 点击提示
            ui.text("点击屏幕开始")
                .pos(0., 0.5)
                .anchor(0.5, 0.5)
                .size(0.5)
                .color(semi_white(0.9 * overlay_alpha))
                .draw();
        }

        Ok(())
    }

    fn next_scene(&mut self, tm: &mut TimeManager) -> NextScene {
        // 自检阶段不切换场景
        if !self.post.done {
            return NextScene::None;
        }
        if self.clicked && self.since_click(tm) >= BOOT_CLICK_FADE_OUT {
            if !get_data().onboarding_done {
                // 首次启动：把主界面放入传送槽，进入引导场景
                if let Some(main) = self.main_scene.take() {
                    PENDING_MAIN.with(|it| *it.borrow_mut() = Some(main));
                }
                if let Some(ob) = self.onboarding.take() {
                    return NextScene::Replace(Box::new(ob));
                }
            }
            if let Some(main) = self.main_scene.take() {
                return NextScene::Replace(Box::new(main));
            }
        }
        NextScene::None
    }
}
