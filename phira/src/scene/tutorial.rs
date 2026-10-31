use anyhow::Result;
use async_trait::async_trait;
use macroquad::prelude::*;
use prpr::{
    ext::{poll_future, LocalTask},
    fs,
    fs::FileSystem,
    info::ChartInfo,
    scene::{show_error, GameMode, GameScene, LoadingScene, NextScene, Scene},
    time::TimeManager,
    ui::Ui,
};
use std::any::Any;

/// 新手教程按平台读取不同的谱面 / extra / info 文件：
/// - Android：chart_android.json + extra_android.json + info_android
/// - 其他平台（Windows / Linux 等桌面端）：chart_pc.json + extra_pc.json + info_pc
#[cfg(target_os = "android")]
fn tutorial_file_names() -> (&'static str, &'static str, &'static str) {
    ("chart_android.json", "extra_android.json", "info_android")
}

#[cfg(not(target_os = "android"))]
fn tutorial_file_names() -> (&'static str, &'static str, &'static str) {
    ("chart_pc.json", "extra_pc.json", "info_pc")
}

/// 包装文件系统：把谱面加载逻辑中固定的 `extra.json` 重定向到平台特定文件；
/// 平台文件不存在时回退到 `extra.json`（兼容旧资源）。
struct TutorialFs {
    inner: Box<dyn FileSystem>,
    extra_dst: String,
}

#[async_trait]
impl FileSystem for TutorialFs {
    async fn load_file(&mut self, path: &str) -> Result<Vec<u8>> {
        if path == "extra.json" {
            if let Ok(bytes) = self.inner.load_file(&self.extra_dst).await {
                return Ok(bytes);
            }
        }
        self.inner.load_file(path).await
    }

    async fn exists(&mut self, path: &str) -> Result<bool> {
        if path == "extra.json" && self.inner.exists(&self.extra_dst).await? {
            return Ok(true);
        }
        self.inner.exists(path).await
    }

    fn list_root(&self) -> Result<Vec<String>> {
        self.inner.list_root()
    }

    fn clone_box(&self) -> Box<dyn FileSystem> {
        Box::new(TutorialFs {
            inner: self.inner.clone_box(),
            extra_dst: self.extra_dst.clone(),
        })
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}

pub struct TutorialLoadingScene {
    load_task: LocalTask<Result<GameScene>>,
    next_scene: Option<NextScene>,
}

impl TutorialLoadingScene {
    pub fn new() -> Result<Self> {
        let mut fs = fs::fs_from_assets("tutorial/")?;
        let load_task: LocalTask<Result<GameScene>> = Some(Box::pin(async move {
            let (chart_name, extra_name, info_base) = tutorial_file_names();
            // 优先读取平台 info 文件（info_pc / info_android，支持无扩展名 / .yml / .txt / .csv）
            let info = match fs::load_info_with_prefix(fs.as_mut(), info_base).await? {
                Some(mut info) => {
                    // info 未指明谱面时回退到平台谱面名
                    if info.chart.is_empty() {
                        info.chart = chart_name.to_owned();
                    }
                    info
                }
                None => {
                    // 回退：直接读取平台谱面 json，不依赖 info 文件
                    let chart_file = if fs.exists(chart_name).await.unwrap_or(false) {
                        chart_name.to_owned()
                    } else {
                        "7995884894025043.json".to_owned()
                    };
                    ChartInfo {
                        name: "Tutorial".to_owned(),
                        chart: chart_file,
                        music: "7995884894025043.mp3".to_owned(),
                        illustration: "7995884894025043.jpg".to_owned(),
                        ..Default::default()
                    }
                }
            };
            let (illustration, background, _) = LoadingScene::load(fs.as_mut(), &info.illustration).await?;
            let config = crate::get_data().config.clone();
            let fs: Box<dyn FileSystem> = Box::new(TutorialFs {
                inner: fs,
                extra_dst: extra_name.to_owned(),
            });
            let scene = GameScene::new(
                GameMode::Tutorial,
                info,
                config,
                fs,
                None,
                background,
                illustration,
                None,
                None,
                None,
                None,
                None,
                false,
                None,
                None,
            )
            .await?;
            Ok(scene)
        }));
        Ok(Self { load_task, next_scene: None })
    }
}

impl Scene for TutorialLoadingScene {
    fn enter(&mut self, tm: &mut TimeManager, _target: Option<RenderTarget>) -> Result<()> {
        tm.reset();
        Ok(())
    }

    fn update(&mut self, _tm: &mut TimeManager) -> Result<()> {
        if let Some(future) = self.load_task.as_mut() {
            match poll_future(future.as_mut()) {
                None => {}
                Some(Ok(scene)) => {
                    self.load_task = None;
                    self.next_scene = Some(NextScene::Replace(Box::new(scene)));
                }
                Some(Err(err)) => {
                    self.load_task = None;
                    show_error(err);
                    self.next_scene = Some(NextScene::Pop);
                }
            }
        }
        Ok(())
    }

    fn render(&mut self, _tm: &mut TimeManager, ui: &mut Ui) -> Result<()> {
        ui.fill_rect(ui.screen_rect(), BLACK);
        Ok(())
    }

    fn next_scene(&mut self, _tm: &mut TimeManager) -> NextScene {
        self.next_scene.take().unwrap_or_default()
    }
}
