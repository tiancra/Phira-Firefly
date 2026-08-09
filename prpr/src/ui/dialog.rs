prpr_l10n::tl_file!("dialog");

use super::{DRectButton, RectButton, Scroll, Ui};
use crate::{core::BOLD_FONT, ext::{RectExt, semi_white}, gamepad, scene::show_message};
use anyhow::Error;
use macroquad::prelude::*;

const WIDTH_RADIO: f32 = 0.5;
const HEIGHT_RATIO: f32 = 0.7;

type DialogListener = dyn FnMut(&mut Dialog, i32) -> bool;

#[must_use]
pub struct Dialog {
    title: String,
    message: String,
    buttons: Vec<String>,
    /// listener function returns `false` to close the dialog, `true` to keep it open
    /// the parameter is the *index* of the button clicked, `-1` for outside click, `-2` for text
    listener: Option<Box<DialogListener>>,

    text_btn: RectButton,

    h: Option<f32>,

    scroll: Scroll,
    window_rect: Option<Rect>,
    rect_buttons: Vec<DRectButton>,
}

impl Default for Dialog {
    fn default() -> Self {
        Self {
            title: tl!("notice").to_string(),
            message: String::new(),
            buttons: vec![tl!("ok").to_string()],
            listener: None,

            text_btn: RectButton::new(),

            h: None,

            scroll: Scroll::new(),
            window_rect: None,
            rect_buttons: vec![DRectButton::new()],
        }
    }
}

impl Dialog {
    pub fn simple(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            ..Default::default()
        }
    }

    pub fn plain(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            ..Default::default()
        }
    }

    pub fn error(error: Error) -> Self {
        let error = format!("{error:?}");
        Self {
            title: tl!("error").to_string(),
            message: error.clone(),
            buttons: vec![tl!("error-copy").to_string(), tl!("ok").to_string()],
            listener: Some(Box::new(move |_dialog, pos| {
                if pos == 0 {
                    unsafe { get_internal_gl() }.quad_context.clipboard_set(&error);
                    show_message(tl!("error-copied")).ok();
                }
                false
            })),

            rect_buttons: vec![DRectButton::new(); 2],
            ..Default::default()
        }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn message(mut self, message: impl Into<String>) -> Self {
        self.set_message(message);
        self
    }

    pub fn set_message(&mut self, message: impl Into<String>) {
        self.message = message.into();
    }

    pub fn buttons(mut self, buttons: Vec<String>) -> Self {
        self.set_buttons(buttons);
        self
    }

    pub fn set_buttons(&mut self, buttons: Vec<String>) {
        self.buttons = buttons;
        self.rect_buttons = vec![DRectButton::new(); self.buttons.len()];
    }

    pub fn listener(mut self, f: impl FnMut(&mut Dialog, i32) -> bool + 'static) -> Self {
        self.listener = Some(Box::new(f));
        self
    }

    pub fn show(self) {
        crate::scene::DIALOG.with(|it| *it.borrow_mut() = Some(self));
    }

    fn should_show_gamepad_hint_for_buttons(&self) -> bool {
        // Show gamepad hint for dialogs with 1..=3 buttons
        let n = self.buttons.len();
        n >= 1 && n <= 3
    }

    fn should_show_gamepad_hint(&self) -> bool {
        gamepad::is_connected() && self.should_show_gamepad_hint_for_buttons()
    }

    fn invoke_listener(&mut self, pos: i32) -> bool {
        if let Some(mut listener) = self.listener.take() {
            let keep_open = listener(self, pos);
            self.listener = Some(listener);
            keep_open
        } else {
            false
        }
    }

    pub fn handle_gamepad_confirm(&mut self) -> bool {
        // Route to generic handler so single-button dialogs work with A as confirm
        if self.should_show_gamepad_hint() {
            self.handle_gamepad_button('A')
        } else {
            false
        }
    }

    pub fn handle_gamepad_cancel(&mut self) -> bool {
        // Route to generic handler so single-button dialogs work with B as cancel
        if self.should_show_gamepad_hint() {
            self.handle_gamepad_button('B')
        } else {
            false
        }
    }

    fn gamepad_index_for(&self, btn: char) -> Option<i32> {
        let n = self.buttons.len();
        match n {
            1 => {
                // Single-button dialogs: accept A or B to activate the only button
                match btn {
                    'A' | 'B' => Some(0),
                    _ => None,
                }
            }
            2 => {
                // keep previous mapping: A -> second, B -> first
                match btn {
                    'A' => Some(1),
                    'B' => Some(0),
                    _ => None,
                }
            }
            3 => {
                match btn {
                    'B' => Some(0),
                    'Y' => Some(1),
                    'X' => Some(2),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    pub fn handle_gamepad_button(&mut self, btn: char) -> bool {
        if !self.should_show_gamepad_hint() { return false; }
        if let Some(idx) = self.gamepad_index_for(btn) {
            self.invoke_listener(idx)
        } else {
            false
        }
    }

    pub fn touch(&mut self, touch: &Touch, t: f32) -> bool {
        self.scroll.touch(touch, t);
        let mut exit = false;
        for (index, btn) in self.rect_buttons.iter_mut().enumerate() {
            if btn.touch(touch, t) {
                if !self.invoke_listener(index as i32) {
                    exit = true;
                }
                break;
            }
        }
        if !self.should_show_gamepad_hint() && self.text_btn.touch(touch) {
            if !self.invoke_listener(-2) {
                exit = true;
            }
        }
        if exit {
            return false;
        }

        if self
            .window_rect
            .is_none_or(|rect| rect.contains(touch.position) || touch.phase != TouchPhase::Started)
        {
            true
        } else {
            if let Some(mut listener) = self.listener.take() {
                if listener(self, -1) {
                    return true;
                }
                self.listener = Some(listener);
            }
            false
        }
    }

    pub fn update(&mut self, t: f32) {
        self.scroll.update(t);
    }

    pub fn render(&mut self, ui: &mut Ui, t: f32) {
        ui.fill_rect(ui.screen_rect(), Color::new(0., 0., 0., 0.6));

        let mh = ui.top * 2. * HEIGHT_RATIO;
        let s = 0.02;
        let pad = 0.02;
        let bh = 0.09;

        if self.h.is_none() {
            self.h = Some(
                (ui.text(&self.message)
                    .size(0.5)
                    .max_width(2. * WIDTH_RADIO - pad * 3.)
                    .multiline()
                    .measure()
                    .h
                    + ui.text(&self.title).size(0.95).no_baseline().measure().h
                    + bh
                    + 0.22)
                    .min(mh),
            );
        }
        let mut wr = Rect::new(0., 0., 2. * WIDTH_RADIO, self.h.unwrap());
        wr.x = -wr.w / 2.;
        wr.y = -wr.h / 2.;
        self.window_rect = Some(ui.rect_to_global(wr));
        ui.fill_path(&wr.rounded(0.01), ui.background());

        ui.scope(|ui| {
            let s = 0.01;
            let pad = 0.02;
            let mut h = 0.;
            macro_rules! dy {
                ($val:expr) => {{
                    let dy = $val;
                    h += dy;
                    ui.dy(dy);
                }};
            }
            dy!(wr.y + s * 3.);
            let r = ui
                .text(&self.title)
                .pos(wr.x + pad * 2., 0.)
                .anchor(0., 0.)
                .size(0.95)
                .max_width(wr.w - pad * 2.)
                .no_baseline()
                .draw_using(&BOLD_FONT);
            dy!(r.h + s * 2.);
            self.scroll.size((wr.w - pad * 2., wr.bottom() - h - bh - s * 2.));
            ui.dx(wr.x + pad);
            self.scroll.render(ui, |ui| {
                let r = ui
                    .text(&self.message)
                    .pos(pad, 0.)
                    .size(0.5)
                    .max_width(wr.w - pad * 3.)
                    .multiline()
                    .draw();
                self.text_btn.set(ui, r);
                (r.w, r.h + 0.04)
            });
        });
        if self.should_show_gamepad_hint() {
            let mut hints: Vec<(char, &str)> = Vec::new();
            match self.buttons.len() {
                1 => hints.push(('A', self.buttons[0].as_str())),
                2 => {
                    // A -> second, B -> first
                    hints.push(('A', self.buttons[1].as_str()));
                    hints.push(('B', self.buttons[0].as_str()));
                }
                3 => {
                    hints.push(('B', self.buttons[0].as_str()));
                    hints.push(('Y', self.buttons[1].as_str()));
                    hints.push(('X', self.buttons[2].as_str()));
                }
                _ => {}
            }
            // render hints: 1 -> center, 2 -> left/right halves centered, 3 -> centered sequence
            let y = wr.bottom() - s - bh / 2.;
            match hints.len() {
                1 => {
                    let (ch, txt) = hints.into_iter().next().unwrap();
                    let label = format!("[{}] {}", ch, txt);
                    ui.text(label.as_str())
                        .pos(wr.x + wr.w / 2.0, y)
                        .anchor(0.5, 0.5)
                        .size(0.38)
                        .no_baseline()
                        .color(semi_white(0.95))
                        .draw();
                }
                2 => {
                    let mut it = hints.into_iter();
                    let (ch1, txt1) = it.next().unwrap();
                    let (ch2, txt2) = it.next().unwrap();
                    let label1 = format!("[{}] {}", ch1, txt1);
                    let label2 = format!("[{}] {}", ch2, txt2);
                    let left_x = wr.x + wr.w * 0.25;
                    let right_x = wr.x + wr.w * 0.75;
                    ui.text(label1.as_str())
                        .pos(left_x, y)
                        .anchor(0.5, 0.5)
                        .size(0.38)
                        .no_baseline()
                        .color(semi_white(0.95))
                        .draw();
                    ui.text(label2.as_str())
                        .pos(right_x, y)
                        .anchor(0.5, 0.5)
                        .size(0.38)
                        .no_baseline()
                        .color(semi_white(0.95))
                        .draw();
                }
                _ => {
                    let mut x = wr.x + wr.w / 2.;
                    let gap = 0.04;
                    let total_w = hints.len() as f32 * 0.6 + (hints.len().saturating_sub(1) as f32) * gap;
                    let mut cx = x - total_w / 2.;
                    for (_i, (ch, txt)) in hints.into_iter().enumerate() {
                        let label = format!("[{}] {}", ch, txt);
                        ui.text(label.as_str())
                            .pos(cx + 0.3, y)
                            .anchor(0.0, 0.5)
                            .size(0.38)
                            .no_baseline()
                            .color(semi_white(0.95))
                            .draw();
                        cx += 0.6 + gap;
                    }
                }
            }
        } else {
            ui.scope(|ui| {
                let bw = (wr.w - pad * (self.buttons.len() + 1) as f32) / self.buttons.len() as f32;
                let mut r = Rect::new(wr.x + pad, wr.bottom() - s - bh, bw, bh);
                for (text, btn) in self.buttons.iter().zip(self.rect_buttons.iter_mut()) {
                    btn.render_text(ui, r, t, text, 0.5, true);
                    r.x += bw + pad;
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Dialog;

    #[test]
    fn confirm_dialog_buttons_are_detected_for_gamepad_hint() {
        let dialog = Dialog::plain("title", "message").buttons(vec!["取消".to_string(), "确定".to_string()]);
        assert!(dialog.should_show_gamepad_hint_for_buttons());
    }

    #[test]
    fn non_confirm_dialog_buttons_do_not_use_gamepad_hint() {
        let dialog = Dialog::plain("title", "message").buttons(vec!["复制".to_string(), "好的".to_string()]);
        assert!(!dialog.should_show_gamepad_hint_for_buttons());
    }
}
