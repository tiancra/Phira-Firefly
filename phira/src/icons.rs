use crate::{load_theme_texture, scene::TEX_ICON_BACK};
use anyhow::Result;
use prpr::ext::SafeTexture;

pub struct Icons {
    pub icon: SafeTexture,
    pub play: SafeTexture,
    pub medal: SafeTexture,
    pub respack: SafeTexture,
    pub msg: SafeTexture,
    pub settings: SafeTexture,
    pub back: SafeTexture,
    pub lang: SafeTexture,
    pub download: SafeTexture,
    pub user: SafeTexture,
    pub info: SafeTexture,
    pub delete: SafeTexture,
    pub menu: SafeTexture,
    pub edit: SafeTexture,
    pub ldb: SafeTexture,
    pub close: SafeTexture,
    pub search: SafeTexture,
    pub order: SafeTexture,
    pub filter: SafeTexture,
    pub r#mod: SafeTexture,
    pub star: SafeTexture,
    pub star_outline: SafeTexture,
    pub heart: SafeTexture,
    pub heart_outline: SafeTexture,
    pub cloud_none: SafeTexture,
    pub cloud_check: SafeTexture,
    pub plus: SafeTexture,
    pub select: SafeTexture,

    pub r#abstract: SafeTexture,
}

impl Icons {
    pub async fn new() -> Result<Self> {
        Ok(Self {
            icon: load_theme_texture("icon.png").await?,
            play: load_theme_texture("resume.png").await?,
            medal: load_theme_texture("medal.png").await?,
            respack: load_theme_texture("respack.png").await?,
            msg: load_theme_texture("message.png").await?,
            settings: load_theme_texture("settings.png").await?,
            lang: load_theme_texture("language.png").await?,
            back: TEX_ICON_BACK.with(|it| it.borrow().clone().unwrap()),
            download: load_theme_texture("download.png").await?,
            user: load_theme_texture("user.png").await?,
            info: load_theme_texture("info.png").await?,
            delete: load_theme_texture("delete.png").await?,
            menu: load_theme_texture("menu.png").await?,
            edit: load_theme_texture("edit.png").await?,
            ldb: load_theme_texture("leaderboard.png").await?,
            close: load_theme_texture("close.png").await?,
            search: load_theme_texture("search.png").await?,
            order: load_theme_texture("order.png").await?,
            filter: load_theme_texture("filter.png").await?,
            r#mod: load_theme_texture("mod.png").await?,
            star: load_theme_texture("star.png").await?,
            star_outline: load_theme_texture("star_outline.png").await?,
            heart: load_theme_texture("heart.png").await?,
            heart_outline: load_theme_texture("heart_outline.png").await?,
            cloud_none: load_theme_texture("cloud_none.png").await?,
            cloud_check: load_theme_texture("cloud_check.png").await?,
            plus: load_theme_texture("plus.png").await?,
            select: load_theme_texture("select.png").await?,

            r#abstract: load_theme_texture("abstract.jpg").await?,
        })
    }
}
