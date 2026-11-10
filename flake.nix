{
  description = "type-clipboard の開発環境 (手元の Linux から Windows の exe を作る)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { nixpkgs, rust-overlay, ... }:
    let
      inherit (nixpkgs) lib;

      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      forAllSystems =
        f:
        lib.genAttrs systems (
          system:
          f (
            import nixpkgs {
              inherit system;
              overlays = [ rust-overlay.overlays.default ];
            }
          )
        );
    in
    {
      formatter = forAllSystems (pkgs: pkgs.nixfmt);

      devShells = forAllSystems (
        pkgs:
        let
          # 版は rust-toolchain.toml (CI と同じ)。手元では Windows の target の std も入れる
          rust = (pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml).override {
            targets = [ "x86_64-pc-windows-msvc" ];
          };
        in
        {
          default = pkgs.mkShellNoCC {
            packages = [
              rust
              # cargo xwin build --release --target x86_64-pc-windows-msvc
              # (初回に Microsoft の CRT と Windows SDK を取ってくる)
              pkgs.cargo-xwin
              pkgs.lld
              # README の「使い方」の図を作る (python3 docs/images/make_usage_svgs.py)
              pkgs.python3
            ];
          };
        }
      );
    };
}
