//! 低レベルのキーボードとマウスの hook。入れるのは「クリックで選ぶ」の間と、打っている間だけ。
//!
//! - クリックで選ぶ: マウスの位置を主窓に知らせ、左クリックを握りつぶして (相手の窓に届けず) 決める。
//!   Esc・左以外のボタンでやめる。押す方を握りつぶしたボタンは、離す方も握りつぶす (`PickMouse`)。
//! - 打っている間: このアプリが送った入力 (`INPUT_MARK` の印つき) でないキーの押下かクリックがあれば、
//!   止める合図を立てる。キーの押下は打つ先に届けない。
//!
//! hook は入れた thread (UI の thread) で呼ばれるので、状態は thread_local に置く。

use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use type_clipboard::geometry::pack_point;
use type_clipboard::pick::{Button, MouseEvent, PickMouse, PickNotice};
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
    pick: PickMouse,
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
            pick: PickMouse::default(),
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

/// 「クリックで選ぶ」で、押す方を握りつぶして、まだ離されていないボタンがあるか。
pub fn buttons_pending() -> bool {
    STATE.with(|state| {
        state
            .borrow()
            .as_ref()
            .is_some_and(|s| s.pick.has_pending_up())
    })
}

fn post(hwnd: HWND, msg: u32, lparam: isize) {
    // SAFETY: 主窓へ知らせを置くだけ (hook の中で待たない)
    let _ = unsafe { PostMessageW(Some(hwnd), msg, WPARAM(0), LPARAM(lparam)) };
}

/// LL hook の mouse の message を、ボタンの扱いの入力にする。
fn mouse_event(msg: u32) -> MouseEvent {
    match msg {
        WM_MOUSEMOVE => MouseEvent::Move,
        WM_LBUTTONDOWN => MouseEvent::Down(Button::Left),
        WM_LBUTTONUP => MouseEvent::Up(Button::Left),
        WM_RBUTTONDOWN => MouseEvent::Down(Button::Right),
        WM_RBUTTONUP => MouseEvent::Up(Button::Right),
        WM_MBUTTONDOWN => MouseEvent::Down(Button::Middle),
        WM_MBUTTONUP => MouseEvent::Up(Button::Middle),
        WM_XBUTTONDOWN => MouseEvent::Down(Button::X),
        WM_XBUTTONUP => MouseEvent::Up(Button::X),
        _ => MouseEvent::Other,
    }
}

/// 主窓へ知らせる。動いた・クリックの点は LPARAM に載せる (後から読むと、その間に動いた点になるため)。
fn notify_pick(hwnd: HWND, notice: PickNotice, point: POINT) {
    let at = pack_point(point.x, point.y);
    match notice {
        PickNotice::Move => post(hwnd, WM_APP_PICK_MOVE, at),
        PickNotice::Click => post(hwnd, WM_APP_PICK_CLICK, at),
        PickNotice::Cancel => post(hwnd, WM_APP_PICK_CANCEL, 0),
        PickNotice::Released => post(hwnd, WM_APP_PICK_RELEASED, 0),
    }
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
                post(s.notify, WM_APP_PICK_CANCEL, 0);
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
                HookMode::Pick => {
                    let (swallow, notice) = s.pick.handle(mouse_event(msg));
                    if let Some(notice) = notice {
                        notify_pick(s.notify, notice, info.pt);
                    }
                    swallow
                }
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
