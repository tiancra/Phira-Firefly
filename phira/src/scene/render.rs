//! Chart-to-video rendering scenes.
//!
//! - [`RenderSettingsScene`]: black settings page with resolution/fps/ending/
//!   codec/hardware-accel controls and a render button.
//! - [`RenderProgressScene`]: progress page that spawns the headless render
//!   child process and shows progress / fps / ETA.

use anyhow::Result;
use macroquad::prelude::*;
use prpr::{
    core::BOLD_FONT,
    ext::open_url,
    info::ChartInfo,
    scene::{show_message, NextScene, Scene},
    time::TimeManager,
    ui::{DRectButton, Dialog, Slider, Ui},
};
use crate::popup::ChooseButton;
use serde::Deserialize;
use std::{
    cell::RefCell,
    io::{BufRead, BufReader, BufWriter, Write},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};

use crate::render_worker::{RenderCodec, RenderJob};

// ---------------------------------------------------------------------------
// Shared progress state
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct RenderState {
    pub mixing: bool,
    pub total_frames: u32,
    pub done_frames: u32,
    pub fps: f32,
    pub finished: bool,
    pub success: bool,
    pub error: Option<String>,
    pub output_path: Option<String>,
    started_at: Option<Instant>,
    frame_times: Vec<f32>,
}

impl RenderState {
    fn on_frame(&mut self, index: u32, fps: f32) {
        self.done_frames = index;
        self.fps = fps;
        let now = self.started_at.get_or_insert(Instant::now()).elapsed().as_secs_f32();
        self.frame_times.push(now);
        while self.frame_times.len() > 2 && self.frame_times[0] < now - 0.5 {
            self.frame_times.remove(0);
        }
    }

    pub fn eta(&self) -> Option<f32> {
        if self.total_frames == 0 || self.done_frames == 0 {
            return None;
        }
        let elapsed = self.started_at?.elapsed().as_secs_f32();
        if elapsed < 0.5 {
            return None;
        }
        let rate = self.done_frames as f32 / elapsed;
        if rate <= 0.1 {
            return None;
        }
        Some((self.total_frames - self.done_frames) as f32 / rate)
    }
}

// ---------------------------------------------------------------------------
// Settings scene
// ---------------------------------------------------------------------------

const RESOLUTIONS: &[(u32, u32)] = &[(1920, 1080), (2560, 1440), (3840, 2160), (1280, 720)];
const FPS_OPTIONS: &[u32] = &[30, 60, 120, 240, 360, 480, 640];
const CODEC_NAMES: &[&str] = &["H.264", "HEVC", "AV1"];
const RES_NAMES: &[&str] = &["1920×1080", "2560×1440", "3840×2160", "1280×720"];

thread_local! {
    /// (pending output path, confirmed flag, job to start)
    static PENDING: RefCell<Option<(PathBuf, Arc<AtomicBool>)>> = const { RefCell::new(None) };
}

pub struct RenderSettingsScene {
    chart_path: String,
    info: ChartInfo,

    res_idx: usize,
    fps_idx: usize,
    ending: f32,
    codec_idx: usize,
    hardware: bool,

    btn_render: DRectButton,
    btn_back: DRectButton,
    slider_ending: Slider,
    btn_res: ChooseButton,
    btn_fps: ChooseButton,
    btn_codec: ChooseButton,
    btn_hw: DRectButton,

    bg: Option<Texture2D>,
    // Client config snapshot
    player_name: String,
    player_rks: f32,
    avatar_bytes: Option<Vec<u8>>,
    dynamic_background: prpr::config::DynamicBackgroundMode,
    particle: bool,
    disable_effect: bool,
    double_hint: bool,
    fxaa: bool,
    note_scale: f32,
    speed: f32,
    ap_fc_indicator: bool,
    show_acc: bool,

    next: Option<RenderJob>,
    go_back: bool,
}

impl RenderSettingsScene {
    pub fn new(chart_path: String, info: ChartInfo, cfg: &prpr::config::Config, player_name: String, player_rks: f32, avatar_bytes: Option<Vec<u8>>) -> Self {
        Self {
            chart_path,
            info,
            res_idx: 0,
            fps_idx: 1,
            ending: 3.0,
            codec_idx: 0,
            hardware: true,
            btn_render: DRectButton::new(),
            btn_back: DRectButton::new(),
            slider_ending: Slider::new(1.0..10.0, 0.5),
            btn_res: ChooseButton::new()
                .with_options(RES_NAMES.iter().map(|s| s.to_string()).collect())
                .with_selected(0),
            btn_fps: ChooseButton::new()
                .with_options(FPS_OPTIONS.iter().map(|f| format!("{f}")).collect())
                .with_selected(1),
            btn_codec: ChooseButton::new()
                .with_options(CODEC_NAMES.iter().map(|s| s.to_string()).collect())
                .with_selected(0),
            btn_hw: DRectButton::new(),
            bg: None,
            player_name,
            player_rks,
            avatar_bytes,
            dynamic_background: cfg.dynamic_background,
            particle: cfg.particle,
            disable_effect: cfg.disable_effect,
            double_hint: cfg.double_hint,
            fxaa: cfg.fxaa,
            note_scale: cfg.note_scale,
            speed: cfg.speed,
            ap_fc_indicator: cfg.ap_fc_indicator,
            show_acc: cfg.show_acc,
            next: None,
            go_back: false,
        }
    }

    fn codec(&self) -> RenderCodec {
        match self.codec_idx {
            1 => RenderCodec::HEVC,
            2 => RenderCodec::AV1,
            _ => RenderCodec::H264,
        }
    }
}

impl Scene for RenderSettingsScene {
    fn enter(&mut self, tm: &mut TimeManager, _target: Option<RenderTarget>) -> Result<()> {
        tm.reset();
        if self.bg.is_none() {
            self.bg = Some(Texture2D::from_image(&Image::from_file_with_format(
                &std::fs::read("assets/background.jpg")?,
                Some(ImageFormat::Jpeg),
            )));
        }
        Ok(())
    }

    fn update(&mut self, tm: &mut TimeManager) -> Result<()> {
        let t = tm.now() as f32;
        self.btn_res.update(t);
        self.btn_fps.update(t);
        self.btn_codec.update(t);
        if self.btn_res.changed() {
            self.res_idx = self.btn_res.selected();
        }
        if self.btn_fps.changed() {
            self.fps_idx = self.btn_fps.selected();
        }
        if self.btn_codec.changed() {
            self.codec_idx = self.btn_codec.selected();
        }
        // Pick up pending confirmation — don't consume until confirmed
        let start = PENDING.with(|p| {
            p.borrow().as_ref().map(|(path, confirmed)| {
                confirmed.load(Ordering::SeqCst).then(|| path.clone())
            }).flatten()
        });
        if let Some(path) = start {
            PENDING.with(|p| *p.borrow_mut() = None);
            let (w, h) = RESOLUTIONS[self.res_idx];
            let fps = FPS_OPTIONS[self.fps_idx];
            self.next = Some(RenderJob {
                chart_path: self.chart_path.clone(),
                info: self.info.clone(),
                width: w,
                height: h,
                fps,
                ending_length: self.ending as f64,
                codec: self.codec(),
                hardware_accel: self.hardware,
                xcsim: self.chart_path.contains("xcsim"),
                output_path: path.to_string_lossy().to_string(),
                player_name: self.player_name.clone(),
                player_rks: self.player_rks,
                avatar_bytes: self.avatar_bytes.clone(),
                dynamic_background: self.dynamic_background,
                particle: self.particle,
                disable_effect: self.disable_effect,
                double_hint: self.double_hint,
                fxaa: self.fxaa,
                note_scale: self.note_scale,
                speed: self.speed,
                ap_fc_indicator: self.ap_fc_indicator,
                show_acc: self.show_acc,
            });
        }
        Ok(())
    }

    fn render(&mut self, tm: &mut TimeManager, ui: &mut Ui) -> Result<()> {
        let t = tm.now() as f32;
        clear_background(BLACK);
        if let Some(bg) = &self.bg {
            let screen = ui.screen_rect();
            draw_texture_ex(*bg, screen.x, screen.y, WHITE, DrawTextureParams {
                dest_size: Some(screen.size()),
                ..Default::default()
            });
        }
        ui.fill_rect(ui.screen_rect(), Color::new(0., 0., 0., 0.7));

        let top = ui.top; // y=-top is TOP, y=+top is BOTTOM
        let content_w = 1.1;
        let left = -content_w / 2.;

        // Back button top-left
        self.btn_back.render_text(ui, Rect::new(-0.95, -top + 0.1, 0.2, 0.08), t, "<", 0.4, false);

        ui.scope(|ui| {
            ui.dx(left);
            ui.dy(-top + 0.12);

            // Title centered
            ui.text("渲染谱面")
                .pos(content_w / 2., 0.0)
                .anchor(0.5, 0.)
                .size(0.45)
                .draw_using(&BOLD_FONT);

            // Row 0: Resolution
            let y0 = 0.1;
            ui.text("视频分辨率")
                .pos(0., y0)
                .anchor(0., 1.)
                .size(0.25)
                .draw();
            self.btn_res.render(ui, Rect::new(0., y0 + 0.01, content_w, 0.08), t);

            // Row 1: FPS
            let y1 = y0 + 0.12;
            ui.text("视频帧率")
                .pos(0., y1)
                .anchor(0., 1.)
                .size(0.25)
                .draw();
            self.btn_fps.render(ui, Rect::new(0., y1 + 0.01, content_w, 0.08), t);

            // Row 2: Ending slider
            let y2 = y1 + 0.12;
            let slider_w = 1.1;
            let slider_x = 0.7 - 0.4 * slider_w;
            self.slider_ending.render(
                ui,
                Rect::new(slider_x, y2, slider_w, 0.06),
                t,
                self.ending,
                format!("结算时长: {:.1}s", self.ending),
            );

            // Row 3: Codec
            let y3 = y2 + 0.12;
            ui.text("视频编码器")
                .pos(0., y3)
                .anchor(0., 1.)
                .size(0.25)
                .draw();
            self.btn_codec.render(ui, Rect::new(0., y3 + 0.01, content_w, 0.08), t);

            // Row 4: Hardware accel toggle
            let y4 = y3 + 0.12;
            let hw_text = if self.hardware { "硬件加速: 开启 (GPU)" } else { "硬件加速: 关闭 (CPU)" };
            self.btn_hw.render_text(ui, Rect::new(0., y4, content_w, 0.08), t, hw_text, 0.25, false);
        });

        // Render button bottom-right
        self.btn_render.render_text(
            ui,
            Rect::new(0.55, top - 0.12, 0.4, 0.08),
            t,
            "渲染",
            0.4,
            false,
        );

        // Popups on top
        self.btn_res.render_top(ui, t, 1.0);
        self.btn_fps.render_top(ui, t, 1.0);
        self.btn_codec.render_top(ui, t, 1.0);
        Ok(())
    }

    fn touch(&mut self, tm: &mut TimeManager, touch: &Touch) -> Result<bool> {
        let t = tm.now() as f32;
        // Popups first
        if self.btn_res.top_touch(touch, t) || self.btn_fps.top_touch(touch, t) || self.btn_codec.top_touch(touch, t) {
            return Ok(true);
        }
        if self.btn_back.touch(touch, t) {
            self.next = None;
            // Signal back via a flag — use next_scene Pop
            self.go_back = true;
            return Ok(true);
        }
        if self.btn_res.touch(touch, t) || self.btn_fps.touch(touch, t) || self.btn_codec.touch(touch, t) {
            return Ok(true);
        }
        if self.btn_hw.touch(touch, t) {
            self.hardware = !self.hardware;
            return Ok(true);
        }
        if let Some(true) = self.slider_ending.touch(touch, t, &mut self.ending) {
            return Ok(true);
        }
        if self.btn_render.touch(touch, t) {
            let default_name = format!("{}.mp4", self.info.name.replace('/', "_"));
            if let Some(path) = rfd::FileDialog::new()
                .set_file_name(&default_name)
                .add_filter("MP4 video", &["mp4"])
                .save_file()
            {
                let confirmed = Arc::new(AtomicBool::new(false));
                let c = confirmed.clone();
                Dialog::plain(
                    "确认渲染".to_string(),
                    format!("即将渲染到:\n{}\n\n谱面: {}\n分辨率: {}x{}\n帧率: {}\n编码器: {}\n硬件加速: {}",
                        path.display(),
                        self.info.name,
                        RESOLUTIONS[self.res_idx].0,
                        RESOLUTIONS[self.res_idx].1,
                        FPS_OPTIONS[self.fps_idx],
                        CODEC_NAMES[self.codec_idx],
                        if self.hardware { "开启" } else { "关闭" },
                    ),
                )
                .buttons(vec!["取消".to_string(), "开始渲染".to_string()])
                .listener(move |_, id| {
                    if id == 1 {
                        c.store(true, Ordering::SeqCst);
                    }
                    false
                })
                .show();
                PENDING.with(|p| *p.borrow_mut() = Some((path, confirmed)));
            }
            return Ok(true);
        }
        Ok(false)
    }

    fn next_scene(&mut self, _tm: &mut TimeManager) -> NextScene {
        if self.go_back {
            return NextScene::Pop;
        }
        if let Some(job) = self.next.take() {
            return NextScene::Replace(Box::new(RenderProgressScene::new(job)));
        }
        NextScene::None
    }

    fn nav_enabled(&self) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Progress scene
// ---------------------------------------------------------------------------

pub struct RenderProgressScene {
    job: RenderJob,
    child: Option<std::process::Child>,
    state: Arc<Mutex<RenderState>>,
    reader_handle: Option<std::thread::JoinHandle<()>>,
    started: bool,
    done: bool,
}

impl RenderProgressScene {
    pub fn new(job: RenderJob) -> Self {
        Self {
            job,
            child: None,
            state: Arc::new(Mutex::new(RenderState::default())),
            reader_handle: None,
            started: false,
            done: false,
        }
    }

    fn start(&mut self) -> Result<()> {
        let exe = std::env::current_exe()?;
        let mut child = Command::new(&exe)
            .arg("render")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // Write job JSON to stdin
        {
            let stdin = child.stdin.as_mut().unwrap();
            let mut writer = BufWriter::new(stdin);
            writeln!(writer, "{}", serde_json::to_string(&self.job)?)?;
            writer.flush()?;
        }

        // Spawn reader thread: read stdout events, then wait for exit code
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let state = self.state.clone();
        let output_path = self.job.output_path.clone();
        self.reader_handle = Some(std::thread::spawn(move || {
            // Read stderr in a separate thread
            let stderr_handle = std::thread::spawn(move || {
                use std::io::Read;
                let mut stderr = stderr;
                let mut buf = Vec::new();
                let _ = stderr.read_to_end(&mut buf);
                String::from_utf8_lossy(&buf).to_string()
            });

            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                let Ok(line) = line else { break };
                #[derive(Deserialize)]
                #[serde(tag = "event", rename_all = "camelCase")]
                enum Ev {
                    Mixing,
                    Frames { total: u32 },
                    Frame { index: u32, fps: f32 },
                    Done { duration_secs: f64 },
                    Error { message: String },
                }
                match serde_json::from_str::<Ev>(&line) {
                    Ok(Ev::Mixing) => {
                        state.lock().unwrap().mixing = true;
                    }
                    Ok(Ev::Frames { total }) => {
                        let mut s = state.lock().unwrap();
                        s.mixing = false;
                        s.total_frames = total;
                    }
                    Ok(Ev::Frame { index, fps }) => {
                        state.lock().unwrap().on_frame(index, fps);
                    }
                    Ok(Ev::Done { .. }) => {
                        let mut s = state.lock().unwrap();
                        s.finished = true;
                        s.success = true;
                        s.output_path = Some(output_path.clone());
                    }
                    Ok(Ev::Error { message }) => {
                        let mut s = state.lock().unwrap();
                        s.finished = true;
                        s.success = false;
                        s.error = Some(message);
                    }
                    Err(_) => {}
                }
            }

            // stdout EOF: wait for child exit
            // Note: child is dropped here since we don't have it in the thread.
            // The parent will wait on it in update().
            let stderr_text = stderr_handle.join().unwrap_or_default();
            // Always write stderr to log for debugging
            if let Ok(exe_dir) = std::env::current_exe() {
                if let Some(dir) = exe_dir.parent() {
                    let log_path = dir.join("phira-render-error.log");
                    let _ = std::fs::write(&log_path, &stderr_text);
                }
            }
            let mut s = state.lock().unwrap();
            if !s.finished {
                s.finished = true;
                s.success = false;
                s.error = if stderr_text.trim().is_empty() {
                    Some("渲染进程意外退出".to_string())
                } else {
                    Some(format!("渲染进程错误:\n{}", stderr_text.trim()))
                };
            }
        }));

        self.child = Some(child);
        self.started = true;
        Ok(())
    }
}

impl Scene for RenderProgressScene {
    fn enter(&mut self, _tm: &mut TimeManager, _target: Option<RenderTarget>) -> Result<()> {
        if !self.started {
            self.start()?;
        }
        Ok(())
    }

    fn update(&mut self, _tm: &mut TimeManager) -> Result<()> {
        let finished = self.state.lock().unwrap().finished;
        if finished && !self.done {
            self.done = true;
            if let Some(mut child) = self.child.take() {
                let _ = child.wait();
            }
            if let Some(h) = self.reader_handle.take() {
                let _ = h.join();
            }
        }
        Ok(())
    }

    fn render(&mut self, _tm: &mut TimeManager, ui: &mut Ui) -> Result<()> {
        clear_background(BLACK);
        ui.fill_rect(ui.screen_rect(), BLACK);
        let state = self.state.lock().unwrap();
        let top = ui.top; // y=-top is TOP, y=+top is BOTTOM
        let bar_w = 1.6;

        // Title at top center
        ui.text("正在渲染谱面")
            .pos(0.0, -top + 0.25)
            .anchor(0.5, 0.)
            .size(0.7)
            .draw_using(&BOLD_FONT);

        ui.scope(|ui| {
            ui.dx(-bar_w / 2.);
            ui.dy(0.1); // content slightly below center

            if state.mixing {
                ui.text("正在混合音频...")
                    .pos(bar_w / 2., 0.0)
                    .anchor(0.5, 0.)
                    .size(0.5)
                    .color(WHITE)
                    .draw();
            } else if state.total_frames > 0 {
                let p = state.done_frames as f32 / state.total_frames as f32;
                let bar_h = 0.05;
                ui.fill_rect(Rect::new(0.0, 0.0, bar_w, bar_h), Color::from_rgba(50, 50, 50, 255));
                ui.fill_rect(Rect::new(0.0, 0.0, bar_w * p, bar_h), Color::from_rgba(80, 180, 255, 255));
                ui.dy(bar_h + 0.08);

                ui.text(format!("{:.1}%", p * 100.0))
                    .pos(bar_w / 2., 0.0)
                    .anchor(0.5, 0.)
                    .size(0.5)
                    .draw();
                ui.dy(0.15);

                ui.text(format!("FPS: {:.0}", state.fps))
                    .pos(bar_w / 2., 0.0)
                    .anchor(0.5, 0.)
                    .size(0.4)
                    .color(Color::new(1., 1., 1., 0.8))
                    .draw();
                ui.dy(0.08);

                if let Some(eta) = state.eta() {
                    ui.text(format!("预计剩余: {:.0} 秒", eta))
                        .pos(bar_w / 2., 0.0)
                        .anchor(0.5, 0.)
                        .size(0.4)
                        .color(Color::new(1., 1., 1., 0.8))
                        .draw();
                }
            }
        });

        // Bottom hint
        ui.text("正在渲染谱面，请不要关机或退出游戏")
            .pos(0.0, top - 0.12)
            .anchor(0.5, 0.)
            .size(0.35)
            .color(Color::new(1., 1., 1., 0.6))
            .draw();
        Ok(())
    }

    fn next_scene(&mut self, _tm: &mut TimeManager) -> NextScene {
        if self.done {
            let state = self.state.lock().unwrap();
            if state.success {
                show_message("渲染完成").ok();
                if let Some(path) = &state.output_path {
                    #[cfg(not(any(target_os = "android", target_os = "ios")))]
                    {
                        #[cfg(target_os = "windows")]
                        {
                            use std::os::windows::process::CommandExt;
                            let _ = std::process::Command::new("explorer")
                                .args(["/select,", path])
                                .spawn();
                        }
                        #[cfg(not(target_os = "windows"))]
                        {
                            if let Some(dir) = std::path::Path::new(path).parent() {
                                let _ = open_url(&format!("file://{}", dir.display()));
                            }
                        }
                    }
                }
                NextScene::Pop
            } else {
                let msg = state.error.clone().unwrap_or_else(|| "渲染失败".to_string());
                drop(state);
                // Write error log to file
                if let Ok(exe) = std::env::current_exe() {
                    if let Some(dir) = exe.parent() {
                        let log_path = dir.join("phira-render-error.log");
                        use std::io::Write;
                        if let Ok(mut f) = std::fs::File::create(&log_path) {
                            let _ = writeln!(f, "{msg}");
                            let _ = f.flush();
                        }
                    }
                }
                show_message(format!("渲染失败，详见 phira-render-error.log")).error();
                NextScene::Pop
            }
        } else {
            NextScene::None
        }
    }

    fn nav_enabled(&self) -> bool {
        false
    }
}
