use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// app.manifest の中で、Cargo.toml の版に置き換える印
const VERSION_MARK: &str = "@VERSION@";

fn main() {
    // cargo への指示は標準出力に書く決まり (ログではない)。clippy の print_stdout は build script を見ないので、
    // Cargo.toml の [lints.clippy] で deny にしていても外す指定は要らない
    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rerun-if-changed=build.rs");
    // Windows の target のときだけ、app.manifest に版を入れてから exe に埋め込む。版を書く所は Cargo.toml だけにする
    if env::var_os("CARGO_CFG_WINDOWS").is_some() {
        let template = fs::read_to_string("app.manifest").expect("app.manifest を読めない");
        let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR が無い"));
        let out = out_dir.join("app.manifest");
        write_manifest(&out, &fill_version(&template, &manifest_version()));
        embed_manifest::embed_manifest_file(&out).expect("app.manifest を埋め込めない");
    }
}

/// manifest の版は 0〜65535 の数 4 つ。Cargo.toml の版の major.minor.patch に 0 を足す (pre-release の印は入れない)
fn manifest_version() -> String {
    let part = |name: &str| -> u16 {
        env::var(name)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(|| panic!("{name} を manifest の版 (0〜65535) にできない"))
    };
    format!(
        "{}.{}.{}.0",
        part("CARGO_PKG_VERSION_MAJOR"),
        part("CARGO_PKG_VERSION_MINOR"),
        part("CARGO_PKG_VERSION_PATCH")
    )
}

/// 印がちょうど 1 つあることを確かめてから、版に置き換える
fn fill_version(template: &str, version: &str) -> String {
    let marks = template.matches(VERSION_MARK).count();
    assert!(
        marks == 1,
        "app.manifest の {VERSION_MARK} は 1 つのはずが、{marks} つある"
    );
    template.replace(VERSION_MARK, version)
}

#[expect(
    clippy::disallowed_methods,
    reason = "build のときに、版を入れた manifest を OUT_DIR に書くだけ。打つ文字は通らない"
)]
fn write_manifest(path: &Path, xml: &str) {
    fs::write(path, xml).expect("版を入れた manifest を OUT_DIR に書けない");
}
