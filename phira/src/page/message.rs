prpr_l10n::tl_file!("message");

use std::{borrow::Cow, sync::Arc};

use super::{Page, SharedState};
use crate::{
    client::{recv_raw, Chart, Client, Message, Ptr},
    get_data, get_data_mut,
    icons::Icons,
    page::{ChartItem, SFader},
    save_data,
    scene::{ProfileScene, SongScene},
};
use anyhow::Result;
use chrono::Local;
use macroquad::prelude::*;
use prpr::{
    ext::{open_url, semi_black, semi_white, RectExt, SafeTexture},
    scene::show_error,
    task::Task,
    ui::{DRectButton, Scroll, Ui},
};

pub struct MessagePage {
    msgs: Option<Vec<(Message, DRectButton)>>,
    load_task: Option<Task<Result<Vec<Message>>>>,

    index: Option<usize>,

    btns_scroll: Scroll,
    scroll: Scroll,

    action_btns: Vec<DRectButton>,

    close_btn: DRectButton,

    sf: SFader,
    chart_task: Option<Task<Result<Arc<Chart>>>>,
    icons: Arc<Icons>,
    rank_icons: [SafeTexture; 8],
}

impl MessagePage {
    pub fn new(icons: Arc<Icons>, rank_icons: [SafeTexture; 8]) -> Self {
        Self {
            msgs: None,
            load_task: None,

            index: None,

            btns_scroll: Scroll::new(),
            scroll: Scroll::new(),

            action_btns: Vec::new(),

            close_btn: DRectButton::new(),

            sf: SFader::new(),
            chart_task: None,
            icons,
            rank_icons,
        }
    }

    pub fn load(&mut self) {
        if self.load_task.is_some() {
            return;
        }
        let before = self.msgs.as_ref().and_then(|it| it.last().map(|it| it.0.time));
        self.load_task = Some(Task::new(async move {
            let mut req = Client::get("/message/list");
            if let Some(before) = before {
                req = req.query(&[("before", before)]);
            }
            Ok(recv_raw(req).await?.json().await?)
        }));
    }

    fn execute_action(&mut self, t: f32, action: String) -> Result<()> {
        let (ty, param) = match action.split_once(':') {
            Some(it) => it,
            None => {
                warn!("invalid action: {action}");
                return Ok(());
            }
        };
        match ty {
            "url" => {
                open_url(param)?;
            }
            "chart" => {
                let id = match param.parse::<i32>() {
                    Ok(it) => it,
                    Err(_) => {
                        warn!("invalid chart id: {param}");
                        return Ok(());
                    }
                };
                self.chart_task = Some(Task::new(async move { Ptr::<Chart>::new(id).fetch().await }));
            }
            "user" => {
                let id = match param.parse::<i32>() {
                    Ok(it) => it,
                    Err(_) => {
                        warn!("invalid user id: {param}");
                        return Ok(());
                    }
                };
                self.sf.goto(t, ProfileScene::new(id, self.icons.user.clone(), self.rank_icons.clone()));
            }
            _ => {
                warn!("unknown action type: {ty}");
            }
        }

        Ok(())
    }
}

impl Page for MessagePage {
    fn label(&self) -> Cow<'static, str> {
        tl!("label")
    }

    fn enter(&mut self, _s: &mut SharedState) -> Result<()> {
        self.load();
        Ok(())
    }

    fn touch(&mut self, touch: &Touch, s: &mut SharedState) -> Result<bool> {
        let t = s.t;
        if prpr::ui::PREFER_XCHS_UI.load(std::sync::atomic::Ordering::Relaxed) && self.index.is_some() {
            if self.close_btn.touch(touch, t) {
                self.index = None;
            }
            return Ok(true);
        }
        if self.chart_task.is_some() {
            return Ok(false);
        }
        if self.load_task.is_none() {
            if self.btns_scroll.touch(touch, t) {
                return Ok(true);
            }
            if let Some(msgs) = &mut self.msgs {
                for (index, item) in msgs.iter_mut().enumerate() {
                    if item.1.touch(touch, t) {
                        if self.index == Some(index) {
                            self.index = None;
                        } else {
                            if get_data().message_check_time.is_none_or(|it| it < item.0.time) {
                                get_data_mut().message_check_time = Some(item.0.time);
                                save_data()?;
                            }
                            self.index = Some(index);
                        }
                        return Ok(true);
                    }
                }
                if let Some(index) = self.index {
                    for (btn, action) in self.action_btns.iter_mut().zip(&msgs[index].0.actions) {
                        if btn.touch(touch, t) {
                            let action = action.action.clone();
                            self.execute_action(t, action)?;
                            return Ok(true);
                        }
                    }
                }
            }
        }
        if self.scroll.touch(touch, t) {
            return Ok(true);
        }
        Ok(false)
    }

    fn update(&mut self, s: &mut SharedState) -> Result<()> {
        let t = s.t;
        let xchs = prpr::ui::PREFER_XCHS_UI.load(std::sync::atomic::Ordering::Relaxed);
        if self.btns_scroll.y_scroller.pulled_down && !xchs && self.index.is_none() {
            self.load();
        }
        if !xchs || self.index.is_none() {
            self.btns_scroll.update(t);
        }
        self.scroll.update(t);
        if let Some(task) = &mut self.load_task {
            if let Some(res) = task.take() {
                match res {
                    Err(err) => {
                        show_error(err.context(tl!("load-msg-fail")));
                    }
                    Ok(val) => {
                        let mt = match &mut self.msgs {
                            None => self.msgs.insert(Vec::new()),
                            Some(x) => x,
                        };
                        mt.extend(val.into_iter().map(|it| (it, DRectButton::new().with_delta(-0.001))));
                    }
                }
                self.load_task = None;
            }
        }
        if self.chart_task.is_some() {
            if let Some(res) = self.chart_task.as_mut().unwrap().take() {
                match res {
                    Err(err) => {
                        show_error(err);
                    }
                    Ok(chart) => {
                        let data = get_data();
                        let (local_path, mods) = data
                            .charts
                            .iter()
                            .find(|it| it.info.id == Some(chart.id))
                            .map(|it| (Some(it.local_path.clone()), it.mods))
                            .unwrap_or_default();
                        self.sf.goto(
                            t,
                            SongScene::new(ChartItem::from_remote(chart.as_ref()), local_path, self.icons.clone(), self.rank_icons.clone(), mods),
                        );
                    }
                }
                self.chart_task = None;
            }
        }
        Ok(())
    }

    fn render(&mut self, ui: &mut Ui, s: &mut SharedState) -> Result<()> {
        let t = s.t;
        let xchs = prpr::ui::PREFER_XCHS_UI.load(std::sync::atomic::Ordering::Relaxed);

        if xchs {
            let accent = Color::new(0.949, 0.412, 0.580, 1.0);
            let card_bg = Color::new(0.255, 0.130, 0.190, 0.96);
            let card_sel = Color::new(0.37, 0.185, 0.265, 0.99);
            let dark_text = Color::new(0.984, 0.973, 0.886, 1.0);
            let muted = Color::new(1.0, 0.776, 0.847, 0.66);
            let border_c = Color::new(1.0, 0.776, 0.847, 0.40);
            let cr = ui.content_rect();
            let gtop = cr.y + 0.155;
            let gh = cr.bottom() - gtop - 0.03;
            let cols = 3usize;
            let gap = 0.025_f32;
            let side = 0.04_f32;
            let cw = (cr.w - side * 2. - gap * (cols as f32 - 1.)) / cols as f32;
            let chh = 0.18_f32;
            let br = ui.back_rect();
            s.render_fader(ui, |ui| {
                ui.fill_path(&br.feather(-0.004).rounded(0.02), Color::new(1.0, 0.58, 0.706, 0.14));
                ui.text("\u{2190}").pos(br.center().x, br.center().y).anchor(0.5, 0.5).no_baseline().size(0.5).color(accent).draw();
                ui.text(format!("\u{2661} {}", tl!("label"))).pos(br.right() + 0.03, cr.y + 0.075).anchor(0., 0.5).no_baseline().size(0.74).color(accent).draw();
                if let Some(msgs) = &mut self.msgs {
                    if msgs.is_empty() {
                        ui.text(tl!("no-msg")).pos(cr.center().x, gtop + gh * 0.4).anchor(0.5, 0.5).no_baseline().size(0.7).color(muted).draw();
                    } else if self.index.is_none() {
                        self.btns_scroll.size((cr.w, gh));
                        ui.scope(|ui| {
                            ui.dx(cr.x);
                            ui.dy(gtop);
                            self.btns_scroll.render(ui, |ui| {
                                let count = msgs.len();
                                for (index, item) in msgs.iter_mut().enumerate() {
                                    let col = index % cols;
                                    let row = index / cols;
                                    let rr = Rect::new(side + col as f32 * (cw + gap), row as f32 * (chh + gap), cw, chh);
                                    let chosen = Some(index) == self.index;
                                    let time = item.0.time.with_timezone(&Local).format("%Y-%m-%d").to_string();
                                    item.1.render_shadow(ui, rr, t, |ui, path| {
                                        ui.fill_path(&path, if chosen { card_sel } else { card_bg });
                                        ui.stroke_path(&path, 0.003, border_c);
                                        ui.fill_path(&Rect::new(rr.x + 0.018, rr.y + 0.022, 0.006, chh - 0.044).rounded(0.003), accent);
                                        ui.text(&item.0.title).pos(rr.x + 0.036, rr.y + 0.03).anchor(0., 0.).no_baseline().multiline().max_width(rr.w - 0.05).size(0.5).color(dark_text).draw();
                                        ui.text(format!("\u{2661} {} \u{00b7} {}", item.0.author, time)).pos(rr.x + 0.036, rr.bottom() - 0.028).anchor(0., 1.).no_baseline().max_width(rr.w - 0.05).size(0.32).color(muted).draw();
                                    });
                                }
                                let rows = count.div_ceil(cols);
                                (cr.w, rows as f32 * (chh + gap) + 0.02)
                            });
                        });
                    }
                }
                if self.load_task.is_some() {
                    ui.loading(cr.center().x, gtop + gh * 0.4, t, WHITE, ());
                }
                if let Some(idx) = self.index {
                    if self.msgs.as_ref().is_some_and(|m| idx < m.len()) {
                        ui.fill_rect(ui.screen_rect(), semi_black(0.5));
                        let ov = Rect::new(cr.x + cr.w * 0.12, cr.y + 0.06, cr.w * 0.76, cr.h - 0.12);
                        ui.fill_path(&ov.rounded(0.02), Color::new(0.20, 0.10, 0.155, 0.99));
                        ui.stroke_path(&ov.rounded(0.02), 0.004, border_c);
                        let head_h = 0.085_f32;
                        ui.fill_path(&Rect::new(ov.x, ov.y, ov.w, head_h + 0.02).rounded(0.02), accent);
                        let msg = &self.msgs.as_ref().unwrap()[idx].0;
                        ui.text(&msg.title).pos(ov.x + 0.03, ov.y + head_h * 0.5).anchor(0., 0.5).no_baseline().size(0.55).max_width(ov.w - 0.13).color(WHITE).draw();
                        let cb = Rect::new(ov.right() - 0.065, ov.y + 0.016, 0.05, head_h - 0.032);
                        self.close_btn.render_shadow(ui, cb, t, |ui, path| {
                            ui.fill_path(&path, Color::new(1.0, 1.0, 1.0, 0.15));
                            ui.text("✕").pos(cb.center().x, cb.center().y).anchor(0.5, 0.5).no_baseline().size(0.5).color(WHITE).draw();
                        });
                        let pad = 0.03_f32;
                        let content_r = Rect::new(ov.x + pad, ov.y + head_h + pad, ov.w - pad * 2., ov.h - head_h - pad * 2.);
                        self.scroll.size((content_r.w, content_r.h));
                        ui.scissor(content_r, |ui| {
                            ui.scope(|ui| {
                                ui.dx(content_r.x);
                                ui.dy(content_r.y);
                                self.scroll.render(ui, |ui| {
                                    let mw = content_r.w;
                                    ui.text(&msg.author).size(0.35).color(muted).max_width(mw).draw();
                                    ui.dy(0.02);
                                    let r = ui.text(&msg.content).size(0.45).color(dark_text).multiline().max_width(mw).draw();
                                    (content_r.w, r.bottom() + 0.02)
                                });
                            });
                        });
                    }
                }
            });
            return Ok(());
        }

        let mut cr = ui.content_rect();
        let d = 0.29;
        cr.x += d;
        cr.w -= d;
        let r = Rect::new(-0.92, cr.y, 0.47, cr.h);
        let panel = semi_black(0.4);
        s.render_fader(ui, |ui| {
            ui.fill_path(&r.rounded(0.005), panel);
            let ct = r.center();
            let pad = 0.014;
            self.btns_scroll.size((r.w, r.h - pad));
            if let Some(msgs) = &mut self.msgs {
                if msgs.is_empty() {
                    ui.text(tl!("no-msg")).pos(ct.x, ct.y).anchor(0.5, 0.5).no_baseline().size(0.8).draw();
                } else {
                    ui.scope(|ui| {
                        ui.dx(r.x);
                        ui.dy(r.y + pad);
                        self.btns_scroll.render(ui, |ui| {
                            let w = r.w - pad * 2.;
                            let mut h = 0.;
                            let r = Rect::new(pad, 0., r.w - pad * 2., 0.09);
                            for (index, item) in msgs.iter_mut().enumerate() {
                                item.1.render_text_left(ui, r, t, 1., &item.0.title, 0.5, Some(index) == self.index);
                                ui.dy(r.h + pad);
                                h += r.h + pad;
                            }
                            h += pad;
                            (w, h)
                        });
                    });
                }
            }
            if self.load_task.is_some() {
                ui.fill_path(&r.rounded(0.005), semi_white(0.3));
                ui.loading(ct.x, ct.y, t, WHITE, ());
            }
        });
        s.render_fader(ui, |ui| {
            ui.fill_path(&cr.rounded(0.005), panel);

            if let Some(msg) = self.index.and_then(|it| self.msgs.as_ref().map(|msgs| &msgs[it].0)) {
                let pad = 0.03;
                ui.scope(|ui| {
                    ui.dx(cr.right() - pad);
                    ui.dy(cr.bottom() - pad);
                    self.action_btns.resize_with(msg.actions.len(), DRectButton::new);
                    let mut r = Rect::new(0., 0., 0.28, 0.1);
                    r.x -= r.w;
                    r.y -= r.h;
                    for (btn, action) in self.action_btns.iter_mut().zip(&msg.actions) {
                        btn.render_text(ui, r, t, &action.name, 0.5, false);
                        ui.dy(-r.h - 0.01);
                    }
                });

                ui.dx(cr.x + pad + 0.01);
                ui.dy(cr.y + pad);
                let mw = cr.w - pad * 2. - 0.01;
                let mut h = 0.;
                macro_rules! dy {
                    ($e:expr) => {{
                        let e = $e;
                        ui.dy(e);
                        h += e;
                    }};
                }
                dy!(ui.text(&msg.title).size(0.9).multiline().max_width(mw).draw().h + 0.017);
                let th = ui.text(
                    tl!("subtitle", "author" => msg.author.as_str(), "time" => msg.time.with_timezone(&Local).format("%Y-%m-%d %H:%M").to_string()),
                )
                .pos(0.01, 0.)
                .size(0.4)
                .color(semi_white( 0.7))
                .draw().h;
                dy!(th + 0.016);
                ui.fill_rect(Rect::new(0., 0., mw, 0.006), semi_white(0.8));
                dy!(0.015);
                self.scroll.size((mw, cr.h - h - pad));
                self.scroll.render(ui, |ui| {
                    let r = ui.text(&msg.content).size(0.46).multiline().max_width(mw).draw();
                    (mw, r.h + 0.04)
                });
            }
        });
        self.sf.render(ui, t);
        if self.chart_task.is_some() {
            ui.full_loading_simple(t);
        }
        Ok(())
    }

    fn next_scene(&mut self, s: &mut SharedState) -> prpr::scene::NextScene {
        self.sf.next_scene(s.t).unwrap_or_default()
    }
}
