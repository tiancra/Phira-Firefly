pub mod bin;
pub mod config;
pub mod core;
pub mod dir;
pub mod ext;
pub mod fs;
pub mod gamepad;
pub mod info;
pub mod judge;
pub mod parse;
pub mod particle;
pub mod scene;
pub mod task;
pub mod time;
pub mod ui;

#[cfg(feature = "log")]
pub mod log;

#[rustfmt::skip]
#[cfg(closed)]
pub mod inner;

pub use scene::Main;

pub fn build_conf() -> macroquad::window::Conf {
    macroquad::window::Conf {
        window_title: "Phira".to_string(),
        window_width: 973,
        window_height: 608,
        platform: miniquad::conf::Platform {
            swap_interval: Some(0), // 禁用 VSync，解锁帧率
            ..Default::default()
        },
        ..Default::default()
    }
}


/// 运行时设置垂直同步间隔（0=禁用，1=60FPS，2=30FPS）
/// 仅 Windows 平台支持，其他平台无操作
pub fn set_swap_interval(interval: i32) {
    #[cfg(target_os = "windows")]
    {
        #[link(name = "opengl32")]
        extern "system" {
            fn wglGetProcAddress(name: *const u8) -> *const std::ffi::c_void;
            fn wglGetCurrentContext() -> *const std::ffi::c_void;
        }
        type WglSwapIntervalEXT = unsafe extern "system" fn(interval: i32) -> i32;
        unsafe {
            let ctx = wglGetCurrentContext();
            if ctx.is_null() {
                eprintln!("[VSync] no current GL context, skip");
                return;
            }
            let proc = wglGetProcAddress(b"wglSwapIntervalEXT\0".as_ptr());
            if proc.is_null() {
                eprintln!("[VSync] wglSwapIntervalEXT not found");
                return;
            }
            let swap_interval: WglSwapIntervalEXT = std::mem::transmute(proc);
            let ret = swap_interval(interval);
            eprintln!("[VSync] set_swap_interval({}) = {}", interval, ret);
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = interval;
    }
}
