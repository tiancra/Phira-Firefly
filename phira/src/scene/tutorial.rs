use anyhow::Result;
use macroquad::prelude::*;
use prpr::{
    ext::{poll_future, LocalTask},
    fs,
    scene::{show_error, GameMode, GameScene, LoadingScene, NextScene, Scene},
    time::TimeManager,
    ui::Ui,
};

pub struct TutorialLoadingScene {
    load_task: LocalTask<Result<GameScene>>,
    next_scene: Option<NextScene>,
}

impl TutorialLoadingScene {
    pub fn new() -> Result<Self> {
        let mut fs = fs::fs_from_assets("tutorial/")?;
        let load_task: LocalTask<Result<GameScene>> = Some(Box::pin(async move {
            let info = fs::load_info(fs.as_mut()).await?;
            let (illustration, background, _) = LoadingScene::load(fs.as_mut(), &info.illustration).await?;
            let config = crate::get_data().config.clone();
            let scene = GameScene::new(GameMode::Tutorial, info, config, fs, None, background, illustration, None, None, None, None).await?;
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
