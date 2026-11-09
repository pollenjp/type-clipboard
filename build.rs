fn main() {
    // Windows の target のときだけ、app.manifest を exe に埋め込む
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        embed_manifest::embed_manifest_file("app.manifest").expect("app.manifest を埋め込めない");
    }
    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rerun-if-changed=build.rs");
}
