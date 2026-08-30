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

fn main() {
    #[cfg(target_os = "windows")]
    set_high_performance();
    phira::quad_main();
}
