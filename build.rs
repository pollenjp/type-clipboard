fn main() {
    // Windows の target のときだけ、app.manifest を exe に埋め込む
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        embed_manifest::embed_manifest_file("app.manifest").expect("app.manifest を埋め込めない");
    }
    // cargo への指示は標準出力に書く決まり (ログではない)。clippy の print_stdout は build script を見ないので、
    // Cargo.toml の [lints.clippy] で deny にしていても外す指定は要らない
    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rerun-if-changed=build.rs");
}
