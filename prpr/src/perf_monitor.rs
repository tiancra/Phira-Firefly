//! 实时性能监测：FPS / CPU / 内存 / GPU / 硬盘占用
//!
//! Windows 显示全部五项；Android 仅显示 FPS。
//! - FPS：每秒统计一次平均帧率（过去 1 秒内的帧数）
//! - GPU：通过 Windows PDH 性能计数器读取 `\GPU Engine(*engtype_3D)\Utilization Percentage`
//! - CPU/内存/硬盘：通过 sysinfo 读取
//! 数据通过全局单例维护，每秒刷新一次系统指标。

use std::sync::Mutex;

use once_cell::sync::Lazy;

#[cfg(not(target_arch = "wasm32"))]
use sysinfo::{CpuRefreshKind, Disks, MemoryRefreshKind, System};

/// 全局性能监测器实例
static MONITOR: Lazy<Mutex<PerfMonitor>> = Lazy::new(|| Mutex::new(PerfMonitor::new()));

pub struct PerfMonitor {
    /// 当前显示的 FPS（每秒更新一次）
    fps: f32,
    /// 过去 1 秒内的帧计数
    fps_frame_count: u32,
    /// 上一次 FPS 统计的时间戳（秒）
    fps_last_time: f64,
    /// 最近一次系统数据刷新的时间戳（秒）
    last_refresh: f64,
    /// 系统信息采集器（非 wasm 平台）
    #[cfg(not(target_arch = "wasm32"))]
    system: System,
    /// 缓存的 CPU 使用率（%）
    cpu_usage: f32,
    /// 缓存的内存使用率（%）
    mem_usage: f32,
    /// 缓存的 GPU 使用率（%）
    gpu_usage: f32,
    /// 缓存的硬盘使用率（%）
    disk_usage: f32,
    /// PDH GPU 监测器（Windows）
    #[cfg(target_os = "windows")]
    gpu_monitor: Option<GpuPdhMonitor>,
}

impl PerfMonitor {
    fn new() -> Self {
        Self {
            fps: 0.,
            fps_frame_count: 0,
            fps_last_time: 0.,
            last_refresh: 0.,
            #[cfg(not(target_arch = "wasm32"))]
            system: System::new(),
            cpu_usage: 0.,
            mem_usage: 0.,
            gpu_usage: 0.,
            disk_usage: 0.,
            #[cfg(target_os = "windows")]
            gpu_monitor: None,
        }
    }

    /// 每帧调用：累加帧计数，每秒计算一次平均 FPS；必要时刷新系统指标
    pub fn update(now: f64, _frame_delta: f64) {
        if let Ok(mut m) = MONITOR.lock() {
            // FPS 统计：每秒更新一次
            if m.fps_last_time == 0. {
                m.fps_last_time = now;
            }
            m.fps_frame_count += 1;
            let elapsed = now - m.fps_last_time;
            if elapsed >= 1.0 {
                m.fps = m.fps_frame_count as f32 / elapsed as f32;
                m.fps_frame_count = 0;
                m.fps_last_time = now;
            }

            #[cfg(not(target_arch = "wasm32"))]
            {
                // 每秒刷新一次系统指标
                if now - m.last_refresh >= 1.0 {
                    m.refresh_system(now);
                    m.last_refresh = now;
                }
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn refresh_system(&mut self, now: f64) {
        // CPU：需要两次采样才能得到使用率
        self.system
            .refresh_cpu_specifics(CpuRefreshKind::everything());
        self.cpu_usage = self.system.global_cpu_usage() as f32;

        // 内存
        self.system
            .refresh_memory_specifics(MemoryRefreshKind::everything());
        let total = self.system.total_memory() as f64;
        let used = self.system.used_memory() as f64;
        self.mem_usage = if total > 0. { (used / total * 100.) as f32 } else { 0. };

        // GPU：PDH 性能计数器
        #[cfg(target_os = "windows")]
        {
            if self.gpu_monitor.is_none() {
                self.gpu_monitor = Some(GpuPdhMonitor::new());
            }
            if let Some(gm) = &mut self.gpu_monitor {
                self.gpu_usage = gm.usage();
            }
        }

        // 硬盘：取第一个可写磁盘的使用率
        let disks = Disks::new_with_refreshed_list();
        if let Some(disk) = disks.first() {
            let total = disk.total_space() as f64;
            let available = disk.available_space() as f64;
            self.disk_usage = if total > 0. { ((total - available) / total * 100.) as f32 } else { 0. };
        }

        let _ = now;
    }

    /// 获取快照（用于渲染）
    pub fn snapshot() -> PerfSnapshot {
        if let Ok(m) = MONITOR.lock() {
            PerfSnapshot {
                fps: m.fps,
                cpu_usage: m.cpu_usage,
                mem_usage: m.mem_usage,
                gpu_usage: m.gpu_usage,
                disk_usage: m.disk_usage,
            }
        } else {
            PerfSnapshot::default()
        }
    }
}

#[derive(Default, Clone)]
pub struct PerfSnapshot {
    pub fps: f32,
    pub cpu_usage: f32,
    pub mem_usage: f32,
    pub gpu_usage: f32,
    pub disk_usage: f32,
}

// ============================================================================
// Windows PDH GPU 使用率监测
// ============================================================================

#[cfg(target_os = "windows")]
mod pdh {
    use std::ffi::c_void;

    pub type PdhHQuery = *mut c_void;
    pub type PdhHCounter = *mut c_void;
    pub type PdhStatus = i32;

    #[repr(C)]
    pub struct PdhFmtCounterValue {
        pub c_status: u32,
        pub double_value: f64,
    }

    #[repr(C)]
    pub struct PdhFmtCounterValueItem {
        pub sz_name: *const u16,
        pub fmt_value: PdhFmtCounterValue,
    }

    extern "system" {
        pub fn PdhOpenQueryW(
            sz_data_source: *const u16,
            dw_user_data: usize,
            ph_query: *mut PdhHQuery,
        ) -> PdhStatus;
        pub fn PdhAddEnglishCounterW(
            h_query: PdhHQuery,
            sz_full_counter_path: *const u16,
            dw_user_data: usize,
            ph_counter: *mut PdhHCounter,
        ) -> PdhStatus;
        pub fn PdhCollectQueryData(h_query: PdhHQuery) -> PdhStatus;
        pub fn PdhGetFormattedCounterArrayW(
            h_counter: PdhHCounter,
            dw_format: u32,
            lpdw_buffer_size: *mut u32,
            lpdw_item_count: *mut u32,
            item_buffer: *mut c_void,
        ) -> PdhStatus;
        pub fn PdhCloseQuery(h_query: PdhHQuery) -> PdhStatus;
    }

    pub const PDH_FMT_DOUBLE: u32 = 0x00000200;
    pub const ERROR_SUCCESS: PdhStatus = 0;
    pub const PDH_MORE_DATA: PdhStatus = -2147481390; // 0x800007D2
}

#[cfg(target_os = "windows")]
struct GpuPdhMonitor {
    query: pdh::PdhHQuery,
    counter: pdh::PdhHCounter,
    ok: bool,
}

// PDH 句柄可跨线程安全传递
#[cfg(target_os = "windows")]
unsafe impl Send for GpuPdhMonitor {}

#[cfg(target_os = "windows")]
impl GpuPdhMonitor {
    fn new() -> Self {
        use pdh::*;
        let mut query: PdhHQuery = std::ptr::null_mut();
        let mut counter: PdhHCounter = std::ptr::null_mut();
        let mut ok = false;

        unsafe {
            if PdhOpenQueryW(std::ptr::null(), 0, &mut query) == ERROR_SUCCESS {
                // 通配符匹配所有 GPU 的 3D 引擎实例
                let path = "\\GPU Engine(*engtype_3D)\\Utilization Percentage";
                let mut wide: Vec<u16> = path.encode_utf16().collect();
                wide.push(0);
                if PdhAddEnglishCounterW(query, wide.as_ptr(), 0, &mut counter) == ERROR_SUCCESS {
                    // PDH 需两次采样，先收集一次
                    PdhCollectQueryData(query);
                    ok = true;
                }
            }
        }

        Self { query, counter, ok }
    }

    fn usage(&mut self) -> f32 {
        use pdh::*;
        if !self.ok {
            return 0.0;
        }
        unsafe {
            if PdhCollectQueryData(self.query) != ERROR_SUCCESS {
                return 0.0;
            }

            // 通配符计数器：获取所有实例的值，取最大值
            let mut buf_size: u32 = 0;
            let mut item_count: u32 = 0;
            let status = PdhGetFormattedCounterArrayW(
                self.counter,
                PDH_FMT_DOUBLE,
                &mut buf_size,
                &mut item_count,
                std::ptr::null_mut(),
            );
            if status != PDH_MORE_DATA || buf_size == 0 {
                return 0.0;
            }

            let mut buf = vec![0u8; buf_size as usize];
            let status = PdhGetFormattedCounterArrayW(
                self.counter,
                PDH_FMT_DOUBLE,
                &mut buf_size,
                &mut item_count,
                buf.as_mut_ptr() as *mut std::ffi::c_void,
            );
            if status != ERROR_SUCCESS || item_count == 0 {
                return 0.0;
            }

            let items = buf.as_ptr() as *const PdhFmtCounterValueItem;
            let mut max_usage = 0.0f64;
            for i in 0..item_count {
                let item = &*items.add(i as usize);
                if item.fmt_value.c_status == ERROR_SUCCESS as u32 {
                    max_usage = max_usage.max(item.fmt_value.double_value);
                }
            }
            max_usage as f32
        }
    }
}

#[cfg(target_os = "windows")]
impl Drop for GpuPdhMonitor {
    fn drop(&mut self) {
        if self.ok {
            unsafe {
                pdh::PdhCloseQuery(self.query);
            }
        }
    }
}
