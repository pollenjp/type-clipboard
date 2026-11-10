//! type-clipboard: クリップボードの文字を、選んだ窓へキー入力として 1 文字ずつ打ち込む。

// console の窓を出さない (test のときは出力が見えるよう外す)
#![cfg_attr(not(test), windows_subsystem = "windows")]
// test の失敗の文には Debug の書式を使ってよい (Cargo.toml の [lints.clippy] の use_debug)
#![cfg_attr(test, allow(clippy::use_debug))]

#[cfg(windows)]
mod win;

#[cfg(windows)]
fn main() {
    // クリップボードに触れる前に、写しがプロセスの外に出る経路を閉じてから、窓を出す
    if let Err(e) = win::harden().and_then(|()| win::run()) {
        win::show_fatal(&e);
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
#[expect(
    clippy::print_stderr,
    reason = "Windows でないときに、決まった 1 行だけを出す (打つ文字とは関わらない)"
)]
fn main() {
    eprintln!("type-clipboard は Windows でだけ動きます。");
    std::process::exit(1);
}
