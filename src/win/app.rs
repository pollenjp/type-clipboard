//! 主窓: 状態と window procedure、操作への応答。

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;

use type_clipboard::candidate::{Elevation, is_pickable};
use type_clipboard::geometry::{Rect, unpack_point};
use type_clipboard::secret::{self, StackLock};
use type_clipboard::settings::{self, Settings, WindowRect};
use type_clipboard::status::{self, Kind, Status};
use type_clipboard::typing::{Mode, Outcome, Refusal, TEST_SAMPLE, plan, typeable_len};
use windows::Win32::Foundation::{
    COLORREF, E_OUTOFMEMORY, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, COLOR_WINDOW, COLOR_WINDOWTEXT, EndPaint, GetSysColor, GetSysColorBrush, HBRUSH,
    HDC, MONITOR_DEFAULTTONULL, MonitorFromRect, PAINTSTRUCT, SetBkMode, SetTextColor, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{
    ICC_LISTVIEW_CLASSES, ICC_PROGRESS_CLASS, ICC_STANDARD_CLASSES, ICC_UPDOWN_CLASS,
    INITCOMMONCONTROLSEX, InitCommonControlsEx, LVIS_SELECTED, LVN_ITEMCHANGED, NMHDR, NMLISTVIEW,
    PBST_NORMAL, PBST_PAUSED, WM_MOUSELEAVE,
};
use windows::Win32::UI::HiDpi::{GetDpiForSystem, GetDpiForWindow};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SetFocus, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    BN_CLICKED, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, CreateWindowExW, DefWindowProcW,
    DispatchMessageW, GA_ROOT, GWLP_USERDATA, GetAncestor, GetClientRect, GetMessageW,
    GetWindowLongPtrW, GetWindowPlacement, HWND_NOTOPMOST, HWND_TOPMOST, IDC_ARROW,
    IsDialogMessageW, IsIconic, IsWindow, KillTimer, LoadCursorW, MINMAXINFO, MSG, PM_NOREMOVE,
    PeekMessageW, PostQuitMessage, RegisterClassExW, SW_RESTORE, SW_SHOWNORMAL, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetForegroundWindow, SetTimer, SetWindowLongPtrW,
    SetWindowPlacement, SetWindowPos, ShowWindow, TranslateMessage, WA_ACTIVE, WINDOWPLACEMENT,
    WM_ACTIVATE, WM_CLIPBOARDUPDATE, WM_COMMAND, WM_CTLCOLORSTATIC, WM_DESTROY, WM_DPICHANGED,
    WM_GETMINMAXINFO, WM_MOUSEMOVE, WM_NCDESTROY, WM_NOTIFY, WM_PAINT, WM_SIZE, WM_TIMER,
    WNDCLASSEXW, WS_CLIPCHILDREN, WS_EX_CONTROLPARENT, WS_OVERLAPPEDWINDOW, WindowFromPoint,
};
use windows::core::{PCWSTR, w};

use super::clipboard::{self, ClipText};
use super::frame::Frame;
use super::hooks::{self, HookGuard, HookMode};
use super::preview::{self, Preview, Role, Shown};
use super::target::{self, WindowEntry};
use super::typer::{self, Job};
use super::ui::{self, AMBER, BLUE, Controls, Fonts, GREEN, MUTED, ORANGE, scale};
use super::{
    WM_APP_DONE, WM_APP_PICK_CANCEL, WM_APP_PICK_CLICK, WM_APP_PICK_MOVE, WM_APP_PICK_RELEASED,
    WM_APP_PROGRESS,
};

const CLASS_NAME: PCWSTR = w!("TypeClipboardMain");

/// 打つ先が閉じていないか・サムネイルの大きさが変わっていないかを見る
const TIMER_POLL: usize = 1;
const POLL_MS: u32 = 500;
/// 打つ先を決めた直後に、青の枠を少しだけ出す
const TIMER_FLASH: usize = 2;
const FLASH_MS: u32 = 700;
/// 「クリックで選ぶ」のクリックのボタンを離すのを待つ上限
const TIMER_PICK_RELEASE: usize = 3;
const PICK_RELEASE_MS: u32 = 2000;

const LIST_SUBCLASS_ID: usize = 1;

/// 打っている最中の仕事の控え。
#[derive(Clone, Copy)]
struct Run {
    total: usize,
    trial: bool,
    /// 打った文字の数 (進み具合の知らせで更新する)
    typed: usize,
}

struct App {
    hwnd: HWND,
    ui: Controls,
    fonts: RefCell<Fonts>,
    dpi: Cell<u32>,
    preview: Preview,
    frame: Frame,
    settings_path: Option<PathBuf>,
    /// プレビューの欄 (主窓の client の座標)
    panel: Cell<Rect>,
    shown: Cell<Shown>,
    entries: RefCell<Vec<WindowEntry>>,
    target: Cell<Option<HWND>>,
    candidate: Cell<Option<HWND>>,
    picking: Cell<bool>,
    flashing: Cell<bool>,
    hooks: RefCell<Option<HookGuard>>,
    run: Cell<Option<Run>>,
    abort: RefCell<Arc<AtomicBool>>,
    worker: RefCell<Option<JoinHandle<()>>>,
    /// ListView の選択を自分で変えている間は、打つ先を決めたとみなさない
    suppress_list: Cell<bool>,
    tracking_list_mouse: Cell<bool>,
    status_kind: Cell<Kind>,
    /// 秘密に触れた後 (数えた・打つ列を作った・打ち終えた) に、message loop で UI の thread の stack を上書きする
    scrub_pending: Cell<bool>,
}

/// 主窓を作り、閉じられるまで message loop を回す。
pub fn run() -> windows::core::Result<()> {
    // この thread (UI の thread) の stack のうち使うあたりを、閉じるまで物理メモリに留める。打つ列を作るときに
    // decode した文字や、打っている間に hook が受け取る (このアプリが送った) キーが乗るため
    let _stack = StackLock::here().map_err(|_| {
        windows::core::Error::new(
            E_OUTOFMEMORY,
            "UI の thread の stack を物理メモリに留められませんでした",
        )
    })?;
    // SAFETY: この thread (UI の thread) で窓を作り、message を回す
    unsafe {
        let instance: HINSTANCE = GetModuleHandleW(None)?.into();
        let controls = INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_LISTVIEW_CLASSES
                | ICC_PROGRESS_CLASS
                | ICC_UPDOWN_CLASS
                | ICC_STANDARD_CLASSES,
        };
        let _ = InitCommonControlsEx(&controls);
        register_class(instance)?;
        Frame::register_class(instance)?;

        let settings_path = settings::default_path();
        let saved = settings_path
            .as_deref()
            .map(settings::load)
            .unwrap_or_default();
        let dpi = GetDpiForSystem();
        let hwnd = CreateWindowExW(
            WS_EX_CONTROLPARENT,
            CLASS_NAME,
            w!("type-clipboard"),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            scale(400, dpi),
            scale(800, dpi),
            None,
            None,
            Some(instance),
            None,
        )?;
        let app = Box::new(App::new(hwnd, instance, settings_path)?);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, &*app as *const App as isize);
        app.start(&saved);

        let mut msg = MSG::default();
        loop {
            // 秘密に触れた処理 (window procedure と hook) の frame に残った写しを、それらより上の
            // この loop から上書きする
            if app.scrub_pending.take() {
                secret::scrub_stack();
            }
            let got = GetMessageW(&mut msg, None, 0, 0).0;
            if got == 0 || got == -1 {
                break;
            }
            if IsDialogMessageW(hwnd, &msg).as_bool() {
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        drop(app);
        Ok(())
    }
}

fn register_class(instance: HINSTANCE) -> windows::core::Result<()> {
    // SAFETY: class は呼ぶ間だけ生きていればよい
    unsafe {
        let class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as usize as *mut c_void),
            lpszClassName: CLASS_NAME,
            ..Default::default()
        };
        if RegisterClassExW(&class) == 0 {
            return Err(windows::core::Error::from_thread());
        }
        Ok(())
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: GWLP_USERDATA には run() の App を入れてあり、窓が消える (WM_NCDESTROY) まで生きている
    unsafe {
        let app = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const App;
        if msg == WM_NCDESTROY {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        } else if let Some(app) = app.as_ref()
            && let Some(result) = app.handle(msg, wparam, lparam)
        {
            return result;
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

unsafe extern "system" fn list_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    data: usize,
) -> LRESULT {
    // SAFETY: data は App の pointer で、App は ListView (主窓の子) より長く生きる
    unsafe {
        let app = &*(data as *const App);
        match msg {
            WM_MOUSEMOVE => app.on_list_hover(lparam),
            WM_MOUSELEAVE => app.on_list_leave(),
            WM_NCDESTROY => {
                let _ = RemoveWindowSubclass(hwnd, Some(list_proc), LIST_SUBCLASS_ID);
            }
            _ => {}
        }
        DefSubclassProc(hwnd, msg, wparam, lparam)
    }
}

/// hook が LPARAM に載せた画面の座標の点。
fn point_from(lparam: LPARAM) -> POINT {
    let (x, y) = unpack_point(lparam.0);
    POINT { x, y }
}

fn loword(value: usize) -> u16 {
    (value & 0xFFFF) as u16
}

fn hiword(value: usize) -> u16 {
    ((value >> 16) & 0xFFFF) as u16
}

impl App {
    fn new(
        hwnd: HWND,
        instance: HINSTANCE,
        settings_path: Option<PathBuf>,
    ) -> windows::core::Result<App> {
        // SAFETY: 窓の dpi を読むだけ
        let dpi = unsafe { GetDpiForWindow(hwnd) };
        let ui = Controls::create(hwnd, instance)?;
        let fonts = Fonts::new(dpi);
        ui.set_fonts(&fonts);
        Ok(App {
            hwnd,
            ui,
            fonts: RefCell::new(fonts),
            dpi: Cell::new(dpi),
            preview: Preview::new(hwnd),
            frame: Frame::new(instance)?,
            settings_path,
            panel: Cell::new(Rect::default()),
            shown: Cell::new(Shown::Nothing),
            entries: RefCell::new(Vec::new()),
            target: Cell::new(None),
            candidate: Cell::new(None),
            picking: Cell::new(false),
            flashing: Cell::new(false),
            hooks: RefCell::new(None),
            run: Cell::new(None),
            abort: RefCell::new(Arc::new(AtomicBool::new(false))),
            worker: RefCell::new(None),
            suppress_list: Cell::new(false),
            tracking_list_mouse: Cell::new(false),
            status_kind: Cell::new(Kind::Info),
            scrub_pending: Cell::new(false),
        })
    }

    /// 設定を当て、窓を出す。
    fn start(&self, saved: &Settings) {
        // SAFETY: self は Box の中にあり、ListView より長く生きる
        unsafe {
            let _ = SetWindowSubclass(
                self.ui.list,
                Some(list_proc),
                LIST_SUBCLASS_ID,
                self as *const App as usize,
            );
        }
        let mode_is_key = saved.mode == Mode::KeyPosition;
        ui::set_checked(self.ui.mode_key, mode_is_key);
        ui::set_checked(self.ui.mode_unicode, !mode_is_key);
        self.ui.select_layout(saved.layout);
        self.ui.enable_layout(mode_is_key);
        self.ui.set_interval(saved.interval_ms);
        ui::set_checked(self.ui.topmost, saved.always_on_top);
        self.ui.set_progress(0, 1, PBST_NORMAL);

        if !self.restore_placement(saved.window) {
            // SAFETY: 窓を出すだけ
            let _ = unsafe { ShowWindow(self.hwnd, SW_SHOWNORMAL) };
        }
        // 最前面は、窓を出してから当てる
        self.apply_topmost();
        self.arrange();
        self.refresh_list();
        self.update_clipboard();
        self.set_status(status::choose_target(Kind::Info));
        clipboard::listen(self.hwnd);
        // SAFETY: 主窓の timer
        unsafe { SetTimer(Some(self.hwnd), TIMER_POLL, POLL_MS, None) };
        // SAFETY: focus を一覧に置く
        let _ = unsafe { SetFocus(Some(self.ui.list)) };
    }

    /// 保存した位置が今の画面のどれかに入っていれば、そこに出す。
    fn restore_placement(&self, window: Option<WindowRect>) -> bool {
        let Some(r) = window else {
            return false;
        };
        let rect = RECT {
            left: r.x,
            top: r.y,
            right: r.x + r.width,
            bottom: r.y + r.height,
        };
        // SAFETY: rect を渡して、どの画面にあるかを問い合わせるだけ
        if unsafe { MonitorFromRect(&rect, MONITOR_DEFAULTTONULL) }.is_invalid() {
            return false;
        }
        let placement = WINDOWPLACEMENT {
            length: size_of::<WINDOWPLACEMENT>() as u32,
            showCmd: SW_SHOWNORMAL.0 as u32,
            rcNormalPosition: rect,
            ..Default::default()
        };
        // SAFETY: placement は呼ぶ間だけ生きていればよい
        unsafe { SetWindowPlacement(self.hwnd, &placement) }.is_ok()
    }

    fn handle(&self, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
        match msg {
            WM_SIZE => {
                self.arrange();
                Some(LRESULT(0))
            }
            WM_GETMINMAXINFO => {
                // SAFETY: lparam は MINMAXINFO を指す
                let info = unsafe { &mut *(lparam.0 as *mut MINMAXINFO) };
                let dpi = self.dpi.get();
                info.ptMinTrackSize = POINT {
                    x: scale(320, dpi),
                    y: Controls::min_client_height(dpi, self.fonts.borrow().char_height)
                        + scale(40, dpi),
                };
                Some(LRESULT(0))
            }
            WM_DPICHANGED => {
                self.on_dpi_changed(u32::from(hiword(wparam.0)), lparam);
                Some(LRESULT(0))
            }
            WM_PAINT => {
                self.paint();
                Some(LRESULT(0))
            }
            WM_COMMAND => {
                self.on_command(loword(wparam.0), u32::from(hiword(wparam.0)));
                Some(LRESULT(0))
            }
            WM_NOTIFY => {
                self.on_notify(lparam);
                None
            }
            WM_CTLCOLORSTATIC => Some(self.static_color(wparam, lparam)),
            WM_TIMER => {
                self.on_timer(wparam.0);
                Some(LRESULT(0))
            }
            WM_CLIPBOARDUPDATE => {
                self.update_clipboard();
                Some(LRESULT(0))
            }
            WM_ACTIVATE => {
                // キーボード (Alt+Tab など) で前に来たときだけ読み直す。クリックで前に来たときに
                // 読み直すと、スクロールした一覧が先頭に戻り、クリックした行と違う窓を選んでしまう
                if u32::from(loword(wparam.0)) == WA_ACTIVE && self.is_idle() {
                    self.refresh_list();
                }
                None
            }
            WM_APP_PROGRESS => {
                self.on_progress(wparam.0);
                Some(LRESULT(0))
            }
            WM_APP_DONE => {
                self.on_done(wparam.0, usize::try_from(lparam.0).unwrap_or(0));
                Some(LRESULT(0))
            }
            WM_APP_PICK_MOVE => {
                self.on_pick_move(point_from(lparam));
                Some(LRESULT(0))
            }
            WM_APP_PICK_CLICK => {
                self.on_pick_click(point_from(lparam));
                Some(LRESULT(0))
            }
            WM_APP_PICK_CANCEL => {
                self.cancel_pick();
                Some(LRESULT(0))
            }
            WM_APP_PICK_RELEASED => {
                self.release_pick_hooks();
                Some(LRESULT(0))
            }
            WM_DESTROY => {
                self.shutdown();
                // SAFETY: message loop を終える
                unsafe { PostQuitMessage(0) };
                Some(LRESULT(0))
            }
            _ => None,
        }
    }

    fn is_idle(&self) -> bool {
        !self.picking.get() && self.run.get().is_none()
    }

    fn mode(&self) -> Mode {
        if ui::is_checked(self.ui.mode_unicode) {
            Mode::Unicode
        } else {
            Mode::KeyPosition
        }
    }

    fn s(&self, value: i32) -> i32 {
        scale(value, self.dpi.get())
    }

    // ---- 並べる・描く ----

    fn arrange(&self) {
        let mut client = RECT::default();
        // SAFETY: client に書くだけ
        let _ = unsafe { GetClientRect(self.hwnd, &mut client) };
        let char_height = self.fonts.borrow().char_height;
        let panel = self.ui.arrange(client, self.dpi.get(), char_height);
        self.panel.set(panel);
        self.place_thumbnail();
        ui::invalidate(self.hwnd, Some(panel));
    }

    /// サムネイルを置く欄 (札の帯を除いた内側)。
    fn thumbnail_area(&self) -> Rect {
        let panel = self.panel.get();
        let pad = self.s(8);
        let badge = preview::badge_height(self.dpi.get(), self.fonts.borrow().char_height);
        Rect {
            left: panel.left + pad,
            top: panel.top + pad,
            right: panel.right - pad,
            bottom: panel.bottom - pad - badge - self.s(4),
        }
    }

    fn place_thumbnail(&self) {
        let shown = self.preview.place(self.thumbnail_area());
        if shown != self.shown.replace(shown) {
            ui::invalidate(self.hwnd, Some(self.panel.get()));
        }
    }

    fn paint(&self) {
        let mut paint = PAINTSTRUCT::default();
        // SAFETY: BeginPaint と EndPaint の組
        unsafe {
            let hdc = BeginPaint(self.hwnd, &mut paint);
            let fonts = self.fonts.borrow();
            preview::paint_panel(
                hdc,
                self.panel.get(),
                self.role(),
                self.shown.get(),
                (fonts.normal, fonts.bold),
                self.dpi.get(),
            );
            let _ = EndPaint(self.hwnd, &paint);
        }
    }

    fn role(&self) -> Role {
        match (self.candidate.get(), self.target.get()) {
            (Some(candidate), target) if Some(candidate) != target => Role::Candidate,
            (_, Some(_)) => Role::Target,
            _ => Role::Empty,
        }
    }

    fn static_color(&self, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        let hdc = HDC(wparam.0 as *mut c_void);
        let control = HWND(lparam.0 as *mut c_void);
        let color = if control == self.ui.status {
            match self.status_kind.get() {
                Kind::Info => COLORREF(0),
                Kind::Busy => BLUE,
                Kind::Ok => GREEN,
                Kind::Warn => AMBER,
            }
        } else if control == self.ui.status_sub || control == self.ui.caption {
            MUTED
        } else {
            // SAFETY: system の色を読むだけ
            COLORREF(unsafe { GetSysColor(COLOR_WINDOWTEXT) })
        };
        // SAFETY: 渡された hdc に色を設定し、地の brush を返す
        unsafe {
            SetTextColor(hdc, color);
            SetBkMode(hdc, TRANSPARENT);
            LRESULT(GetSysColorBrush(COLOR_WINDOW).0 as isize)
        }
    }

    fn on_dpi_changed(&self, dpi: u32, lparam: LPARAM) {
        self.dpi.set(dpi);
        let fonts = Fonts::new(dpi);
        self.ui.set_fonts(&fonts);
        // 古いフォントは、コントロールに新しいフォントを渡した後で消す
        drop(self.fonts.replace(fonts));
        // SAFETY: lparam は推奨の窓の矩形を指す
        unsafe {
            let suggested = &*(lparam.0 as *const RECT);
            let _ = SetWindowPos(
                self.hwnd,
                None,
                suggested.left,
                suggested.top,
                suggested.right - suggested.left,
                suggested.bottom - suggested.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
        self.arrange();
    }

    // ---- 状態の行 ----

    fn set_status(&self, status: Status) {
        self.status_kind.set(status.kind);
        ui::set_text(self.ui.status, &status.text);
        ui::set_text(self.ui.status_sub, &status.sub);
    }

    fn update_clipboard(&self) {
        // 写しは作らず、クリップボードを開いている間に数える
        let counted = clipboard::with_text(self.hwnd, typeable_len);
        self.scrub_pending.set(true);
        let text = match counted {
            ClipText::Text(0) | ClipText::Empty => "クリップボード: 空".to_string(),
            ClipText::Text(n) => format!("クリップボード: {n} 文字"),
            ClipText::NotText => "クリップボード: 文字ではない".to_string(),
            ClipText::Busy => return,
        };
        ui::set_text(self.ui.clip, &text);
    }

    // ---- 一覧と打つ先 ----

    fn refresh_list(&self) {
        let entries = target::list();
        self.suppress_list.set(true);
        ui::list::clear(self.ui.list);
        for (index, entry) in entries.iter().enumerate() {
            ui::list::insert(self.ui.list, index, &entry.title, &entry.exe);
        }
        self.suppress_list.set(false);
        *self.entries.borrow_mut() = entries;
        self.select_in_list(self.target.get());
    }

    fn select_in_list(&self, hwnd: Option<HWND>) {
        let index = hwnd.and_then(|hwnd| {
            self.entries
                .borrow()
                .iter()
                .position(|entry| entry.hwnd == hwnd)
        });
        self.suppress_list.set(true);
        ui::list::select_only(self.ui.list, index);
        self.suppress_list.set(false);
    }

    fn on_notify(&self, lparam: LPARAM) {
        // SAFETY: WM_NOTIFY の lparam は NMHDR を指す
        let header = unsafe { &*(lparam.0 as *const NMHDR) };
        if header.hwndFrom != self.ui.list || header.code != LVN_ITEMCHANGED {
            return;
        }
        // SAFETY: LVN_ITEMCHANGED の lparam は NMLISTVIEW を指す
        let change = unsafe { &*(lparam.0 as *const NMLISTVIEW) };
        let selected = LVIS_SELECTED.0;
        let newly_selected = change.uNewState & selected != 0 && change.uOldState & selected == 0;
        if !newly_selected || self.suppress_list.get() {
            return;
        }
        let hwnd = usize::try_from(change.iItem)
            .ok()
            .and_then(|i| self.entries.borrow().get(i).map(|entry| entry.hwnd));
        if let Some(hwnd) = hwnd {
            self.choose_target(hwnd);
        }
    }

    fn on_list_hover(&self, lparam: LPARAM) {
        if !self.is_idle() {
            return;
        }
        if !self.tracking_list_mouse.replace(true) {
            let mut track = TRACKMOUSEEVENT {
                cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: self.ui.list,
                dwHoverTime: 0,
            };
            // SAFETY: track は呼ぶ間だけ生きていればよい
            let _ = unsafe { TrackMouseEvent(&mut track) };
        }
        let point = POINT {
            x: i32::from(loword(lparam.0 as usize) as i16),
            y: i32::from(hiword(lparam.0 as usize) as i16),
        };
        let hovered = ui::list::hit(self.ui.list, point)
            .and_then(|i| self.entries.borrow().get(i).map(|entry| entry.hwnd));
        if hovered != self.candidate.get() {
            self.candidate.set(hovered);
            self.refresh_preview();
        }
    }

    fn on_list_leave(&self) {
        self.tracking_list_mouse.set(false);
        if !self.picking.get() && self.candidate.take().is_some() {
            self.refresh_preview();
        }
    }

    fn choose_target(&self, hwnd: HWND) {
        self.target.set(Some(hwnd));
        self.candidate.set(None);
        self.select_in_list(Some(hwnd));
        self.refresh_preview();
        self.flash_target();
        self.set_status(status::target_decided());
    }

    /// 欄に映す窓・下の題・デスクトップの枠を、今の候補と打つ先に合わせる。
    fn refresh_preview(&self) {
        let shown = self.candidate.get().or(self.target.get());
        self.preview.show(shown);
        self.place_thumbnail();
        let caption = shown
            .map(|hwnd| {
                let entry = target::entry(hwnd);
                let title = if entry.title.is_empty() {
                    "(題なし)"
                } else {
                    &entry.title
                };
                format!("{title} ({})", entry.exe)
            })
            .unwrap_or_default();
        ui::set_text(self.ui.caption, &caption);
        ui::invalidate(self.hwnd, Some(self.panel.get()));
        match self.candidate.get() {
            Some(candidate) => {
                let color = if Some(candidate) == self.target.get() {
                    BLUE
                } else {
                    ORANGE
                };
                self.frame.show_around(candidate, color, self.s(4));
            }
            None if !self.flashing.get() => self.frame.hide(),
            None => {}
        }
    }

    fn flash_target(&self) {
        let Some(target) = self.target.get() else {
            return;
        };
        self.frame.show_around(target, BLUE, self.s(4));
        self.flashing.set(true);
        // SAFETY: 主窓の timer
        unsafe { SetTimer(Some(self.hwnd), TIMER_FLASH, FLASH_MS, None) };
    }

    fn on_timer(&self, id: usize) {
        match id {
            TIMER_POLL => self.poll(),
            TIMER_FLASH => {
                // SAFETY: 主窓の timer を止める
                let _ = unsafe { KillTimer(Some(self.hwnd), TIMER_FLASH) };
                self.flashing.set(false);
                if self.candidate.get().is_none() {
                    self.frame.hide();
                }
            }
            TIMER_PICK_RELEASE => self.release_pick_hooks(),
            _ => {}
        }
    }

    /// 打つ thread が知らせずに終わっていないか・打つ先が閉じたかを見て、サムネイルを窓の今の大きさに合わせる。
    fn poll(&self) {
        self.check_worker();
        let alive = |hwnd: Option<HWND>| {
            // SAFETY: 窓があるかを問い合わせるだけ
            hwnd.is_none_or(|hwnd| unsafe { IsWindow(Some(hwnd)) }.as_bool())
        };
        if !alive(self.candidate.get()) {
            self.candidate.set(None);
            self.refresh_preview();
        }
        if !alive(self.target.get()) && self.run.get().is_none() {
            self.target.set(None);
            self.refresh_list();
            self.refresh_preview();
            self.set_status(status::target_closed());
        }
        self.place_thumbnail();
    }

    /// 打つ thread が、終わりを知らせずに終わっていたら (知らせを置く前に panic した・外から止められたなど)、
    /// hook を外して知らせる。打つ thread は panic しても知らせるので、ここで拾うのは、それもできなかったときだけ。
    fn check_worker(&self) {
        let exited = self.worker.borrow().as_ref().is_some_and(typer::exited);
        if let Some(run) = self.run.get()
            && exited
            && !self.done_queued()
        {
            self.on_done(Outcome::Crashed.code(), run.typed);
        }
    }

    /// 打つ thread の終わりの知らせが、まだ message の列にあるか (終わった thread の知らせは、もう列に入っている)。
    fn done_queued(&self) -> bool {
        let mut msg = MSG::default();
        // SAFETY: 列を覗くだけ (取り出さない)
        unsafe {
            PeekMessageW(
                &mut msg,
                Some(self.hwnd),
                WM_APP_DONE,
                WM_APP_DONE,
                PM_NOREMOVE,
            )
        }
        .as_bool()
    }

    // ---- クリックで選ぶ ----

    fn start_pick(&self) {
        if !self.is_idle() {
            return;
        }
        // 前のクリックのボタンを離すのを待っている hook があれば外す
        self.release_pick_hooks();
        let unused = Arc::new(AtomicBool::new(false));
        match hooks::install(HookMode::Pick, self.hwnd, unused) {
            Ok(guard) => *self.hooks.borrow_mut() = Some(guard),
            Err(e) => {
                self.set_status(Status::new(
                    Kind::Warn,
                    "クリックで選ぶを始められませんでした",
                    e.message(),
                ));
                return;
            }
        }
        self.picking.set(true);
        ui::set_text(self.ui.pick, "やめる (Esc)");
        self.set_status(status::picking());
    }

    /// 画面の座標 `point` にある、選べる窓。
    fn pickable_at(&self, point: POINT) -> Option<HWND> {
        // SAFETY: 窓を問い合わせるだけ
        let root = unsafe {
            let hit = WindowFromPoint(point);
            if hit.is_invalid() {
                return None;
            }
            GetAncestor(hit, GA_ROOT)
        };
        (!root.is_invalid() && is_pickable(&target::facts(root))).then_some(root)
    }

    fn on_pick_move(&self, point: POINT) {
        if !self.picking.get() {
            return;
        }
        let pointed = self.pickable_at(point);
        if pointed != self.candidate.get() {
            self.candidate.set(pointed);
            self.refresh_preview();
        }
    }

    /// `point` はクリックした点 (hook がクリックのときに載せたもの)。
    fn on_pick_click(&self, point: POINT) {
        if !self.picking.get() {
            return;
        }
        let chosen = self.pickable_at(point);
        self.end_pick();
        self.release_pick_hooks_after_buttons();
        match chosen {
            Some(hwnd) => self.choose_target(hwnd),
            None => {
                self.refresh_preview();
                self.set_status(status::pick_cancelled());
            }
        }
    }

    fn cancel_pick(&self) {
        if !self.picking.get() {
            return;
        }
        self.end_pick();
        self.release_pick_hooks_after_buttons();
        self.refresh_preview();
        self.set_status(status::pick_cancelled());
    }

    /// 押す方を握りつぶしたボタンがあれば、離す方も握りつぶすため、離すまで (長くて
    /// `PICK_RELEASE_MS`) hook を残す。無ければすぐ外す。
    fn release_pick_hooks_after_buttons(&self) {
        if hooks::buttons_pending() {
            // SAFETY: 主窓の timer
            unsafe { SetTimer(Some(self.hwnd), TIMER_PICK_RELEASE, PICK_RELEASE_MS, None) };
        } else {
            self.release_pick_hooks();
        }
    }

    fn end_pick(&self) {
        self.picking.set(false);
        self.candidate.set(None);
        ui::set_text(self.ui.pick, "クリックで選ぶ");
    }

    /// 選び終えた「クリックで選ぶ」の hook を外す (打っている間の hook は外さない)。
    fn release_pick_hooks(&self) {
        // SAFETY: 主窓の timer を止める
        let _ = unsafe { KillTimer(Some(self.hwnd), TIMER_PICK_RELEASE) };
        if self.is_idle() {
            self.hooks.borrow_mut().take();
        }
    }

    // ---- 打つ ----

    fn start_typing(&self, trial: bool) {
        if !self.is_idle() {
            return;
        }
        self.release_pick_hooks();
        let Some(target) = self.target.get() else {
            self.set_status(status::choose_target(Kind::Warn));
            return;
        };
        // SAFETY: 窓があるかを問い合わせるだけ
        if !unsafe { IsWindow(Some(target)) }.as_bool() {
            self.target.set(None);
            self.refresh_preview();
            self.set_status(status::target_closed());
            return;
        }
        if target::elevation(target) == Elevation::Higher {
            self.set_status(status::elevated());
            return;
        }
        let mode = self.mode();
        let layout = self.ui.selected_layout();
        let planned = if trial {
            let sample: Vec<u16> = TEST_SAMPLE.encode_utf16().collect();
            plan(&sample, mode, layout)
        } else {
            // 写しは作らず、クリップボードを開いている間に打つ列を作る (打つ列は物理メモリに留めたページに置く)
            let read = clipboard::with_text(self.hwnd, |text| plan(text, mode, layout));
            self.scrub_pending.set(true);
            match read {
                ClipText::Text(planned) => planned,
                ClipText::Empty | ClipText::NotText => Err(Refusal::Empty),
                ClipText::Busy => {
                    self.set_status(status::clipboard_busy());
                    return;
                }
            }
        };
        let strokes = match planned {
            Ok(strokes) => strokes,
            Err(refusal) => {
                self.set_status(status::refusal(&refusal, mode));
                return;
            }
        };
        if typer::modifiers_held() {
            self.set_status(status::outcome(Outcome::ModifierHeld, 0, 0, trial));
            return;
        }
        if mode == Mode::KeyPosition && typer::caps_lock_on() {
            self.set_status(status::caps_lock());
            return;
        }

        let abort = Arc::new(AtomicBool::new(false));
        match hooks::install(HookMode::Typing, self.hwnd, abort.clone()) {
            Ok(guard) => *self.hooks.borrow_mut() = Some(guard),
            Err(e) => {
                self.set_status(Status::new(
                    Kind::Warn,
                    "打つ準備ができませんでした",
                    e.message(),
                ));
                return;
            }
        }
        *self.abort.borrow_mut() = abort.clone();
        let total = strokes.len();
        self.run.set(Some(Run {
            total,
            trial,
            typed: 0,
        }));
        self.frame.hide();
        self.ui.enable_inputs(false, mode == Mode::KeyPosition);
        self.ui.set_progress(0, total, PBST_NORMAL);
        self.set_status(status::typing(0, total));

        // 打つ先を前に出す (このアプリが前面にいる今なら出せる)
        // SAFETY: 窓の表示を変えるだけ
        unsafe {
            if IsIconic(target).as_bool() {
                let _ = ShowWindow(target, SW_RESTORE);
            }
            let _ = SetForegroundWindow(target);
        }
        let job = Job {
            target: target.0 as isize,
            strokes,
            interval_ms: self.ui.interval_ms(),
            notify: self.hwnd.0 as isize,
            abort,
        };
        match typer::spawn(job) {
            Ok(handle) => *self.worker.borrow_mut() = Some(handle),
            Err(_) => self.on_done(Outcome::SendFailed.code(), 0),
        }
    }

    fn on_progress(&self, typed: usize) {
        if let Some(run) = self.run.get() {
            self.run.set(Some(Run { typed, ..run }));
            self.ui.set_progress(typed, run.total, PBST_NORMAL);
            self.set_status(status::typing(typed, run.total));
        }
    }

    fn on_done(&self, code: usize, typed: usize) {
        self.hooks.borrow_mut().take();
        // 打っている間に hook が受け取った (このアプリが送った) キーが、この thread の stack に残るので上書きする
        self.scrub_pending.set(true);
        if let Some(worker) = self.worker.borrow_mut().take() {
            typer::finish(worker);
        }
        let Some(run) = self.run.take() else {
            return;
        };
        let outcome = Outcome::from_code(code).unwrap_or(Outcome::SendFailed);
        let mode_is_key = self.mode() == Mode::KeyPosition;
        self.ui.enable_inputs(true, mode_is_key);
        let state = if outcome == Outcome::Completed {
            PBST_NORMAL
        } else {
            PBST_PAUSED
        };
        self.ui.set_progress(typed, run.total, state);
        self.set_status(status::outcome(outcome, typed, run.total, run.trial));
    }

    // ---- ボタンなど ----

    fn on_command(&self, id: u16, code: u32) {
        if code != BN_CLICKED {
            return;
        }
        match id {
            ui::ID_REFRESH => self.refresh_list(),
            ui::ID_PICK if self.picking.get() => self.cancel_pick(),
            ui::ID_PICK => self.start_pick(),
            ui::ID_MODE_KEY | ui::ID_MODE_UNICODE => {
                self.ui.enable_layout(self.mode() == Mode::KeyPosition);
            }
            ui::ID_TOPMOST => self.apply_topmost(),
            ui::ID_TYPE => self.start_typing(false),
            ui::ID_TRIAL => self.start_typing(true),
            _ => {}
        }
    }

    fn apply_topmost(&self) {
        let after = if ui::is_checked(self.ui.topmost) {
            HWND_TOPMOST
        } else {
            HWND_NOTOPMOST
        };
        // SAFETY: 主窓の重なりの順を変えるだけ
        let _ = unsafe {
            SetWindowPos(
                self.hwnd,
                Some(after),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            )
        };
    }

    // ---- 閉じる ----

    fn shutdown(&self) {
        // 打つ thread が SendInput で hook を待たないよう、先に hook を外してから止める
        self.hooks.borrow_mut().take();
        self.abort.borrow().store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.borrow_mut().take() {
            typer::finish(worker);
        }
        // SAFETY: 主窓の timer を止める
        unsafe {
            let _ = KillTimer(Some(self.hwnd), TIMER_POLL);
            let _ = KillTimer(Some(self.hwnd), TIMER_FLASH);
            let _ = KillTimer(Some(self.hwnd), TIMER_PICK_RELEASE);
        }
        clipboard::unlisten(self.hwnd);
        self.preview.show(None);
        self.frame.hide();
        self.save_settings();
    }

    fn save_settings(&self) {
        let Some(path) = &self.settings_path else {
            return;
        };
        let mut placement = WINDOWPLACEMENT {
            length: size_of::<WINDOWPLACEMENT>() as u32,
            ..Default::default()
        };
        // SAFETY: placement に書くだけ
        let window = unsafe { GetWindowPlacement(self.hwnd, &mut placement) }
            .is_ok()
            .then(|| {
                let r = placement.rcNormalPosition;
                WindowRect {
                    x: r.left,
                    y: r.top,
                    width: r.right - r.left,
                    height: r.bottom - r.top,
                }
            });
        let settings = Settings {
            mode: self.mode(),
            layout: self.ui.selected_layout(),
            interval_ms: self.ui.interval_ms(),
            always_on_top: ui::is_checked(self.ui.topmost),
            window,
        };
        let _ = settings::save(path, &settings);
    }
}
