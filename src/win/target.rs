//! 打つ先の候補の窓: 列挙・題・exe の名前・整合性のレベル・画面の上の位置。

use std::ffi::c_void;

use type_clipboard::candidate::{
    Elevation, LevelError, WindowFacts, elevation_from_levels, is_listed, is_shell_class,
};
use windows::Win32::Foundation::{CloseHandle, ERROR_ACCESS_DENIED, HANDLE, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{
    DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute,
};
use windows::Win32::Security::{
    GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TOKEN_MANDATORY_LABEL,
    TOKEN_QUERY, TokenIntegrityLevel,
};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GWL_EXSTYLE, GetClassNameW, GetWindow, GetWindowLongPtrW, GetWindowRect,
    GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
    WINDOW_EX_STYLE, WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};
use windows::core::{BOOL, PWSTR};

/// 一覧の 1 行。
pub struct WindowEntry {
    pub hwnd: HWND,
    pub title: String,
    pub exe: String,
}

/// 一覧に出す窓 (Z の順。前にある窓から)。
pub fn list() -> Vec<WindowEntry> {
    let mut all: Vec<HWND> = Vec::new();
    // SAFETY: collect は EnumWindows の間だけ `all` を使う
    let _ = unsafe { EnumWindows(Some(collect), LPARAM(&mut all as *mut Vec<HWND> as isize)) };
    all.into_iter()
        .filter(|&hwnd| is_listed(&facts(hwnd)))
        .map(entry)
        .collect()
}

unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: lparam は list() が渡した &mut Vec<HWND>
    let all = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
    all.push(hwnd);
    BOOL(1)
}

/// 窓の判定に使う性質を読む。
pub fn facts(hwnd: HWND) -> WindowFacts {
    // SAFETY: どれも窓の性質を読むだけ (閉じた窓なら失敗して既定の値になる)
    unsafe {
        let ex_style = WINDOW_EX_STYLE(GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32);
        WindowFacts {
            visible: IsWindowVisible(hwnd).as_bool(),
            cloaked: is_cloaked(hwnd),
            title_len: usize::try_from(GetWindowTextLengthW(hwnd)).unwrap_or(0),
            tool_window: ex_style.contains(WS_EX_TOOLWINDOW),
            app_window: ex_style.contains(WS_EX_APPWINDOW),
            no_activate: ex_style.contains(WS_EX_NOACTIVATE),
            has_owner: GetWindow(hwnd, GW_OWNER).is_ok_and(|owner| !owner.is_invalid()),
            is_self: process_id(hwnd) == GetCurrentProcessId(),
            shell: is_shell_class(&class_name(hwnd)),
        }
    }
}

pub fn entry(hwnd: HWND) -> WindowEntry {
    WindowEntry {
        hwnd,
        title: title(hwnd),
        exe: exe_name(hwnd),
    }
}

fn title(hwnd: HWND) -> String {
    // SAFETY: 窓の題を読むだけ。buf の長さを渡している
    unsafe {
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return String::new();
        }
        let mut buf = vec![0u16; len as usize + 1];
        let n = GetWindowTextW(hwnd, &mut buf);
        String::from_utf16_lossy(&buf[..usize::try_from(n).unwrap_or(0)])
    }
}

fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    // SAFETY: buf の長さを渡している
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..usize::try_from(n).unwrap_or(0)])
}

fn process_id(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    // SAFETY: pid に書くだけ
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid
}

/// exe のファイルの名前 (`notepad.exe` など)。読めなければ `?`。
fn exe_name(hwnd: HWND) -> String {
    // SAFETY: 開いた handle は閉じる。buf の長さを size で渡している
    unsafe {
        let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id(hwnd))
        else {
            return "?".to_string();
        };
        let mut buf = [0u16; 1024];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut size,
        )
        .is_ok();
        let _ = CloseHandle(process);
        if !ok {
            return "?".to_string();
        }
        let path = String::from_utf16_lossy(&buf[..size as usize]);
        path.rsplit('\\').next().unwrap_or_default().to_string()
    }
}

fn is_cloaked(hwnd: HWND) -> bool {
    let mut cloaked = 0u32;
    // SAFETY: cloaked の大きさを渡している
    unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut u32 as *mut c_void,
            size_of::<u32>() as u32,
        )
    }
    .is_ok()
        && cloaked != 0
}

/// 画面の上で見えている窓の矩形 (影や見えない縁を除く)。
pub fn visible_bounds(hwnd: HWND) -> Option<RECT> {
    let mut rect = RECT::default();
    // SAFETY: rect の大きさを渡している
    let ok = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut rect as *mut RECT as *mut c_void,
            size_of::<RECT>() as u32,
        )
        .is_ok()
            || GetWindowRect(hwnd, &mut rect).is_ok()
    };
    (ok && rect.right > rect.left && rect.bottom > rect.top).then_some(rect)
}

/// 打つ先が、このアプリより高い整合性のレベル (管理者など) で動いているか。
pub fn elevation(hwnd: HWND) -> Elevation {
    // SAFETY: GetCurrentProcess は閉じなくてよい疑似の handle。開いた handle は閉じる
    unsafe {
        let ours = integrity_level(GetCurrentProcess()).ok();
        let target = match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id(hwnd)) {
            Ok(process) => {
                let level = integrity_level(process);
                let _ = CloseHandle(process);
                level
            }
            Err(e) => Err(level_error(&e)),
        };
        elevation_from_levels(ours, target)
    }
}

fn level_error(error: &windows::core::Error) -> LevelError {
    if error.code() == ERROR_ACCESS_DENIED.to_hresult() {
        LevelError::AccessDenied
    } else {
        LevelError::Other
    }
}

/// process の整合性のレベル (mandatory label の RID)。
fn integrity_level(process: HANDLE) -> Result<u32, LevelError> {
    let mut token = HANDLE::default();
    // SAFETY: token は開けたら閉じる
    unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) }.map_err(|e| level_error(&e))?;
    let level = token_integrity_level(token);
    // SAFETY: 開いた token を閉じる
    let _ = unsafe { CloseHandle(token) };
    level.ok_or(LevelError::Other)
}

fn token_integrity_level(token: HANDLE) -> Option<u32> {
    let mut len = 0u32;
    // SAFETY: 大きさを問い合わせる (失敗して len に要る大きさが入る)
    let _ = unsafe { GetTokenInformation(token, TokenIntegrityLevel, None, 0, &mut len) };
    if len == 0 {
        return None;
    }
    // TOKEN_MANDATORY_LABEL は pointer を含むので、8 byte に揃えた buffer に受ける
    let mut buf = vec![0u64; (len as usize).div_ceil(8)];
    // SAFETY: buf は len byte 以上あり、TOKEN_MANDATORY_LABEL の並びに揃っている。
    // Sid は buf の中を指し、buf が生きている間だけ使う
    unsafe {
        GetTokenInformation(
            token,
            TokenIntegrityLevel,
            Some(buf.as_mut_ptr() as *mut c_void),
            len,
            &mut len,
        )
        .ok()?;
        let label = &*(buf.as_ptr() as *const TOKEN_MANDATORY_LABEL);
        let count = *GetSidSubAuthorityCount(label.Label.Sid);
        if count == 0 {
            return None;
        }
        Some(*GetSidSubAuthority(label.Label.Sid, u32::from(count - 1)))
    }
}
