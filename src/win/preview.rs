//! プレビュー: DWM のサムネイルで、候補か打つ先の窓の今の姿を映す。欄の地・枠・札は主窓が描く。
//!
//! サムネイルは主窓の描いたものの上に DWM が重ねる (このアプリは画素を持たない)。

use std::cell::Cell;

use type_clipboard::geometry::{Rect, fit_within};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{
    DWM_THUMBNAIL_PROPERTIES, DWM_TNP_OPACITY, DWM_TNP_RECTDESTINATION,
    DWM_TNP_SOURCECLIENTAREAONLY, DWM_TNP_VISIBLE, DwmQueryThumbnailSourceSize,
    DwmRegisterThumbnail, DwmUnregisterThumbnail, DwmUpdateThumbnailProperties,
};
use windows::Win32::Graphics::Gdi::{
    CreatePen, CreateSolidBrush, DT_CALCRECT, DT_CENTER, DT_END_ELLIPSIS, DT_NOPREFIX,
    DT_SINGLELINE, DT_VCENTER, DeleteObject, DrawTextW, Ellipse, FillRect, GetStockObject, HDC,
    HFONT, NULL_BRUSH, NULL_PEN, PS_DASH, PS_SOLID, Rectangle, RoundRect, SelectObject, SetBkMode,
    SetTextColor, TRANSPARENT,
};
use windows::Win32::UI::WindowsAndMessaging::IsIconic;

use super::ui::{BLACK, BLUE, LINE, MUTED, ORANGE, PANEL, RED, WHITE, scale};

/// 欄に映しているものの役。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Empty,
    /// 一覧をなぞっている・クリックで選ぶの最中に指している窓
    Candidate,
    /// 決めた打つ先
    Target,
}

/// サムネイルの今の様子。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shown {
    Nothing,
    Live,
    /// 最小化していて中身が描かれない
    Minimized,
    /// DWM が映させてくれない
    Unavailable,
}

pub struct Preview {
    dest: HWND,
    source: Cell<Option<HWND>>,
    thumbnail: Cell<Option<isize>>,
}

impl Preview {
    pub fn new(dest: HWND) -> Preview {
        Preview {
            dest,
            source: Cell::new(None),
            thumbnail: Cell::new(None),
        }
    }

    /// 映す窓を替える (`None` で映さない)。
    pub fn show(&self, source: Option<HWND>) {
        if self.source.get() == source {
            return;
        }
        self.unregister();
        self.source.set(source);
        if let Some(source) = source {
            // SAFETY: 主窓に source のサムネイルを重ねる登録
            if let Ok(thumbnail) = unsafe { DwmRegisterThumbnail(self.dest, source) } {
                self.thumbnail.set(Some(thumbnail));
            }
        }
    }

    fn unregister(&self) {
        if let Some(thumbnail) = self.thumbnail.take() {
            // SAFETY: show が登録したものを外す
            let _ = unsafe { DwmUnregisterThumbnail(thumbnail) };
        }
    }

    /// サムネイルを `area` (主窓の client の座標) に、縦横比を保って収める。
    pub fn place(&self, area: Rect) -> Shown {
        let Some(source) = self.source.get() else {
            return Shown::Nothing;
        };
        let Some(thumbnail) = self.thumbnail.get() else {
            return Shown::Unavailable;
        };
        // SAFETY: 登録したサムネイルの大きさを読み、映す場所を決める
        unsafe {
            let minimized = IsIconic(source).as_bool();
            let dest = DwmQueryThumbnailSourceSize(thumbnail)
                .ok()
                .and_then(|size| fit_within(size.cx, size.cy, area));
            let visible = !minimized && dest.is_some();
            let dest = dest.unwrap_or_default();
            let properties = DWM_THUMBNAIL_PROPERTIES {
                dwFlags: DWM_TNP_RECTDESTINATION
                    | DWM_TNP_VISIBLE
                    | DWM_TNP_OPACITY
                    | DWM_TNP_SOURCECLIENTAREAONLY,
                rcDestination: RECT {
                    left: dest.left,
                    top: dest.top,
                    right: dest.right,
                    bottom: dest.bottom,
                },
                opacity: 255,
                fVisible: visible.into(),
                fSourceClientAreaOnly: false.into(),
                ..Default::default()
            };
            if DwmUpdateThumbnailProperties(thumbnail, &properties).is_err() {
                return Shown::Unavailable;
            }
            if minimized {
                Shown::Minimized
            } else if visible {
                Shown::Live
            } else {
                Shown::Unavailable
            }
        }
    }
}

impl Drop for Preview {
    fn drop(&mut self) {
        self.unregister();
    }
}

fn to_rect(r: Rect) -> RECT {
    RECT {
        left: r.left,
        top: r.top,
        right: r.right,
        bottom: r.bottom,
    }
}

/// 札の高さ (サムネイルを置かない帯の高さ) の目安。
pub fn badge_height(dpi: u32, char_height: i32) -> i32 {
    char_height + scale(8, dpi)
}

/// 欄の地・枠・中央の文言・左下の札を描く。
pub fn paint_panel(
    hdc: HDC,
    panel: Rect,
    role: Role,
    shown: Shown,
    fonts: (HFONT, HFONT),
    dpi: u32,
) {
    let s = |v| scale(v, dpi);
    let (normal, bold) = fonts;
    // SAFETY: 描いた GDI の object は元に戻して消す
    unsafe {
        let background = CreateSolidBrush(PANEL);
        FillRect(hdc, &to_rect(panel), background);
        let _ = DeleteObject(background.into());

        let (color, width, style) = match role {
            Role::Empty => (LINE, 1, PS_DASH),
            Role::Candidate => (ORANGE, s(3), PS_SOLID),
            Role::Target => (BLUE, s(3), PS_SOLID),
        };
        let pen = CreatePen(style, width, color);
        let old_pen = SelectObject(hdc, pen.into());
        let old_brush = SelectObject(hdc, GetStockObject(NULL_BRUSH));
        let half = width / 2;
        let _ = Rectangle(
            hdc,
            panel.left + half,
            panel.top + half,
            panel.right - half,
            panel.bottom - half,
        );
        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        let _ = DeleteObject(pen.into());

        SetBkMode(hdc, TRANSPARENT);
        let message = match (role, shown) {
            (Role::Empty, _) => Some("打つ先を選ぶと、ここに映る"),
            (_, Shown::Minimized) => Some("最小化しているので映りません (打つときは元に戻す)"),
            (_, Shown::Unavailable) => Some("この窓は映せません"),
            _ => None,
        };
        if let Some(message) = message {
            let old_font = SelectObject(hdc, normal.into());
            SetTextColor(hdc, MUTED);
            let mut text: Vec<u16> = message.encode_utf16().collect();
            let mut rect = to_rect(panel);
            DrawTextW(
                hdc,
                &mut text,
                &mut rect,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
            );
            SelectObject(hdc, old_font);
        }

        let (label, fill, ink) = match role {
            Role::Empty => return,
            Role::Candidate => ("候補", ORANGE, BLACK),
            Role::Target => ("打つ先", BLUE, WHITE),
        };
        let old_font = SelectObject(hdc, bold.into());
        let mut text: Vec<u16> = label.encode_utf16().collect();
        let mut measure = RECT::default();
        DrawTextW(
            hdc,
            &mut text,
            &mut measure,
            DT_CALCRECT | DT_SINGLELINE | DT_NOPREFIX,
        );
        let dot = if role == Role::Target { s(7) } else { 0 };
        let dot_gap = if dot > 0 { s(5) } else { 0 };
        let pad = s(8);
        let height = (measure.bottom - measure.top) + s(4);
        let width = (measure.right - measure.left) + pad * 2 + dot + dot_gap;
        let left = panel.left + s(6);
        let bottom = panel.bottom - s(6);
        let top = bottom - height;

        let brush = CreateSolidBrush(fill);
        let old_brush = SelectObject(hdc, brush.into());
        let old_pen = SelectObject(hdc, GetStockObject(NULL_PEN));
        let _ = RoundRect(hdc, left, top, left + width, bottom, height, height);
        SelectObject(hdc, old_brush);
        let _ = DeleteObject(brush.into());

        SetTextColor(hdc, ink);
        let mut rect = RECT {
            left: left + pad,
            top,
            right: left + width - pad - dot - dot_gap,
            bottom,
        };
        DrawTextW(
            hdc,
            &mut text,
            &mut rect,
            DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
        );
        if dot > 0 {
            let red = CreateSolidBrush(RED);
            let old = SelectObject(hdc, red.into());
            let x = rect.right + dot_gap;
            let y = top + (height - dot) / 2;
            let _ = Ellipse(hdc, x, y, x + dot, y + dot);
            SelectObject(hdc, old);
            let _ = DeleteObject(red.into());
        }
        SelectObject(hdc, old_pen);
        SelectObject(hdc, old_font);
    }
}
