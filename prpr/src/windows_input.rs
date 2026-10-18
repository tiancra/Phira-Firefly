//! Windows 系统输入框：进程内 Win32 模态对话框。
//!
//! 替代 inputbox 的 PowerShell 后端：以游戏主窗口的子窗口形式弹出，
//! 在父窗口客户区居中显示，控件统一使用系统 UI 字体（DEFAULT_GUI_FONT），
//! 弹出瞬时（无需冷启动子进程）、无控制台黑窗；Edit 自带系统 IME，
//! 所有文字输入均由系统提供。

#![cfg(target_os = "windows")]

use inputbox::{InputBox, InputMode};
use std::{ffi::c_void, ptr};
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM, WIN32_ERROR};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, GetStockObject, MonitorFromWindow, MONITOR_DEFAULTTONEAREST, MONITORINFO,
    DEFAULT_GUI_FONT, HBRUSH, HFONT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE,
    REG_OPTION_NON_VOLATILE, REG_SZ,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetActiveWindow, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::{
    BS_DEFPUSHBUTTON, BS_PUSHBUTTON, CreateWindowExW, CS_HREDRAW, CS_VREDRAW, DefWindowProcW,
    DestroyWindow, DispatchMessageW, ES_AUTOVSCROLL, ES_MULTILINE, ES_PASSWORD, GetClientRect,
    GetMessageW, GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW, GWLP_USERDATA,
    HICON, HMENU, IDC_ARROW, IDCANCEL, IDOK, IsDialogMessageW, IsWindow, LoadCursorW, MSG,
    RegisterClassW, SendMessageW, SetWindowLongPtrW, ShowWindow, SW_SHOW, TranslateMessage,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_COMMAND, WM_DESTROY, WM_NCCREATE, WM_SETFONT,
    WS_CAPTION, WS_CHILD, WS_EX_CLIENTEDGE, WS_EX_TOPMOST, WS_POPUP, WS_SYSMENU, WS_TABSTOP,
    WS_VISIBLE, WNDCLASSW, CREATESTRUCTW, GetWindowRect,
};

const ID_EDIT: usize = 0x1001;
const ID_PROMPT: usize = 0x1002;

struct Ctx {
    edit: HWND,
    result: Option<String>,
}

unsafe extern "system" fn dlg_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_NCCREATE {
            let create = &*(lparam.0 as *const CREATESTRUCTW);
            let _ = SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        let ctx = (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Ctx).as_mut();
        match msg {
            WM_COMMAND => {
                let id = (wparam.0 & 0xffff) as usize;
                if id == IDOK.0 as usize || id == IDCANCEL.0 as usize {
                    if id == IDOK.0 as usize {
                        if let Some(ctx) = ctx {
                            let len = GetWindowTextLengthW(ctx.edit);
                            let mut buf = vec![0u16; (len + 1) as usize];
                            if len > 0 {
                                let _ = GetWindowTextW(ctx.edit, &mut buf);
                            }
                            ctx.result = Some(String::from_utf16_lossy(&buf[..len as usize]));
                        }
                    }
                    let _ = DestroyWindow(hwnd);
                    return LRESULT(0);
                }
                return LRESULT(0);
            }
            WM_CLOSE => {
                let _ = DestroyWindow(hwnd);
                return LRESULT(0);
            }
            WM_DESTROY => return LRESULT(0),
            _ => {}
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

/// 判断游戏窗口是否处于全屏（窗口矩形完全覆盖所在显示器）。
fn is_game_fullscreen() -> bool {
    unsafe {
        let hwnd = GetActiveWindow();
        if hwnd.0.is_null() {
            return false;
        }
        let mut rc = RECT::default();
        let _ = GetWindowRect(hwnd, &mut rc);
        let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO::default();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        let _ = GetMonitorInfoW(mon, &mut mi);
        rc.left <= mi.rcMonitor.left
            && rc.top <= mi.rcMonitor.top
            && rc.right >= mi.rcMonitor.right
            && rc.bottom >= mi.rcMonitor.bottom
    }
}

/// 为当前程序禁用 Windows「全屏优化」（independent flip）。
///
/// 游戏的全屏实际是无边框窗口化全屏（DWM 合成），但 Win10/11 检测到全屏窗口后
/// 会自动启用「全屏优化」，配合 NVIDIA 驱动直接接管显示输出，导致 Win32 子窗口
/// 无法显示。这里写入 per-app 注册表设置（HKCU，无需管理员）关掉它，
/// 游戏全屏即回到普通 DWM 合成，输入框子窗口可直接显示，无需任何全屏切换。
pub fn disable_fullscreen_optimization() {
    unsafe {
        let Ok(exe) = std::env::current_exe() else { return };
        let path = exe.to_string_lossy().to_string();
        let subkey = HSTRING::from("Software\\Microsoft\\DirectX\\UserGpuPreferences");
        let mut key: HKEY = HKEY::default();
        let status = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            0,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut key,
            None,
        );
        if status == WIN32_ERROR(0) {
            let data = "FullscreenOptimization=0";
            let bytes: Vec<u16> = data.encode_utf16().chain(std::iter::once(0)).collect();
            let u8_slice = unsafe {
                std::slice::from_raw_parts(bytes.as_ptr() as *const u8, bytes.len() * 2)
            };
            let value_name = HSTRING::from(&*path);
            let _ = RegSetValueExW(
                key,
                PCWSTR(value_name.as_ptr()),
                0,
                REG_SZ,
                Some(u8_slice),
            );
            let _ = RegCloseKey(key);
        }
    }
}

/// 以游戏主窗口的子窗口形式弹出系统输入框，居中显示，返回用户输入（取消/关闭返回 None）。
pub fn show_system_input_dialog(config: &InputBox) -> Option<String> {
    show_system_input_dialog_impl(config)
}

fn show_system_input_dialog_impl(config: &InputBox) -> Option<String> {
    let title = config.title.clone().unwrap_or_default();
    let prompt = config.prompt.clone().unwrap_or_default();
    let default = config.default.to_string();
    let ok_label = config
        .ok_label
        .clone()
        .unwrap_or_else(|| "OK".into());
    let cancel_label = config
        .cancel_label
        .clone()
        .unwrap_or_else(|| "Cancel".into());
    let multiline = matches!(config.mode, InputMode::Multiline);
    let password = matches!(config.mode, InputMode::Password);

    unsafe {
        let hmodule = GetModuleHandleW(PCWSTR::null()).unwrap();
        let hinst = HINSTANCE(hmodule.0);
        let class = HSTRING::from("PhiraSystemInputDialog");
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(dlg_proc),
            hInstance: hinst,
            hIcon: HICON(ptr::null_mut()),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
            hbrBackground: HBRUSH((5 + 1) as *mut c_void), // COLOR_WINDOW + 1
            lpszMenuName: PCWSTR::null(),
            lpszClassName: PCWSTR(class.as_ptr()),
            cbClsExtra: 0,
            cbWndExtra: 0,
        };
        let _ = RegisterClassW(&wc);

        let parent = GetActiveWindow();
        let w = 420;
        let h = if multiline { 250 } else { 150 };
        // 顶层窗口（owner = 游戏窗口）+ 置顶，居中于游戏窗口
        let style = WINDOW_STYLE(WS_POPUP.0 | WS_CAPTION.0 | WS_SYSMENU.0 | WS_VISIBLE.0);
        let ex = WINDOW_EX_STYLE(WS_EX_TOPMOST.0);
        let mut ctx = Box::new(Ctx { edit: HWND::default(), result: None });
        let ctx_ptr = &mut *ctx as *mut Ctx as *const c_void;

        // 居中于游戏窗口（屏幕坐标）
        let mut rc = RECT::default();
        let _ = GetWindowRect(parent, &mut rc);
        let x = rc.left + ((rc.right - rc.left - w) / 2).max(0);
        let y = rc.top + ((rc.bottom - rc.top - h) / 2).max(0);
        let dlg = CreateWindowExW(
            ex,
            PCWSTR(class.as_ptr()),
            PCWSTR(HSTRING::from(&*title).as_ptr()),
            style,
            x,
            y,
            w,
            h,
            parent,
            HMENU::default(),
            hinst,
            Some(ctx_ptr),
        )
        .unwrap();

        // prompt 提示文本
        let prompt_hwnd = if !prompt.is_empty() {
            Some(
                CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    w!("Static"),
                    PCWSTR(HSTRING::from(&*prompt).as_ptr()),
                    WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0),
                    16,
                    12,
                    w - 32,
                    24,
                    dlg,
                    HMENU(ID_PROMPT as isize as *mut c_void),
                    hinst,
                    None,
                )
                .unwrap(),
            )
        } else {
            None
        };

        let edit_y = if prompt.is_empty() { 16 } else { 40 };
        let edit_h = if multiline { h - 96 } else { 26 };
        let mut edit_style = WS_CHILD | WS_VISIBLE | WS_TABSTOP;
        if multiline {
            edit_style = edit_style | WINDOW_STYLE((ES_MULTILINE | ES_AUTOVSCROLL | 0x00200000) as u32); // WS_VSCROLL
        }
        if password {
            edit_style = edit_style | WINDOW_STYLE(ES_PASSWORD as u32);
        }
        let edit = CreateWindowExW(
            WS_EX_CLIENTEDGE,
            w!("Edit"),
            PCWSTR(HSTRING::from(default.as_str()).as_ptr()),
            edit_style,
            16,
            edit_y,
            w - 32,
            edit_h,
            dlg,
            HMENU(ID_EDIT as isize as *mut c_void),
            hinst,
            None,
        )
        .unwrap();
        ctx.edit = edit;

        let btn_h = 30;
        let btn_w = 90;
        let btn_y = h - btn_h - 12;
        let ok_x = w - 16 - btn_w * 2 - 10;
        let ok_hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("Button"),
            PCWSTR(HSTRING::from(&*ok_label).as_ptr()),
            WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | WS_TABSTOP.0 | BS_DEFPUSHBUTTON as u32),
            ok_x,
            btn_y,
            btn_w,
            btn_h,
            dlg,
            HMENU(IDOK.0 as isize as *mut c_void),
            hinst,
            None,
        )
        .unwrap();
        let cancel_hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("Button"),
            PCWSTR(HSTRING::from(&*cancel_label).as_ptr()),
            WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | WS_TABSTOP.0 | BS_PUSHBUTTON as u32),
            ok_x + btn_w + 10,
            btn_y,
            btn_w,
            btn_h,
            dlg,
            HMENU(IDCANCEL.0 as isize as *mut c_void),
            hinst,
            None,
        )
        .unwrap();

        // 统一使用系统 UI 字体
        let font = GetStockObject(DEFAULT_GUI_FONT);
        let hfont = HFONT(font.0 as *mut c_void);
        let font_wparam = WPARAM(hfont.0 as usize);
        if let Some(h) = prompt_hwnd {
            let _ = SendMessageW(h, WM_SETFONT, font_wparam, LPARAM(1));
        }
        let _ = SendMessageW(edit, WM_SETFONT, font_wparam, LPARAM(1));
        let _ = SendMessageW(ok_hwnd, WM_SETFONT, font_wparam, LPARAM(1));
        let _ = SendMessageW(cancel_hwnd, WM_SETFONT, font_wparam, LPARAM(1));

        let _ = ShowWindow(dlg, SW_SHOW);
        let _ = SetFocus(edit);

        // 模态消息循环：Enter/Escape/Tab 等由 IsDialogMessageW 交给系统处理
        let mut msg = MSG::default();
        loop {
            let ret = GetMessageW(&mut msg, HWND::default(), 0, 0);
            if ret.0 == 0 {
                break;
            }
            if !IsDialogMessageW(dlg, &msg).as_bool() {
                let _ = TranslateMessage(&msg);
                let _ = DispatchMessageW(&msg);
            }
            if !IsWindow(dlg).as_bool() {
                break;
            }
        }

        let result = ctx.result.take();
        drop(ctx);
        result
    }
}

use windows::core::w;
