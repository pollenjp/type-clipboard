//! 低レベルのキーボードとマウスの hook。入れるのは「クリックで選ぶ」の間と、打っている間だけ。
//!
//! - クリックで選ぶ: マウスの位置を主窓に知らせ、左クリックを握りつぶして (相手の窓に届けず) 決める。
//!   Esc・右クリックでやめる。
//! - 打っている間: このアプリが送った入力 (`INPUT_MARK` の印つき) でないキーの押下かクリックがあれば、
//!   止める合図を立てる。キーの押下は打つ先に届けない。
//!
//! hook は入れた thread (UI の thread) で呼ばれるので、状態は thread_local に置く。

use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT, MSLLHOOKSTRUCT, PostMessageW,
    SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE, WM_RBUTTONDOWN,
    WM_RBUTTONUP, WM_SYSKEYDOWN, WM_XBUTTONDOWN, WM_XBUTTONUP,
};

use super::{WM_APP_PICK_CANCEL, WM_APP_PICK_CLICK, WM_APP_PICK_MOVE, WM_APP_PICK_RELEASED};

/// このアプリが SendInput で送る入力の `dwExtraInfo`。利用者の入力と見分ける。
pub const INPUT_MARK: usize = 0x5459_4350;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HookMode {
    Pick,
    Typing,
}

struct HookState {
    mode: HookMode,
    notify: HWND,
    abort: Arc<AtomicBool>,
    point: POINT,
    /// 握りつぶした左クリックの、ボタンを離す方も握りつぶす
    swallow_left_up: bool,
}

thread_local! {
    static STATE: RefCell<Option<HookState>> = const { RefCell::new(None) };
}

/// 入れている間の hook。Drop で外す。
pub struct HookGuard {
    keyboard: HHOOK,
    mouse: HHOOK,
}

impl Drop for HookGuard {
    fn drop(&mut self) {
        // SAFETY: install が入れた hook を外す
        unsafe {
            let _ = UnhookWindowsHookEx(self.keyboard);
            let _ = UnhookWindowsHookEx(self.mouse);
        }
        STATE.with(|state| *state.borrow_mut() = None);
    }
}

/// hook を入れる。`notify` に WM_APP_PICK_* を送り、打っている間の利用者の入力で `abort` を立てる。
pub fn install(
    mode: HookMode,
    notify: HWND,
    abort: Arc<AtomicBool>,
) -> windows::core::Result<HookGuard> {
    STATE.with(|state| {
        *state.borrow_mut() = Some(HookState {
            mode,
            notify,
            abort,
            point: POINT::default(),
            swallow_left_up: false,
        })
    });
    let clear = || STATE.with(|state| *state.borrow_mut() = None);
    // SAFETY: hook の手続きは、この module の 'static な関数
    unsafe {
        let module: HINSTANCE = GetModuleHandleW(None).inspect_err(|_| clear())?.into();
        let keyboard = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), Some(module), 0)
            .inspect_err(|_| clear())?;
        let mouse = match SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), Some(module), 0) {
            Ok(mouse) => mouse,
            Err(e) => {
                let _ = UnhookWindowsHookEx(keyboard);
                clear();
                return Err(e);
            }
        };
        Ok(HookGuard { keyboard, mouse })
    }
}

/// 「クリックで選ぶ」の最後のマウスの位置 (画面の座標)。
pub fn last_point() -> POINT {
    STATE.with(|state| state.borrow().as_ref().map(|s| s.point).unwrap_or_default())
}

fn post(hwnd: HWND, msg: u32) {
    // SAFETY: 主窓へ知らせを置くだけ (hook の中で待たない)
    let _ = unsafe { PostMessageW(Some(hwnd), msg, WPARAM(0), LPARAM(0)) };
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: WH_KEYBOARD_LL の lparam は KBDLLHOOKSTRUCT を指す
        let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let pressed = matches!(wparam.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
        let ours = info.dwExtraInfo == INPUT_MARK;
        let swallow = STATE.with(|state| match state.borrow().as_ref() {
            Some(s) if s.mode == HookMode::Typing && pressed && !ours => {
                s.abort.store(true, Ordering::SeqCst);
                true
            }
            Some(s)
                if s.mode == HookMode::Pick && pressed && info.vkCode == u32::from(VK_ESCAPE.0) =>
            {
                post(s.notify, WM_APP_PICK_CANCEL);
                true
            }
            _ => false,
        });
        if swallow {
            return LRESULT(1);
        }
    }
    // SAFETY: 次の hook へ渡す
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: WH_MOUSE_LL の lparam は MSLLHOOKSTRUCT を指す
        let info = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        let msg = wparam.0 as u32;
        let ours = info.dwExtraInfo == INPUT_MARK;
        let swallow = STATE.with(|state| {
            let mut state = state.borrow_mut();
            let Some(s) = state.as_mut() else {
                return false;
            };
            match s.mode {
                HookMode::Typing => {
                    if !ours && is_button_down(msg) {
                        s.abort.store(true, Ordering::SeqCst);
                    }
                    false
                }
                HookMode::Pick => match msg {
                    WM_MOUSEMOVE => {
                        s.point = info.pt;
                        post(s.notify, WM_APP_PICK_MOVE);
                        false
                    }
                    WM_LBUTTONDOWN => {
                        s.point = info.pt;
                        s.swallow_left_up = true;
                        post(s.notify, WM_APP_PICK_CLICK);
                        true
                    }
                    WM_LBUTTONUP if s.swallow_left_up => {
                        s.swallow_left_up = false;
                        post(s.notify, WM_APP_PICK_RELEASED);
                        true
                    }
                    WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN => {
                        post(s.notify, WM_APP_PICK_CANCEL);
                        true
                    }
                    WM_RBUTTONUP | WM_MBUTTONUP | WM_XBUTTONUP => true,
                    _ => false,
                },
            }
        });
        if swallow {
            return LRESULT(1);
        }
    }
    // SAFETY: 次の hook へ渡す
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn is_button_down(msg: u32) -> bool {
    matches!(
        msg,
        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN
    )
}
