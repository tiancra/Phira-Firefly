//! Headless render worker.
//!
//! This module is the child-process entry. It is launched by the main game
//! binary with the argument `render`. It reads a JSON job description from
//! stdin, mixes audio, renders every frame off-screen, encodes an mp4 with
//! ffmpeg, and emits progress events as one-line JSON objects on stdout.

use anyhow::{anyhow, Context, Result};
use macroquad::prelude::*;
use prpr::{
    config::{Config, Mods},
    core::{internal_id, MSRenderTarget, NoteKind, BOLD_FONT, PGR_FONT},
    ext::RectExt,
    fs,
    info::ChartInfo,
    scene::{BasicPlayer, GameMode, GameScene, LoadingScene, Main},
    time::TimeManager,
    ui::{FontArc, TextPainter},
};
use serde::{Deserialize, Serialize};
use sasa::AudioClip;
use std::{
    cell::RefCell,
    cmp::max,
    io::{BufRead, BufReader, BufWriter, Write},
    ops::DerefMut,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    rc::Rc,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Instant,
};

// Intro/outro timing, adapted to this codebase.
// LoadingScene::BEFORE_TIME(1.0) + transition(1.4) + GameScene::BEFORE_TIME(0.7)
// LoadingScene: BEFORE_TIME(1.0) + transition(1.4) + wait(0.4) + GameScene::BEFORE_TIME(0.7)
const O: f64 = 1.0 + 1.4 + 0.4 + GameScene::BEFORE_TIME;
// Wait + after + fade buffer.
const A: f64 = 0.5 + 0.7 + 0.3;

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub enum RenderCodec {
    H264,
    HEVC,
    AV1,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RenderJob {
    /// Absolute path to the chart folder (the folder containing info.yml).
    pub chart_path: String,
    pub info: ChartInfo,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub ending_length: f64,
    pub codec: RenderCodec,
    pub hardware_accel: bool,
    pub xcsim: bool,
    /// Absolute path of the output mp4.
    pub output_path: String,
    // Client config passthrough
    pub player_name: String,
    pub player_rks: f32,
    pub avatar_bytes: Option<Vec<u8>>,
    pub dynamic_background: prpr::config::DynamicBackgroundMode,
    pub particle: bool,
    pub disable_effect: bool,
    pub double_hint: bool,
    pub fxaa: bool,
    pub note_scale: f32,
    pub speed: f32,
    pub ap_fc_indicator: bool,
    pub show_acc: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "camelCase")]
enum Progress {
    Mixing,
    Frames { total: u32 },
    Frame { index: u32, fps: f32 },
    Done { duration_secs: f64 },
    Error { message: String },
}

fn send(ev: Progress) {
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{}", serde_json::to_string(&ev).unwrap());
}

#[cfg(target_os = "windows")]
fn cmd_hidden(program: impl AsRef<std::ffi::OsStr>) -> Command {
    use std::os::windows::process::CommandExt;
    let mut cmd = Command::new(program);
    cmd.creation_flags(0x08000000);
    cmd
}

#[cfg(not(target_os = "windows"))]
fn cmd_hidden(program: impl AsRef<std::ffi::OsStr>) -> Command {
    Command::new(program)
}

fn find_ffmpeg() -> Result<PathBuf> {
    fn test(path: &Path) -> bool {
        cmd_hidden(path).arg("-version").output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    if test(Path::new("ffmpeg")) {
        return Ok(PathBuf::from("ffmpeg"));
    }
    let exe_dir = std::env::current_exe()?.parent().unwrap().to_owned();
    let candidate = exe_dir.join(if cfg!(target_os = "windows") {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    });
    if test(&candidate) {
        Ok(candidate)
    } else {
        Err(anyhow!("FFmpeg not found. Place ffmpeg next to the game or install it."))
    }
}

/// Pick the best encoder based on codec choice and hardware-accel toggle.
fn pick_encoder(codec: RenderCodec, hw: bool) -> &'static str {
    match codec {
        RenderCodec::H264 => {
            if hw {
                "h264_nvenc"
            } else {
                "libx264"
            }
        }
        RenderCodec::HEVC => {
            if hw {
                "hevc_nvenc"
            } else {
                "libx265"
            }
        }
        RenderCodec::AV1 => {
            if hw {
                "av1_nvenc"
            } else {
                "libaom-av1"
            }
        }
    }
}

/// Main entry of the render child process. Must run inside a headless macroquad
/// event loop.
pub async fn run() -> Result<()> {
    let result = run_inner().await;
    if let Err(e) = &result {
        send(Progress::Error {
            message: format!("{e:?}"),
        });
    }
    result
}

async fn run_inner() -> Result<()> {
    eprintln!("[render_worker] starting up");
    let mut stdin = std::io::stdin().lock();
    let mut line = String::new();
    stdin.read_line(&mut line)?;
    eprintln!("[render_worker] got job: {}", line.trim());
    let job: RenderJob = serde_json::from_str(line.trim())?;
    eprintln!("[render_worker] chart_path={} size={}x{} fps={}", job.chart_path, job.width, job.height, job.fps);

    let mut fs = fs::fs_from_file(Path::new(&job.chart_path))?;
    eprintln!("[render_worker] fs opened");

    let font = FontArc::try_from_vec(load_file("font.ttf").await?)?;
    eprintln!("[render_worker] font loaded");
    let mut painter = TextPainter::new(font.clone(), None);

    // Load game-specific fonts (phigros.ttf for score digits, bold.ttf for UI)
    match load_file("phigros.ttf").await {
        Ok(bytes) => match FontArc::try_from_vec(bytes) {
            Ok(pgr) => {
                PGR_FONT.with(|it| *it.borrow_mut() = Some(TextPainter::new(pgr, Some(font.clone()))));
                eprintln!("[render_worker] phigros font loaded");
            }
            Err(e) => eprintln!("[render_worker] failed to parse phigros.ttf: {e}"),
        },
        Err(e) => eprintln!("[render_worker] failed to load phigros.ttf: {e}"),
    }
    // Load bold.ttf: try data/bold.ttf first (user-customizable), then assets/bold.ttf
    let bold_bytes = match std::fs::read("data/bold.ttf") {
        Ok(b) if !b.is_empty() => Some(b),
        _ => load_file("bold.ttf").await.ok(),
    };
    match bold_bytes {
        Some(bytes) => match FontArc::try_from_vec(bytes) {
            Ok(bold) => {
                BOLD_FONT.with(|it| *it.borrow_mut() = Some(TextPainter::new(bold, Some(font.clone()))));
                eprintln!("[render_worker] bold font loaded");
            }
            Err(e) => eprintln!("[render_worker] failed to parse bold.ttf: {e}"),
        },
        None => eprintln!("[render_worker] failed to load bold.ttf"),
    }

    let ffmpeg = find_ffmpeg()?;
    eprintln!("[render_worker] ffmpeg found: {}", ffmpeg.display());

    eprintln!("[render_worker] creating config...");
    let mut config = Config::default();
    config.mods = Mods::AUTOPLAY;
    config.sample_count = 4;
    config.player_name = job.player_name.clone();
    config.player_rks = job.player_rks;
    config.particle = job.particle;
    config.disable_effect = job.disable_effect;
    config.double_hint = job.double_hint;
    config.fxaa = job.fxaa;
    config.note_scale = job.note_scale;
    config.speed = job.speed;
    config.ap_fc_indicator = job.ap_fc_indicator;
    config.show_acc = job.show_acc;
    config.dynamic_background = job.dynamic_background;
    // Mute all audio output in the render child process;
    // we mix audio manually into ffmpeg.
    config.volume_music = 0.0;
    config.volume_sfx = 0.0;
    config.volume_bgm = 0.0;

    let info = job.info;

    eprintln!("[render_worker] loading chart...");
    let (chart, ..) = GameScene::load_chart(fs.deref_mut(), &info)
        .await
        .context("Failed to load chart")?;
    eprintln!("[render_worker] chart loaded");

    macro_rules! ld {
        ($path:literal) => {
            AudioClip::new(load_file($path).await?).with_context(|| format!("Failed to load {}", $path))?
        };
    }
    let music_bytes = fs.load_file(&info.music).await?;
    let music = AudioClip::new(music_bytes).context("Failed to load music")?;
    let ending = ld!("ending.ogg");
    // sasa's AudioClip reports a truncated length in headless mode. Use ffprobe
    // to get the real duration of the music file.
    let track_length = {
        let music_path = Path::new(&job.chart_path).join(&info.music);
        let ffmpeg_path = find_ffmpeg().unwrap_or_else(|_| PathBuf::from("ffmpeg.exe"));
        let ffprobe = ffmpeg_path.parent()
            .map(|p| p.join(if cfg!(windows) { "ffprobe.exe" } else { "ffprobe" }))
            .unwrap_or_else(|| PathBuf::from("ffprobe"));
        let output = std::process::Command::new(&ffprobe)
            .args(["-v", "quiet", "-show_entries", "format=duration", "-of", "default=noprint_wrappers=1:nokey=1"])
            .arg(&music_path)
            .output();
        match output {
            Ok(out) if out.status.success() => {
                let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                s.parse::<f64>().unwrap_or_else(|_| music.length() as f64)
            }
            _ => music.length() as f64,
        }
    };
    eprintln!("[render_worker] track_length computed: {track_length}");
    let sfx_click = ld!("click.ogg");
    let sfx_drag = ld!("drag.ogg");
    let sfx_flick = ld!("flick.ogg");

    // Mix at full volume even though config volumes are 0 (to mute live audio)
    let volume_music = 1.0f32;
    let volume_sfx = 1.0f32;

    let length = track_length - chart.offset.min(0.) as f64 + 1.;
    let video_length = O + length + A + job.ending_length;
    let offset = chart.offset.max(0.);

    let _render_start = Instant::now();

    // ---- Audio mixing ----
    send(Progress::Mixing);
    let mixing_path = std::env::temp_dir().join(format!("phira_mix_{}.mp3", std::process::id()));
    let mixing_path_str = mixing_path.to_string_lossy().to_string();
    let sample_rate: u32 = 44100;
    let mut output = vec![0.0f32; (video_length * sample_rate as f64).ceil() as usize * 2];
    {
        let pos = O - chart.offset.min(0.) as f64;
        let count = (track_length * sample_rate as f64) as usize;
        let start = (pos * sample_rate as f64).round() as usize * 2;
        let ratio = 1. / sample_rate as f64;
        for frame in 0..count {
            let position = frame as f64 * ratio;
            let s = music.sample(position).unwrap_or_default();
            let idx = start + frame * 2;
            output[idx] += s.0 * volume_music;
            output[idx + 1] += s.1 * volume_music;
        }
    }
    let mut place = |pos: f64, clip: &AudioClip, volume: f32| {
        let position = (pos * sample_rate as f64).round() as usize * 2;
        if position >= output.len() {
            return 0;
        }
        let clip_rate = clip.sample_rate() as f64;
        let rate_ratio = clip_rate / sample_rate as f64;
        let frames = clip.frames();
        let total = frames.len();
        let slice = &mut output[position..];
        let max_frames = slice.len() / 2;
        let mut written = 0;
        let mut src_idx = 0.0f64;
        while written < max_frames && (src_idx as usize) < total {
            let s = frames[src_idx as usize];
            slice[written * 2] += s.0 * volume;
            slice[written * 2 + 1] += s.1 * volume;
            src_idx += rate_ratio;
            written += 1;
        }
        written
    };
    for note in chart.lines.iter().flat_map(|it| it.notes.iter()).filter(|it| !it.fake) {
        place(
            O + note.time as f64 + offset as f64,
            match note.kind {
                NoteKind::Click | NoteKind::Hold { .. } => &sfx_click,
                NoteKind::Drag => &sfx_drag,
                NoteKind::Flick => &sfx_flick,
            },
            volume_sfx,
        );
    }
    let mut pos = O + length + A;
    let ending_dur = ending.frame_count() as f64 / ending.sample_rate() as f64;
    while place(pos, &ending, volume_music) != 0 {
        pos += ending_dur;
    }
    let mut mix_proc = cmd_hidden(&ffmpeg)
        .args("-y -f f32le -ar 44100 -ac 2 -i - -c:a libmp3lame -q:a 4 -f mp3".split_whitespace())
        .arg(&mixing_path_str)
        .stdin(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .context("Failed to spawn ffmpeg (audio mix)")?;
    {
        let input = mix_proc.stdin.as_mut().unwrap();
        let mut writer = BufWriter::new(input);
        for sample in output.into_iter() {
            writer.write_all(&sample.to_le_bytes())
                .context("Failed to write audio to ffmpeg (audio encoder crashed)")?;
        }
    }
    let mix_status = mix_proc.wait()?;
    if !mix_status.success() {
        return Err(anyhow!("audio ffmpeg failed (exit {:?})", mix_status.code()));
    }

    // ---- Frame rendering ----
    let (vw, vh) = (job.width, job.height);
    let mst = Rc::new(MSRenderTarget::new((vw, vh), config.sample_count));
    let my_time: Rc<RefCell<f64>> = Rc::new(RefCell::new(0.));
    let tm = TimeManager::manual(Box::new({
        let my_time = Rc::clone(&my_time);
        move || *my_time.borrow()
    }));

    let avatar = job.avatar_bytes.as_deref().map(|bytes| {
        prpr::ext::SafeTexture::from(Texture2D::from_file_with_format(bytes, None))
    });
    let player = BasicPlayer {
        avatar,
        id: 0,
        rks: job.player_rks,
        historic_best: 0,
    };
    // Override the track length in the GameScene's Resources (sasa AudioClip
    // reports a truncated MP3 duration in headless mode).
    prpr::core::TRACK_LENGTH_OVERRIDE.with(|o| *o.borrow_mut() = Some(track_length));
    let mut main = Main::new(
        Box::new(
            LoadingScene::new(
                GameMode::Normal,
                info,
                config,
                fs,
                Some(player),
                None,
                None,
                None,
                job.xcsim,
                None,
                None,
                None,
            )
            .await?,
        ),
        tm,
        {
            let mst = Rc::clone(&mst);
            move || Some(mst.input())
        },
    )
    .await?;
    main.viewport = Some((0, 0, vw as i32, vh as i32));

    let fps = job.fps;
    let total_frames = (video_length * fps as f64).ceil() as u32;
    send(Progress::Frames { total: total_frames });

    // Set up ffmpeg video encoder.
    let encoder = pick_encoder(job.codec, job.hardware_accel);
    let mut enc = cmd_hidden(&ffmpeg)
        .args([
            "-y",
            "-f", "rawvideo",
            "-pix_fmt", "rgba",
            "-s", &format!("{vw}x{vh}"),
            "-r", &fps.to_string(),
            "-i", "-",
            "-i", &mixing_path_str,
            "-c:v", encoder,
            "-pix_fmt", "yuv420p",
            "-crf", "18",
            "-c:a", "copy",
            "-vf", "vflip",
            "-r", &fps.to_string(),
            "-fps_mode", "cfr",
            "-avoid_negative_ts", "make_zero",
            "-movflags", "+faststart",
            &job.output_path,
        ])
        .stdin(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .context("Failed to spawn ffmpeg (video encode)")?;

    // Send pixel frames to a writer thread so rendering doesn't block on ffmpeg pipe.
    let (tx, rx): (Sender<Vec<u8>>, Receiver<Vec<u8>>) = mpsc::channel();
    let stdin = enc.stdin.take().unwrap();
    let writer_thread = thread::Builder::new()
        .name("ffmpeg-writer".into())
        .spawn(move || -> std::io::Result<()> {
            let mut video_in = BufWriter::with_capacity(4 * 1024 * 1024, stdin);
            for frame in rx {
                video_in.write_all(&frame)?;
            }
            video_in.flush()?;
            Ok(())
        })?;

    // Triple-buffered pixel readback: we render into N buffers, letting the
    // GPU work on frame N+1 while we read back frame N.
    let pixel_size = (vw * vh * 4) as usize;
    let mut buffers = vec![
        vec![0u8; pixel_size],
        vec![0u8; pixel_size],
        vec![0u8; pixel_size],
    ];
    let mut buf_idx = 0;

    let mut window: Vec<f32> = Vec::new();
    let start = Instant::now();

    eprintln!("[render_worker] total_frames={total_frames} video_length={video_length} track_length={track_length}");
    for i in 0..total_frames {
        *my_time.borrow_mut() = i as f64 / fps as f64;
        main.update()?;
        main.render(&mut painter)?;
        mst.blit();

        let output_id = internal_id(mst.output());
        unsafe {
            use miniquad::gl::*;
            glBindFramebuffer(GL_FRAMEBUFFER, output_id);
            glReadPixels(
                0, 0,
                vw as _, vh as _,
                GL_RGBA, GL_UNSIGNED_BYTE,
                buffers[buf_idx].as_mut_ptr() as *mut _,
            );
        }
        // Hand off this buffer to the writer thread. The channel has bounded
        // backpressure (the OS pipe buffer), so we don't run away with memory.
        let buf = std::mem::replace(&mut buffers[buf_idx], vec![0u8; pixel_size]);
        tx.send(buf).map_err(|e| anyhow!("failed to send frame: {e}"))?;
        buf_idx = (buf_idx + 1) % buffers.len();

        // FPS sampling over a 0.5s window.
        let elapsed = start.elapsed().as_secs_f32();
        window.push(elapsed);
        while window.len() > 1 && window[0] < elapsed - 0.5 {
            window.remove(0);
        }
        let cur_fps = if window.len() >= 2 {
            (window.len() - 1) as f32 / (window[window.len() - 1] - window[0])
        } else {
            0.0
        };
        if i % max(1, total_frames / 100) == 0 || i + 1 == total_frames {
            send(Progress::Frame {
                index: i + 1,
                fps: cur_fps,
            });
        }
    }
    eprintln!("[render_worker] render loop completed, frames sent={}", total_frames);
    drop(tx);
    writer_thread.join().unwrap()?;
    let status = enc.wait()?;
    let _ = std::fs::remove_file(&mixing_path_str);
    if !status.success() {
        return Err(anyhow!("ffmpeg failed (exit {:?})", status.code()));
    }

    send(Progress::Done {
        duration_secs: start.elapsed().as_secs_f64(),
    });
    Ok(())
}
