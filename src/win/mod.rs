//! Win32 の部分 (Windows でだけ組む)。

mod app;
mod clipboard;
mod frame;
mod hooks;
mod preview;
mod target;
mod typer;
mod ui;

use windows::Win32::System::ErrorReporting::{WER_FAULT_REPORTING_FLAG_NOHEAP, WerSetFlags};
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
/// 打つ thread から (SendMessage で、済むまで待つ。1 文字を送る前に毎回): hook を入れ直して打つ段にする。
/// 入れ直せたら `REARMED` を返す。
const WM_APP_REARM: u32 = WM_APP + 7;
/// `WM_APP_REARM` の答え: 入れ直せた。
const REARMED: usize = 1;

/// 起動のすぐ後 (クリップボードに触れる前) に、秘密の写しがプロセスの外に出る経路を閉じる。
///
/// - panic の文 (どこで・何が) を標準エラーに書かない。console の無い窓のアプリでも、起こした側が標準エラーを
///   ファイルにつないでいると、そこに残るため
/// - クラッシュや応答なしで Windows のエラー報告 (WER) が dump を取るとき、heap を入れない
pub fn harden() -> windows::core::Result<()> {
    std::panic::set_hook(Box::new(|_| {}));
    exclude_heap_from_crash_reports()
}

/// WER の dump に heap を入れない。このプロセスの中だけの設定で、レジストリなど、プロセスの外に残る設定は変えない。
fn exclude_heap_from_crash_reports() -> windows::core::Result<()> {
    // SAFETY: このプロセスの WER の設定を変えるだけ
    unsafe { WerSetFlags(WER_FAULT_REPORTING_FLAG_NOHEAP) }
}

/// 起動できなかったわけを出す。
pub fn show_fatal(error: &windows::core::Error) {
    let text = HSTRING::from(format!("起動できませんでした。\n{error}"));
    // SAFETY: message box を出すだけ
    unsafe { MessageBoxW(None, &text, w!("type-clipboard"), MB_ICONERROR | MB_OK) };
}

#[cfg(test)]
mod tests {
    use windows::Win32::System::ErrorReporting::{WER_FAULT_REPORTING_FLAG_NOHEAP, WerGetFlags};
    use windows::Win32::System::Threading::GetCurrentProcess;

    #[test]
    fn crash_reports_leave_out_the_heap() {
        super::exclude_heap_from_crash_reports().unwrap();
        // SAFETY: このプロセスの WER の設定を読むだけ
        let flags = unsafe { WerGetFlags(GetCurrentProcess()) }.unwrap();
        assert_ne!(flags.0 & WER_FAULT_REPORTING_FLAG_NOHEAP.0, 0);
    }
}
