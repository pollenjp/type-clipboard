//! 打つ thread: 打つ先が前に来るのを待ち、IME と修飾キーを確かめてから、1 文字ずつ SendInput で送る。

use std::ffi::c_void;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use type_clipboard::keymap::SHIFT_SCANCODE;
use type_clipboard::secret::wipe;
use type_clipboard::typing::{Outcome, Stroke, Strokes};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Input::Ime::ImmGetDefaultIMEWnd;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT,
    KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, KEYEVENTF_UNICODE, SendInput, VIRTUAL_KEY, VK_CAPITAL,
    VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, PostMessageW, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_IME_CONTROL,
};

use super::hooks::INPUT_MARK;
use super::{WM_APP_DONE, WM_APP_PROGRESS};

/// 打つ先が前に来るまで待つ上限。
const FOREGROUND_TIMEOUT: Duration = Duration::from_secs(2);
/// 前に来てから打ち始めるまで置く間 (前の PowerShell の script と同じ 500 ms)。
const SETTLE: Duration = Duration::from_millis(500);
/// key down と key up の間。
const HOLD: Duration = Duration::from_millis(5);
/// WM_IME_CONTROL の IMC_GETOPENSTATUS (windows crate に無いので置く)。
const IMC_GETOPENSTATUS: usize = 0x0005;

/// 打つ仕事。HWND は thread をまたげないので isize で持つ。
pub struct Job {
    pub target: isize,
    pub strokes: Strokes,
    pub interval_ms: u32,
    /// 進み具合と終わりを知らせる主窓
    pub notify: isize,
    /// 利用者の入力があったら hook が立てる
    pub abort: Arc<AtomicBool>,
}

pub fn spawn(job: Job) -> std::io::Result<JoinHandle<()>> {
    std::thread::Builder::new()
        .name("typer".to_string())
        .spawn(move || run(job))
}

fn run(job: Job) {
    let Job {
        target,
        strokes,
        interval_ms,
        notify,
        abort,
    } = job;
    let target = HWND(target as *mut c_void);
    let notify = HWND(notify as *mut c_void);
    let (outcome, typed) = type_all(target, notify, &strokes, interval_ms, &abort);
    // 打ち終わったら (止めたときも) 写しを消してから知らせる
    drop(strokes);
    post(notify, WM_APP_DONE, outcome.code(), typed as isize);
}

fn type_all(
    target: HWND,
    notify: HWND,
    strokes: &Strokes,
    interval_ms: u32,
    abort: &AtomicBool,
) -> (Outcome, usize) {
    if let Err(outcome) = wait_for_foreground(target, abort) {
        return (outcome, 0);
    }
    if ime_open(target) == Some(true) {
        return (Outcome::ImeOn, 0);
    }
    if modifiers_held() {
        return (Outcome::ModifierHeld, 0);
    }
    let interval = Duration::from_millis(u64::from(interval_ms));
    for (typed, stroke) in strokes.iter().enumerate() {
        if abort.load(Ordering::SeqCst) {
            return (Outcome::StoppedByInput, typed);
        }
        if foreground() != target {
            return (Outcome::StoppedByForeground, typed);
        }
        if !send(stroke) {
            return (Outcome::SendFailed, typed);
        }
        post(notify, WM_APP_PROGRESS, typed + 1, 0);
        std::thread::sleep(interval);
    }
    (Outcome::Completed, strokes.len())
}

/// 打つ先が前に来て、`SETTLE` だけたつのを待つ。
fn wait_for_foreground(target: HWND, abort: &AtomicBool) -> Result<(), Outcome> {
    let start = Instant::now();
    let mut front_since: Option<Instant> = None;
    loop {
        if abort.load(Ordering::SeqCst) {
            return Err(Outcome::StoppedByInput);
        }
        if foreground() == target {
            let since = *front_since.get_or_insert_with(Instant::now);
            if since.elapsed() >= SETTLE {
                return Ok(());
            }
        } else {
            front_since = None;
            if start.elapsed() >= FOREGROUND_TIMEOUT {
                return Err(Outcome::NotForeground);
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn foreground() -> HWND {
    // SAFETY: 前面の窓を読むだけ
    unsafe { GetForegroundWindow() }
}

fn post(hwnd: HWND, msg: u32, wparam: usize, lparam: isize) {
    // SAFETY: 主窓へ知らせを置くだけ (閉じていれば失敗するだけ)
    let _ = unsafe { PostMessageW(Some(hwnd), msg, WPARAM(wparam), LPARAM(lparam)) };
}

/// Shift・Ctrl・Alt・Win のどれかが押されたままか。
pub fn modifiers_held() -> bool {
    [VK_SHIFT, VK_CONTROL, VK_MENU, VK_LWIN, VK_RWIN]
        .into_iter()
        .any(|key| {
            // SAFETY: キーの状態を読むだけ
            let state = unsafe { GetAsyncKeyState(i32::from(key.0)) };
            state < 0
        })
}

pub fn caps_lock_on() -> bool {
    // SAFETY: キーの状態を読むだけ
    let state = unsafe { GetKeyState(i32::from(VK_CAPITAL.0)) };
    state & 1 != 0
}

/// 打つ先の窓で手元の IME がオンか。判定できなければ `None`。
pub fn ime_open(target: HWND) -> Option<bool> {
    // SAFETY: 打つ先の IME の窓に問い合わせるだけ。固まっていれば待たずに諦める
    unsafe {
        let ime = ImmGetDefaultIMEWnd(target);
        if ime.is_invalid() {
            return None;
        }
        let mut open = 0usize;
        let sent = SendMessageTimeoutW(
            ime,
            WM_IME_CONTROL,
            WPARAM(IMC_GETOPENSTATUS),
            LPARAM(0),
            SMTO_ABORTIFHUNG,
            300,
            Some(&mut open),
        );
        (sent.0 != 0).then_some(open != 0)
    }
}

fn key_input(scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: INPUT_MARK,
            },
        },
    }
}

fn send_inputs(inputs: &[INPUT]) -> bool {
    // SAFETY: inputs は有効な INPUT の並び
    let sent = unsafe { SendInput(inputs, size_of::<INPUT>() as i32) };
    sent as usize == inputs.len()
}

/// 押して、少し置いて、離す。押せたら必ず離す (止める合図があっても)。
fn press_and_release(down: &mut [INPUT], up: &mut [INPUT]) -> bool {
    let pressed = send_inputs(down);
    let released = pressed && {
        std::thread::sleep(HOLD);
        send_inputs(up)
    };
    // 入力の並びにも文字の値が入るので消す
    wipe(down, INPUT::default());
    wipe(up, INPUT::default());
    pressed && released
}

/// 1 文字を送る。
fn send(stroke: &Stroke) -> bool {
    match *stroke {
        Stroke::Key(key) => {
            let down_flags = KEYEVENTF_SCANCODE;
            let up_flags = KEYEVENTF_SCANCODE | KEYEVENTF_KEYUP;
            if key.shift {
                press_and_release(
                    &mut [
                        key_input(SHIFT_SCANCODE, down_flags),
                        key_input(key.scancode, down_flags),
                    ],
                    &mut [
                        key_input(key.scancode, up_flags),
                        key_input(SHIFT_SCANCODE, up_flags),
                    ],
                )
            } else {
                press_and_release(
                    &mut [key_input(key.scancode, down_flags)],
                    &mut [key_input(key.scancode, up_flags)],
                )
            }
        }
        Stroke::Unicode(c) => {
            let mut units = [0u16; 2];
            let ok = c.encode_utf16(&mut units).iter().all(|&unit| {
                press_and_release(
                    &mut [key_input(unit, KEYEVENTF_UNICODE)],
                    &mut [key_input(unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP)],
                )
            });
            wipe(&mut units, 0);
            ok
        }
    }
}
