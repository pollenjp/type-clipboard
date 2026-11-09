//! Win32 の部分 (Windows でだけ組む)。

mod app;
mod clipboard;
mod frame;
mod hooks;
mod preview;
mod target;
mod typer;
mod ui;

use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW, WM_APP};
use windows::core::{HSTRING, w};

pub use app::run;

/// 打つ thread から: 打った文字の数 (WPARAM)。
const WM_APP_PROGRESS: u32 = WM_APP + 1;
/// 打つ thread から: 打ち終わり・止まったわけ (WPARAM = `Outcome::code`) と打った文字の数 (LPARAM)。
const WM_APP_DONE: u32 = WM_APP + 2;
/// hook から (クリックで選ぶ): マウスが動いた。
const WM_APP_PICK_MOVE: u32 = WM_APP + 3;
/// hook から: 左クリック (相手の窓には届けていない)。
const WM_APP_PICK_CLICK: u32 = WM_APP + 4;
/// hook から: Esc か右クリックでやめる。
const WM_APP_PICK_CANCEL: u32 = WM_APP + 5;
/// hook から: 左クリックのボタンを離した。
const WM_APP_PICK_RELEASED: u32 = WM_APP + 6;

/// 起動できなかったわけを出す。
pub fn show_fatal(error: &windows::core::Error) {
    let text = HSTRING::from(format!("起動できませんでした。\n{error}"));
    // SAFETY: message box を出すだけ
    unsafe { MessageBoxW(None, &text, w!("type-clipboard"), MB_ICONERROR | MB_OK) };
}
