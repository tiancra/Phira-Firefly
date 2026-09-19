//! LocalChart 本地谱面分享的网络传输层。
//!
//! - [`ChartServer`]：房主启动的轻量 HTTP 文件服务器，将 `download/{chart_id}`
//!   目录打包为 zip 提供下载。优先监听 IPv6，失败回退 IPv4（V6 -> V4 顺序）。
//! - [`ChartSyncing`]：玩家从房主下载谱面时的共享状态（用于渲染"正在同步谱面"转圈）。

use anyhow::{Context, Result};
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;

use crate::dir;

/// 本地谱面包（打包 / 解包）允许的最大体积上限。
/// 本地谱面可能包含大体积音频/视频，若不设上限，打包（[`pack_chart_dir`]）与
/// 解包（[`download_chart`]）都会一次性在内存中物化整包，存在耗尽内存的风险。
pub const MAX_CHART_ARCHIVE_SIZE: u64 = 512 * 1024 * 1024;

/// 删除会话生成的本地谱面临时目录（`download/{chart_id}` 与 `download/sync_{chart_id}`）。
/// `chart_id` 为空或为在线谱面数字 id 时不做任何处理，只清理本会话生成的 UUID 目录。
pub fn remove_staged_chart(chart_id: &str) {
    if chart_id.is_empty() || chart_id.chars().all(|c| c.is_ascii_digit()) {
        return;
    }
    let Ok(charts) = dir::charts() else { return };
    for name in [format!("download/{chart_id}"), format!("download/sync_{chart_id}")] {
        remove_path(std::path::Path::new(&format!("{charts}/{name}")));
    }
}

/// 删除文件或目录（不存在时静默返回）。
fn remove_path(path: &std::path::Path) {
    if !path.exists() {
        return;
    }
    let _ = if path.is_file() {
        std::fs::remove_file(path)
    } else {
        std::fs::remove_dir_all(path)
    };
}

/// 把本地谱面 `local_path`（相对 `dir::charts()` 的子路径，如 `download/123` 或自定义路径）
/// 对应的目录复制到 `download/{uuid}`，供后续 serve / download 使用同一 UUID 目录。
pub fn stage_local_chart(local_path: &str, uuid: &str) -> Result<()> {
    let src = format!("{}/{}", dir::charts()?, local_path);
    let src_path = std::path::Path::new(&src);
    if !src_path.is_dir() {
        anyhow::bail!("local chart directory not found: {}", src_path.display());
    }
    let dst = format!("{}/download/{uuid}", dir::charts()?);
    let dst_path = std::path::Path::new(&dst);
    remove_path(dst_path);
    copy_dir(src_path, dst_path)?;
    Ok(())
}

fn copy_dir(src: &std::path::Path, dst: &std::path::Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src).with_context(|| format!("read dir {}", src.display()))? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// 把 `charts` 目录下的一个谱面包（`download/{chart_id}`，chart_id 为 UUID）打包成 zip（内存中）。
/// 返回 zip 的字节内容。若目录不存在则报错。
pub fn pack_chart_dir(chart_id: &str) -> Result<Vec<u8>> {
    let root = format!("{}/download/{chart_id}", dir::charts()?);
    let root = std::path::Path::new(&root);
    if !root.is_dir() {
        anyhow::bail!("local chart directory not found: {}", root.display());
    }

    let mut out = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut out));
        let options =
            zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

        fn visit(
            zip: &mut zip::ZipWriter<std::io::Cursor<&mut Vec<u8>>>,
            options: zip::write::SimpleFileOptions,
            base: &std::path::Path,
            path: &std::path::Path,
            total: &mut u64,
        ) -> Result<()> {
            for entry in std::fs::read_dir(path).with_context(|| format!("read dir {}", path.display()))? {
                let entry = entry?;
                let p = entry.path();
                let rel = p.strip_prefix(base)?;
                if p.is_dir() {
                    let name = format!("{}/", rel.to_string_lossy());
                    zip.add_directory(name, options)?;
                    visit(zip, options, base, &p, total)?;
                } else {
                    // 打包前累计体积并校验上限，避免超大的谱面在内存中物化整包
                    *total = total.saturating_add(entry.metadata().map(|m| m.len()).unwrap_or(0));
                    if *total > MAX_CHART_ARCHIVE_SIZE {
                        anyhow::bail!("chart is too large to share (limit {MAX_CHART_ARCHIVE_SIZE} bytes)");
                    }
                    zip.start_file(rel.to_string_lossy().to_string(), options)?;
                    let mut f = std::fs::File::open(&p)?;
                    std::io::copy(&mut f, zip)?;
                }
            }
            Ok(())
        }

        let mut total = 0u64;
        visit(&mut zip, options, root, root, &mut total)?;
        zip.finish()?;
    }
    Ok(out)
}

/// 房主本地谱面 HTTP 下载服务器。
pub struct ChartServer {
    addr: String,
    port: u16,
    config: Arc<ChartServerConfig>,
    shutdown: Arc<AtomicBool>,
    handle: Mutex<Option<thread::JoinHandle<()>>>,
}

struct ChartServerConfig {
    chart_id: String,
    /// 最新打包好的谱面 zip 内容（惰性生成并缓存，保存失败前的版本）
    zip_cache: RwLock<Option<Vec<u8>>>,
    ready: AtomicBool,
    error: Mutex<Option<String>>,
}

impl ChartServer {
    /// 启动服务器。`chart_id` 是要分享的本地谱面 UUID。
    /// 会先尝试监听 IPv6 `[::]:0`，失败则回退监听 IPv4 `0.0.0.0:0`（V6 -> V4 顺序）。
    /// 地址由服务端协调（服务端负责打洞/下发可达地址），此处只报告监听端口。
    pub fn start(chart_id: String) -> Result<Arc<Self>> {
        let config = Arc::new(ChartServerConfig {
            chart_id,
            zip_cache: RwLock::new(None),
            ready: AtomicBool::new(false),
            error: Mutex::new(None),
        });

        let listener = listen_v6_then_v4().context("failed to bind chart download server")?;
        let addr = listener.local_addr()?;
        let shutdown = Arc::new(AtomicBool::new(false));

        let handle = {
            let config = Arc::clone(&config);
            let shutdown = Arc::clone(&shutdown);
            thread::spawn(move || serve_loop(listener, config, shutdown))
        };

        Ok(Arc::new(Self {
            addr: addr.ip().to_string(),
            port: addr.port(),
            config,
            shutdown,
            handle: Mutex::new(Some(handle)),
        }))
    }

    /// 当前可连接的地址（V4 形式，若是 :: 则返回 0.0.0.0）
    #[allow(dead_code)]
    pub fn addr(&self) -> &str {
        &self.addr
    }

    #[allow(dead_code)]
    pub fn port(&self) -> u16 {
        self.port
    }

    /// 服务器是否已经准备好（已监听）
    pub fn ready(&self) -> bool {
        self.config.ready.load(Ordering::SeqCst)
    }

    pub fn stop(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
        // 主动连接一次以唤醒 accept 循环
        let _ = std::net::TcpStream::connect((self.addr.as_str(), self.port));
        if let Some(h) = self.handle.lock().unwrap().take() {
            let _ = h.join();
        }
    }
}

fn listen_v6_then_v4() -> Result<TcpListener> {
    for host in ["[::]:0", "0.0.0.0:0"] {
        if let Ok(l) = TcpListener::bind(host) {
            return Ok(l);
        }
    }
    anyhow::bail!("no usable address to bind")
}

fn serve_loop(
    listener: TcpListener,
    config: Arc<ChartServerConfig>,
    shutdown: Arc<AtomicBool>,
) {
    config.ready.store(true, Ordering::SeqCst);
    listener
        .set_nonblocking(true)
        .ok();

    loop {
        if shutdown.load(Ordering::SeqCst) {
            break;
        }
        match listener.accept() {
            Ok((stream, _)) => {
                let config = Arc::clone(&config);
                thread::spawn(move || handle_conn(stream, config));
            }
            _ => {
                thread::sleep(std::time::Duration::from_millis(10));
            }
        }
    }
}

fn handle_conn(mut stream: TcpStream, config: Arc<ChartServerConfig>) {
    use std::io::{BufRead, BufReader};
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    // 丢弃请求头
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() || line == "\r\n" || line == "\n" {
            break;
        }
    }

    let mut path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .trim_start_matches('/')
        .to_string();
    if let Some(q) = path.find('?') {
        path.truncate(q);
    }

    let body = if path == format!("download/{}/chart.zip", config.chart_id) {
        // 生成并缓存 zip
        if config.zip_cache.read().unwrap().is_none() {
            match pack_chart_dir(&config.chart_id) {
                Ok(bytes) => {
                    *config.zip_cache.write().unwrap() = Some(bytes);
                }
                Err(e) => {
                    *config.error.lock().unwrap() = Some(e.to_string());
                    respond(&mut stream, "404 Not Found", b"chart not found");
                    return;
                }
            }
        }
        config.zip_cache.read().unwrap().clone().unwrap_or_default()
    } else {
        respond(&mut stream, "404 Not Found", b"not found");
        return;
    };

    let header = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(&body);
    let _ = stream.flush();
}

fn respond(stream: &mut TcpStream, status: &str, body: &[u8]) {
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

impl Drop for ChartServer {
    fn drop(&mut self) {
        self.stop();
    }
}

/// 玩家从房主下载谱面时的共享状态（用于渲染"正在同步谱面"转圈）。
pub struct ChartSyncing {
    pub done: AtomicBool,
    pub error: Mutex<Option<String>>,
    pub started: AtomicBool,
}

impl ChartSyncing {
    pub fn new() -> Self {
        Self {
            done: AtomicBool::new(false),
            error: Mutex::new(None),
            started: AtomicBool::new(false),
        }
    }

    pub fn mark_started(&self) {
        self.started.store(true, Ordering::SeqCst);
    }

    pub fn mark_done(&self) {
        self.done.store(true, Ordering::SeqCst);
    }

    pub fn set_error(&self, e: impl Into<String>) {
        *self.error.lock().unwrap() = Some(e.into());
        self.done.store(true, Ordering::SeqCst);
    }

    pub fn error(&self) -> Option<String> {
        self.error.lock().unwrap().clone()
    }
}

/// 房主把本地谱面包（`download/{chart_id}`）打包成 zip，经 game 连接上传到服务端中转。
/// 玩家将经服务端（同一 game 连接）下载，兼容内网穿透（无需额外 web 端口映射）。
pub async fn upload_chart(client: &phira_mp_client::Client, chart_id: &str) -> Result<()> {
    let zip = pack_chart_dir(chart_id)?;
    client.upload_chart(chart_id.to_string(), zip).await?;
    Ok(())
}

/// 玩家经 game 连接从服务端获取谱面包（`download/{chart_id}`），
/// 解压到本地 `download/{chart_id}` 目录。
pub async fn download_chart(
    client: &phira_mp_client::Client,
    chart_id: &str,
    syncing: Arc<ChartSyncing>,
) -> Result<()> {
    syncing.mark_started();
    let bytes = client.download_chart(chart_id.to_string()).await?;

    // 解包前校验体积，避免超大的谱面在解压时占用过多内存/磁盘
    if bytes.len() as u64 > MAX_CHART_ARCHIVE_SIZE {
        anyhow::bail!("chart archive is too large ({} bytes, limit {MAX_CHART_ARCHIVE_SIZE})", bytes.len());
    }

    // 解压到临时目录；失败时清理，避免残留 sync_ 目录
    let tmp = format!("{}/download/sync_{chart_id}", dir::charts()?);
    let tmp_path = std::path::Path::new(&tmp);
    remove_path(tmp_path);
    std::fs::create_dir_all(tmp_path)?;
    if let Err(err) = extract_archive(bytes, tmp_path) {
        remove_path(tmp_path);
        return Err(err);
    }

    // 移动到 download/{chart_id}
    let to = format!("{}/download/{chart_id}", dir::charts()?);
    let to_path = std::path::Path::new(&to);
    remove_path(to_path);
    if let Some(parent) = to_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(tmp_path, to_path)?;

    syncing.mark_done();
    Ok(())
}

/// 把谱面包（zip）解压到 `dst` 目录。失败时由调用方负责清理 `dst`。
fn extract_archive(bytes: Vec<u8>, dst: &std::path::Path) -> Result<()> {
    let chart_dir = prpr::dir::Dir::new(dst)?;
    prpr::ext::unzip_into(std::io::Cursor::new(bytes), &chart_dir, false)?;
    Ok(())
}
