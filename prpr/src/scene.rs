//! Scene management module.
#![allow(unused_macros)]

prpr_l10n::tl_file!("scene" ttl);

mod ending;
pub use ending::{EndingScene, RecordUpdateState};

mod game;
pub use game::{GameMode, GameScene, MpResult, SimpleRecord, mp_reset_result, mp_take_result};

mod gamepad_test;
pub use gamepad_test::GamepadTestScene;

mod loading;
pub use loading::{BasicPlayer, LoadingScene, SaveFn, UpdateFn, UploadFn};

use crate::{
    core::BOLD_FONT,
    ext::{semi_white, LocalTask, RectExt, SafeTexture},
    judge::Judge,
    time::TimeManager,
    ui::{BillBoard, Dialog, DRectButton, Message, MessageHandle, MessageKind, TextPainter, Ui},
};
use anyhow::{Error, Result};
use cfg_if::cfg_if;
use inputbox::{InputBox, InputMode};
use macroquad::prelude::*;
use std::{
    any::Any,
    borrow::Cow,
    cell::RefCell,
    sync::{Arc, Mutex},
};
use tracing::warn;

#[derive(Default)]
pub enum NextScene {
    #[default]
    None,
    Pop,
    PopN(usize),
    PopWithResult(Box<dyn Any>),
    PopNWithResult(usize, Box<dyn Any>),
    Exit,
    Overlay(Box<dyn Scene>),
    Replace(Box<dyn Scene>),
}

thread_local! {
    pub static BILLBOARD: RefCell<(BillBoard, TimeManager)> = RefCell::new((BillBoard::new(), TimeManager::default()));
    pub static DIALOG: RefCell<Option<Dialog>> = const { RefCell::new(None) };
    pub static FULL_LOADING: RefCell<Option<FullLoadingView>> = const { RefCell::new(None) };
    /// 游戏内输入对话框
    pub static INPUT_DIALOG: RefCell<Option<InputDialog>> = const { RefCell::new(None) };
}

pub struct FullLoadingView {
    keep_alive: Arc<()>,
    text: Option<Cow<'static, str>>,
}

impl FullLoadingView {
    pub fn begin() -> Arc<()> {
        Self::begin_inner(None)
    }
    pub fn begin_text(text: Cow<'static, str>) -> Arc<()> {
        Self::begin_inner(Some(text))
    }
    fn begin_inner(text: Option<Cow<'static, str>>) -> Arc<()> {
        let arc = Arc::new(());
        let ret = arc.clone();
        FULL_LOADING.replace(Some(Self { keep_alive: arc, text }));
        ret
    }
}

#[inline]
pub fn show_error(error: Error) {
    warn!("show error: {error:?}");
    Dialog::error(error).show();
}

pub struct MessageBuilder {
    content: String,
    kind: MessageKind,
    duration: f32,
}

impl MessageBuilder {
    pub fn new(content: String) -> Self {
        Self {
            content,
            kind: MessageKind::Info,
            duration: 2.,
        }
    }

    #[inline]
    pub fn kind(mut self, kind: MessageKind) -> Self {
        self.kind = kind;
        self
    }

    #[inline]
    pub fn duration(mut self, t: f32) -> Self {
        self.duration = t;
        self
    }

    #[inline]
    pub fn ok(self) -> Self {
        self.kind(MessageKind::Ok)
    }

    #[inline]
    pub fn warn(self) -> Self {
        self.kind(MessageKind::Warn)
    }

    #[inline]
    pub fn error(self) -> Self {
        self.kind(MessageKind::Error)
    }

    fn show(&mut self) -> MessageHandle {
        match self.kind {
            MessageKind::Ok => crate::ui::toast_ok_sfx(),
            MessageKind::Warn => crate::ui::toast_warning_sfx(),
            MessageKind::Error => crate::ui::toast_error_sfx(),
            _ => {}
        }
        BILLBOARD.with(|it| {
            let mut guard = it.borrow_mut();
            let (msg, handle) = Message::new(std::mem::take(&mut self.content), guard.1.now() as _, self.duration, self.kind.clone());
            guard.0.add(msg);
            handle
        })
    }

    #[inline]
    pub fn handle(mut self) -> MessageHandle {
        let handle = self.show();
        std::mem::forget(self);
        handle
    }
}

impl Drop for MessageBuilder {
    fn drop(&mut self) {
        self.show();
    }
}

#[inline]
pub fn show_message(msg: impl Into<String>) -> MessageBuilder {
    MessageBuilder::new(msg.into())
}

pub static INPUT_TEXT: Mutex<(Option<String>, Option<String>)> = Mutex::new((None, None));
/// 用户取消输入（点“取消”或按 Esc）时记录的输入框 id，供 [`take_input_cancelled`]
/// 取走。与 [`INPUT_TEXT`] 区分：取消时 [`take_input`] 永远返回 `None`，调用方借此
/// 知道用户是取消了而不是还没提交。
pub static INPUT_CANCELLED: Mutex<Option<String>> = Mutex::new(None);
#[cfg(not(target_arch = "wasm32"))]
pub static CHOSEN_FILE: Mutex<(Option<String>, Option<String>)> = Mutex::new((None, None));

/// 光标左移一位（按字符边界，避免拆开多字节字符）
fn prev_boundary(text: &str, idx: usize) -> usize {
    let mut idx = idx.saturating_sub(1);
    while idx > 0 && !text.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

/// 光标右移一位（按字符边界）
fn next_boundary(text: &str, idx: usize) -> usize {
    let mut idx = (idx + 1).min(text.len());
    while idx < text.len() && !text.is_char_boundary(idx) {
        idx += 1;
    }
    idx
}

/// 游戏内输入对话框（机制移植自 Phira-Vrenxz / 上游实现，外观沿用游戏内弹窗风格）：
/// 标题 + 提示 + 输入框 + 取消/确定，自带光标、选区、剪贴板与 IME 开关；
/// 单行回车确认、Esc 取消，多行回车换行。
pub struct InputDialog {
    id: String,
    title: String,
    prompt: String,
    text: String,
    password: bool,
    multiline: bool,
    ok_label: String,
    cancel_label: String,
    ok_btn: DRectButton,
    cancel_btn: DRectButton,
    cursor: usize,
    cursor_timer: f32,
    selection: Option<(usize, usize)>,
}

impl InputDialog {
    fn new(id: String, config: InputBox) -> Self {
        let password = matches!(config.mode, InputMode::Password);
        let multiline = matches!(config.mode, InputMode::Multiline);
        let text = config.default.to_string();
        // 清空打开前累积的字符事件，避免一进输入框就自动打出一串字符
        while get_char_pressed().is_some() {}
        Self {
            id,
            title: config.title.map(|s| s.to_string()).unwrap_or_default(),
            prompt: config.prompt.map(|s| s.to_string()).unwrap_or_default(),
            cursor: text.len(),
            text,
            password,
            multiline,
            ok_label: config.ok_label.map(|s| s.to_string()).unwrap_or_else(|| ttl!("confirm").to_string()),
            cancel_label: config.cancel_label.map(|s| s.to_string()).unwrap_or_else(|| ttl!("cancel").to_string()),
            ok_btn: DRectButton::new(),
            cancel_btn: DRectButton::new(),
            cursor_timer: 0.,
            selection: None,
        }
    }

    fn confirm(&self) {
        INPUT_TEXT.lock().unwrap().1 = Some(self.text.clone());
        set_ime_enabled(false);
        android_show_keyboard(false);
    }

    fn cancel(&self) {
        *INPUT_CANCELLED.lock().unwrap() = Some(self.id.clone());
        set_ime_enabled(false);
        android_show_keyboard(false);
    }

    /// 删除当前选区（若有），返回是否真的删除了内容
    fn delete_selection(&mut self) -> bool {
        if let Some((s, e)) = self.selection.take() {
            self.text.replace_range(s..e, "");
            self.cursor = s;
            true
        } else {
            false
        }
    }

    /// 把光标移动到 `pos`；`extend`（Shift）为真时扩展选区
    fn move_cursor(&mut self, pos: usize, extend: bool) {
        if !extend {
            self.selection = None;
            self.cursor = pos;
            return;
        }
        // 锚点：已有选区时取不靠光标的那一端，否则取原光标位置
        let anchor = match self.selection {
            Some((s, e)) if e == self.cursor => s,
            Some((s, e)) if s == self.cursor => e,
            _ => self.cursor,
        };
        self.cursor = pos;
        self.selection = (anchor != pos).then(|| (anchor.min(pos), anchor.max(pos)));
    }

    /// 全选
    fn select_all(&mut self) {
        self.selection = Some((0, self.text.len()));
        self.cursor = self.text.len();
    }

    /// 复制选区
    fn copy(&self) {
        if let Some((s, e)) = self.selection {
            unsafe { get_internal_gl() }.quad_context.clipboard_set(&self.text[s..e]);
        }
    }

    /// 剪切选区
    fn cut(&mut self) {
        self.copy();
        self.delete_selection();
    }

    /// 粘贴剪贴板内容
    fn paste(&mut self) {
        if let Some(mut clip) = unsafe { get_internal_gl() }.quad_context.clipboard_get() {
            // 单行输入框里丢掉换行，避免粘贴多行文本后显示异常
            if !self.multiline {
                clip.retain(|c| c != '\r' && c != '\n');
            }
            self.delete_selection();
            self.text.insert_str(self.cursor, &clip);
            self.cursor += clip.len();
        }
    }

    /// 退格：有选区则删除选区，否则删除光标前一个字符
    fn backspace(&mut self) {
        if !self.delete_selection() && self.cursor > 0 {
            let idx = prev_boundary(&self.text, self.cursor);
            self.text.replace_range(idx..self.cursor, "");
            self.cursor = idx;
        }
    }

    /// 删除：有选区则删除选区，否则删除光标后一个字符
    fn delete_forward(&mut self) {
        if !self.delete_selection() && self.cursor < self.text.len() {
            let idx = next_boundary(&self.text, self.cursor);
            self.text.replace_range(self.cursor..idx, "");
        }
    }

    /// 处理键盘输入。返回 false 表示对话框已确认或取消，应当被关闭。
    fn update_keyboard(&mut self) -> bool {
        let ctrl = is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl);
        let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);

        if ctrl && is_key_pressed(KeyCode::A) {
            self.select_all();
        }
        if ctrl && is_key_pressed(KeyCode::C) {
            self.copy();
        }
        if ctrl && is_key_pressed(KeyCode::X) {
            self.cut();
        }
        if ctrl && is_key_pressed(KeyCode::V) {
            self.paste();
        }

        // 字符输入（含 IME 上屏）
        let mut input = String::new();
        while let Some(c) = get_char_pressed() {
            if c == '\r' || c == '\n' {
                if self.multiline {
                    input.push('\n');
                } else {
                    self.confirm();
                    return false;
                }
                continue;
            }
            // 退格/删除交给下面的按键处理，避免重复
            if c == '\u{8}' || c == '\u{7f}' {
                continue;
            }
            if c.is_control() && c != '\t' {
                continue;
            }
            input.push(c);
        }
        if !input.is_empty() {
            self.delete_selection();
            // get_char_pressed 是从队列尾部取（后进先出），所以一次提交的一批字符
            // （输入法上屏多个字、一帧内连敲多个键）顺序是反的，反转回来才是实际输入顺序
            let reversed: String = input.chars().rev().collect();
            self.text.insert_str(self.cursor, &reversed);
            self.cursor += reversed.len();
        }

        if is_key_pressed(KeyCode::Backspace) {
            self.backspace();
        }
        if is_key_pressed(KeyCode::Delete) {
            self.delete_forward();
        }
        if is_key_pressed(KeyCode::Left) {
            let pos = prev_boundary(&self.text, self.cursor);
            self.move_cursor(pos, shift);
        }
        if is_key_pressed(KeyCode::Right) {
            let pos = next_boundary(&self.text, self.cursor);
            self.move_cursor(pos, shift);
        }
        if is_key_pressed(KeyCode::Home) {
            self.move_cursor(0, shift);
        }
        if is_key_pressed(KeyCode::End) {
            let end = self.text.len();
            self.move_cursor(end, shift);
        }

        if is_key_pressed(KeyCode::Enter) && !self.multiline {
            self.confirm();
            return false;
        }
        if is_key_pressed(KeyCode::Escape) {
            self.cancel();
            return false;
        }
        true
    }

    /// 处理触摸。返回 false 表示对话框已确认或取消，应当被关闭。
    fn touch(&mut self, touch: &Touch, t: f32) -> bool {
        if self.ok_btn.touch(touch, t) {
            self.confirm();
            return false;
        }
        if self.cancel_btn.touch(touch, t) {
            self.cancel();
            return false;
        }
        true
    }

    fn render(&mut self, ui: &mut Ui, t: f32) {
        self.cursor_timer += get_frame_time();

        // 与 Dialog 保持一致：半透明遮罩 + 圆角卡片，无入场动画
        ui.fill_rect(ui.screen_rect(), Color::new(0., 0., 0., 0.6));

        let field_h: f32 = if self.multiline { 0.26 } else { 0.09 };
        let title_h: f32 = 0.16;
        let prompt_h: f32 = if self.prompt.is_empty() { 0. } else { 0.08 };
        let btn_h: f32 = 0.09;
        let pad: f32 = 0.05;
        let w: f32 = 1.0;
        let h: f32 = (pad * 2. + title_h + prompt_h + 0.04 + field_h + 0.06 + btn_h).min(ui.top * 2. * 0.8);
        let wr = Rect::new(-w / 2., -h / 2., w, h);
        ui.fill_path(&wr.rounded(0.01), ui.background());

        let cx = wr.x + pad;
        let cw = wr.w - pad * 2.;
        let mut y = wr.y + pad;
        ui.text(self.title.as_str())
            .pos(cx, y)
            .size(0.5)
            .max_width(cw)
            .color(semi_white(0.95))
            .draw_using(&BOLD_FONT);
        y += title_h;
        if !self.prompt.is_empty() {
            ui.text(self.prompt.as_str())
                .pos(cx, y)
                .size(0.34)
                .max_width(cw)
                .multiline()
                .color(semi_white(0.6))
                .draw();
            y += prompt_h;
        }
        y += 0.04;

        // 输入框
        let field = Rect::new(cx, y, cw, field_h);
        ui.fill_path(&field.rounded(0.008), Color::new(0., 0., 0., 0.35));
        ui.stroke_path(&field.rounded(0.008), 0.002, semi_white(0.16));

        let text_size = 0.4;
        let text_x = field.x + 0.02;
        let text_max_w = field.w - 0.04;
        let text_y = field.y + field_h / 2.;

        fn mask(text: &str, password: bool) -> String {
            if password {
                text.chars().map(|_| '*').collect()
            } else {
                text.to_string()
            }
        }
        let display = mask(&self.text, self.password);
        let before_cursor = mask(&self.text[..self.cursor], self.password);
        let placeholder = self.text.is_empty() && !self.prompt.is_empty();

        // 选区高亮
        if let Some((s, e)) = self.selection {
            if s < e {
                let off = ui.text(mask(&self.text[..s], self.password)).size(text_size).measure().w;
                let sel_w = ui.text(mask(&self.text[s..e], self.password)).size(text_size).measure().w;
                ui.fill_rect(Rect::new(text_x + off, field.y + 0.012, sel_w, field_h - 0.024), Color { a: 0.45, ..ui.accent() });
            }
        }

        // 文本 / 占位提示
        if placeholder {
            ui.text(self.prompt.as_str())
                .pos(text_x, text_y)
                .anchor(0., 0.5)
                .no_baseline()
                .max_width(text_max_w)
                .size(text_size)
                .color(semi_white(0.35))
                .draw();
        } else {
            ui.text(display.as_str())
                .pos(text_x, text_y)
                .anchor(0., 0.5)
                .no_baseline()
                .max_width(text_max_w)
                .size(text_size)
                .color(Color::new(1., 1., 1., 1.))
                .draw();
        }

        // 光标（闪烁）
        if !placeholder && (self.cursor_timer % 1.0) < 0.5 {
            let off = ui.text(before_cursor.as_str()).size(text_size).measure().w;
            ui.fill_rect(Rect::new(text_x + off, field.y + 0.014, 0.0025, field_h - 0.028), ui.accent());
        }

        // 按钮（与 Dialog 一致的样式）
        let gap = 0.02;
        let bw = (cw - gap) / 2.;
        let by = wr.bottom() - pad - btn_h;
        let cancel_r = Rect::new(cx, by, bw, btn_h);
        let ok_r = Rect::new(cx + bw + gap, by, bw, btn_h);
        self.cancel_btn.render_text(ui, cancel_r, t, self.cancel_label.as_str(), 0.5, false);
        self.ok_btn.render_text(ui, ok_r, t, self.ok_label.as_str(), 0.5, true);
    }
}

/// 对当前输入对话框执行操作（供 Android 软键盘等外部输入源调用）
fn with_input_dialog(f: impl FnOnce(&mut InputDialog)) {
    INPUT_DIALOG.with(|it| {
        if let Some(dlg) = it.borrow_mut().as_mut() {
            f(dlg);
        }
    });
}

/// 全选当前输入框内容
pub fn input_dialog_select_all() {
    with_input_dialog(|d| d.select_all());
}

/// 当前输入框退格
pub fn input_dialog_backspace() {
    with_input_dialog(|d| d.backspace());
}

/// 复制当前输入框选区
pub fn input_dialog_copy() {
    with_input_dialog(|d| d.copy());
}

/// 剪切当前输入框选区
pub fn input_dialog_cut() {
    with_input_dialog(|d| d.cut());
}

/// 粘贴到当前输入框
pub fn input_dialog_paste() {
    with_input_dialog(|d| d.paste());
}

/// 外部输入源（如 Android 输入法工具栏）的编辑键：作用在当前激活的输入上，
/// 弹窗优先，否则是原位输入
fn with_active_input(dialog: impl FnOnce(&mut InputDialog), inline: impl FnOnce()) {
    if INPUT_DIALOG.with(|it| it.borrow().is_some()) {
        with_input_dialog(dialog);
    } else {
        inline();
    }
}

/// 全选当前输入内容
pub fn input_select_all() {
    with_active_input(|d| d.select_all(), crate::ui::inline_input_select_all);
}

/// 当前输入退格
pub fn input_backspace() {
    with_active_input(|d| d.backspace(), crate::ui::inline_input_backspace);
}

/// 复制当前输入选区
pub fn input_copy() {
    with_active_input(|d| d.copy(), crate::ui::inline_input_copy);
}

/// 剪切当前输入选区
pub fn input_cut() {
    with_active_input(|d| d.cut(), crate::ui::inline_input_cut);
}

/// 粘贴到当前输入
pub fn input_paste() {
    with_active_input(|d| d.paste(), crate::ui::inline_input_paste);
}

#[cfg(windows)]
#[link(name = "imm32")]
extern "system" {
    fn GetActiveWindow() -> *mut std::ffi::c_void;
    fn ImmGetContext(hwnd: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    fn ImmSetOpenStatus(himc: *mut std::ffi::c_void, fopen: i32);
    fn ImmReleaseContext(hwnd: *mut std::ffi::c_void, himc: *mut std::ffi::c_void) -> i32;
}

/// 只在 Windows 上切换系统 IME 的开关状态
#[cfg(windows)]
pub(crate) fn set_ime_enabled(enabled: bool) {
    unsafe {
        let hwnd = GetActiveWindow();
        if hwnd.is_null() {
            return;
        }
        let himc = ImmGetContext(hwnd);
        if himc.is_null() {
            return;
        }
        ImmSetOpenStatus(himc, if enabled { 1 } else { 0 });
        ImmReleaseContext(hwnd, himc);
    }
}

// 注意：set_ime_enabled 只在 Windows 上操作系统 IME 状态；Android 的软键盘只能
// 在“确实打开/关闭输入框”时弹出/收起，不能在这里统一挂钩（否则退出谱面等恢复
// IME 的调用也会误弹键盘）。
#[cfg(not(windows))]
pub(crate) fn set_ime_enabled(_enabled: bool) {}

/// Android：打开/关闭软键盘（只在真正弹出/收起输入框时调用）
pub fn android_show_keyboard(show: bool) {
    crate::ui::set_soft_keyboard(show);
}

#[inline]
pub fn request_input(id: impl Into<String>, mut config: InputBox) {
    let id = id.into();
    *INPUT_TEXT.lock().unwrap() = (Some(id.clone()), None);
    *INPUT_CANCELLED.lock().unwrap() = None;
    if config.title.is_none() {
        config = config.title(ttl!("input"));
    }
    if config.prompt.is_none() {
        config = config.prompt(ttl!("input-msg"));
    }
    if config.cancel_label.is_none() {
        config = config.cancel_label(ttl!("cancel"));
    }
    if config.ok_label.is_none() {
        config = config.ok_label(ttl!("confirm"));
    }
    INPUT_DIALOG.with(|it| *it.borrow_mut() = Some(InputDialog::new(id, config)));
    set_ime_enabled(true);
    android_show_keyboard(true);
}

/// 原位输入（不弹对话框）：在刚点击的控件位置就地编辑。登录/注册字段、搜索框、
/// 首启向导字段这类“表单类”输入走这条路，其他一次性提示仍用 [`request_input`]。
/// 结果用 [`take_input`] 取，取消用 [`take_input_cancelled`] 取。
#[inline]
pub fn request_input_inline(id: impl Into<String>, config: InputBox) {
    let id = id.into();
    *INPUT_TEXT.lock().unwrap() = (Some(id.clone()), None);
    *INPUT_CANCELLED.lock().unwrap() = None;
    let password = matches!(config.mode, InputMode::Password);
    // 优先使用最后点击的按钮位置（原位显示），没有则屏幕中间
    let rect = crate::ui::take_last_clicked_rect().unwrap_or(Rect { x: -0.4, y: -0.08, w: 0.8, h: 0.1 });
    crate::ui::activate_inline_input(id, Some(rect), config.default.to_string(), password);
    set_ime_enabled(true);
}

pub fn take_input() -> Option<(String, String)> {
    // 原位输入的结果优先
    if let Some(result) = crate::ui::take_inline_result() {
        return Some(result);
    }
    let mut w = INPUT_TEXT.lock().unwrap();
    w.0.clone().zip(std::mem::take(&mut w.1))
}

/// 取出被取消的输入框 id（见 [`INPUT_CANCELLED`]）
pub fn take_input_cancelled() -> Option<String> {
    crate::ui::take_inline_cancelled().or_else(|| INPUT_CANCELLED.lock().unwrap().take())
}

pub fn return_input(id: String, text: String) {
    *INPUT_TEXT.lock().unwrap() = (Some(id), Some(text));
}

#[cfg(not(target_arch = "wasm32"))]
pub fn request_file(id: impl Into<String>) {
    let id: String = id.into();
    #[cfg(target_env = "ohos")]
    let is_photo = id == "avatar";
    *CHOSEN_FILE.lock().unwrap() = (Some(id), None);
    cfg_if! {
        if #[cfg(target_os = "android")] {
            unsafe {
                let env = miniquad::native::attach_jni_env();
                let ctx = ndk_context::android_context().context();
                let class = (**env).GetObjectClass.unwrap()(env, ctx);
                let method = (**env).GetMethodID.unwrap()(env, class, c"chooseFile".as_ptr() as _, c"()V".as_ptr() as _);
                (**env).CallVoidMethod.unwrap()(env, ctx, method);
            }
        } else if #[cfg(target_os = "ios")] {
            use objc2::{available, define_class, rc::Retained, runtime::ProtocolObject, MainThreadMarker, MainThreadOnly};
            use objc2_foundation::{NSArray, NSObject, NSObjectProtocol, NSString, NSURL};
            use objc2_ui_kit::{UIDocumentPickerDelegate, UIDocumentPickerViewController};

            thread_local! {
                static DELEGATE: RefCell<Option<Retained<PickerDelegate>>> = const { RefCell::new(None) };
            }

            define_class! {
                // SAFETY:
                // - The superclass NSObject does not have any subclassing requirements.
                // - `Delegate` does not implement `Drop`.
                #[unsafe(super = NSObject)]
                #[thread_kind = MainThreadOnly]
                struct PickerDelegate;

                // SAFETY: `NSObjectProtocol` has no safety requirements.
                unsafe impl NSObjectProtocol for PickerDelegate {}

                // SAFETY: `UIDocumentPickerDelegate` has no safety requirements.
                unsafe impl UIDocumentPickerDelegate for PickerDelegate {
                    // SAFETY: The signature is correct.
                    #[unsafe(method(documentPicker:didPickDocumentsAtURLs:))]
                    fn did_pick_documents_at_urls(&self, controller: &UIDocumentPickerViewController, urls: &NSArray<NSURL>) {
                        use objc2_foundation::{NSData, NSDataReadingOptions, NSTemporaryDirectory};

                        let url = urls.firstObject().unwrap();
                        let need_close = unsafe { url.startAccessingSecurityScopedResource() };

                        let data = match NSData::dataWithContentsOfURL_options_error(&url, NSDataReadingOptions::Uncached) {
                            Ok(data) => data,
                            Err(err) => {
                                let message = err.localizedDescription().to_string();
                                show_error(Error::msg(message).context(ttl!("read-file-failed")));
                                return;
                            }
                        };
                        if need_close {
                            unsafe { url.stopAccessingSecurityScopedResource() };
                        }

                        let dir = NSTemporaryDirectory();
                        let path = format!("{}{}", dir, uuid::Uuid::new_v4());
                        data.writeToFile_atomically(&NSString::from_str(&path), true);
                        CHOSEN_FILE.lock().unwrap().1 = Some(path);
                    }
                }
            }

            impl PickerDelegate {
                fn new(mtm: MainThreadMarker) -> Retained<Self> {
                    let this = Self::alloc(mtm).set_ivars(());
                    unsafe { objc2::msg_send![super(this), init] }
                }
            }

            let mtm = MainThreadMarker::new().unwrap();

            let picker = UIDocumentPickerViewController::alloc(mtm);
            let picker = if available!(ios = 14.0.0) {
                use objc2_uniform_type_identifiers::UTType;

                let ext = |e: &str| UTType::typeWithFilenameExtension(&NSString::from_str(e)).unwrap();
                let types = NSArray::from_retained_slice(&[
                    ext("zip"),
                    ext("pez"),
                    ext("jpg"),
                    ext("png"),
                    ext("jpeg"),
                    ext("json"),
                    ext("mp3"),
                    ext("ogg"),
                ]);
                UIDocumentPickerViewController::initForOpeningContentTypes(picker, &types)
            } else {
                #[allow(deprecated)]
                {
                    use objc2_ui_kit::UIDocumentPickerMode;

                    let ext = NSString::from_str;
                    let types = NSArray::from_retained_slice(&[ext("public.image"), ext("public.archive")]);
                    UIDocumentPickerViewController::initWithDocumentTypes_inMode(picker, &types, UIDocumentPickerMode::Import)
                }
            };
            let dlg_obj = PickerDelegate::new(mtm);
            picker.setDelegate(Some(ProtocolObject::from_ref(&*dlg_obj)));
            DELEGATE.with(|it| *it.borrow_mut() = Some(dlg_obj));

            inputbox::backend::IOS::get_top_view_controller(mtm)
                .unwrap()
                .presentViewController_animated_completion(&picker, true, None);
        } else if #[cfg(target_env = "ohos")] {
            miniquad::native::call_request_callback(format!(r#"{{"action": "chooseFile", "isPhoto": {}}}"#, is_photo));
        } else { // desktop
            CHOSEN_FILE.lock().unwrap().1 = rfd::FileDialog::new().pick_file().map(|it| it.display().to_string());
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn take_file() -> Option<(String, String)> {
    let mut w = CHOSEN_FILE.lock().unwrap();
    w.0.clone().zip(std::mem::take(&mut w.1))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn return_file(id: String, file: String) {
    *CHOSEN_FILE.lock().unwrap() = (Some(id), Some(file));
}

pub trait Scene {
    fn enter(&mut self, _tm: &mut TimeManager, _target: Option<RenderTarget>) -> Result<()> {
        Ok(())
    }
    fn pause(&mut self, _tm: &mut TimeManager) -> Result<()> {
        Ok(())
    }
    fn resume(&mut self, _tm: &mut TimeManager) -> Result<()> {
        Ok(())
    }
    fn on_result(&mut self, _tm: &mut TimeManager, _result: Box<dyn Any>) -> Result<()> {
        Ok(())
    }
    fn touch(&mut self, _tm: &mut TimeManager, _touch: &Touch) -> Result<bool> {
        Ok(false)
    }
    fn update(&mut self, tm: &mut TimeManager) -> Result<()>;
    fn render(&mut self, tm: &mut TimeManager, ui: &mut Ui) -> Result<()>;
    fn next_scene(&mut self, _tm: &mut TimeManager) -> NextScene {
        NextScene::None
    }
    fn nav_enabled(&self) -> bool {
        true
    }
}

pub trait RenderTargetChooser {
    fn choose(&mut self) -> Option<RenderTarget>;
}
impl RenderTargetChooser for Option<RenderTarget> {
    fn choose(&mut self) -> Option<RenderTarget> {
        *self
    }
}
impl<F: FnMut() -> Option<RenderTarget>> RenderTargetChooser for F {
    fn choose(&mut self) -> Option<RenderTarget> {
        self()
    }
}

static EXTRA_TOP_RENDER: std::sync::Mutex<Option<fn()>> = std::sync::Mutex::new(None);

pub fn set_extra_top_render(f: Option<fn()>) {
    *EXTRA_TOP_RENDER.lock().unwrap() = f;
}

pub struct Main {
    pub scenes: Vec<Box<dyn Scene>>,
    times: Vec<f64>,
    target_chooser: Box<dyn RenderTargetChooser>,
    tm: TimeManager,
    paused: bool,
    last_update_time: f64,
    should_exit: bool,
    pub top_level: bool,
    touches: Option<Vec<Touch>>,
    pub viewport: Option<(i32, i32, i32, i32)>,
    gp_test_timer: f32,
}

impl Main {
    pub async fn new(mut scene: Box<dyn Scene>, mut tm: TimeManager, mut target_chooser: impl RenderTargetChooser + 'static) -> Result<Self> {
        simulate_mouse_with_touch(false);
        scene.enter(&mut tm, target_chooser.choose())?;
        let last_update_time = tm.now();
        macro_rules! load_tex {
            ($path:literal) => {
                SafeTexture::from(Texture2D::from_image(&load_image($path).await?))
            };
        }
        let icons = [load_tex!("info.png"), load_tex!("warn.png"), load_tex!("ok.png"), load_tex!("error.png")];
        BILLBOARD.with(|it| it.borrow_mut().0.set_icons(icons));
        Ok(Self {
            scenes: vec![scene],
            times: Vec::new(),
            target_chooser: Box::new(target_chooser),
            tm,
            paused: false,
            last_update_time,
            should_exit: false,
            top_level: true,
            touches: None,
            viewport: None,
            gp_test_timer: 0.0,
        })
    }

    pub fn update(&mut self) -> Result<()> {
        self.update_with_mutate(|_| {})
    }

    pub fn update_with_mutate(&mut self, f: impl Fn(&mut Touch)) -> Result<()> {
        if self.paused {
            return Ok(());
        }
        match self.scenes.last_mut().unwrap().next_scene(&mut self.tm) {
            NextScene::None => {}
            NextScene::Pop => {
                self.scenes.pop();
                self.tm.seek_to(self.times.pop().unwrap());
                self.scenes.last_mut().unwrap().enter(&mut self.tm, self.target_chooser.choose())?;
            }
            NextScene::PopN(num) => {
                for _ in 0..num {
                    self.scenes.pop();
                    self.tm.seek_to(self.times.pop().unwrap());
                }
                self.scenes.last_mut().unwrap().enter(&mut self.tm, self.target_chooser.choose())?;
            }
            NextScene::PopWithResult(result) => {
                self.scenes.pop();
                self.tm.seek_to(self.times.pop().unwrap());
                self.scenes.last_mut().unwrap().on_result(&mut self.tm, result)?;
                self.scenes.last_mut().unwrap().enter(&mut self.tm, self.target_chooser.choose())?;
            }
            NextScene::PopNWithResult(num, result) => {
                for _ in 0..num {
                    self.scenes.pop();
                    self.tm.seek_to(self.times.pop().unwrap());
                }
                self.scenes.last_mut().unwrap().on_result(&mut self.tm, result)?;
                self.scenes.last_mut().unwrap().enter(&mut self.tm, self.target_chooser.choose())?;
            }
            NextScene::Exit => {
                self.should_exit = true;
            }
            NextScene::Overlay(mut scene) => {
                self.times.push(self.tm.now());
                scene.enter(&mut self.tm, self.target_chooser.choose())?;
                self.scenes.push(scene);
            }
            NextScene::Replace(mut scene) => {
                scene.enter(&mut self.tm, self.target_chooser.choose())?;
                *self.scenes.last_mut().unwrap() = scene;
            }
        }
        crate::gamepad::poll_global();

        // --- Gamepad test entry detection ---
        {
            let nav_enabled = self.scenes.last().map(|s| s.nav_enabled()).unwrap_or(false)
                && !DIALOG.with(|it| it.borrow().is_some());
            let f9_pressed = is_key_pressed(KeyCode::F9);

            // F9 works in any state, LB+RB only in non-gameplay
            if f9_pressed {
                self.gp_test_timer = 0.0;
                self.times.push(self.tm.now());
                GamepadTestScene::new().enter(&mut self.tm, self.target_chooser.choose())?;
                self.scenes.push(Box::new(GamepadTestScene::new()));
            } else if nav_enabled {
                let raw = crate::gamepad::raw_state_static();
                let both_pressed = raw.lb && raw.rb;
                if both_pressed {
                    self.gp_test_timer += macroquad::prelude::get_frame_time();
                    if self.gp_test_timer >= 3.0 {
                        self.gp_test_timer = 0.0;
                        self.times.push(self.tm.now());
                        GamepadTestScene::new().enter(&mut self.tm, self.target_chooser.choose())?;
                        self.scenes.push(Box::new(GamepadTestScene::new()));
                    }
                } else {
                    self.gp_test_timer = 0.0;
                }
            } else {
                self.gp_test_timer = 0.0;
            }
        }

        Judge::on_new_frame();
        // 处理原位输入框的键盘输入
        crate::ui::update_inline_input();
        let mut touches = Judge::get_touches();
        touches.iter_mut().for_each(f);
        // 原位输入框的触摸处理：激活期间吃掉所有触摸，不让页面响应
        if crate::ui::is_inline_input_active() {
            for touch in &touches {
                use macroquad::input::TouchPhase;
                let phase = match touch.phase {
                    TouchPhase::Started => 0u8,
                    TouchPhase::Moved => 1u8,
                    TouchPhase::Ended | TouchPhase::Cancelled => 2u8,
                    TouchPhase::Stationary => continue,
                };
                crate::ui::handle_inline_input_touch(touch.position.x, touch.position.y, phase);
            }
            touches.clear();
        }
        if !(touches.is_empty() || FULL_LOADING.with(|it| it.borrow().is_some())) {
            let now = self.tm.now();
            let delta = (now - self.last_update_time) / touches.len() as f64;
            let start_time = self.tm.start_time;
            let mut last_err = None;
            DIALOG.with(|it| -> Result<()> {
                let mut index = 1;
                touches.retain_mut(|touch| {
                    let t = self.last_update_time + (index + 1) as f64 * delta;
                    index += 1;
                    let mut guard = it.borrow_mut();
                    if let Some(dialog) = guard.as_mut() {
                        if !dialog.touch(touch, t as _) {
                            drop(guard);
                            *it.borrow_mut() = None;
                        }
                        false
                    } else {
                        drop(guard);
                        // 输入对话框打开时优先消费触摸（不传给场景）
                        let consumed = INPUT_DIALOG.with(|it| {
                            let mut guard = it.borrow_mut();
                            if let Some(dlg) = guard.as_mut() {
                                if !dlg.touch(touch, t as _) {
                                    drop(guard);
                                    *it.borrow_mut() = None;
                                }
                                true
                            } else {
                                false
                            }
                        });
                        if consumed {
                            false
                        } else {
                            self.tm.seek_to(t);
                            match self.scenes.last_mut().unwrap().touch(&mut self.tm, touch) {
                                Ok(val) => !val,
                                Err(err) => {
                                    warn!(?err, "failed to handle touch");
                                    last_err = Some(err);
                                    false
                                }
                            }
                        }
                    }
                });
                Ok(())
            })?;
            if let Some(err) = last_err {
                return Err(err);
            }
            self.tm.start_time = start_time;
        }
        self.touches = Some(touches);
        self.last_update_time = self.tm.now();
        DIALOG.with(|it| {
            if let Some(dialog) = it.borrow_mut().as_mut() {
                dialog.update(self.last_update_time as _);
            }
        });
        self.scenes.last_mut().unwrap().update(&mut self.tm)?;
        Ok(())
    }

    pub fn render(&mut self, painter: &mut TextPainter) -> Result<()> {
        if self.paused {
            return Ok(());
        }

        crate::ui::clear_focus_targets();

        let mut ui = Ui::new(painter, self.viewport);
        ui.set_touches(self.touches.take().unwrap());
        ui.scope(|ui| self.scenes.last_mut().unwrap().render(&mut self.tm, ui))?;

        let has_dialog = DIALOG.with(|it| it.borrow().is_some());

        // If dialog is open, clear scene focus targets so only dialog buttons are navigable
        if has_dialog {
            crate::ui::clear_focus_targets();
        }

        if self.top_level {
            push_camera_state();
            set_camera(&ui.camera());
            let mut gl = unsafe { get_internal_gl() };
            gl.flush();

            // 渲染原位输入框（在 set_camera 之后，确保相机状态正确，避免文字翻转）
            crate::ui::render_inline_input(&mut ui, self.tm.now());
            // 输入对话框：键盘处理也放在这里，避免字符事件先被其他系统消费
            INPUT_DIALOG.with(|it| {
                let mut guard = it.borrow_mut();
                if let Some(dlg) = guard.as_mut() {
                    if !dlg.update_keyboard() {
                        drop(guard);
                        *it.borrow_mut() = None;
                    } else {
                        dlg.render(&mut ui, self.tm.now() as _);
                    }
                }
            });

            BILLBOARD.with(|it| {
                let mut guard = it.borrow_mut();
                let t = guard.1.now() as f32;
                guard.0.render(&mut ui, t);
            });

            // Render dialog before navigation so its buttons register as focus targets
            if has_dialog {
                DIALOG.with(|it| {
                    if let Some(dialog) = it.borrow_mut().as_mut() {
                        dialog.render(&mut ui, self.tm.now() as _);
                    }
                });
            }

            let nav_enabled = self.scenes.last().map(|s| s.nav_enabled()).unwrap_or(false)
                || has_dialog;
            let targets = crate::ui::get_focus_targets();
            let gamepad_input = crate::gamepad::nav_input_static();
            let nav_state = if nav_enabled {
                crate::gamepad::update_nav(&targets, gamepad_input, macroquad::prelude::get_frame_time())
            } else {
                crate::gamepad::NavState::default()
            };

            if nav_enabled && crate::gamepad::is_connected() {
                if let Some(target) = nav_state.current_target(&targets) {
                    ui.draw_focus_frame(target.rect, nav_state.phase);
                }
            }

            if nav_enabled {
                self.handle_nav_actions(&targets, &nav_state, gamepad_input, &mut ui);
            }
            let remove = FULL_LOADING.with(|it| {
                if let Some(loading) = it.borrow_mut().as_mut() {
                    if Arc::strong_count(&loading.keep_alive) > 1 {
                        if let Some(text) = loading.text.as_ref() {
                            ui.full_loading(text.clone(), self.tm.now() as _);
                        } else {
                            ui.full_loading_simple(self.tm.now() as _);
                        }
                        return false;
                    } else {
                        return true;
                    }
                }
                false
            });
            if remove {
                FULL_LOADING.take();
            }
            if let Some(f) = *EXTRA_TOP_RENDER.lock().unwrap() {
                f();
            }
            pop_camera_state();
        }
        Ok(())
    }

    pub fn pause(&mut self) -> Result<()> {
        self.paused = true;
        self.scenes.last_mut().unwrap().pause(&mut self.tm)
    }

    pub fn resume(&mut self) -> Result<()> {
        self.paused = false;
        self.scenes.last_mut().unwrap().resume(&mut self.tm)
    }

    pub fn should_exit(&self) -> bool {
        self.should_exit
    }

    fn handle_nav_actions(
        &mut self,
        targets: &[crate::ui::FocusTarget],
        nav_state: &crate::gamepad::NavState,
        input: crate::gamepad::NavInput,
        ui: &mut Ui,
    ) {
        // A: click focused target, or trigger dialog-mapped A action when a dialog is open
        if input.a_pressed {
            let mut handled = false;
            DIALOG.with(|d| {
                let mut guard = d.borrow_mut();
                if let Some(dialog) = guard.as_mut() {
                    handled = dialog.handle_gamepad_button('A');
                }
            });
            if handled {
                return;
            }
            if let Some(target) = nav_state.current_target(targets) {
                let center = target.rect.center();
                // Convert global UI coords to screen pixels using viewport
                let vp = crate::ext::get_viewport();
                let sh = screen_height();
                let aspect = vp.2 as f32 / vp.3 as f32;
                let px = (center.x + 1.0) * 0.5 * vp.2 as f32 + vp.0 as f32;
                let py = (center.y * aspect + 1.0) * 0.5 * vp.3 as f32 + (sh - (vp.1 + vp.3) as f32);
                crate::gamepad::push_nav_touch(vec2(px, py));
            }
        }

        // B: dialog-mapped B action, or cancel dialog / back
        if input.b_pressed {
            let mut handled_dialog = false;
            DIALOG.with(|d| {
                let mut guard = d.borrow_mut();
                if let Some(dialog) = guard.as_mut() {
                    handled_dialog = dialog.handle_gamepad_button('B');
                }
            });
            if handled_dialog { return; }
            if DIALOG.with(|d| d.borrow().is_some()) {
                DIALOG.with(|d| *d.borrow_mut() = None);
                return;
            }
            crate::gamepad::push_nav_back();
        }

        // HOME/Mode: exit confirmation on main scene
        if input.home_pressed {
            if self.scenes.len() <= 1 && !DIALOG.with(|d| d.borrow().is_some()) {
                let confirm_exit = crate::ui::Dialog::plain(
                    "退出 Phira-Firefly",
                    "确定要退出 Phira-Firefly 吗？",
                ).buttons(vec!["确定".to_string(), "取消".to_string()])
                .listener(move |_dlg, pos| {
                    if pos == 0 {
                        std::process::exit(0);
                    }
                    false
                });
                confirm_exit.show();
            }
        }

        if input.x_pressed {
            // X may map to a dialog action (for 3-button dialogs); prefer dialog handling
            let mut handled = false;
            DIALOG.with(|d| {
                let mut guard = d.borrow_mut();
                if let Some(dialog) = guard.as_mut() {
                    handled = dialog.handle_gamepad_button('X');
                }
            });
            if handled { return; }
            if !DIALOG.with(|d| d.borrow().is_some()) {
                crate::gamepad::push_nav_multilang();
            }
        }

        if input.y_pressed {
            let mut handled = false;
            DIALOG.with(|d| {
                let mut guard = d.borrow_mut();
                if let Some(dialog) = guard.as_mut() {
                    handled = dialog.handle_gamepad_button('Y');
                }
            });
            if handled { return; }
        }
    }
}

fn draw_background(tex: Texture2D, viewport: (i32, i32, i32, i32)) {
    let asp = viewport.2 as f32 / viewport.3 as f32;
    let top = 1. / asp;
    draw_texture_ex(
        tex,
        -1.,
        -top,
        WHITE,
        DrawTextureParams {
            dest_size: Some(vec2(2., top * 2.)),
            ..Default::default()
        },
    );
    draw_rectangle(-1., -top, 2., top * 2., Color::new(0., 0., 0., 0.3));
}

pub type LocalSceneTask = LocalTask<Result<NextScene>>;
