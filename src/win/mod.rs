//! Win32 の部分 (Windows でだけ組む)。

/// 窓を作り、閉じられるまで message loop を回す。
pub fn run() -> windows::core::Result<()> {
    Ok(())
}

/// 起動できなかったわけを出す。
pub fn show_fatal(error: &windows::core::Error) {
    let _ = error;
}
