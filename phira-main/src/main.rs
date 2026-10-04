// === Windows GUI 子系统 ===
// 以 GUI 子系统编译：进程启动时系统不会创建控制台窗口，
// 「调试模式」关闭时命令框全程不存在（无显示、无闪烁、无最小化过程）。
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

// === 强制使用独立显卡（Nvidia Optimus / AMD PowerXpress） ===
// 驱动扫描到这些导出符号时，会自动为应用选择高性能独立 GPU。
#[cfg(target_os = "windows")]
#[no_mangle]
pub static NvOptimusEnablement: i32 = 1;

#[cfg(target_os = "windows")]
#[no_mangle]
pub static AmdPowerXpressRequestHighPerformance: i32 = 1;

#[cfg(target_os = "windows")]
fn set_high_performance() {
    // 设置进程和主线程优先级为高，减少系统调度延迟，让渲染循环占满 CPU
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentThread() -> *mut std::ffi::c_void;
        fn SetThreadPriority(hThread: *mut std::ffi::c_void, nPriority: i32) -> i32;
        fn SetPriorityClass(hProcess: *mut std::ffi::c_void, dwPriorityClass: u32) -> i32;
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
    }
    // THREAD_PRIORITY_HIGHEST = 2
    const THREAD_PRIORITY_HIGHEST: i32 = 2;
    // HIGH_PRIORITY_CLASS = 0x00000080
    const HIGH_PRIORITY_CLASS: u32 = 0x00000080;
    unsafe {
        let thread = GetCurrentThread();
        SetThreadPriority(thread, THREAD_PRIORITY_HIGHEST);
        let process = GetCurrentProcess();
        SetPriorityClass(process, HIGH_PRIORITY_CLASS);
    }
}

/// Windows 命令框（控制台窗口）显隐控制。
///
/// exe 以 GUI 子系统编译，进程启动时系统根本不会创建控制台窗口，
/// 因此「调试模式」关闭时命令框全程不存在（无显示、无闪烁、无最小化过程）。
/// 仅当「调试模式」开启时，在入口分配一个新控制台，日志链路继续可用。
#[cfg(target_os = "windows")]
mod console {
    use std::{ffi::c_void, path::PathBuf};

    #[link(name = "kernel32")]
    extern "system" {
        fn GetConsoleWindow() -> *mut c_void;
        fn AllocConsole() -> i32;
    }

    /// 定位游戏根目录，规则与 `prpr::core::init_assets` 保持一致：
    /// 从可执行文件所在目录逐级上溯，取第一个包含 `assets` 子目录的路径。
    fn game_root() -> Option<PathBuf> {
        let mut exe = std::env::current_exe().ok()?;
        while exe.pop() {
            if exe.join("assets").exists() {
                return Some(exe);
            }
        }
        std::env::current_dir().ok()
    }

    /// 读取存档中的 `debugConsole` 开关。
    /// 存档缺失或解析失败时一律按「关闭」处理，即默认不显示命令框。
    fn debug_console_enabled() -> bool {
        let path = match game_root() {
            Some(root) => root.join("data").join("data.json"),
            None => return false,
        };
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(_) => return false,
        };
        let value: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => return false,
        };
        value
            .get("config")
            .and_then(|c| c.get("debugConsole"))
            .and_then(|d| d.as_bool())
            .unwrap_or(false)
    }

    pub fn apply_visibility() {
        // GUI 子系统下默认没有控制台窗口；仅当调试模式开启时分配一个
        if debug_console_enabled() {
            unsafe {
                if GetConsoleWindow().is_null() {
                    AllocConsole();
                }
            }
        }
    }
}

fn main() {
    #[cfg(target_os = "windows")]
    {
        // 必须在 quad_main 之前执行，尽早隐藏命令框以减少闪烁
        console::apply_visibility();
        set_high_performance();
    }
    phira::quad_main();
}
