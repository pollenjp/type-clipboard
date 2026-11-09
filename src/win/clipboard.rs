//! クリップボードの文字 (CF_UNICODETEXT) を読む。中身は `SecretText` に入れ、使い終えたら消える。

use std::time::Duration;

use type_clipboard::secret::SecretText;
use windows::Win32::Foundation::{HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, CloseClipboard, CountClipboardFormats, GetClipboardData,
    IsClipboardFormatAvailable, OpenClipboard, RemoveClipboardFormatListener,
};
use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::System::Ole::CF_UNICODETEXT;

pub enum ClipText {
    Text(SecretText),
    /// 何も入っていない
    Empty,
    /// 文字でないもの (画像など) だけが入っている
    NotText,
    /// ほかのアプリが開いていて読めない
    Busy,
}

/// クリップボードの文字を読む。`owner` はクリップボードを開く窓。
pub fn read(owner: HWND) -> ClipText {
    // SAFETY: 開いたクリップボードは必ず閉じる
    unsafe {
        if CountClipboardFormats() == 0 {
            return ClipText::Empty;
        }
        if IsClipboardFormatAvailable(u32::from(CF_UNICODETEXT.0)).is_err() {
            return ClipText::NotText;
        }
        let mut opened = false;
        for _ in 0..5 {
            if OpenClipboard(Some(owner)).is_ok() {
                opened = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        if !opened {
            return ClipText::Busy;
        }
        let text = read_opened();
        let _ = CloseClipboard();
        text
    }
}

/// 開いたクリップボードから文字を写す。
unsafe fn read_opened() -> ClipText {
    // SAFETY: クリップボードを開いている間に呼ぶ。lock した memory は unlock する
    unsafe {
        let Ok(handle) = GetClipboardData(u32::from(CF_UNICODETEXT.0)) else {
            return ClipText::NotText;
        };
        let memory = HGLOBAL(handle.0);
        let ptr = GlobalLock(memory) as *const u16;
        if ptr.is_null() {
            return ClipText::NotText;
        }
        let units = GlobalSize(memory) / 2;
        let all = std::slice::from_raw_parts(ptr, units);
        let len = all.iter().position(|&u| u == 0).unwrap_or(units);
        // 長さぴったりで確保して、伸ばし直しで写しを残さない
        let mut text = Vec::with_capacity(len);
        text.extend_from_slice(&all[..len]);
        let _ = GlobalUnlock(memory);
        if text.is_empty() {
            ClipText::Empty
        } else {
            ClipText::Text(SecretText::from_utf16(text))
        }
    }
}

/// クリップボードが変わったら `hwnd` に WM_CLIPBOARDUPDATE を送ってもらう。
pub fn listen(hwnd: HWND) {
    // SAFETY: 窓を登録するだけ
    let _ = unsafe { AddClipboardFormatListener(hwnd) };
}

pub fn unlisten(hwnd: HWND) {
    // SAFETY: 登録を外すだけ
    let _ = unsafe { RemoveClipboardFormatListener(hwnd) };
}
