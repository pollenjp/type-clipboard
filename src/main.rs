//! type-clipboard: クリップボードの文字を、選んだ窓へキー入力として 1 文字ずつ打ち込む。

// console の窓を出さない (test のときは出力が見えるよう外す)
#![cfg_attr(not(test), windows_subsystem = "windows")]

#[cfg(windows)]
mod win;

#[cfg(windows)]
fn main() {
    if let Err(e) = win::run() {
        win::show_fatal(&e);
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("type-clipboard は Windows でだけ動きます。");
    std::process::exit(1);
}
