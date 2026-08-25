//! UI utilities.
prpr_l10n::tl_file!("scene" ttl);
mod billboard;
pub use billboard::{BillBoard, Message, MessageHandle, MessageKind};

mod chart_info;
pub use chart_info::*;

mod dialog;
pub use dialog::Dialog;

mod scroll;
use inputbox::{InputBox, InputMode};
pub use scroll::*;

mod shading;
pub use shading::*;

mod shadow;
pub use shadow::*;

mod text;
pub use text::{DrawText, TextPainter};

pub use glyph_brush::ab_glyph::FontArc;

use crate::{
    core::{Matrix, Point, Vector},
    ext::{get_viewport, nalgebra_to_glm, semi_black, semi_white, source_of_image, RectExt, SafeTexture, ScaleType},
    judge::Judge,
    scene::{request_input, return_input, show_error, take_input},
};
use core::f32;
use lyon::{
    lyon_tessellation::{
        BuffersBuilder, FillOptions, FillTessellator, FillVertex, FillVertexConstructor, StrokeOptions, StrokeTessellator, StrokeVertex,
        StrokeVertexConstructor, VertexBuffers,
    },
    math as lm,
    path::{LineCap, Path, PathEvent},
};
use macroquad::prelude::*;
use miniquad::PassAction;
use sasa::{AudioManager, PlaySfxParams, Sfx};
use std::{
    borrow::Cow,
    cell::RefCell,
    collections::HashMap,
    ops::Range,
    sync::atomic::{AtomicBool, Ordering},
};

pub static PREFER_REDUCED_MOTION: AtomicBool = AtomicBool::new(false);

#[derive(Default, Clone, Copy)]
pub struct Gravity(u8);

impl Gravity {
    pub const LEFT: u8 = 0;
    pub const HCENTER: u8 = 1;
    pub const RIGHT: u8 = 2;
    pub const TOP: u8 = 0;
    pub const VCENTER: u8 = 4;
    pub const BOTTOM: u8 = 8;

    pub const BEGIN: u8 = Self::LEFT | Self::TOP;
    pub const CENTER: u8 = Self::HCENTER | Self::VCENTER;
    pub const END: u8 = Self::RIGHT | Self::BOTTOM;

    fn value(mode: u8) -> f32 {
        match mode {
            0 => 0.,
            1 => 0.5,
            2 => 1.,
            _ => unreachable!(),
        }
    }

    pub fn offset(&self, total: (f32, f32), content: (f32, f32)) -> (f32, f32) {
        (Self::value(self.0 & 3) * (total.0 - content.0), Self::value((self.0 >> 2) & 3) * (total.1 - content.1))
    }

    pub fn from_point(&self, point: (f32, f32), content: (f32, f32)) -> (f32, f32) {
        (point.0 - content.0 * Self::value(self.0 & 3), point.1 - content.1 * Self::value((self.0 >> 2) & 3))
    }
}

impl From<u8> for Gravity {
    fn from(val: u8) -> Self {
        Self(val)
    }
}

struct ShadedConstructor<T: Shading>(Matrix, pub T, f32);
impl<T: Shading> FillVertexConstructor<Vertex> for ShadedConstructor<T> {
    fn new_vertex(&mut self, vertex: FillVertex) -> Vertex {
        let pos = vertex.position();
        self.1.new_vertex(&self.0, &Point::new(pos.x, pos.y), self.2)
    }
}
impl<T: Shading> StrokeVertexConstructor<Vertex> for ShadedConstructor<T> {
    fn new_vertex(&mut self, vertex: StrokeVertex) -> Vertex {
        let pos = vertex.position();
        self.1.new_vertex(&self.0, &Point::new(pos.x, pos.y), self.2)
    }
}

pub struct VertexBuilder<T: Shading> {
    matrix: Matrix,
    vertices: Vec<Vertex>,
    indices: Vec<u16>,
    shading: T,
    alpha: f32,
}

impl<T: Shading> VertexBuilder<T> {
    fn new(matrix: Matrix, shading: T, alpha: f32) -> Self {
        Self {
            matrix,
            vertices: Vec::new(),
            indices: Vec::new(),
            shading,
            alpha,
        }
    }

    pub fn add(&mut self, x: f32, y: f32) {
        self.vertices.push(self.shading.new_vertex(&self.matrix, &Point::new(x, y), self.alpha));
    }

    pub fn triangle(&mut self, x: u16, y: u16, z: u16) {
        self.indices.push(x);
        self.indices.push(y);
        self.indices.push(z);
    }

    pub fn commit(&self) {
        let gl = unsafe { get_internal_gl() }.quad_gl;
        gl.texture(self.shading.texture());
        gl.draw_mode(DrawMode::Triangles);
        gl.geometry(&self.vertices, &self.indices);
    }
}

#[derive(Default)]
pub struct LongTouchState {
    start: Option<(Vec2, f32)>,
}
impl LongTouchState {
    pub fn reset(&mut self) {
        self.start = None;
    }
}

#[derive(Clone, Copy)]
pub struct RectButton {
    pts: Option<[Vec2; 4]>,
    global_rect: Option<Rect>,
    id: Option<u64>,
}

impl Default for RectButton {
    fn default() -> Self {
        Self::new()
    }
}

impl RectButton {
    pub fn new() -> Self {
        Self { pts: None, global_rect: None, id: None }
    }

    pub fn touching(&self) -> bool {
        self.id.is_some()
    }

    pub fn contains(&self, pos: Vec2) -> bool {
        if let Some([a, b, c, d]) = self.pts {
            let abp = (b - a).perp_dot(pos - a);
            let bcp = (c - b).perp_dot(pos - b);
            let cdp = (d - c).perp_dot(pos - c);
            let dap = (a - d).perp_dot(pos - d);
            (abp >= 0. && bcp >= 0. && cdp >= 0. && dap >= 0.) || (abp <= 0. && bcp <= 0. && cdp <= 0. && dap <= 0.)
        } else {
            false
        }
    }

    pub fn set(&mut self, ui: &mut Ui, rect: Rect) {
        // Register focus target in global UI coordinates (-1..1)
        let tl = ui.to_global((rect.x, rect.y));
        let br = ui.to_global((rect.right(), rect.bottom()));
        let global_rect = Rect::new(tl.0.min(br.0), tl.1.min(br.1), (br.0 - tl.0).abs(), (br.1 - tl.1).abs());
        self.global_rect = Some(global_rect);
        register_focus_target(global_rect, FocusType::Button, format!("rb_{:p}", self as *mut _));
        let mat = nalgebra_to_glm(&ui.transform) * ui.gl_transform;
        let tr = |x: f32, y: f32| {
            let pos = mat * vec4(x, y, 0., 1.);
            pos.xy() / pos.w
        };
        self.pts = Some([
            tr(rect.x, rect.y),
            tr(rect.right(), rect.y),
            tr(rect.right(), rect.bottom()),
            tr(rect.x, rect.bottom()),
        ]);
    }

    pub fn touch(&mut self, touch: &Touch) -> bool {
        let inside = self.contains(touch.position);
        match touch.phase {
            TouchPhase::Started => {
                if inside {
                    self.id = Some(touch.id);
                }
            }
            TouchPhase::Moved | TouchPhase::Stationary => {
                if self.id == Some(touch.id) && !inside {
                    self.id = None;
                }
            }
            TouchPhase::Cancelled => {
                self.id = None;
            }
            TouchPhase::Ended => {
                if self.id.take() == Some(touch.id) && inside {
                    if let Some(r) = self.global_rect {
                        set_last_clicked_rect(r);
                    }
                    return true;
                }
            }
        }
        false
    }

    pub fn long_touch(&mut self, touch: &Touch, t: f32, state: &mut LongTouchState) -> bool {
        match touch.phase {
            TouchPhase::Started => {
                if self.id == Some(touch.id) {
                    state.start = Some((touch.position, t));
                }
            }
            TouchPhase::Moved | TouchPhase::Stationary => {
                if self.id == Some(touch.id) {
                    if let Some((start_pos, start_time)) = state.start {
                        if (touch.position - start_pos).length() > 0.02 {
                            state.reset();
                        } else if t > start_time + 0.5 {
                            state.reset();
                            return true;
                        }
                    }
                }
            }
            TouchPhase::Cancelled => {
                if self.id == Some(touch.id) {
                    state.reset();
                }
            }
            TouchPhase::Ended => {
                if self.id.take() == Some(touch.id) {
                    state.reset();
                }
            }
        }
        false
    }

    pub fn update_long_touch(&self, t: f32, state: &mut LongTouchState) -> bool {
        if self.id.is_some() {
            if let Some((_, start_time)) = state.start {
                if t > start_time + 0.5 {
                    state.reset();
                    return true;
                }
            }
        }
        false
    }
}

#[derive(Clone)]
pub struct DRectButton {
    pub inner: RectButton,
    last_touching: bool,
    start_time: Option<f32>,
    pub config: ShadowConfig,
    delta: f32,
    play_sound: bool,
}
impl Default for DRectButton {
    fn default() -> Self {
        Self::new()
    }
}
impl DRectButton {
    pub const TIME: f32 = 0.2;

    pub fn new() -> Self {
        Self {
            inner: RectButton::new(),
            last_touching: false,
            start_time: None,
            config: ShadowConfig::default(),
            delta: -0.006,
            play_sound: true,
        }
    }

    pub fn build(&mut self, ui: &mut Ui, t: f32, r: Rect, f: impl FnOnce(&mut Ui, Path)) {
        self.inner.set(ui, r);
        // let r = r.feather((1. - self.progress(t)) * self.delta);
        let ct = r.center();
        let ct = Vector::new(ct.x, ct.y);
        if PREFER_REDUCED_MOTION.load(Ordering::Relaxed) {
            f(ui, r.rounded(self.config.radius));
            return;
        }
        ui.with(
            Matrix::new_translation(&-ct)
                .append_scaling(1. - (1. - self.progress(t)) * 0.04)
                .append_translation(&ct),
            |ui| {
                f(ui, r.rounded(self.config.radius));
            },
        );
    }

    pub fn invalidate(&mut self) {
        self.inner.pts = None;
    }

    pub fn render_shadow(&mut self, ui: &mut Ui, r: Rect, t: f32, f: impl FnOnce(&mut Ui, Path)) {
        let p = self.progress(t);
        let config = ShadowConfig {
            elevation: self.config.elevation * p,
            radius: self.config.radius,
            ..self.config
        };
        ui.scope(|ui| {
            ui.dy((1. - p) * 0.004);
            self.build(ui, t, r, |ui, path| {
                rounded_rect_shadow(ui, r, &config);
                f(ui, path);
            });
        });
    }

    pub fn render_text<'a>(&mut self, ui: &mut Ui, r: Rect, t: f32, text: impl Into<Cow<'a, str>>, size: f32, chosen: bool) {
        let oh = r.h;
        self.build(ui, t, r, |ui, path| {
            let ct = r.center();
            ui.fill_path(&path, if chosen { WHITE } else { semi_black(0.4) });
            ui.text(text)
                .pos(ct.x, ct.y)
                .anchor(0.5, 0.5)
                .no_baseline()
                .size(size * (1. - (1. - r.h / oh).powf(1.3)))
                .max_width(r.w)
                .color(if chosen { Color::new(0.3, 0.3, 0.3, 1.) } else { WHITE })
                .draw();
        });
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render_text_left<'a>(&mut self, ui: &mut Ui, r: Rect, t: f32, alpha: f32, text: impl Into<Cow<'a, str>>, size: f32, chosen: bool) {
        let oh = r.h;
        self.build(ui, t, r, |ui, path| {
            ui.fill_path(&path, if chosen { WHITE } else { semi_black(0.4) });
            ui.text(text)
                .pos(r.x + 0.02, r.center().y)
                .anchor(0., 0.5)
                .max_width(r.w - 0.04)
                .no_baseline()
                .size(size * r.h / oh)
                .color(if chosen { Color::new(0.3, 0.3, 0.3, alpha) } else { semi_white(alpha) })
                .draw();
        });
    }

    #[inline]
    pub fn render_input<'a>(&mut self, ui: &mut Ui, r: Rect, t: f32, text: impl Into<Cow<'a, str>>, hint: impl Into<Cow<'a, str>>, size: f32) {
        let text = text.into();
        if text.trim().is_empty() {
            self.render_text_left(ui, r, t, 0.7, hint, size, false);
        } else {
            self.render_text_left(ui, r, t, 1., text, size, false);
        }
    }

    #[inline]
    pub fn no_sound(mut self) -> Self {
        self.play_sound = false;
        self
    }

    #[inline]
    pub fn with_radius(mut self, radius: f32) -> Self {
        self.config.radius = radius;
        self
    }

    #[inline]
    pub fn with_elevation(mut self, elevation: f32) -> Self {
        self.config.elevation = elevation;
        self
    }

    #[inline]
    pub fn with_base(mut self, base: f32) -> Self {
        self.config.base = base;
        self
    }

    #[inline]
    pub fn with_delta(mut self, delta: f32) -> Self {
        self.delta = delta;
        self
    }

    pub fn progress(&mut self, t: f32) -> f32 {
        if self.start_time.as_ref().is_some_and(|it| t > *it + Self::TIME) || PREFER_REDUCED_MOTION.load(Ordering::Relaxed) {
            self.start_time = None;
        }
        let p = if let Some(time) = &self.start_time {
            (t - time) / Self::TIME
        } else {
            1.
        };
        if self.last_touching {
            1. - p
        } else {
            p
        }
    }

    pub fn touch(&mut self, touch: &Touch, t: f32) -> bool {
        let res = self.inner.touch(touch);
        let touching = self.inner.touching();
        if self.last_touching != touching {
            self.last_touching = touching;
            self.start_time = Some(t);
        }
        if res && self.play_sound {
            button_hit();
        }
        res
    }

    pub fn long_touch(&mut self, touch: &Touch, t: f32, state: &mut LongTouchState) -> bool {
        if self.inner.long_touch(touch, t, state) {
            self.last_touching = false;
            self.start_time = Some(t);
            if self.play_sound {
                button_hit();
            }
            true
        } else {
            false
        }
    }

    pub fn update_long_touch(&mut self, t: f32, state: &mut LongTouchState) -> bool {
        if self.inner.update_long_touch(t, state) {
            self.last_touching = false;
            self.start_time = Some(t);
            if self.play_sound {
                button_hit();
            }
            true
        } else {
            false
        }
    }
}

pub struct Slider {
    range: Range<f32>,
    step: f32,

    btn_dec: DRectButton,
    btn_inc: DRectButton,

    touch: Option<(u64, f32, bool)>,
    rect: Rect,
    pos: f32,
}

impl Slider {
    const RADIUS: f32 = 0.028;
    const THRESHOLD: f32 = 0.05;

    pub fn new(range: Range<f32>, step: f32) -> Self {
        Self {
            range,
            step,

            btn_dec: DRectButton::new().with_delta(-0.002),
            btn_inc: DRectButton::new().with_delta(-0.002),

            touch: None,
            rect: Rect::default(),
            pos: f32::INFINITY,
        }
    }

    pub fn touch(&mut self, touch: &Touch, t: f32, dst: &mut f32) -> Option<bool> {
        if self.btn_dec.touch(touch, t) {
            *dst = (*dst - self.step).max(self.range.start);
            return Some(true);
        }
        if self.btn_inc.touch(touch, t) {
            *dst = (*dst + self.step).min(self.range.end);
            return Some(true);
        }
        if let Some((id, start_pos, unlocked)) = &mut self.touch {
            if touch.id == *id {
                match touch.phase {
                    TouchPhase::Started | TouchPhase::Moved | TouchPhase::Stationary => {
                        if (touch.position.x - *start_pos).abs() >= Self::THRESHOLD {
                            *unlocked = true;
                        }
                        if *unlocked {
                            let p = (touch.position.x - self.rect.x) / self.rect.w;
                            let p = p.clamp(0., 1.);
                            let p = self.range.start + (self.range.end - self.range.start) * p;
                            *dst = (p / self.step).round() * self.step;
                            return Some(true);
                        }
                    }
                    TouchPhase::Cancelled | TouchPhase::Ended => {
                        self.touch = None;
                    }
                }
                return Some(false);
            }
        } else if touch.phase == TouchPhase::Started {
            let pos = (self.pos, self.rect.center().y);
            if (touch.position.x - pos.0).hypot(touch.position.y - pos.1) <= Self::RADIUS {
                self.touch = Some((touch.id, touch.position.x, false));
                return Some(false);
            }
        }
        None
    }

    pub fn render(&mut self, ui: &mut Ui, mut r: Rect, t: f32, p: f32, text: String) {
        r.x -= 0.1;
        r.x -= r.w * 0.2;
        r.w *= 1.2;
        let pad = 0.04;
        let size = 0.026;
        let cy = r.center().y;
        self.btn_dec
            .render_text(ui, Rect::new(r.x - pad - size, cy, 0., 0.).feather(size), t, "-", 0.7, true);
        self.btn_inc
            .render_text(ui, Rect::new(r.right() + pad + size, cy, 0., 0.).feather(size), t, "+", 0.7, true);
        self.rect = ui.rect_to_global(r);
        ui.text(text)
            .pos(r.x - (pad + size) * 2., cy)
            .anchor(1., 0.5)
            .no_baseline()
            .size(0.6)
            .draw();
        let p = (p - self.range.start) / (self.range.end - self.range.start);
        let pos = (r.x + r.w * p, cy);
        self.pos = ui.to_global(pos).0;
        use lyon::math::point;
        ui.stroke_options = ui.stroke_options.with_line_cap(LineCap::Round);
        ui.stroke_path(
            &{
                let mut p = Path::builder();
                p.begin(point(r.x, cy));
                p.line_to(point(pos.0, cy));
                p.end(false);
                p.build()
            },
            0.02,
            Color { a: 0.8, ..ui.background() },
        );
        ui.stroke_path(
            &{
                let mut p = Path::builder();
                p.begin(point(pos.0, cy));
                p.line_to(point(r.right(), cy));
                p.end(false);
                p.build()
            },
            0.02,
            semi_white(0.8),
        );
        ui.stroke_options = ui.stroke_options.with_line_cap(LineCap::Square);
        rounded_rect_shadow(
            ui,
            Rect::new(pos.0, pos.1, 0., 0.).feather(Self::RADIUS),
            &ShadowConfig {
                radius: Self::RADIUS,
                base: 0.7,
                ..Default::default()
            },
        );
        ui.fill_circle(pos.0, pos.1, Self::RADIUS, WHITE);
    }
}

thread_local! {
    static STATE: RefCell<HashMap<String, Option<u64>>> = RefCell::new(HashMap::new());
}

#[derive(Debug, Clone, Copy)]
pub enum FocusType {
    Button,
    Slider,
    Checkbox,
    Input,
    Back,
    Tab,
}

#[derive(Debug, Clone)]
pub struct FocusTarget {
    pub rect: Rect,
    pub focus_type: FocusType,
    pub id: String,
}

thread_local! {
    pub static FOCUS_TARGETS: RefCell<Vec<FocusTarget>> = const { RefCell::new(Vec::new()) };
}

pub fn clear_focus_targets() {
    FOCUS_TARGETS.with(|it| it.borrow_mut().clear());
}

pub fn get_focus_targets() -> Vec<FocusTarget> {
    FOCUS_TARGETS.with(|it| it.borrow().clone())
}

pub fn register_focus_target(rect: Rect, focus_type: FocusType, id: impl Into<String>) {
    FOCUS_TARGETS.with(|it| {
        it.borrow_mut().push(FocusTarget { rect, focus_type, id: id.into() });
    });
}

pub struct InputParams<'a> {
    pub changed: Option<&'a mut bool>,
    pub mode: InputMode,
    pub length: f32,
}

impl From<()> for InputParams<'_> {
    fn from(_: ()) -> Self {
        Self {
            changed: None,
            mode: InputMode::Text,
            length: 0.3,
        }
    }
}

impl From<InputMode> for InputParams<'_> {
    fn from(mode: InputMode) -> Self {
        Self { mode, ..().into() }
    }
}

impl From<f32> for InputParams<'_> {
    fn from(length: f32) -> Self {
        Self { length, ..().into() }
    }
}

impl<'a> From<(f32, &'a mut bool)> for InputParams<'a> {
    fn from((length, changed): (f32, &'a mut bool)) -> Self {
        Self {
            changed: Some(changed),
            mode: InputMode::Text,
            length,
        }
    }
}

// === 游戏内输入框 ===

struct InlineInputState {
    id: String,
    rect: Option<Rect>, // None=在Ui::input中原位绘制, Some=在Main::render指定位置绘制
    text: String,
    cursor: usize,
    active: bool,
    confirmed: bool,
    show_at: f64, // 延迟显示的时间戳（等软键盘弹出后再显示输入框 UI）
    // 按键重复状态
    backspace_held: f64,  // Backspace 已按住时间
    backspace_next: f64,  // 下一次重复删除的时间点
    arrow_held: Option<KeyCode>, // 当前按住的方向键
    arrow_next: f64,       // 下一次方向键重复的时间点
    v_was_down: bool,      // Ctrl+V 边沿检测：上一帧 V 是否按下
    // 文本选择（字节索引，None 表示无选择）
    selection: Option<(usize, usize)>,
    // 撤销栈（保存修改前的 text + cursor）
    undo_stack: Vec<(String, usize)>,
    redo_stack: Vec<(String, usize)>,
    // 触摸拖拽选择跟踪
    touch_dragging: bool,
    touch_anchor: usize,
    // 待处理的触摸（在 render 中用 Ui 处理）
    pending_touches: Vec<(f32, f32, u8)>, // (x, y, phase)
}

impl Default for InlineInputState {
    fn default() -> Self {
        Self {
            id: String::new(),
            rect: None,
            text: String::new(),
            cursor: 0,
            active: false,
            confirmed: false,
            show_at: 0.,
            backspace_held: 0.,
            backspace_next: 0.,
            arrow_held: None,
            arrow_next: 0.,
            v_was_down: false,
            selection: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            touch_dragging: false,
            touch_anchor: 0,
            pending_touches: Vec::new(),
        }
    }
}

static INLINE_INPUT: std::sync::Mutex<InlineInputState> = std::sync::Mutex::new(InlineInputState {
    id: String::new(),
    rect: None,
    text: String::new(),
    cursor: 0,
    active: false,
    confirmed: false,
    show_at: 0.,
    backspace_held: 0.,
    backspace_next: 0.,
    arrow_held: None,
    arrow_next: 0.,
    v_was_down: false,
    selection: None,
    undo_stack: Vec::new(),
    redo_stack: Vec::new(),
    touch_dragging: false,
    touch_anchor: 0,
    pending_touches: Vec::new(),
});

/// 激活游戏内输入框。rect=None时在Ui::input中原位绘制，rect=Some时在指定位置绘制
pub fn activate_inline_input(id: impl Into<String>, rect: Option<Rect>, default: impl Into<String>) {
    // 先弹出系统软键盘（Android），再创建输入框，避免时序问题
    set_soft_keyboard(true);
    let now = miniquad::date::now();
    let mut state = INLINE_INPUT.lock().unwrap();
    let text = default.into();
    state.id = id.into();
    state.rect = rect;
    state.cursor = text.len(); // 字节索引，支持中文等多字节字符
    state.text = text;
    state.active = true;
    state.confirmed = false;
    // Android 上延迟显示，等软键盘弹出动画完成；其他平台无延迟
    #[cfg(target_os = "android")]
    { state.show_at = now + 0.3; }
    #[cfg(not(target_os = "android"))]
    { state.show_at = now; }
    state.backspace_held = 0.;
    state.backspace_next = 0.;
    state.arrow_held = None;
    state.arrow_next = 0.;
    state.v_was_down = false;
    state.selection = None;
    state.undo_stack.clear();
    state.redo_stack.clear();
    state.touch_dragging = false;
    state.pending_touches.clear();
    drop(state);
    // 清空 get_char_pressed 队列，避免游玩时累积的按键一次性输入
    while get_char_pressed().is_some() {}
}

/// 当前是否有激活的游戏内输入框
pub fn is_inline_input_active() -> bool {
    INLINE_INPUT.lock().unwrap().active
}

/// 获取当前激活输入框的全局rect（仅rect=Some模式），用于触摸命中判断
pub fn inline_input_rect() -> Option<Rect> {
    let state = INLINE_INPUT.lock().unwrap();
    if state.active { state.rect } else { None }
}

/// 显示或隐藏系统软键盘（Android 上有效，其他平台为空操作）
fn set_soft_keyboard(show: bool) {
    unsafe { get_internal_gl() }.quad_context.show_keyboard(show);
}

/// 确认当前输入框（点击空白处时调用）
pub fn confirm_inline_input() {
    let mut state = INLINE_INPUT.lock().unwrap();
    if state.active {
        state.active = false;
        state.confirmed = true;
        drop(state);
        set_soft_keyboard(false);
    }
}

/// 取消当前输入框
pub fn cancel_inline_input() {
    let mut state = INLINE_INPUT.lock().unwrap();
    if state.active {
        state.active = false;
        state.confirmed = false;
        drop(state);
        set_soft_keyboard(false);
    }
}

/// 最后点击的按钮全局 rect（供 request_input 原位显示使用）
static LAST_CLICKED_RECT: std::sync::Mutex<Option<Rect>> = std::sync::Mutex::new(None);

pub fn set_last_clicked_rect(rect: Rect) {
    *LAST_CLICKED_RECT.lock().unwrap() = Some(rect);
}

pub fn take_last_clicked_rect() -> Option<Rect> {
    LAST_CLICKED_RECT.lock().unwrap().take()
}

/// 取出输入结果（id, text），确认后返回
pub fn take_inline_result() -> Option<(String, String)> {
    let mut state = INLINE_INPUT.lock().unwrap();
    if state.confirmed {
        state.confirmed = false;
        Some((state.id.clone(), state.text.clone()))
    } else {
        None
    }
}

// === Android 输入法操作（通过 JNI 调用） ===

/// 全选当前输入框文本
pub fn inline_input_select_all() {
    let mut state = INLINE_INPUT.lock().unwrap();
    if state.active {
        state.selection = Some((0, state.text.len()));
        state.cursor = state.text.len();
    }
}

/// 删除一个字符（退格），供 Android 输入法直接调用
pub fn inline_input_backspace() {
    let mut state = INLINE_INPUT.lock().unwrap();
    if !state.active {
        return;
    }
    push_undo(&mut state);
    if !delete_selection(&mut state) {
        // 没有选区时删除光标前一个字符
        if state.cursor > 0 {
            let cursor = state.cursor;
            let mut start = cursor - 1;
            while start > 0 && !state.text.is_char_boundary(start) {
                start -= 1;
            }
            state.text.drain(start..cursor);
            state.cursor = start;
        }
    }
}

/// 复制选中文本到剪贴板
pub fn inline_input_copy() {
    let state = INLINE_INPUT.lock().unwrap();
    if state.active {
        if let Some(text) = selected_text(&state) {
            if !text.is_empty() {
                unsafe { get_internal_gl() }.quad_context.clipboard_set(&text);
            }
        }
    }
}

/// 剪切选中文本
pub fn inline_input_cut() {
    let mut state = INLINE_INPUT.lock().unwrap();
    if state.active {
        if let Some(text) = selected_text(&state) {
            if !text.is_empty() {
                unsafe { get_internal_gl() }.quad_context.clipboard_set(&text);
                push_undo(&mut state);
                delete_selection(&mut state);
            }
        }
    }
}

/// 粘贴剪贴板内容
pub fn inline_input_paste() {
    let mut state = INLINE_INPUT.lock().unwrap();
    if state.active {
        if let Some(clip) = unsafe { get_internal_gl() }.quad_context.clipboard_get() {
            if !clip.is_empty() {
                push_undo(&mut state);
                delete_selection(&mut state);
                let cursor = state.cursor;
                state.text.insert_str(cursor, &clip);
                state.cursor = cursor + clip.len();
            }
        }
    }
}

// === 输入框辅助函数 ===

/// 保存当前状态到撤销栈
fn push_undo(state: &mut InlineInputState) {
    state.undo_stack.push((state.text.clone(), state.cursor));
    if state.undo_stack.len() > 100 {
        state.undo_stack.remove(0);
    }
    state.redo_stack.clear();
}

/// 获取规范化的选择范围（start <= end），无选择返回 None
fn selection_range(state: &InlineInputState) -> Option<(usize, usize)> {
    state.selection.map(|(a, b)| if a <= b { (a, b) } else { (b, a) })
}

/// 删除选中的文本，返回是否删除了内容
fn delete_selection(state: &mut InlineInputState) -> bool {
    if let Some((start, end)) = selection_range(state) {
        state.text.drain(start..end);
        state.cursor = start;
        state.selection = None;
        true
    } else {
        false
    }
}

/// 获取选中的文本
fn selected_text(state: &InlineInputState) -> Option<String> {
    selection_range(state).map(|(start, end)| state.text[start..end].to_string())
}

/// 将光标移动到指定位置（字节索引，自动对齐到字符边界）
fn move_cursor(state: &mut InlineInputState, pos: usize, extend: bool) {
    let mut pos = pos.min(state.text.len());
    while pos > 0 && !state.text.is_char_boundary(pos) {
        pos -= 1;
    }
    if extend {
        match state.selection {
            Some((anchor, _)) => {
                state.selection = Some((anchor, pos));
            }
            None => {
                state.selection = Some((state.cursor, pos));
            }
        }
    } else {
        state.selection = None;
    }
    state.cursor = pos;
}

/// 应用方向键操作（Left/Right/Up/Down），支持 Ctrl 修饰和 Shift 选择
fn apply_arrow_key(state: &mut InlineInputState, key: KeyCode, ctrl: bool, shift: bool) {
    match key {
        KeyCode::Left => {
            if ctrl {
                // Ctrl+Left: 按词左移
                let mut pos = state.cursor;
                if pos > 0 {
                    pos -= 1;
                    while pos > 0 && !state.text.is_char_boundary(pos) { pos -= 1; }
                    while pos > 0 && state.text[..pos].chars().last().map_or(false, |c| c.is_whitespace()) {
                        pos -= 1;
                        while pos > 0 && !state.text.is_char_boundary(pos) { pos -= 1; }
                    }
                    while pos > 0 && state.text[..pos].chars().last().map_or(false, |c| !c.is_whitespace()) {
                        pos -= 1;
                        while pos > 0 && !state.text.is_char_boundary(pos) { pos -= 1; }
                    }
                }
                move_cursor(state, pos, shift);
            } else {
                // 普通左移
                if state.cursor > 0 {
                    let mut cursor = state.cursor - 1;
                    while cursor > 0 && !state.text.is_char_boundary(cursor) { cursor -= 1; }
                    move_cursor(state, cursor, shift);
                } else if shift {
                    state.selection = Some((0, 0));
                }
            }
        }
        KeyCode::Right => {
            if ctrl {
                // Ctrl+Right: 按词右移
                let mut pos = state.cursor;
                if pos < state.text.len() {
                    while pos < state.text.len() && state.text[pos..].chars().next().map_or(false, |c| !c.is_whitespace()) {
                        pos += 1;
                        while pos < state.text.len() && !state.text.is_char_boundary(pos) { pos += 1; }
                    }
                    while pos < state.text.len() && state.text[pos..].chars().next().map_or(false, |c| c.is_whitespace()) {
                        pos += 1;
                        while pos < state.text.len() && !state.text.is_char_boundary(pos) { pos += 1; }
                    }
                }
                move_cursor(state, pos, shift);
            } else {
                // 普通右移
                if state.cursor < state.text.len() {
                    let mut cursor = state.cursor + 1;
                    while cursor < state.text.len() && !state.text.is_char_boundary(cursor) { cursor += 1; }
                    move_cursor(state, cursor, shift);
                } else if shift {
                    state.selection = Some((state.cursor, state.cursor));
                }
            }
        }
        KeyCode::Up => {
            // 上键：移动到开头
            move_cursor(state, 0, shift);
        }
        KeyCode::Down => {
            // 下键：移动到结尾
            let end = state.text.len();
            move_cursor(state, end, shift);
        }
        _ => {}
    }
}

/// 处理键盘输入，每帧调用一次。Enter 确认，Esc 取消。
/// cursor 是字节索引（不是字符计数），以支持中文等多字节字符。
pub fn update_inline_input() {
    let mut state = INLINE_INPUT.lock().unwrap();
    if !state.active {
        return;
    }

    let ctrl = is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl);
    let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);

    // Ctrl+A: 全选
    if ctrl && is_key_pressed(KeyCode::A) {
        state.selection = Some((0, state.text.len()));
        state.cursor = state.text.len();
    }

    // Ctrl+C: 复制
    if ctrl && is_key_pressed(KeyCode::C) {
        if let Some(text) = selected_text(&state) {
            unsafe { get_internal_gl() }.quad_context.clipboard_set(&text);
        }
    }

    // Ctrl+X: 剪切
    if ctrl && is_key_pressed(KeyCode::X) {
        if let Some(text) = selected_text(&state) {
            unsafe { get_internal_gl() }.quad_context.clipboard_set(&text);
            push_undo(&mut state);
            delete_selection(&mut state);
        }
    }

    // Ctrl+V: 粘贴（用 is_key_down 边沿检测，避免修饰键导致 is_key_pressed 失效）
    if ctrl {
        let v_down = is_key_down(KeyCode::V);
        if v_down && !state.v_was_down {
            if let Some(clip) = unsafe { get_internal_gl() }.quad_context.clipboard_get() {
                if !clip.is_empty() {
                    push_undo(&mut state);
                    delete_selection(&mut state);
                    let cursor = state.cursor;
                    state.text.insert_str(cursor, &clip);
                    state.cursor = cursor + clip.len();
                }
            }
        }
        state.v_was_down = v_down;
    } else {
        state.v_was_down = false;
    }

    // Ctrl+Z: 撤销
    if ctrl && is_key_pressed(KeyCode::Z) && !shift {
        if let Some((text, cursor)) = state.undo_stack.pop() {
            let old_text = std::mem::take(&mut state.text);
            let old_cursor = state.cursor;
            state.redo_stack.push((old_text, old_cursor));
            state.text = text;
            state.cursor = cursor;
            state.selection = None;
        }
    }

    // Ctrl+Y 或 Ctrl+Shift+Z: 重做
    if (ctrl && is_key_pressed(KeyCode::Y)) || (ctrl && shift && is_key_pressed(KeyCode::Z)) {
        if let Some((text, cursor)) = state.redo_stack.pop() {
            let old_text = std::mem::take(&mut state.text);
            let old_cursor = state.cursor;
            state.undo_stack.push((old_text, old_cursor));
            state.text = text;
            state.cursor = cursor;
            state.selection = None;
        }
    }

    // 字符输入（每帧都消费队列，避免累积；Ctrl+组合键不会产生可打印字符）
    let mut chars: Vec<char> = Vec::new();
    while let Some(c) = get_char_pressed() {
        if !c.is_control() {
            chars.push(c);
        }
    }
    if !chars.is_empty() {
        push_undo(&mut state);
        delete_selection(&mut state);
        for c in chars.into_iter().rev() {
            let cursor = state.cursor;
            state.text.insert(cursor, c);
            state.cursor = cursor + c.len_utf8();
        }
    }

    // 退格
    let bs_pressed = is_key_pressed(KeyCode::Backspace);
    let bs_down = is_key_down(KeyCode::Backspace);
    if bs_pressed {
        state.backspace_held = 0.;
        state.backspace_next = 0.;
    }
    if bs_down {
        let dt = get_frame_time() as f64;
        state.backspace_held += dt;
        const INITIAL_DELAY: f64 = 0.4;
        const REPEAT_INTERVAL: f64 = 0.03;
        let should_delete = if state.backspace_held < INITIAL_DELAY {
            bs_pressed
        } else {
            if state.backspace_next == 0. {
                state.backspace_next = state.backspace_held + REPEAT_INTERVAL;
                true
            } else if state.backspace_held >= state.backspace_next {
                state.backspace_next += REPEAT_INTERVAL;
                true
            } else {
                false
            }
        };
        if should_delete {
            if state.selection.is_some() {
                push_undo(&mut state);
                delete_selection(&mut state);
                state.backspace_held = 0.;
                state.backspace_next = 0.;
            } else if state.cursor > 0 {
                if bs_pressed { push_undo(&mut state); }
                let mut cursor = state.cursor;
                while cursor > 0 && !state.text.is_char_boundary(cursor) { cursor -= 1; }
                if cursor > 0 {
                    let mut start = cursor - 1;
                    while start > 0 && !state.text.is_char_boundary(start) { start -= 1; }
                    state.text.drain(start..cursor);
                    state.cursor = start;
                }
            }
        }
    } else {
        state.backspace_held = 0.;
        state.backspace_next = 0.;
    }

    // Delete 键
    if is_key_pressed(KeyCode::Delete) {
        if state.selection.is_some() {
            push_undo(&mut state);
            delete_selection(&mut state);
        } else if state.cursor < state.text.len() {
            push_undo(&mut state);
            let mut cursor = state.cursor;
            while cursor < state.text.len() && !state.text.is_char_boundary(cursor) { cursor += 1; }
            if cursor < state.text.len() {
                let mut end = cursor + 1;
                while end < state.text.len() && !state.text.is_char_boundary(end) { end += 1; }
                state.text.drain(cursor..end);
            }
        }
    }

    // === 方向键处理（支持长按重复） ===
    let now = get_time();
    // 检测当前按下的方向键（优先首次按下的）
    let pressed_arrow = if is_key_pressed(KeyCode::Left) { Some(KeyCode::Left) }
        else if is_key_pressed(KeyCode::Right) { Some(KeyCode::Right) }
        else if is_key_pressed(KeyCode::Up) { Some(KeyCode::Up) }
        else if is_key_pressed(KeyCode::Down) { Some(KeyCode::Down) }
        else { None };

    if let Some(key) = pressed_arrow {
        state.arrow_held = Some(key);
        state.arrow_next = now + 0.4; // 400ms 初始延迟
        apply_arrow_key(&mut state, key, ctrl, shift);
    } else if let Some(key) = state.arrow_held {
        // 检查是否还在按住
        if !is_key_down(key) {
            state.arrow_held = None;
        } else if now >= state.arrow_next {
            apply_arrow_key(&mut state, key, ctrl, shift);
            state.arrow_next = now + 0.03; // 30ms 重复间隔
        }
    }

    // Enter 确认
    if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter) {
        state.active = false;
        state.confirmed = true;
        drop(state);
        set_soft_keyboard(false);
        return;
    }

    // Esc 取消
    if is_key_pressed(KeyCode::Escape) {
        state.active = false;
        state.confirmed = false;
        drop(state);
        set_soft_keyboard(false);
        return;
    }
}

/// 在指定 rect 处绘制输入框编辑状态（通用，调用方负责坐标上下文）
/// draw_text: 是否渲染文字（原位模式由按钮显示文字，传 false；指定位置模式传 true）
fn draw_input_editor(ui: &mut Ui, rect: Rect, text: &str, cursor: usize, selection: Option<(usize, usize)>, time: f64, draw_text: bool) {
    // 强制 alpha=1，避免场景半透明容器导致文字透明
    let old_alpha = ui.alpha;
    ui.alpha = 1.;

    // 透明背景
    ui.fill_rect(rect, Color::new(0., 0., 0., 0.25));
    // 边框
    let bw = 0.003;
    ui.fill_rect(Rect::new(rect.x, rect.y, rect.w, bw), Color::new(1., 1., 1., 0.6));
    ui.fill_rect(Rect::new(rect.x, rect.y + rect.h - bw, rect.w, bw), Color::new(1., 1., 1., 0.6));
    ui.fill_rect(Rect::new(rect.x, rect.y, bw, rect.h), Color::new(1., 1., 1., 0.6));
    ui.fill_rect(Rect::new(rect.x + rect.w - bw, rect.y, bw, rect.h), Color::new(1., 1., 1., 0.6));

    let font_size = (rect.h * 6.0).clamp(0.3, 0.6);
    let text_y = rect.y + rect.h / 2.;
    let padding = 0.01;
    let visible_width = rect.w - padding * 2.;

    // 测量文本总宽度和光标前文本宽度
    let text_width = ui.text(text).size(font_size).no_baseline().measure().w;
    let before_cursor = &text[..cursor.min(text.len())];
    let cursor_x_abs = ui.text(before_cursor).size(font_size).no_baseline().measure().w;

    // 计算滚动偏移量，确保光标始终在可见范围内
    let mut scroll_x = 0f32;
    if text_width > visible_width {
        let max_scroll = text_width - visible_width;
        if cursor_x_abs > scroll_x + visible_width - padding {
            scroll_x = cursor_x_abs - visible_width + padding;
        }
        if cursor_x_abs < scroll_x + padding {
            scroll_x = (cursor_x_abs - padding).max(0.);
        }
        scroll_x = scroll_x.clamp(0., max_scroll);
    }

    // scissor 裁剪，超出输入框范围的文字不显示
    ui.scissor(rect, |ui| {
        // 选中高亮
        if let Some((sel_start, sel_end)) = selection {
            let (s, e) = if sel_start <= sel_end { (sel_start, sel_end) } else { (sel_end, sel_start) };
            let sel_before = &text[..s.min(text.len())];
            let sel_x = ui.text(sel_before).size(font_size).no_baseline().measure().w;
            let sel_text = &text[s.min(text.len())..e.min(text.len())];
            let sel_w = ui.text(sel_text).size(font_size).no_baseline().measure().w;
            ui.fill_rect(
                Rect::new(rect.x + padding + sel_x - scroll_x, rect.y + rect.h * 0.15, sel_w, rect.h * 0.7),
                Color::new(0.2, 0.4, 0.8, 0.6),
            );
        }

        if draw_text {
            ui.text(text)
                .pos(rect.x + padding - scroll_x, text_y)
                .anchor(0., 0.5)
                .size(font_size)
                .color(Color::new(1., 1., 1., 1.))
                .no_baseline()
                .draw();
        }

        // 闪烁光标（500ms 周期）
        let cursor_visible = (time % 1.0) < 0.5;
        if cursor_visible {
            let cursor_x = rect.x + padding + cursor_x_abs - scroll_x;
            ui.fill_rect(
                Rect::new(cursor_x, rect.y + rect.h * 0.2, 0.003, rect.h * 0.6),
                Color::new(1., 1., 1., 1.),
            );
        }
    });

    ui.alpha = old_alpha;
}

/// 根据触摸位置计算文本中的字节索引（用于点击定位光标和拖拽选择）
fn position_to_offset(ui: &mut Ui, text: &str, rect: Rect, touch_x: f32, font_size: f32) -> usize {
    let padding = 0.01;
    let relative_x = touch_x - rect.x - padding;
    if relative_x <= 0. {
        return 0;
    }
    // 逐字符累加宽度，找到最接近的位置
    let mut offset = 0;
    let mut acc_width = 0f32;
    for (i, c) in text.char_indices() {
        let char_text = c.to_string();
        let char_width = ui.text(&char_text).size(font_size).no_baseline().measure().w;
        if relative_x < acc_width + char_width / 2. {
            return offset;
        }
        acc_width += char_width;
        offset = i + c.len_utf8();
    }
    text.len()
}

/// 处理输入框内的触摸事件（存储触摸信息，在 render 中用 Ui 处理光标定位和选择）
/// touch_phase: 0=开始, 1=移动, 2=结束
pub fn handle_inline_input_touch(touch_x: f32, touch_y: f32, touch_phase: u8) {
    let mut state = INLINE_INPUT.lock().unwrap();
    if !state.active {
        return;
    }
    state.pending_touches.push((touch_x, touch_y, touch_phase));
}

/// 渲染游戏内输入框（仅 rect=Some 模式，在 Main::render 中调用）
pub fn render_inline_input(ui: &mut Ui, time: f64) {
    let mut state = INLINE_INPUT.lock().unwrap();
    if !state.active {
        return;
    }
    // 延迟显示：等软键盘弹出后再渲染输入框 UI
    let now = miniquad::date::now();
    if now < state.show_at {
        return;
    }
    let Some(rect) = state.rect else { return };

    // 处理待处理的触摸（光标定位和拖拽选择）
    let pending: Vec<(f32, f32, u8)> = std::mem::take(&mut state.pending_touches);
    for (touch_x, touch_y, phase) in pending {
        // 判断是否在输入框区域内
        let in_rect = touch_x >= rect.x && touch_x <= rect.x + rect.w
            && touch_y >= rect.y && touch_y <= rect.y + rect.h;
        if !in_rect {
            // 区域外：确认输入（只在触摸开始时确认，避免移动时误触发）
            if phase == 0 {
                state.active = false;
                state.confirmed = true;
                drop(state);
                set_soft_keyboard(false);
                return;
            }
            continue;
        }
        // 区域内：处理光标定位和拖拽选择
        let font_size = (rect.h * 6.0).clamp(0.3, 0.6);
        let text = state.text.clone();
        let offset = position_to_offset(ui, &text, rect, touch_x, font_size);
        match phase {
            0 => {
                state.cursor = offset;
                state.touch_anchor = offset;
                state.touch_dragging = true;
                state.selection = None;
            }
            1 => {
                if state.touch_dragging {
                    state.cursor = offset;
                    if offset != state.touch_anchor {
                        state.selection = Some((state.touch_anchor, offset));
                    } else {
                        state.selection = None;
                    }
                }
            }
            2 => {
                state.touch_dragging = false;
            }
            _ => {}
        }
    }

    let text = state.text.clone();
    let cursor = state.cursor;
    let selection = state.selection;
    drop(state);
    draw_input_editor(ui, rect, &text, cursor, selection, time, true);
}

/// Ui::input 原位绘制：如果当前激活的输入框 id 匹配，在指定 rect 处绘制背景、边框、光标（文字由按钮显示）
pub fn render_inline_input_inline(ui: &mut Ui, id: &str, rect: Rect) -> bool {
    let state = INLINE_INPUT.lock().unwrap();
    if !state.active || state.rect.is_some() || state.id != id {
        return false;
    }
    // 延迟显示：等软键盘弹出后再渲染输入框 UI
    let now = miniquad::date::now();
    if now < state.show_at {
        return false;
    }
    let text = state.text.clone();
    let cursor = state.cursor;
    let selection = state.selection;
    drop(state);
    draw_input_editor(ui, rect, &text, cursor, selection, get_time(), false);
    true
}

pub struct Ui<'a> {
    pub top: f32,
    pub viewport: (i32, i32, i32, i32),

    pub text_painter: &'a mut TextPainter,

    pub transform: Matrix,
    pub gl_transform: Mat4,
    scissor: Option<(i32, i32, i32, i32)>,
    touches: Option<Vec<Touch>>,

    vertex_buffers: VertexBuffers<Vertex, u16>,
    fill_tess: FillTessellator,
    fill_options: FillOptions,
    stroke_tess: StrokeTessellator,
    pub stroke_options: StrokeOptions,

    pub alpha: f32,
}

impl<'a> Ui<'a> {
    pub fn new(text_painter: &'a mut TextPainter, viewport: Option<(i32, i32, i32, i32)>) -> Self {
        unsafe { get_internal_gl() }.quad_context.begin_default_pass(PassAction::Clear {
            depth: None,
            stencil: Some(0),
            color: None,
        });
        let viewport = viewport.unwrap_or_else(|| (0, 0, screen_width() as i32, screen_height() as i32));
        Self {
            top: viewport.3 as f32 / viewport.2 as f32,
            viewport,

            text_painter,

            transform: Matrix::identity(),
            gl_transform: Mat4::IDENTITY,
            scissor: None,
            touches: None,

            vertex_buffers: VertexBuffers::new(),
            fill_tess: FillTessellator::new(),
            fill_options: FillOptions::default(),
            stroke_tess: StrokeTessellator::new(),
            stroke_options: StrokeOptions::default(),

            alpha: 1.,
        }
    }

    pub fn camera(&self) -> Camera2D {
        Camera2D {
            zoom: vec2(1., -self.viewport.2 as f32 / self.viewport.3 as f32),
            viewport: Some(self.viewport),
            ..Default::default()
        }
    }

    pub fn ensure_touches(&mut self) -> &mut Vec<Touch> {
        if self.touches.is_none() {
            self.touches = Some(Judge::get_touches());
        }
        self.touches.as_mut().unwrap()
    }

    pub(crate) fn set_touches(&mut self, touches: Vec<Touch>) {
        self.touches = Some(touches);
    }

    pub fn builder<T: IntoShading>(&self, shading: T) -> VertexBuilder<T::Target> {
        VertexBuilder::new(self.transform, shading.into_shading(), self.alpha)
    }

    pub fn fill_rect(&mut self, rect: Rect, shading: impl IntoShading) {
        let mut b = self.builder(shading);
        b.add(rect.x, rect.y);
        b.add(rect.x + rect.w, rect.y);
        b.add(rect.x, rect.y + rect.h);
        b.add(rect.x + rect.w, rect.y + rect.h);
        b.triangle(0, 1, 2);
        b.triangle(1, 2, 3);
        b.commit();
    }

    fn set_tolerance(&mut self) {
        let tol = 0.15 / (self.transform.transform_vector(&Vector::new(1., 0.)).norm() * screen_width() / 2.);
        self.fill_options.tolerance = tol;
        self.stroke_options.tolerance = tol;
    }

    fn draw_lyon<T: Shading>(&mut self, shading: T, f: impl FnOnce(&mut Self, ShadedConstructor<T>)) {
        self.set_tolerance();
        let shaded = ShadedConstructor(self.transform, shading.into_shading(), self.alpha);
        let tex = shaded.1.texture();
        f(self, shaded);
        self.emit_lyon(tex);
    }

    pub fn fill_path(&mut self, path: impl IntoIterator<Item = PathEvent>, shading: impl IntoShading) {
        self.draw_lyon(shading.into_shading(), |this, shaded| {
            this.fill_tess
                .tessellate(path, &this.fill_options, &mut BuffersBuilder::new(&mut this.vertex_buffers, shaded))
                .unwrap();
        });
    }

    pub fn fill_circle(&mut self, x: f32, y: f32, radius: f32, shading: impl IntoShading) {
        self.draw_lyon(shading.into_shading(), |this, shaded| {
            this.fill_tess
                .tessellate_circle(lm::point(x, y), radius, &this.fill_options, &mut BuffersBuilder::new(&mut this.vertex_buffers, shaded))
                .unwrap();
        });
    }

    pub fn stroke_circle(&mut self, x: f32, y: f32, radius: f32, width: f32, shading: impl IntoShading) {
        self.draw_lyon(shading.into_shading(), |this, shaded| {
            this.stroke_options.line_width = width;
            this.stroke_tess
                .tessellate_circle(lm::point(x, y), radius, &this.stroke_options, &mut BuffersBuilder::new(&mut this.vertex_buffers, shaded))
                .unwrap();
        });
    }

    pub fn stroke_path(&mut self, path: &Path, width: f32, shading: impl IntoShading) {
        self.draw_lyon(shading.into_shading(), |this, shaded| {
            this.stroke_options.line_width = width;
            this.stroke_tess
                .tessellate_path(path, &this.stroke_options, &mut BuffersBuilder::new(&mut this.vertex_buffers, shaded))
                .unwrap();
        });
    }

    fn emit_lyon(&mut self, texture: Option<Texture2D>) {
        let gl = unsafe { get_internal_gl() }.quad_gl;
        gl.texture(texture);
        gl.draw_mode(DrawMode::Triangles);
        gl.geometry(&std::mem::take(&mut self.vertex_buffers.vertices), &std::mem::take(&mut self.vertex_buffers.indices));
    }

    pub fn screen_rect(&self) -> Rect {
        Rect::new(-1., -self.top, 2., self.top * 2.)
    }

    pub fn dialog_rect() -> Rect {
        let hw = 0.45;
        let hh = 0.34;
        Rect::new(-hw, -hh, hw * 2., hh * 2.)
    }

    pub fn rect_to_global(&self, rect: Rect) -> Rect {
        let pt = self.to_global((rect.x, rect.y));
        let vec = self.vec_to_global((rect.w, rect.h));
        Rect::new(pt.0, pt.1, vec.0, vec.1)
    }

    pub fn vec_to_global(&self, vec: (f32, f32)) -> (f32, f32) {
        let r = self.transform.transform_vector(&Vector::new(vec.0, vec.1));
        (r.x, r.y)
    }

    pub fn to_global(&self, pt: (f32, f32)) -> (f32, f32) {
        let r = self.transform.transform_point(&Point::new(pt.0, pt.1));
        (r.x, r.y)
    }

    pub fn to_local(&self, pt: (f32, f32)) -> (f32, f32) {
        let r = self.transform.try_inverse().unwrap().transform_point(&Point::new(pt.0, pt.1));
        (r.x, r.y)
    }

    pub fn dx(&mut self, x: f32) {
        self.transform.append_translation_mut(&Vector::new(x, 0.));
    }

    pub fn dy(&mut self, y: f32) {
        self.transform.append_translation_mut(&Vector::new(0., y));
    }

    #[inline]
    pub fn alpha<R>(&mut self, alpha: f32, f: impl FnOnce(&mut Self) -> R) -> R {
        let old = self.alpha;
        self.alpha = old * alpha;
        let res = f(self);
        self.alpha = old;
        res
    }

    #[inline]
    pub fn with<R>(&mut self, transform: Matrix, f: impl FnOnce(&mut Self) -> R) -> R {
        let old = self.transform;
        self.transform = old * transform;
        let res = f(self);
        self.transform = old;
        res
    }

    #[inline]
    pub fn scope<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let old = self.transform;
        let res = f(self);
        self.transform = old;
        res
    }

    #[inline]
    pub fn abs_scope<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let old = self.transform;
        self.transform = Matrix::identity();
        let res = f(self);
        self.transform = old;
        res
    }

    #[inline]
    pub fn with_gl<R>(&mut self, transform: Mat4, f: impl FnOnce(&mut Self) -> R) -> R {
        let old = self.gl_transform;
        // self.gl_transform = old * transform;
        let gl = unsafe { get_internal_gl() }.quad_gl;
        gl.push_model_matrix(transform);
        let res = f(self);
        self.gl_transform = old;
        unsafe { get_internal_gl() }.flush();
        gl.pop_model_matrix();
        res
    }

    #[inline]
    pub fn apply<R>(&mut self, f: impl FnOnce(&mut Ui) -> R) -> R {
        unsafe { get_internal_gl() }.quad_gl.push_model_matrix(nalgebra_to_glm(&self.transform));
        let res = f(self);
        unsafe { get_internal_gl() }.quad_gl.pop_model_matrix();
        res
    }

    pub fn scissor<R>(&mut self, rect: Rect, f: impl FnOnce(&mut Ui) -> R) -> R {
        let igl = unsafe { get_internal_gl() };
        let gl = igl.quad_gl;
        let rect = self.rect_to_global(rect);
        let vp = get_viewport();
        let pt = (
            vp.0 as f32 + (rect.x + 1.) / 2. * vp.2 as f32,
            (screen_height() - (vp.1 + vp.3) as f32) + (rect.y * vp.2 as f32 / vp.3 as f32 + 1.) / 2. * vp.3 as f32,
        );

        let old = self.scissor;
        self.scissor = {
            let mut l = pt.0 as i32;
            let mut t = pt.1 as i32;
            let mut r = (pt.0 + rect.w * vp.2 as f32 / 2.) as i32;
            let mut b = (pt.1 + rect.h * vp.2 as f32 / 2.) as i32;
            if let Some((l0, t0, w0, h0)) = old {
                l = l.max(l0);
                t = t.max(t0);
                r = r.min(l0 + w0);
                b = b.min(t0 + h0);
            }
            Some((l, t, r - l, b - t))
        };

        gl.scissor(self.scissor);
        let res = f(self);
        self.scissor = old;
        gl.scissor(old);
        res
    }

    pub fn text<'s, 'ui>(&'ui mut self, text: impl Into<Cow<'s, str>>) -> DrawText<'a, 's, 'ui> {
        DrawText::new(self, text.into())
    }

    fn clicked(&mut self, rect: Rect, entry: &mut Option<u64>) -> bool {
        let rect = self.rect_to_global(rect);
        let mut exists = false;
        let mut any = false;
        let old_entry = *entry;
        let mut res = false;
        self.ensure_touches().retain(|touch| {
            exists = exists || old_entry == Some(touch.id);
            if !rect.contains(touch.position) {
                return true;
            }
            any = true;
            match touch.phase {
                TouchPhase::Started => {
                    *entry = Some(touch.id);
                    false
                }
                TouchPhase::Moved | TouchPhase::Stationary => {
                    if *entry != Some(touch.id) {
                        *entry = None;
                        true
                    } else {
                        false
                    }
                }
                TouchPhase::Cancelled => {
                    *entry = None;
                    true
                }
                TouchPhase::Ended => {
                    if entry.take() == Some(touch.id) {
                        res = true;
                        false
                    } else {
                        true
                    }
                }
            }
        });
        if res {
            // 记录最后点击的按钮全局 rect，供 request_input 原位显示使用
            set_last_clicked_rect(rect);
            return true;
        }
        if !any && exists {
            *entry = None;
        }
        false
    }

    pub fn accent(&self) -> Color {
        Color::from_hex_rgb(0x2196f3)
    }

    pub fn background(&self) -> Color {
        Color::from_hex_rgb(0x2a323c)
    }

    pub fn button(&mut self, id: &str, rect: Rect, text: impl Into<String>) -> bool {
        let text = text.into();
        // Register focus target in global UI coordinates
        let tl = self.to_global((rect.x, rect.y));
        let br = self.to_global((rect.right(), rect.bottom()));
        let global_rect = Rect::new(tl.0.min(br.0), tl.1.min(br.1), (br.0 - tl.0).abs(), (br.1 - tl.1).abs());
        register_focus_target(global_rect, FocusType::Button, id);
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            let entry = state.entry(id.to_owned()).or_default();
            self.fill_path(
                &rect.rounded(0.01),
                Color {
                    a: if entry.is_some() { 0.5 } else { 1. },
                    ..self.background()
                },
            );
            let ct = rect.center();
            self.text(text)
                .pos(ct.x, ct.y)
                .anchor(0.5, 0.5)
                .max_width(rect.w)
                .size(0.42)
                .color(WHITE)
                .no_baseline()
                .draw();
            self.clicked(rect, entry)
        })
    }

    pub fn checkbox(&mut self, text: impl Into<String>, value: &mut bool) -> Rect {
        let label = text.into();
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            let entry = state.entry(format!("chkbox#{label}")).or_default();
            let w = 0.08;
            let s = 0.025;
            let text = self.text(label.clone()).pos(w, 0.).size(0.47).no_baseline().draw();
            let r = Rect::new(w / 2. - s, text.center().y - s, s * 2., s * 2.);
            self.fill_path(
                &r.rounded(0.01),
                Color {
                    a: if entry.is_some() { 0.5 } else { 1. },
                    ..if *value { WHITE } else { self.background() }
                },
            );
            let r = Rect::new(r.x, r.y, text.right() - r.x, (text.bottom() - r.y).max(w));
            self.clicked(r, entry);
            let tl = self.to_global((r.x, r.y));
            let br = self.to_global((r.right(), r.bottom()));
            let gr = Rect::new(tl.0.min(br.0), tl.1.min(br.1), (br.0 - tl.0).abs(), (br.1 - tl.1).abs());
            register_focus_target(gr, FocusType::Checkbox, label);
            r
        })
    }

    pub fn input<'b>(&mut self, label: impl Into<String>, value: &mut String, params: impl Into<InputParams<'b>>) -> Rect {
        let label = label.into();
        let params = params.into();
        let id = format!("input#{label}");
        let r = self.text(label.as_str()).anchor(1., 0.).size(0.47).draw();
        let lf = r.x;
        let r = Rect::new(0.02, r.y - 0.01, params.length, r.h + 0.02);
        // 如果当前激活的是本输入框（原位模式），按钮显示编辑中的文字
        let editing_text = {
            let state = INLINE_INPUT.lock().unwrap();
            if state.active && state.rect.is_none() && state.id == id {
                Some(state.text.clone())
            } else {
                None
            }
        };
        let display_text = if let Some(et) = &editing_text {
            if params.mode == InputMode::Password {
                "*".repeat(et.chars().count())
            } else {
                et.lines().next().unwrap_or_default().to_owned()
            }
        } else if params.mode == InputMode::Password {
            "*".repeat(value.chars().count())
        } else {
            value.lines().next().unwrap_or_default().to_owned()
        };
        if self.button(&id, r, display_text) {
            // 点击输入框：激活游戏内输入框，原位模式（rect=None）
            activate_inline_input(&id, None, value.as_str());
        }
        // 如果当前激活的是本输入框（原位模式），在原位置覆盖绘制背景、边框、光标（文字由按钮显示）
        let _ = render_inline_input_inline(self, &id, r);
        // 检查游戏内输入框的结果
        if let Some((its_id, text)) = take_inline_result() {
            if its_id == id {
                if let Some(changed) = params.changed {
                    *changed = true;
                }
                *value = text;
            } else {
                return_input(its_id, text);
            }
        }
        Rect::new(lf, r.y, r.right() - lf, r.h)
    }

    pub fn slider(&mut self, text: impl Into<String>, range: Range<f32>, step: f32, value: &mut f32, length: Option<f32>) -> Rect {
        let text = text.into();
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            let entry = state.entry(text.clone()).or_default();

            let len = length.unwrap_or(0.3);
            let s = 0.002;
            let tr = self.text(format!("{text}: {value:.3}")).size(0.4).draw();
            let cy = tr.h + 0.03;
            let r = Rect::new(0., cy - s, len, s * 2.);
            self.fill_rect(r, WHITE);
            let p = (*value - range.start) / (range.end - range.start);
            let p = p.clamp(0., 1.);
            self.fill_circle(len * p, cy, 0.015, self.accent());
            let r = r.feather(0.015 - s);
            let r = self.rect_to_global(r);
            self.ensure_touches();
            if let Some(id) = entry {
                if let Some(touch) = self.touches.as_ref().unwrap().iter().rfind(|it| it.id == *id) {
                    let Vec2 { x, y } = touch.position;
                    let (x, _) = self.to_local((x, y));
                    let p = (x / len).clamp(0., 1.);
                    *value = range.start + (range.end - range.start) * p;
                    *value = (*value / step).round() * step;
                    if matches!(touch.phase, TouchPhase::Cancelled | TouchPhase::Ended) {
                        *entry = None;
                    }
                }
            } else if let Some(touch) = self.touches.as_ref().unwrap().iter().find(|it| r.contains(it.position)) {
                if touch.phase == TouchPhase::Started {
                    *entry = Some(touch.id);
                }
            }

            let s = 0.025;
            let mut x = len + 0.02;
            let r = Rect::new(x, cy - s, s * 2., s * 2.);
            self.fill_path(&r.rounded(0.008), self.background());
            self.text("-")
                .pos(r.center().x, r.center().y)
                .anchor(0.5, 0.5)
                .size(0.4)
                .color(WHITE)
                .no_baseline()
                .draw();
            if self.clicked(r, state.entry(format!("{text}:-")).or_default()) {
                *value = (*value - step).max(range.start);
            }
            x += s * 2. + 0.01;
            let r = Rect::new(x, cy - s, s * 2., s * 2.);
            self.fill_path(&r.rounded(0.008), self.background());
            self.text("+")
                .pos(r.center().x, r.center().y)
                .anchor(0.5, 0.5)
                .size(0.4)
                .color(WHITE)
                .no_baseline()
                .draw();
            if self.clicked(r, state.entry(format!("{text}:+")).or_default()) {
                *value = (*value + step).min(range.end);
            }

            let sr = Rect::new(0., cy - s, x + s * 2., cy + s);
            let tl = self.to_global((sr.x, sr.y));
            let br = self.to_global((sr.right(), sr.bottom()));
            let gr = Rect::new(tl.0.min(br.0), tl.1.min(br.1), (br.0 - tl.0).abs(), (br.1 - tl.1).abs());
            register_focus_target(gr, FocusType::Slider, text);

            Rect::new(0., 0., x + s * 2., cy + s)
        })
    }

    pub fn hgrids(&mut self, width: f32, height: f32, row_num: u32, count: u32, mut content: impl FnMut(&mut Self, u32)) -> (f32, f32) {
        let mut sh = 0.;
        let w = width / row_num as f32;
        for i in (0..count).step_by(row_num as usize) {
            let mut sw = 0.;
            for j in 0..(count - i).min(row_num) {
                content(self, i + j);
                self.dx(w);
                sw += w;
            }
            self.dx(-sw);
            self.dy(height);
            sh += height;
        }
        self.dy(-sh);
        (width, sh)
    }

    pub fn avatar(&mut self, cx: f32, cy: f32, r: f32, t: f32, avatar: Result<Option<SafeTexture>, SafeTexture>) -> Rect {
        rounded_rect_shadow(
            self,
            Rect::new(cx - r, cy - r, r * 2., r * 2.),
            &ShadowConfig {
                radius: r,
                ..Default::default()
            },
        );
        let rect = Rect::new(cx - r, cy - r, r * 2., r * 2.);
        match avatar {
            Ok(Some(avatar)) => {
                self.fill_circle(cx, cy, r, (*avatar, rect));
            }
            Ok(None) => {
                self.loading(
                    cx,
                    cy,
                    t,
                    WHITE,
                    LoadingParams {
                        radius: r * 0.6,
                        width: 0.008,
                        ..Default::default()
                    },
                );
            }
            Err(icon) => {
                self.fill_circle(cx, cy, r, semi_black(0.2));
                self.fill_circle(cx, cy, r, (*icon, rect.feather(-0.025), ScaleType::CropCenter, WHITE));
            }
        }
        self.stroke_circle(cx, cy, r, 0.004, WHITE);
        rect
    }

    pub fn loading_path(start: f32, len: f32, r: f32) -> Path {
        use lyon::math::{point, vector, Angle};
        let mut path = Path::svg_builder();
        let pt = |a: f32| {
            let (sin, cos) = a.sin_cos();
            point(sin * r, cos * r)
        };
        path.move_to(pt(-start));
        path.arc(point(0., 0.), vector(r, r), Angle::radians(len), Angle::radians(0.));
        path.build()
    }

    const LOADING_SCALE: f32 = 0.74;
    const LOADING_CHANGE_SPEED: f32 = 3.5;
    const LOADING_ROTATE_SPEED: f32 = 4.1;

    pub fn loading<'b>(&mut self, cx: f32, cy: f32, t: f32, shading: impl IntoShading, params: impl Into<LoadingParams<'b>>) {
        use std::f32::consts::PI;

        let params = params.into();
        let (st, mut len) = if let Some(p) = params.progress {
            (t * Self::LOADING_ROTATE_SPEED, p * PI * 2.)
        } else {
            let ct = t * Self::LOADING_CHANGE_SPEED;
            let round = (ct / (PI * 2.)).floor();
            let st = round * Self::LOADING_SCALE + {
                let t = ct - round * PI * 2.;
                if t < PI {
                    0.
                } else {
                    ((t - PI * 3. / 2.).sin() + 1.) * Self::LOADING_SCALE / 2.
                }
            };
            let st = st * PI * 2. + t * Self::LOADING_ROTATE_SPEED;
            let len = (-ct.cos() * Self::LOADING_SCALE / 2. + 0.5) * PI * 2.;
            (st, len)
        };
        if let Some(last) = params.last {
            len = (*last * 5. + len) / 6.;
            *last = len;
        }
        self.scope(|ui| {
            ui.dx(cx);
            ui.dy(cy);
            ui.stroke_path(&Self::loading_path(st, len, params.radius), params.width, shading);
        });
    }

    #[inline]
    pub fn back_rect(&self) -> Rect {
        Rect::new(-0.97, -self.top + 0.04, 0.08, 0.08)
    }

    #[inline]
    pub fn tab_rects<'b>(&mut self, t: f32, it: impl IntoIterator<Item = (&'b mut DRectButton, Cow<'b, str>, bool)>) {
        let mut r = Rect::new(-0.92, -self.top + 0.18, 0.2, 0.11);
        for (btn, text, chosen) in it {
            btn.render_text(self, r, t, text, 0.5, chosen);
            r.y += 0.125;
        }
    }

    #[inline]
    pub fn content_rect(&self) -> Rect {
        Rect::new(-0.7, -self.top + 0.15, 1.67, self.top * 2. - 0.18)
    }

    pub fn draw_focus_frame(&mut self, rect: Rect, phase: f32) {
        let breath = 1.0 + 0.08 * (phase * std::f32::consts::TAU).sin();
        let padded = rect.feather(0.012);
        let glow = rect.feather(0.02);

        self.stroke_path(
            &glow.rounded(0.02),
            0.004,
            Color::new(0.33, 0.69, 1.0, 0.3 * breath),
        );
        self.stroke_path(
            &padded.rounded(0.015),
            0.0025,
            Color::new(0.33, 0.69, 1.0, 0.9 * breath),
        );
    }

    pub fn full_loading<'b>(&mut self, text: impl Into<Cow<'b, str>>, t: f32) {
        self.fill_rect(self.screen_rect(), semi_black(0.6));
        self.loading(0., -0.03, t, WHITE, ());
        self.text(text.into()).pos(0., 0.05).anchor(0.5, 0.).size(0.6).draw();
    }

    pub fn full_loading_simple(&mut self, t: f32) {
        self.fill_rect(self.screen_rect(), semi_black(0.6));
        self.loading(0., 0., t, WHITE, ());
    }

    pub fn main_sub_colors(use_black: bool, alpha: f32) -> (Color, Color) {
        if use_black {
            (semi_black(alpha), semi_black(alpha * 0.64))
        } else {
            (semi_white(alpha), semi_white(alpha * 0.64))
        }
    }
}

pub struct LoadingParams<'a> {
    pub radius: f32,
    pub width: f32,
    pub progress: Option<f32>,
    pub last: Option<&'a mut f32>,
}
impl Default for LoadingParams<'_> {
    fn default() -> Self {
        Self {
            radius: 0.05,
            width: 0.012,
            progress: None,
            last: None,
        }
    }
}
impl From<()> for LoadingParams<'_> {
    fn from(_: ()) -> Self {
        Self::default()
    }
}
impl From<f32> for LoadingParams<'_> {
    fn from(progress: f32) -> Self {
        Self {
            progress: Some(progress),
            ..Self::default()
        }
    }
}
impl<'a> From<(Option<f32>, &'a mut f32)> for LoadingParams<'a> {
    fn from((progress, last): (Option<f32>, &'a mut f32)) -> Self {
        Self {
            progress,
            last: Some(last),
            ..Self::default()
        }
    }
}
// This function is used to create UI audio manager.
#[allow(clippy::blocks_in_conditions)]
fn build_audio() -> AudioManager {
    match {
        #[cfg(target_os = "android")]
        {
            use sasa::backend::oboe::*;
            AudioManager::new(OboeBackend::new(OboeSettings {
                performance_mode: PerformanceMode::PowerSaving,
                usage: Usage::Game,
                ..Default::default()
            }))
        }
        #[cfg(target_env = "ohos")]
        {
            use sasa::backend::ohos::*;
            AudioManager::new(OhosBackend::new(OhosSettings {
                buffer_size: Some(512),
                sample_rate: Some(48000),
                channels: 2,
            }))
        }
        #[cfg(not(any(target_os = "android", target_env = "ohos")))]
        {
            use sasa::backend::cpal::*;
            AudioManager::new(CpalBackend::new(CpalSettings::default()))
        }
    } {
        Ok(manager) => manager,
        Err(e) => {
            show_error(e.context(ttl!("audio-backend-init-failed")));
            AudioManager::new(DummyBackend).expect("Failed to create dummy audio backend, this should not happen")
        }
    }
}

struct DummyBackend;

impl sasa::backend::Backend for DummyBackend {
    fn setup(&mut self, setup: sasa::backend::BackendSetup) -> anyhow::Result<()> {
        let _ = setup;
        Ok(())
    }
    fn start(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
    fn consume_broken(&self) -> bool {
        false
    }
}

thread_local! {
    pub static UI_AUDIO: RefCell<AudioManager> = RefCell::new(build_audio());
    pub static UI_BTN_HITSOUND_LARGE: RefCell<Option<Sfx>> = const { RefCell::new(None) };
    pub static UI_BTN_HITSOUND: RefCell<Option<Sfx>> = const { RefCell::new(None) };
    pub static UI_SWITCH_SOUND: RefCell<Option<Sfx>> = const { RefCell::new(None) };
}

pub fn button_hit() {
    UI_BTN_HITSOUND.with(|it| {
        if let Some(sfx) = it.borrow_mut().as_mut() {
            let _ = sfx.play(PlaySfxParams::default());
        }
    });
}

pub fn button_hit_large() {
    UI_BTN_HITSOUND_LARGE.with(|it| {
        if let Some(sfx) = it.borrow_mut().as_mut() {
            let _ = sfx.play(PlaySfxParams::default());
        }
    });
}

pub fn list_switch() {
    UI_SWITCH_SOUND.with(|it| {
        if let Some(sfx) = it.borrow_mut().as_mut() {
            let _ = sfx.play(PlaySfxParams::default());
        }
    });
}
