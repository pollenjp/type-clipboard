//! クリップボードの文字 (CF_UNICODETEXT) を読む。このアプリの側に写しは作らず、クリップボードを開いている間に
//! 中身を読ませる (数える・打つ列を作る)。

use std::time::Duration;

use windows::Win32::Foundation::{HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, CloseClipboard, CountClipboardFormats, GetClipboardData,
    IsClipboardFormatAvailable, OpenClipboard, RemoveClipboardFormatListener,
};
use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::System::Ole::CF_UNICODETEXT;

pub enum ClipText<T> {
    /// 文字が入っていた。中身を読ませた結果
    Text(T),
    /// 何も入っていない
    Empty,
    /// 文字でないもの (画像など) だけが入っている
    NotText,
    /// ほかのアプリが開いていて読めない
    Busy,
}

/// クリップボードの文字 (UTF-16。終わりの NUL は除く) を `read` に読ませ、その結果を返す。
/// `owner` はクリップボードを開く窓。
///
/// 中身は写さない。`read` に渡す slice は、クリップボードを開いている間だけ読める、Windows が渡す中身そのもの。
pub fn with_text<T>(owner: HWND, read: impl FnOnce(&[u16]) -> T) -> ClipText<T> {
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
        let text = read_opened(read);
        let _ = CloseClipboard();
        text
    }
}

/// 開いたクリップボードの文字を `read` に読ませる。
unsafe fn read_opened<T>(read: impl FnOnce(&[u16]) -> T) -> ClipText<T> {
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
        let text = if len == 0 {
            ClipText::Empty
        } else {
            ClipText::Text(read(&all[..len]))
        };
        let _ = GlobalUnlock(memory);
        text
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
