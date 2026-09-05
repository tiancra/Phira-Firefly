//! Render backend manager.
//!
//! Supports switching between wgpu (Vulkan/Metal/DX12) and macroquad (OpenGL ES)
//! rendering backends at runtime via settings.
//!
//! The wgpu backend provides multithreaded command recording on Windows and Android,
//! while the macroquad backend serves as a compatibility fallback.

use crate::config::RenderBackend;
use once_cell::sync::OnceCell;
use std::sync::{Arc, Mutex};

/// Global render backend state.
pub struct RenderBackendManager {
    selected: RenderBackend,
    wgpu_available: bool,
    wgpu_backend: Option<Arc<prpr_render::wgpu_backend::WgpuBackend>>,
}

impl RenderBackendManager {
    fn new() -> Self {
        Self {
            selected: RenderBackend::Auto,
            wgpu_available: false,
            wgpu_backend: None,
        }
    }

    /// Get the global render backend manager instance.
    pub fn global() -> &'static Mutex<RenderBackendManager> {
        static INSTANCE: OnceCell<Mutex<RenderBackendManager>> = OnceCell::new();
        INSTANCE.get_or_init(|| Mutex::new(RenderBackendManager::new()))
    }

    /// Set the selected render backend.
    pub fn set_backend(&mut self, backend: RenderBackend) {
        self.selected = backend;
    }

    /// Get the selected render backend.
    pub fn backend(&self) -> RenderBackend {
        self.selected
    }

    /// Check if wgpu backend is available on this platform.
    pub fn is_wgpu_available(&self) -> bool {
        self.wgpu_available
    }

    /// Get the initialized wgpu backend instance, if available.
    pub fn wgpu_backend(&self) -> Option<Arc<prpr_render::wgpu_backend::WgpuBackend>> {
        self.wgpu_backend.clone()
    }

    /// Check if wgpu should be used based on current selection.
    pub fn use_wgpu(&self) -> bool {
        match self.selected {
            RenderBackend::Auto => self.wgpu_available,
            RenderBackend::Wgpu => true,
            RenderBackend::OpenGl => false,
        }
    }

    /// Actually initialize the wgpu backend (creates instance/adapter/device).
    /// Runs on a dedicated thread to avoid blocking the macroquad async runtime,
    /// and is wrapped in catch_unwind to prevent wgpu-internal panics from aborting.
    /// Returns true if initialization succeeded.
    fn init_wgpu(&mut self) -> bool {
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = std::thread::Builder::new()
            .name("wgpu-init".to_owned())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    prpr_render::wgpu_backend::WgpuBackend::new_blocking()
                }));
                let _ = tx.send(result);
            });

        if handle.is_err() {
            tracing::error!("Failed to spawn wgpu init thread");
            self.wgpu_available = false;
            self.wgpu_backend = None;
            return false;
        }

        match rx.recv_timeout(std::time::Duration::from_secs(30)) {
            Ok(Ok(Ok(backend))) => {
                self.wgpu_backend = Some(Arc::new(backend));
                self.wgpu_available = true;
                tracing::info!("wgpu backend initialized successfully");
                true
            }
            Ok(Ok(Err(e))) => {
                tracing::error!("Failed to initialize wgpu backend: {}", e);
                self.wgpu_available = false;
                self.wgpu_backend = None;
                false
            }
            Ok(Err(_panic)) => {
                tracing::error!("wgpu initialization panicked (caught and suppressed)");
                self.wgpu_available = false;
                self.wgpu_backend = None;
                false
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                tracing::error!("wgpu initialization timed out after 30s");
                self.wgpu_available = false;
                self.wgpu_backend = None;
                false
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                tracing::error!("wgpu init thread disconnected unexpectedly");
                self.wgpu_available = false;
                self.wgpu_backend = None;
                false
            }
        }
    }
}

/// Initialize the render backend based on config.
///
/// This should be called once at startup after the window is created.
/// Returns true if wgpu backend was successfully initialized.
pub fn init_render_backend(backend: RenderBackend) -> bool {
    let mut manager = RenderBackendManager::global().lock().unwrap();
    manager.set_backend(backend);

    // Platforms where wgpu is supported
    let platform_supported = cfg!(any(
        target_os = "windows",
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
    ));

    if !platform_supported {
        tracing::info!("Render backend: macroquad/OpenGL (wgpu not supported on this platform)");
        return false;
    }

    // Try to initialize wgpu if selected or auto
    let should_init = match backend {
        RenderBackend::Auto | RenderBackend::Wgpu => true,
        RenderBackend::OpenGl => false,
    };

    if should_init {
        let ok = manager.init_wgpu();
        if ok {
            tracing::info!("Render backend: wgpu (requested: {:?})", backend);
        } else {
            tracing::info!("Render backend: macroquad/OpenGL fallback (wgpu init failed, requested: {:?})", backend);
        }
        ok
    } else {
        tracing::info!("Render backend: macroquad/OpenGL (requested: {:?})", backend);
        false
    }
}
