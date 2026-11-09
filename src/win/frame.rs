//! デスクトップの上で、候補 (橙) や打つ先 (青) の窓の周りに出す枠。
//!
//! 枠の形の region を付けた、クリックを通す (WS_EX_TRANSPARENT) 最前面の popup。

use std::ffi::c_void;

use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CombineRgn, CreateRectRgn, CreateSolidBrush, DeleteObject, EndPaint, FillRect, HDC,
    InvalidateRect, PAINTSTRUCT, RGN_DIFF, SetWindowRgn,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GWLP_USERDATA, GetClientRect,
    GetWindowLongPtrW, HTTRANSPARENT, HWND_TOPMOST, LWA_ALPHA, RegisterClassExW, SW_HIDE,
    SWP_NOACTIVATE, SWP_SHOWWINDOW, SetLayeredWindowAttributes, SetWindowLongPtrW, SetWindowPos,
    ShowWindow, WM_ERASEBKGND, WM_NCHITTEST, WM_PAINT, WNDCLASSEXW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::{PCWSTR, w};

use super::target::visible_bounds;

const CLASS_NAME: PCWSTR = w!("TypeClipboardFrame");

pub struct Frame {
    hwnd: HWND,
}

impl Frame {
    pub fn register_class(instance: HINSTANCE) -> windows::core::Result<()> {
        let class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(frame_proc),
            hInstance: instance,
            lpszClassName: CLASS_NAME,
            ..Default::default()
        };
        // SAFETY: class は呼ぶ間だけ生きていればよい
        if unsafe { RegisterClassExW(&class) } == 0 {
            return Err(windows::core::Error::from_thread());
        }
        Ok(())
    }

    pub fn new(instance: HINSTANCE) -> windows::core::Result<Frame> {
        // SAFETY: 枠の窓を作るだけ
        unsafe {
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TRANSPARENT
                    | WS_EX_TOOLWINDOW
                    | WS_EX_TOPMOST
                    | WS_EX_NOACTIVATE,
                CLASS_NAME,
                w!(""),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(instance),
                None,
            )?;
            SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA)?;
            Ok(Frame { hwnd })
        }
    }

    /// `target` の窓の内側の縁に、太さ `thickness` の枠を `color` で出す。
    pub fn show_around(&self, target: HWND, color: COLORREF, thickness: i32) {
        let Some(bounds) = visible_bounds(target) else {
            self.hide();
            return;
        };
        let width = bounds.right - bounds.left;
        let height = bounds.bottom - bounds.top;
        // SAFETY: 枠の窓を動かし、region を付ける (付けた region は system が持つ)
        unsafe {
            SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, color.0 as isize);
            let outer = CreateRectRgn(0, 0, width, height);
            let inner = CreateRectRgn(thickness, thickness, width - thickness, height - thickness);
            CombineRgn(Some(outer), Some(outer), Some(inner), RGN_DIFF);
            let _ = DeleteObject(inner.into());
            SetWindowRgn(self.hwnd, Some(outer), true);
            let _ = SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                bounds.left,
                bounds.top,
                width,
                height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            let _ = InvalidateRect(Some(self.hwnd), None, true);
        }
    }

    pub fn hide(&self) {
        // SAFETY: 隠すだけ
        let _ = unsafe { ShowWindow(self.hwnd, SW_HIDE) };
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        // SAFETY: 作った thread (UI の thread) で消す
        let _ = unsafe { DestroyWindow(self.hwnd) };
    }
}

unsafe extern "system" fn frame_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: 枠の窓の message を扱う
    unsafe {
        match msg {
            WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
            WM_ERASEBKGND => {
                let hdc = HDC(wparam.0 as *mut c_void);
                let color = COLORREF(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as u32);
                let brush = CreateSolidBrush(color);
                let mut rect = RECT::default();
                let _ = GetClientRect(hwnd, &mut rect);
                FillRect(hdc, &rect, brush);
                let _ = DeleteObject(brush.into());
                LRESULT(1)
            }
            WM_PAINT => {
                let mut paint = PAINTSTRUCT::default();
                BeginPaint(hwnd, &mut paint);
                let _ = EndPaint(hwnd, &paint);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
