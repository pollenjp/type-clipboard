//! 主窓のコントロール: 作る・並べる・フォント・小さな操作。

use std::ffi::c_void;

use type_clipboard::geometry::Rect;
use type_clipboard::keymap::Layout;
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateFontIndirectW, DeleteObject, FW_BOLD, HFONT, InvalidateRect, LOGFONTW,
};
use windows::Win32::System::SystemServices::{
    SS_CENTERIMAGE, SS_ENDELLIPSIS, SS_LEFT, SS_NOPREFIX,
};
use windows::Win32::UI::Controls::{
    LIST_VIEW_ITEM_STATE_FLAGS, LVCF_TEXT, LVCF_WIDTH, LVCOLUMNW, LVHITTESTINFO, LVIF_TEXT,
    LVIS_FOCUSED, LVIS_SELECTED, LVITEMW, LVM_DELETEALLITEMS, LVM_ENSUREVISIBLE, LVM_HITTEST,
    LVM_INSERTCOLUMNW, LVM_INSERTITEMW, LVM_SETCOLUMNWIDTH, LVM_SETEXTENDEDLISTVIEWSTYLE,
    LVM_SETITEMSTATE, LVM_SETITEMTEXTW, LVS_EX_DOUBLEBUFFER, LVS_EX_FULLROWSELECT,
    LVS_NOSORTHEADER, LVS_REPORT, LVS_SHOWSELALWAYS, LVS_SINGLESEL, PBM_SETPOS, PBM_SETRANGE32,
    PBM_SETSTATE, PROGRESS_CLASSW, UDM_SETBUDDY, UDM_SETPOS32, UDM_SETRANGE32, UDS_ALIGNRIGHT,
    UDS_ARROWKEYS, UDS_NOTHOUSANDS, UDS_SETBUDDYINT, UPDOWN_CLASSW, WC_BUTTONW, WC_COMBOBOXW,
    WC_EDITW, WC_LISTVIEWW, WC_STATICW,
};
use windows::Win32::UI::HiDpi::SystemParametersInfoForDpi;
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, BM_GETCHECK, BM_SETCHECK, BS_AUTOCHECKBOX, BS_AUTORADIOBUTTON, BS_PUSHBUTTON,
    CB_ADDSTRING, CB_GETCURSEL, CB_SETCURSEL, CBS_DROPDOWNLIST, CreateMenu, CreatePopupMenu,
    CreateWindowExW, ES_AUTOHSCROLL, ES_NUMBER, GetWindowTextW, HMENU, MF_POPUP, MF_STRING,
    NONCLIENTMETRICSW, SPI_GETNONCLIENTMETRICS, SWP_NOACTIVATE, SWP_NOZORDER, SendMessageW,
    SetWindowPos, SetWindowTextW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_SETFONT, WS_CHILD,
    WS_EX_CLIENTEDGE, WS_GROUP, WS_TABSTOP, WS_VISIBLE,
};
use windows::core::{PCWSTR, PWSTR};

pub const ID_REFRESH: u16 = 101;
pub const ID_PICK: u16 = 102;
pub const ID_LIST: u16 = 103;
pub const ID_MODE_KEY: u16 = 104;
pub const ID_MODE_UNICODE: u16 = 105;
pub const ID_LAYOUT: u16 = 106;
pub const ID_INTERVAL: u16 = 107;
pub const ID_INTERVAL_SPIN: u16 = 108;
pub const ID_TOPMOST: u16 = 109;
pub const ID_TYPE: u16 = 110;
pub const ID_TRIAL: u16 = 111;
/// メニューの「バージョン情報」。子の窓の id とも、IsDialogMessage が Enter・Esc で送る IDOK・IDCANCEL (1・2) とも
/// 重ねない (重ねると、そのボタンやキーでも開く)。
pub const ID_ABOUT: u16 = 201;
const ID_STATIC: u16 = 0xFFFF;

/// 候補 (橙)
pub const ORANGE: COLORREF = rgb(0xF5, 0x9E, 0x0B);
/// 打つ先 (青)
pub const BLUE: COLORREF = rgb(0x25, 0x63, 0xEB);
/// 打ち終わり (緑)
pub const GREEN: COLORREF = rgb(0x15, 0x80, 0x3D);
/// 止めた・断った (茶)
pub const AMBER: COLORREF = rgb(0xB4, 0x53, 0x09);
/// 札の点 (赤)
pub const RED: COLORREF = rgb(0xEF, 0x44, 0x44);
/// 控えめな文字 (灰)
pub const MUTED: COLORREF = rgb(0x6B, 0x6B, 0x6B);
/// 空の欄の枠 (薄い灰)
pub const LINE: COLORREF = rgb(0xC9, 0xCE, 0xD6);
/// プレビューの欄の地
pub const PANEL: COLORREF = rgb(0xFA, 0xFA, 0xFA);
pub const BLACK: COLORREF = rgb(0x11, 0x11, 0x11);
pub const WHITE: COLORREF = rgb(0xFF, 0xFF, 0xFF);

pub const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(r as u32 | (g as u32) << 8 | (b as u32) << 16)
}

/// 96 dpi での長さを、今の dpi の pixel にする。
pub fn scale(value: i32, dpi: u32) -> i32 {
    (i64::from(value) * i64::from(dpi) / 96) as i32
}

/// NUL で終わる UTF-16。
pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

pub fn set_text(hwnd: HWND, text: &str) {
    let text = wide(text);
    // SAFETY: text は呼ぶ間だけ生きていればよい
    let _ = unsafe { SetWindowTextW(hwnd, PCWSTR(text.as_ptr())) };
}

pub fn text_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 64];
    // SAFETY: buf の長さを渡している
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..usize::try_from(n).unwrap_or(0)])
}

pub fn enable(hwnd: HWND, on: bool) {
    // SAFETY: 窓の状態を変えるだけ
    let _ = unsafe { EnableWindow(hwnd, on) };
}

pub fn is_checked(hwnd: HWND) -> bool {
    // SAFETY: button への問い合わせ
    unsafe { SendMessageW(hwnd, BM_GETCHECK, None, None) }.0 == 1
}

pub fn set_checked(hwnd: HWND, on: bool) {
    // SAFETY: button への指示
    unsafe { SendMessageW(hwnd, BM_SETCHECK, Some(WPARAM(usize::from(on))), None) };
}

fn place(hwnd: HWND, x: i32, y: i32, width: i32, height: i32) {
    // SAFETY: 子の窓を動かすだけ
    let _ = unsafe {
        SetWindowPos(
            hwnd,
            None,
            x,
            y,
            width.max(0),
            height.max(0),
            SWP_NOZORDER | SWP_NOACTIVATE,
        )
    };
}

/// 主窓のメニューの帯: 「ヘルプ」の中に「バージョン情報」。窓に付けたメニューは、窓と一緒に消える。
pub fn menu_bar() -> windows::core::Result<HMENU> {
    let help_text = wide("ヘルプ(&H)");
    let about_text = wide("バージョン情報(&A)");
    // SAFETY: メニューを作って項目を足すだけ。文字列は呼ぶ間だけ生きていればよい。作れなければ起動しないので、
    // 途中まで作ったメニューは消さない (プロセスと一緒に消える)
    unsafe {
        let help = CreatePopupMenu()?;
        AppendMenuW(
            help,
            MF_STRING,
            usize::from(ID_ABOUT),
            PCWSTR(about_text.as_ptr()),
        )?;
        let bar = CreateMenu()?;
        AppendMenuW(bar, MF_POPUP, help.0 as usize, PCWSTR(help_text.as_ptr()))?;
        Ok(bar)
    }
}

/// 主窓のコントロール。
pub struct Controls {
    pub label_target: HWND,
    pub refresh: HWND,
    pub pick: HWND,
    pub list: HWND,
    pub caption: HWND,
    pub label_mode: HWND,
    pub mode_key: HWND,
    pub mode_unicode: HWND,
    pub label_layout: HWND,
    pub layout: HWND,
    pub label_interval: HWND,
    pub interval: HWND,
    pub interval_spin: HWND,
    pub topmost: HWND,
    pub clip: HWND,
    pub type_button: HWND,
    pub trial: HWND,
    pub status: HWND,
    pub status_sub: HWND,
    pub progress: HWND,
}

fn child(
    parent: HWND,
    instance: HINSTANCE,
    class: PCWSTR,
    text: &str,
    style: WINDOW_STYLE,
    ex_style: WINDOW_EX_STYLE,
    id: u16,
) -> windows::core::Result<HWND> {
    let text = wide(text);
    // SAFETY: 文字列は呼ぶ間だけ生きていればよい。id は子の窓の識別子として HMENU に入れる
    unsafe {
        CreateWindowExW(
            ex_style,
            class,
            PCWSTR(text.as_ptr()),
            WS_CHILD | WS_VISIBLE | style,
            0,
            0,
            0,
            0,
            Some(parent),
            Some(HMENU(usize::from(id) as *mut c_void)),
            Some(instance),
            None,
        )
    }
}

impl Controls {
    pub fn create(parent: HWND, instance: HINSTANCE) -> windows::core::Result<Controls> {
        let make = |class, text, style, ex_style, id| {
            child(parent, instance, class, text, style, ex_style, id)
        };
        let none = WINDOW_EX_STYLE::default();
        let label = WINDOW_STYLE(SS_LEFT.0 | SS_NOPREFIX.0 | SS_ENDELLIPSIS.0);
        let row_label = WINDOW_STYLE(SS_LEFT.0 | SS_NOPREFIX.0 | SS_CENTERIMAGE.0);
        let button = WINDOW_STYLE(BS_PUSHBUTTON as u32) | WS_TABSTOP;
        let radio = WINDOW_STYLE(BS_AUTORADIOBUTTON as u32);
        let list_style =
            WINDOW_STYLE(LVS_REPORT | LVS_SINGLESEL | LVS_SHOWSELALWAYS | LVS_NOSORTHEADER);
        let spin_style =
            WINDOW_STYLE(UDS_SETBUDDYINT | UDS_ALIGNRIGHT | UDS_ARROWKEYS | UDS_NOTHOUSANDS);
        let controls = Controls {
            label_target: make(WC_STATICW, "打つ先", row_label, none, ID_STATIC)?,
            refresh: make(WC_BUTTONW, "↻", button, none, ID_REFRESH)?,
            pick: make(WC_BUTTONW, "クリックで選ぶ", button, none, ID_PICK)?,
            list: make(
                WC_LISTVIEWW,
                "",
                list_style | WS_TABSTOP,
                WS_EX_CLIENTEDGE,
                ID_LIST,
            )?,
            caption: make(WC_STATICW, "", label, none, ID_STATIC)?,
            label_mode: make(WC_STATICW, "打ち方", label, none, ID_STATIC)?,
            mode_key: make(
                WC_BUTTONW,
                "キーの位置 (VNC・RDP 向け)",
                radio | WS_GROUP | WS_TABSTOP,
                none,
                ID_MODE_KEY,
            )?,
            mode_unicode: make(WC_BUTTONW, "文字 (Unicode)", radio, none, ID_MODE_UNICODE)?,
            label_layout: make(
                WC_STATICW,
                "打つ先の配列",
                row_label | WS_GROUP,
                none,
                ID_STATIC,
            )?,
            layout: make(
                WC_COMBOBOXW,
                "",
                WINDOW_STYLE(CBS_DROPDOWNLIST as u32) | WS_TABSTOP,
                none,
                ID_LAYOUT,
            )?,
            label_interval: make(
                WC_STATICW,
                "1 文字ごとの間隔 (ms)",
                row_label,
                none,
                ID_STATIC,
            )?,
            interval: make(
                WC_EDITW,
                "",
                WINDOW_STYLE((ES_NUMBER | ES_AUTOHSCROLL) as u32) | WS_TABSTOP,
                WS_EX_CLIENTEDGE,
                ID_INTERVAL,
            )?,
            interval_spin: make(UPDOWN_CLASSW, "", spin_style, none, ID_INTERVAL_SPIN)?,
            topmost: make(
                WC_BUTTONW,
                "常に手前",
                WINDOW_STYLE(BS_AUTOCHECKBOX as u32) | WS_TABSTOP,
                none,
                ID_TOPMOST,
            )?,
            clip: make(WC_STATICW, "クリップボード: 空", label, none, ID_STATIC)?,
            type_button: make(WC_BUTTONW, "クリップボードを打つ", button, none, ID_TYPE)?,
            trial: make(WC_BUTTONW, "試し打ち", button, none, ID_TRIAL)?,
            status: make(WC_STATICW, "", label, none, ID_STATIC)?,
            // 試し打ちの見本を折り返して全部見せるため、省略しない
            status_sub: make(
                WC_STATICW,
                "",
                WINDOW_STYLE(SS_LEFT.0 | SS_NOPREFIX.0),
                none,
                ID_STATIC,
            )?,
            progress: make(PROGRESS_CLASSW, "", WINDOW_STYLE(0), none, ID_STATIC)?,
        };
        controls.init_list();
        controls.init_layout_choices();
        // SAFETY: up-down への指示 (範囲と相方の edit)
        unsafe {
            SendMessageW(
                controls.interval_spin,
                UDM_SETRANGE32,
                Some(WPARAM(0)),
                Some(LPARAM(type_clipboard::settings::MAX_INTERVAL_MS as isize)),
            );
            SendMessageW(
                controls.interval_spin,
                UDM_SETBUDDY,
                Some(WPARAM(controls.interval.0 as usize)),
                None,
            );
        }
        Ok(controls)
    }

    fn init_list(&self) {
        let extended = (LVS_EX_FULLROWSELECT | LVS_EX_DOUBLEBUFFER) as isize;
        // SAFETY: ListView への指示。列の名前は呼ぶ間だけ生きていればよい
        unsafe {
            SendMessageW(
                self.list,
                LVM_SETEXTENDEDLISTVIEWSTYLE,
                Some(WPARAM(extended as usize)),
                Some(LPARAM(extended)),
            );
            for (index, name) in ["窓の題", "exe"].into_iter().enumerate() {
                let mut text = wide(name);
                let column = LVCOLUMNW {
                    mask: LVCF_TEXT | LVCF_WIDTH,
                    cx: 100,
                    pszText: PWSTR(text.as_mut_ptr()),
                    ..Default::default()
                };
                SendMessageW(
                    self.list,
                    LVM_INSERTCOLUMNW,
                    Some(WPARAM(index)),
                    Some(LPARAM(&column as *const LVCOLUMNW as isize)),
                );
            }
        }
    }

    fn init_layout_choices(&self) {
        for layout in Layout::ALL {
            let text = wide(layout.label());
            // SAFETY: combobox への指示。文字列は呼ぶ間だけ生きていればよい
            unsafe {
                SendMessageW(
                    self.layout,
                    CB_ADDSTRING,
                    None,
                    Some(LPARAM(text.as_ptr() as isize)),
                )
            };
        }
    }

    /// 打っている間は、打つ先や打ち方を変えられないようにする。
    pub fn enable_inputs(&self, on: bool, mode_is_key: bool) {
        for hwnd in [
            self.refresh,
            self.pick,
            self.list,
            self.mode_key,
            self.mode_unicode,
            self.interval,
            self.interval_spin,
            self.type_button,
            self.trial,
        ] {
            enable(hwnd, on);
        }
        self.enable_layout(on && mode_is_key);
    }

    /// 配列は「キーの位置」の方式のときだけ使う。
    pub fn enable_layout(&self, on: bool) {
        enable(self.label_layout, on);
        enable(self.layout, on);
    }

    pub fn set_fonts(&self, fonts: &Fonts) {
        let normal = WPARAM(fonts.normal.0 as usize);
        for hwnd in [
            self.label_target,
            self.refresh,
            self.pick,
            self.list,
            self.caption,
            self.label_mode,
            self.mode_key,
            self.mode_unicode,
            self.label_layout,
            self.layout,
            self.label_interval,
            self.interval,
            self.topmost,
            self.clip,
            self.type_button,
            self.trial,
            self.status_sub,
        ] {
            // SAFETY: font は Fonts が持っていて、差し替えるまで消さない
            unsafe { SendMessageW(hwnd, WM_SETFONT, Some(normal), Some(LPARAM(1))) };
        }
        let bold = WPARAM(fonts.bold.0 as usize);
        for hwnd in [self.status, self.label_target, self.label_mode] {
            // SAFETY: 同上
            unsafe { SendMessageW(hwnd, WM_SETFONT, Some(bold), Some(LPARAM(1))) };
        }
    }

    pub fn selected_layout(&self) -> Layout {
        // SAFETY: combobox への問い合わせ
        let index = unsafe { SendMessageW(self.layout, CB_GETCURSEL, None, None) }.0;
        usize::try_from(index)
            .ok()
            .and_then(|i| Layout::ALL.get(i).copied())
            .unwrap_or(type_clipboard::settings::DEFAULT_LAYOUT)
    }

    pub fn select_layout(&self, layout: Layout) {
        let index = Layout::ALL.iter().position(|l| *l == layout).unwrap_or(0);
        // SAFETY: combobox への指示
        unsafe { SendMessageW(self.layout, CB_SETCURSEL, Some(WPARAM(index)), None) };
    }

    pub fn set_interval(&self, ms: u32) {
        // SAFETY: up-down への指示
        unsafe {
            SendMessageW(
                self.interval_spin,
                UDM_SETPOS32,
                Some(WPARAM(0)),
                Some(LPARAM(ms as isize)),
            )
        };
    }

    /// 間隔の欄の値。読めなければ既定、上限を超えれば上限。
    pub fn interval_ms(&self) -> u32 {
        text_of(self.interval)
            .trim()
            .parse::<u32>()
            .map(|ms| ms.min(type_clipboard::settings::MAX_INTERVAL_MS))
            .unwrap_or(type_clipboard::settings::DEFAULT_INTERVAL_MS)
    }

    pub fn set_progress(&self, position: usize, total: usize, state: u32) {
        // SAFETY: progress bar への指示
        unsafe {
            SendMessageW(
                self.progress,
                PBM_SETRANGE32,
                Some(WPARAM(0)),
                Some(LPARAM(total.max(1) as isize)),
            );
            SendMessageW(
                self.progress,
                PBM_SETSTATE,
                Some(WPARAM(state as usize)),
                None,
            );
            SendMessageW(self.progress, PBM_SETPOS, Some(WPARAM(position)), None);
        }
    }

    /// 並べる。プレビューの欄 (コントロールではなく主窓が描く) の矩形を返す。
    pub fn arrange(&self, client: RECT, dpi: u32, char_height: i32) -> Rect {
        let s = |v| scale(v, dpi);
        let margin = s(10);
        let gap = s(6);
        let line = (char_height + s(8)).max(s(20));
        let button = (char_height + s(14)).max(s(26));
        let x = margin;
        let width = (client.right - client.left - margin * 2).max(s(200));
        let mut y = margin;

        let pick_width = s(124);
        let refresh_width = s(36);
        place(
            self.label_target,
            x,
            y,
            width - pick_width - refresh_width - gap * 2,
            button,
        );
        place(
            self.refresh,
            x + width - pick_width - gap - refresh_width,
            y,
            refresh_width,
            button,
        );
        place(self.pick, x + width - pick_width, y, pick_width, button);
        y += button + gap;

        let list_height = line * 7;
        place(self.list, x, y, width, list_height);
        let exe_width = s(120);
        // SAFETY: ListView への指示 (列の幅)
        unsafe {
            SendMessageW(
                self.list,
                LVM_SETCOLUMNWIDTH,
                Some(WPARAM(0)),
                Some(LPARAM((width - exe_width - s(28)).max(s(60)) as isize)),
            );
            SendMessageW(
                self.list,
                LVM_SETCOLUMNWIDTH,
                Some(WPARAM(1)),
                Some(LPARAM(exe_width as isize)),
            );
        }
        y += list_height + gap;

        let button_row = button + s(4);
        let below = line + gap // 窓の題
            + line * 3 + gap // 打ち方
            + (button + gap) * 2 // 配列・間隔
            + line + gap // 常に手前
            + line + gap // クリップボード
            + button_row + gap // ボタン
            + line + line * 2 + s(10) // 状態・進み具合
            + margin;
        let panel_height = (client.bottom - y - below).max(s(110));
        let panel = Rect {
            left: x,
            top: y,
            right: x + width,
            bottom: y + panel_height,
        };
        y += panel_height + s(2);

        place(self.caption, x, y, width, line);
        y += line + gap;
        place(self.label_mode, x, y, width, line);
        y += line;
        place(self.mode_key, x + s(8), y, width - s(8), line);
        y += line;
        place(self.mode_unicode, x + s(8), y, width - s(8), line);
        y += line + gap;

        let label_width = width * 45 / 100;
        place(self.label_layout, x, y, label_width, button);
        place(
            self.layout,
            x + label_width,
            y,
            (width - label_width).min(s(170)),
            button + s(120),
        );
        y += button + gap;
        place(self.label_interval, x, y, label_width, button);
        place(self.interval, x + label_width, y, s(90), button);
        // edit を動かしたので、up-down を付け直す
        // SAFETY: up-down への指示
        unsafe {
            SendMessageW(
                self.interval_spin,
                UDM_SETBUDDY,
                Some(WPARAM(self.interval.0 as usize)),
                None,
            )
        };
        y += button + gap;
        place(self.topmost, x, y, width, line);
        y += line + gap;
        place(self.clip, x, y, width, line);
        y += line + gap;

        let trial_width = s(96);
        place(
            self.type_button,
            x,
            y,
            width - trial_width - gap,
            button_row,
        );
        place(
            self.trial,
            x + width - trial_width,
            y,
            trial_width,
            button_row,
        );
        y += button_row + gap;
        place(self.status, x, y, width, line);
        y += line;
        place(self.status_sub, x, y, width, line * 2);
        y += line * 2;
        place(self.progress, x, y + s(2), width, s(8));
        panel
    }

    /// この窓の最小の大きさ (client の高さ) の目安。
    pub fn min_client_height(dpi: u32, char_height: i32) -> i32 {
        let s = |v| scale(v, dpi);
        let line = (char_height + s(8)).max(s(20));
        let button = (char_height + s(14)).max(s(26));
        s(10) * 2 + button + line * 7 + s(110) + line * 14 + button * 3 + s(80)
    }
}

/// ListView の行の操作。
pub mod list {
    use super::*;

    pub fn clear(list: HWND) {
        // SAFETY: ListView への指示
        unsafe { SendMessageW(list, LVM_DELETEALLITEMS, None, None) };
    }

    pub fn insert(list: HWND, index: usize, title: &str, exe: &str) {
        let mut title = wide(title);
        let mut exe = wide(exe);
        let mut item = LVITEMW {
            mask: LVIF_TEXT,
            iItem: index as i32,
            pszText: PWSTR(title.as_mut_ptr()),
            ..Default::default()
        };
        // SAFETY: 文字列は呼ぶ間だけ生きていればよい
        unsafe {
            let row = SendMessageW(
                list,
                LVM_INSERTITEMW,
                None,
                Some(LPARAM(&item as *const LVITEMW as isize)),
            )
            .0;
            item.iSubItem = 1;
            item.pszText = PWSTR(exe.as_mut_ptr());
            SendMessageW(
                list,
                LVM_SETITEMTEXTW,
                Some(WPARAM(row as usize)),
                Some(LPARAM(&item as *const LVITEMW as isize)),
            );
        }
    }

    /// `index` の行だけを選ぶ (`None` なら選ばない)。
    pub fn select_only(list: HWND, index: Option<usize>) {
        let mask = LIST_VIEW_ITEM_STATE_FLAGS(LVIS_SELECTED.0 | LVIS_FOCUSED.0);
        let clear = LVITEMW {
            stateMask: mask,
            ..Default::default()
        };
        // SAFETY: ListView への指示 (-1 は全部の行)
        unsafe {
            SendMessageW(
                list,
                LVM_SETITEMSTATE,
                Some(WPARAM(usize::MAX)),
                Some(LPARAM(&clear as *const LVITEMW as isize)),
            );
        }
        if let Some(index) = index {
            let select = LVITEMW {
                stateMask: mask,
                state: mask,
                ..Default::default()
            };
            // SAFETY: 同上
            unsafe {
                SendMessageW(
                    list,
                    LVM_SETITEMSTATE,
                    Some(WPARAM(index)),
                    Some(LPARAM(&select as *const LVITEMW as isize)),
                );
                SendMessageW(
                    list,
                    LVM_ENSUREVISIBLE,
                    Some(WPARAM(index)),
                    Some(LPARAM(0)),
                );
            }
        }
    }

    /// client の座標 `point` にある行。
    pub fn hit(list: HWND, point: POINT) -> Option<usize> {
        let mut info = LVHITTESTINFO {
            pt: point,
            ..Default::default()
        };
        // SAFETY: info は呼ぶ間だけ生きていればよい
        let row = unsafe {
            SendMessageW(
                list,
                LVM_HITTEST,
                None,
                Some(LPARAM(&mut info as *mut LVHITTESTINFO as isize)),
            )
        }
        .0;
        usize::try_from(row).ok()
    }
}

/// 画面の文字のフォント (Windows の UI のフォント)。dpi ごとに作り直す。
pub struct Fonts {
    pub normal: HFONT,
    pub bold: HFONT,
    /// 文字の高さ (pixel)
    pub char_height: i32,
}

impl Fonts {
    pub fn new(dpi: u32) -> Fonts {
        let mut metrics = NONCLIENTMETRICSW {
            cbSize: size_of::<NONCLIENTMETRICSW>() as u32,
            ..Default::default()
        };
        // SAFETY: metrics の大きさを cbSize で渡している
        let ok = unsafe {
            SystemParametersInfoForDpi(
                SPI_GETNONCLIENTMETRICS.0,
                metrics.cbSize,
                Some(&mut metrics as *mut NONCLIENTMETRICSW as *mut c_void),
                0,
                dpi,
            )
        }
        .is_ok();
        let mut font = if ok {
            metrics.lfMessageFont
        } else {
            let mut font = LOGFONTW {
                lfHeight: -scale(12, dpi),
                ..Default::default()
            };
            for (dst, src) in font
                .lfFaceName
                .iter_mut()
                .zip("Yu Gothic UI".encode_utf16())
            {
                *dst = src;
            }
            font
        };
        // SAFETY: font は有効な LOGFONTW
        let normal = unsafe { CreateFontIndirectW(&font) };
        font.lfWeight = FW_BOLD.0 as i32;
        // SAFETY: 同上
        let bold = unsafe { CreateFontIndirectW(&font) };
        Fonts {
            normal,
            bold,
            char_height: font.lfHeight.abs().max(scale(12, dpi)),
        }
    }
}

impl Drop for Fonts {
    fn drop(&mut self) {
        // SAFETY: このフォントを使うコントロールには、先に新しいフォントを渡してある
        unsafe {
            let _ = DeleteObject(self.normal.into());
            let _ = DeleteObject(self.bold.into());
        }
    }
}

/// 主窓の一部を描き直す。
pub fn invalidate(hwnd: HWND, rect: Option<Rect>) {
    let rect = rect.map(|r| RECT {
        left: r.left,
        top: r.top,
        right: r.right,
        bottom: r.bottom,
    });
    // SAFETY: rect は呼ぶ間だけ生きていればよい
    let _ = unsafe { InvalidateRect(Some(hwnd), rect.as_ref().map(|r| r as *const RECT), true) };
}
