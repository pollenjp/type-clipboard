# type-clipboard 実装計画

> **agent の作業者へ:** この計画は superpowers:subagent-driven-development か superpowers:executing-plans で、
> タスクごとに進める。手順は checkbox (`- [ ]`) で追う。

**Goal:** クリップボードの文字を、選んだ窓へキー入力として 1 文字ずつ打ち込む Windows のアプリ (単一の exe) を作る。

**Architecture:** 1 つの crate に、OS に触らない部分 (`src/lib.rs` 以下。Linux でも test が通る) と、Win32 の部分
(`src/win/`。`cfg(windows)`) を分けて置く。GUI は Win32 のネイティブのコントロールで組み、プレビューは DWM の
サムネイル、打つ処理は別の thread から SendInput で送る。打つ間と「クリックで選ぶ」の間だけ、低レベルの
キーボードとマウスの hook を UI の thread に入れ、利用者の入力で止める。

**Tech Stack:** Rust 1.99.1 (edition 2024)・`windows` 0.62.2・`embed-manifest` 1.5.1 (build)・GitHub Actions
(windows-latest)・nix flake (rust-overlay・cargo-xwin) の devShell。

**Spec:** `docs/superpowers/specs/2026-11-09-type-clipboard-design.md`

## Global Constraints

- クリップボードの中身を、画面・ログ・ファイル・設定に出さない・残さない。手元の写しは使い終えたら上書きして消す (spec §3・§5.4)。
- 打つ先でない窓へ打たない。1 文字ごとに前面の窓を確かめる。Enter を押さない (spec §3・§5.3)。
- UI・README・commit の message は日本語 (spec §5.8)。
- 単一の exe (x86_64-pc-windows-msvc)。console の窓を出さない。manifest で comctl32 v6 と Per-Monitor V2 (spec §5.6)。
- 依存は公開から 7 日以上たった版だけ (Cargo.lock の全 package・Actions・flake.lock)。cargo には仕組みが無いので、
  lock を作ったら公開日を確かめる。flake.lock は素の `nix flake update` / `nix flake lock` で作らない。
- Actions は 40 桁の SHA で固定し、行末に `# vX.Y.Z`。`GITHUB_TOKEN` は既定で `contents: read`、Release の job だけ
  `contents: write` (spec §5.7)。
- public な repo: 作者の環境の固有の情報 (マシン名・VM 名・IP・ユーザー名・ホームのパス)・非公開の repo の名前・
  チケットの ID や URL・作者の画面のスクリーンショット・本物のパスワードを書かない。例は vm01・example.com・
  192.0.2.x (spec §8)。
- 試し打ちの見本は英字の大文字と小文字・数字・ASCII の記号の一通り (spec §5.3)。

## Review Focus

1. 行末が `\r\n`・`\r`・`\n\n` のクリップボード (エディタや表計算からのコピー): 末尾の改行だけを落とし、数える文字にも入れない。
   → Task 3 の `trailing_crlf_variants_are_trimmed`。
2. 絵文字など UTF-16 で 2 単位になる文字: 1 文字と数え、キーの位置の方式では打てない文字 1 個、Unicode の方式では
   2 単位を送る。→ Task 3 の `astral_char_counts_as_one`・`unicode_mode_keeps_astral_char`。
3. 対になっていない surrogate を含むクリップボード: U+FFFD に置き換えて打たず、打てない文字として断る。
   → Task 3 の `unpaired_surrogate_is_unsupported`。
4. 手で書き換えた・壊れた設定ファイル (数でない間隔・範囲の外・知らない行): 落ちずに既定値か範囲の端に寄せる。
   → Task 4 の `broken_values_fall_back`。
5. 管理者として動く窓 (token を開けない): 打つ前に断る。整合性のレベルを読めないだけのときは止めない。
   → Task 5 の `elevation_from_levels`。

---

## ファイルの構成

| ファイル | 役目 |
| --- | --- |
| `rust-toolchain.toml` | Rust の版 (1.99.1) と clippy・rustfmt。CI と devShell が同じ版を使う |
| `flake.nix` / `flake.lock` | devShell: rust-overlay で toolchain (+ windows の target)、cargo-xwin・lld で手元から exe を作る |
| `Cargo.toml` / `Cargo.lock` | 依存。`windows` は `cfg(windows)` の target だけ |
| `build.rs` / `app.manifest` | Windows の target のとき manifest を埋め込む |
| `src/main.rs` | 入口。`cfg(windows)` なら `win::run()`、ほかは案内を出して終わる |
| `src/lib.rs` | OS に触らない module を公開する |
| `src/keymap.rs` | 配列 (JIS・US) の表: 文字 → scancode と Shift |
| `src/typing.rs` | 打つ文字の下ごしらえ: 末尾の改行・断る条件・打つ列 (`Strokes`)・試し打ちの見本・結果の種類 |
| `src/secret.rs` | 写しを上書きして消す (`wipe`・`SecretText`) |
| `src/settings.rs` | 設定の形・読み書き (`%APPDATA%\type-clipboard\settings.ini`) |
| `src/candidate.rs` | 一覧に出す窓・クリックで選べる窓の判定、昇格の判定 |
| `src/geometry.rs` | 縦横比を保って欄に収める計算 |
| `src/status.rs` | 状態の行の文言 (断る・打った・止めた) |
| `src/win/mod.rs` | `run()`: 窓を作り、message loop を回す |
| `src/win/app.rs` | 主窓: 状態・window procedure・操作への応答 |
| `src/win/ui.rs` | コントロールの作成・配置・フォント・DPI・ListView の操作 |
| `src/win/target.rs` | 窓の列挙・題・exe の名前・整合性のレベル・枠の位置 |
| `src/win/preview.rs` | DWM のサムネイル |
| `src/win/frame.rs` | デスクトップの上の枠 (橙・青) |
| `src/win/hooks.rs` | 低レベルの hook (クリックで選ぶ・打つ間の利用者の入力) |
| `src/win/clipboard.rs` | クリップボードの文字の読み取りと変化の知らせ |
| `src/win/typer.rs` | 打つ thread: 前に出す・IME・修飾キー・SendInput |
| `.github/workflows/ci.yml` | PR と main: fmt・clippy・test・release ビルド・exe を artifact に。flake.lock の猶予の番人 |
| `.github/workflows/release.yml` | tag `v*`: exe を作って Release に置く |
| `README.md` / `LICENSE` | 日本語の説明 / MIT |

## Task 1: toolchain と crate の骨組み

**Files:**
- Create: `rust-toolchain.toml`, `flake.nix`, `flake.lock`, `Cargo.toml`, `Cargo.lock`, `build.rs`, `app.manifest`,
  `src/main.rs`, `src/lib.rs`, `.gitignore`

**Interfaces:**
- Produces: `cargo test` (Linux でも動く)、`cargo clippy --target x86_64-pc-windows-msvc` (手元で Win32 の部分を型検査)、
  `cargo xwin build --release --target x86_64-pc-windows-msvc` (手元で exe)

- [ ] **Step 1:** `rust-toolchain.toml` に `channel = "1.99.1"`・`components = ["clippy", "rustfmt"]`・`profile = "minimal"`。
- [ ] **Step 2:** `flake.nix`: inputs は `nixpkgs` (`github:NixOS/nixpkgs/nixpkgs-unstable`) と `rust-overlay`
  (`github:oxalica/rust-overlay`、nixpkgs を follows)。devShell に `rust-bin.fromRustupToolchainFile ./rust-toolchain.toml`
  に target `x86_64-pc-windows-msvc` を足したもの・`cargo-xwin`・`lld` を入れる。systems は x86_64-linux・aarch64-linux・aarch64-darwin。
- [ ] **Step 3:** `git add flake.nix` の後、`nix run 'github:pollenjp/dotfiles?dir=nix#flake-lock-age' -- update` で lock を作る。
- [ ] **Step 4:** `Cargo.toml`: package `type-clipboard`・version `0.1.0`・edition `2024`・license `MIT`。
  `[target.'cfg(windows)'.dependencies] windows = { version = "0.62.2", features = [...] }` (Task 5〜9 で使う feature だけ)、
  `[build-dependencies] embed-manifest = "1.5.1"`。`[profile.release]` は `lto = true`・`codegen-units = 1`・`strip = true`。
- [ ] **Step 5:** `app.manifest`: comctl32 v6 の dependency・`dpiAwareness` PerMonitorV2 (と `dpiAware` true/pm)・
  `requestedExecutionLevel asInvoker`・supportedOS Windows 10/11。`build.rs` は `CARGO_CFG_WINDOWS` があるときだけ
  `embed_manifest::embed_manifest_file("app.manifest")`、`rerun-if-changed` を出す。
- [ ] **Step 6:** `src/main.rs`: `#![cfg_attr(not(test), windows_subsystem = "windows")]`。`cfg(windows)` で `win::run()`、
  ほかは「Windows でだけ動きます」を stderr に出して exit 1。
- [ ] **Step 7:** 確かめる: `nix develop --command cargo test` が通る・`cargo clippy --target x86_64-pc-windows-msvc -- -D warnings`
  が通る・`cargo xwin build --release --target x86_64-pc-windows-msvc` で exe ができる。
- [ ] **Step 8:** Cargo.lock の全 package の版の公開日を crates.io で見て、7 日未満があれば `cargo update -p <name> --precise <版>` で下げる。
- [ ] **Step 9:** commit。

## Task 2: 配列の表 (`keymap`)

**Files:** Create: `src/keymap.rs` (test は同じファイルの `mod tests`)

**Interfaces:**
- Produces:
  - `pub enum Layout { Jis, Us }`、`Layout::ALL: [Layout; 2]`、`fn label(self) -> &'static str` (`"日本語 (JIS)"`・`"英語 (US)"`)、
    `fn key(self) -> &'static str` (`"jis"`・`"us"`)、`fn from_key(&str) -> Option<Layout>`
  - `pub struct KeyStroke { pub scancode: u16, pub shift: bool }`
  - `impl Layout { pub fn stroke_for(self, c: char) -> Option<KeyStroke> }`
  - `pub const SHIFT_SCANCODE: u16 = 0x2A`

- [ ] **Step 1: 失敗する test を書く**

```rust
#[test]
fn every_printable_ascii_maps_in_both_layouts() {
    for layout in Layout::ALL {
        for b in 0x20u8..=0x7E {
            assert!(layout.stroke_for(b as char).is_some(), "{layout:?} {:?}", b as char);
        }
    }
}
#[test]
fn layouts_differ_where_keycaps_differ() {
    assert_eq!(Layout::Jis.stroke_for('@'), Some(KeyStroke { scancode: 0x1A, shift: false }));
    assert_eq!(Layout::Us.stroke_for('@'), Some(KeyStroke { scancode: 0x03, shift: true }));
    assert_eq!(Layout::Jis.stroke_for('_'), Some(KeyStroke { scancode: 0x73, shift: true }));
    assert_eq!(Layout::Jis.stroke_for('\\'), Some(KeyStroke { scancode: 0x73, shift: false }));
    assert_eq!(Layout::Jis.stroke_for('|'), Some(KeyStroke { scancode: 0x7D, shift: true }));
    assert_eq!(Layout::Us.stroke_for('A'), Some(KeyStroke { scancode: 0x1E, shift: true }));
    assert_eq!(Layout::Us.stroke_for(' '), Some(KeyStroke { scancode: 0x39, shift: false }));
}
#[test]
fn non_ascii_and_controls_are_not_mapped() {
    for c in ['あ', 'Ａ', '€', '¥', '\t', '\n', '\u{7F}'] {
        assert_eq!(Layout::Jis.stroke_for(c), None);
        assert_eq!(Layout::Us.stroke_for(c), None);
    }
}
#[test]
fn layout_keys_round_trip() {
    for layout in Layout::ALL { assert_eq!(Layout::from_key(layout.key()), Some(layout)); }
}
```

- [ ] **Step 2:** `cargo test keymap` が「`Layout` が無い」で落ちるのを見る。
- [ ] **Step 3:** 実装。表は Set 1 の scancode。US は ANSI の 104 キー、JIS は 106/109 キー (数字の段の Shift は `!"#$%&'()`、
  `-`/`=` は 0x0C、`^`/`~` は 0x0D、`@`/`` ` `` は 0x1A、`[`/`{` は 0x1B、`;`/`+` は 0x27、`:`/`*` は 0x28、`]`/`}` は 0x2B、
  `\`/`_` はろのキー 0x73、`|` は ¥ のキーの Shift 0x7D)。英字は小文字が Shift なし、大文字が Shift あり。
- [ ] **Step 4:** `cargo test keymap` が通る。
- [ ] **Step 5:** commit。

## Task 3: 打つ文字の下ごしらえ (`typing`・`secret`)

**Files:** Create: `src/typing.rs`, `src/secret.rs`

**Interfaces:**
- Consumes: `Layout`・`KeyStroke` (Task 2)
- Produces:
  - `secret::wipe<T: Copy>(buf: &mut [T], fill: T)` (volatile に上書き)、`pub struct SecretText(Vec<u16>)`
    (`from_utf16(Vec<u16>)`、`as_slice() -> &[u16]`、Drop で wipe)
  - `pub enum Mode { KeyPosition, Unicode }` (`key()` `"key"`・`"unicode"`、`from_key`)
  - `pub enum Stroke { Key(KeyStroke), Unicode(char) }`、`pub struct Strokes(Vec<Stroke>)` (Deref で `[Stroke]`、Drop で wipe)
  - `pub enum Refusal { Empty, LineBreakOrTab, Unsupported { count: usize } }`
  - `pub fn trim_trailing_newlines(text: &[u16]) -> &[u16]`
  - `pub fn typeable_len(text: &[u16]) -> usize` (末尾の改行を落とした後の文字数。surrogate の対は 1 文字)
  - `pub fn plan(text: &[u16], mode: Mode, layout: Layout) -> Result<Strokes, Refusal>`
  - `pub const TEST_SAMPLE: &str = "ABCXYZ abcxyz 0123456789 !\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~";`
  - `pub enum Outcome { Completed, StoppedByInput, StoppedByForeground, NotForeground, ImeOn, ModifierHeld, SendFailed }`
    (`code(self) -> usize`・`from_code(usize) -> Option<Outcome>`。thread の間で WPARAM に載せる)

- [ ] **Step 1: 失敗する test を書く** (`fn u(s: &str) -> Vec<u16>` を test の中に置く)

```rust
#[test]
fn trailing_crlf_variants_are_trimmed() {
    for s in ["pw\r\n", "pw\n", "pw\r", "pw\r\n\r\n", "pw\n\n"] {
        assert_eq!(trim_trailing_newlines(&u(s)), &u("pw")[..], "{s:?}");
        assert_eq!(typeable_len(&u(s)), 2);
    }
}
#[test]
fn inner_line_break_or_tab_is_refused() {
    for s in ["a\nb", "a\r\nb", "a\tb", "\tab"] {
        assert_eq!(plan(&u(s), Mode::KeyPosition, Layout::Us).err(), Some(Refusal::LineBreakOrTab));
        assert_eq!(plan(&u(s), Mode::Unicode, Layout::Us).err(), Some(Refusal::LineBreakOrTab));
    }
}
#[test]
fn empty_after_trim_is_refused() {
    for s in ["", "\r\n", "\n\n"] { assert_eq!(plan(&u(s), Mode::Unicode, Layout::Us).err(), Some(Refusal::Empty)); }
}
#[test]
fn unsupported_chars_are_counted_not_shown() {
    assert_eq!(plan(&u("aあbＡ"), Mode::KeyPosition, Layout::Jis).err(), Some(Refusal::Unsupported { count: 2 }));
}
#[test]
fn astral_char_counts_as_one() {
    assert_eq!(typeable_len(&u("a😀")), 2);
    assert_eq!(plan(&u("a😀"), Mode::KeyPosition, Layout::Us).err(), Some(Refusal::Unsupported { count: 1 }));
}
#[test]
fn unicode_mode_keeps_astral_char() {
    let s = plan(&u("aあ😀"), Mode::Unicode, Layout::Us).unwrap();
    assert_eq!(&s[..], &[Stroke::Unicode('a'), Stroke::Unicode('あ'), Stroke::Unicode('😀')]);
}
#[test]
fn unicode_mode_refuses_controls() {
    assert_eq!(plan(&u("a\u{7}b"), Mode::Unicode, Layout::Us).err(), Some(Refusal::Unsupported { count: 1 }));
}
#[test]
fn unpaired_surrogate_is_unsupported() {
    let text = vec![0x61, 0xD800, 0x62];
    assert_eq!(typeable_len(&text), 3);
    assert_eq!(plan(&text, Mode::Unicode, Layout::Us).err(), Some(Refusal::Unsupported { count: 1 }));
}
#[test]
fn key_mode_maps_shifted_chars() {
    let s = plan(&u("aA"), Mode::KeyPosition, Layout::Us).unwrap();
    assert_eq!(&s[..], &[Stroke::Key(KeyStroke { scancode: 0x1E, shift: false }), Stroke::Key(KeyStroke { scancode: 0x1E, shift: true })]);
}
#[test]
fn test_sample_is_typeable_everywhere() {
    for layout in Layout::ALL { for mode in [Mode::KeyPosition, Mode::Unicode] {
        assert_eq!(plan(&u(TEST_SAMPLE), mode, layout).unwrap().len(), TEST_SAMPLE.chars().count());
    } }
}
#[test]
fn outcome_codes_round_trip() { /* Outcome の全部で from_code(code()) == Some(o) */ }
#[test]
fn wipe_overwrites_every_element() {
    let mut v = vec![1u16, 2, 3];
    secret::wipe(&mut v, 0);
    assert_eq!(v, [0, 0, 0]);
}
```

- [ ] **Step 2:** `cargo test typing secret` が落ちるのを見る。
- [ ] **Step 3:** 実装。`plan` の順: 末尾の改行を落とす → 空なら `Empty` → `\r`・`\n`・`\t` があれば `LineBreakOrTab` →
  `char::decode_utf16` で 1 文字ずつ見て、キーの位置は `layout.stroke_for`、Unicode は制御文字 (`char::is_control`) と
  対の無い surrogate を打てない文字に数える → 1 個でもあれば `Unsupported { count }`。`Strokes` の Vec は文字数で
  `with_capacity` し、伸ばし直して写しが残らないようにする。
- [ ] **Step 4:** `cargo test` が通る。
- [ ] **Step 5:** commit。

## Task 4: 設定 (`settings`)

**Files:** Create: `src/settings.rs`

**Interfaces:**
- Consumes: `Mode` (Task 3)・`Layout` (Task 2)
- Produces:
  - `pub struct WindowRect { pub x: i32, pub y: i32, pub width: i32, pub height: i32 }`
  - `pub struct Settings { pub mode: Mode, pub layout: Option<Layout>, pub interval_ms: u32, pub always_on_top: bool, pub window: Option<WindowRect> }`
    (`layout: None` は「手元の今の配列に合わせる」)
  - `Default` (`KeyPosition`・`None`・30・`true`・`None`)、`pub const DEFAULT_INTERVAL_MS: u32 = 30`、`pub const MAX_INTERVAL_MS: u32 = 1000`
  - `Settings::parse(text: &str) -> Settings`、`Settings::to_text(&self) -> String` (`key=value` の行: `mode`・`layout`・
    `interval_ms`・`always_on_top`・`window=x,y,w,h`)
  - `pub fn default_path() -> Option<PathBuf>` (`%APPDATA%\type-clipboard\settings.ini`)、`pub fn load(path: &Path) -> Settings`
    (無い・読めないなら既定)、`pub fn save(path: &Path, s: &Settings) -> std::io::Result<()>` (親の dir を作る)

- [ ] **Step 1: 失敗する test を書く**

```rust
#[test]
fn round_trip() {
    let s = Settings { mode: Mode::Unicode, layout: Some(Layout::Us), interval_ms: 80, always_on_top: false,
        window: Some(WindowRect { x: -1200, y: 40, width: 380, height: 700 }) };
    assert_eq!(Settings::parse(&s.to_text()), s);
}
#[test]
fn broken_values_fall_back() {
    let s = Settings::parse("mode=fast\nlayout=dvorak\ninterval_ms=abc\nalways_on_top=maybe\nwindow=1,2\nunknown=1\n");
    assert_eq!(s, Settings::default());
    assert_eq!(Settings::parse("interval_ms=999999").interval_ms, MAX_INTERVAL_MS);
    assert_eq!(Settings::parse("interval_ms=-5").interval_ms, DEFAULT_INTERVAL_MS);
}
#[test]
fn load_missing_file_gives_default() { /* 無い path → Settings::default() */ }
#[test]
fn save_then_load() { /* std::env::temp_dir() の下の固有の dir に save → load が同じ。後で消す */ }
#[test]
fn no_text_fields_exist() { /* to_text() の key が mode・layout・interval_ms・always_on_top・window だけ */ }
```

- [ ] **Step 2:** 落ちるのを見る。 **Step 3:** 実装。 **Step 4:** 通る。 **Step 5:** commit。

## Task 5: 窓の判定と収め方 (`candidate`・`geometry`・`status`)

**Files:** Create: `src/candidate.rs`, `src/geometry.rs`, `src/status.rs`

**Interfaces:**
- Produces:
  - `pub struct WindowFacts { pub visible: bool, pub cloaked: bool, pub title_len: usize, pub tool_window: bool,
    pub app_window: bool, pub no_activate: bool, pub has_owner: bool, pub is_self: bool, pub shell: bool }`
    (`shell` は class が `Progman`・`WorkerW`・`Shell_TrayWnd`・`Shell_SecondaryTrayWnd`)
  - `pub fn is_listed(f: &WindowFacts) -> bool` (Alt+Tab に出る窓: 見えている・cloaked でない・題がある・自分でない・
    shell でない・`app_window` か (`tool_window` でも `no_activate` でもなく、owner が無い))
  - `pub fn is_pickable(f: &WindowFacts) -> bool` (クリックで選べる窓: 見えている・cloaked でない・自分でない・shell でない。
    owner のある窓・題の無い窓も選べる)
  - `pub enum Elevation { Higher, NotHigher, Unknown }`、`pub fn elevation_from_levels(ours: Option<u32>, target: Result<u32, LevelError>) -> Elevation`、
    `pub enum LevelError { AccessDenied, Other }`
  - `geometry::Rect { left, top, right, bottom: i32 }` (`width()`・`height()`)、
    `pub fn fit_within(src_w: i32, src_h: i32, area: Rect) -> Option<Rect>` (縦横比を保ち、拡大はしない、中央に置く)
  - `status::{Kind { Info, Busy, Ok, Warn }, Status { kind, text: String, sub: String }}` と、
    `refusal(&Refusal, Mode) -> Status`・`outcome(Outcome, typed: usize, total: usize, trial: bool) -> Status`・
    `typing(typed: usize, total: usize) -> Status`

- [ ] **Step 1: 失敗する test を書く**

```rust
#[test]
fn listed_follows_alt_tab_rules() {
    let base = WindowFacts { visible: true, cloaked: false, title_len: 5, tool_window: false, app_window: false,
        no_activate: false, has_owner: false, is_self: false, shell: false };
    assert!(is_listed(&base));
    assert!(!is_listed(&WindowFacts { cloaked: true, ..base }));
    assert!(!is_listed(&WindowFacts { title_len: 0, ..base }));
    assert!(!is_listed(&WindowFacts { tool_window: true, ..base }));
    assert!(is_listed(&WindowFacts { tool_window: true, app_window: true, ..base }));
    assert!(!is_listed(&WindowFacts { has_owner: true, ..base }));
    assert!(!is_listed(&WindowFacts { is_self: true, ..base }));
    assert!(!is_listed(&WindowFacts { shell: true, ..base }));
    assert!(is_pickable(&WindowFacts { has_owner: true, title_len: 0, ..base }));
    assert!(!is_pickable(&WindowFacts { shell: true, ..base }));
}
#[test]
fn elevation_from_levels() {
    use Elevation::*;
    assert!(matches!(elevation_from_levels(Some(0x2000), Ok(0x3000)), Higher));
    assert!(matches!(elevation_from_levels(Some(0x2000), Ok(0x2000)), NotHigher));
    assert!(matches!(elevation_from_levels(Some(0x2000), Err(LevelError::AccessDenied)), Higher));
    assert!(matches!(elevation_from_levels(Some(0x3000), Err(LevelError::AccessDenied)), Unknown));
    assert!(matches!(elevation_from_levels(Some(0x2000), Err(LevelError::Other)), Unknown));
    assert!(matches!(elevation_from_levels(None, Ok(0x3000)), Unknown));
}
#[test]
fn fit_keeps_aspect_and_never_upscales() {
    let area = Rect { left: 10, top: 20, right: 210, bottom: 120 };           // 200x100
    assert_eq!(fit_within(400, 400, area), Some(Rect { left: 60, top: 20, right: 160, bottom: 120 }));
    assert_eq!(fit_within(100, 50, area), Some(Rect { left: 60, top: 45, right: 160, bottom: 95 }));
    let wide = fit_within(3000, 10, area).unwrap();
    assert!(wide.width() <= 200 && wide.height() >= 1);
    assert_eq!(fit_within(0, 100, area), None);
}
#[test]
fn status_texts() {
    assert_eq!(status::outcome(Outcome::Completed, 60, 60, false).text, "60 文字を打ちました");
    assert_eq!(status::outcome(Outcome::StoppedByForeground, 12, 60, false).text, "止めました (12 / 60)");
    assert_eq!(status::typing(3, 60).text, "打っています 3 / 60");
    assert!(status::refusal(&Refusal::Unsupported { count: 2 }, Mode::KeyPosition).text.contains("2"));
}
```

- [ ] **Step 2:** 落ちるのを見る。 **Step 3:** 実装 (文言は spec §4 の画面の動きに合わせる。完了の副の行は
  「Enter は打つ先で自分で押す」、止めた副の行は理由 (「ほかの窓が前に来たため」「キー入力かクリックがあったため」)、
  試し打ちの完了は副の行に `見本: {TEST_SAMPLE}`)。 **Step 4:** 通る。 **Step 5:** commit。

## Task 6: Win32 の土台 (主窓・コントロール・設定・クリップボード)

**Files:** Create: `src/win/mod.rs`, `src/win/app.rs`, `src/win/ui.rs`, `src/win/clipboard.rs`, `src/win/target.rs`

**Interfaces:**
- Consumes: `settings::*` (Task 4)、`typing::typeable_len`・`SecretText` (Task 3)、`candidate::*` (Task 5)
- Produces:
  - `win::run() -> windows::core::Result<()>`
  - `target::WindowEntry { hwnd: HWND, title: String, exe: String }`、`target::list(self_hwnd: HWND) -> Vec<WindowEntry>`、
    `target::entry(hwnd: HWND) -> WindowEntry`、`target::facts(hwnd: HWND, self_hwnd: HWND) -> WindowFacts`、
    `target::elevation(hwnd: HWND) -> Elevation`、`target::visible_bounds(hwnd: HWND) -> Option<RECT>`
  - `clipboard::read(owner: HWND) -> ClipText` (`enum ClipText { Text(SecretText), Empty, NotText, Busy }`)、
    `clipboard::listen(hwnd)`・`clipboard::unlisten(hwnd)`
  - 主窓の WM_APP の番号 (`app.rs` に定数): `WM_APP_PROGRESS`・`WM_APP_DONE`・`WM_APP_PICK_MOVE`・`WM_APP_PICK_CLICK`・`WM_APP_PICK_CANCEL`

- [ ] **Step 1:** 主窓 (題 `type-clipboard`、`WS_EX_CONTROLPARENT`、`WS_CLIPCHILDREN`) と、spec §4 の順のコントロールを作る:
  「打つ先」+「↻」+「クリックで選ぶ」/ ListView (列「窓の題」「exe」、report・1 行選択・行全体) / プレビューの欄
  (WM_PAINT で描く) と下の題 / 「打ち方」のラジオ 2 つ (「キーの位置 (VNC・RDP 向け)」「文字 (Unicode)」) / 「打つ先の配列」の
  combobox / 「1 文字の間隔 (ms)」の edit と up-down (0〜1000) / 「常に手前」/ クリップボードの行 / 「クリップボードを打つ」
  「試し打ち」/ 状態の 2 行と進み具合のバー。Tab で移れるよう `IsDialogMessageW` を message loop で通す。
- [ ] **Step 2:** 配置は WM_SIZE で計算し、ListView は約 7 行の高さ、プレビューに残りを渡す。DPI は `GetDpiForWindow` で
  掛け、`WM_DPICHANGED` でフォント (`SystemParametersInfoForDpi` の message font と太字) を作り直して並べ直す。
  `WM_GETMINMAXINFO` で最小の大きさを決める。
- [ ] **Step 3:** 設定を起動時に読み、配列が `None` なら `GetKeyboardLayout(0)` の言語が日本語 (0x11) なら JIS、ほかは US。
  保存した位置が `MonitorFromRect(.., MONITOR_DEFAULTTONULL)` で画面に無ければ使わない。閉じるときに保存する。
  「常に手前」は `SetWindowPos(HWND_TOPMOST/NOTOPMOST)`。
- [ ] **Step 4:** クリップボード: `AddClipboardFormatListener` と `WM_CLIPBOARDUPDATE` で数え直し、「クリップボード: N 文字」
  「クリップボード: 空」「クリップボード: 文字ではない」を出す。読んだ写しはその場で消す (`SecretText` の Drop)。
- [ ] **Step 5:** 窓の一覧: `EnumWindows` → `target::facts` → `is_listed`。Z の順のまま並べ、↻ と主窓が前に来たときに読み直す
  (打つ先が一覧にあれば選んだままにする)。500 ms の timer で打つ先が閉じたかを見て、閉じたら外して「打つ先の窓が閉じました」。
- [ ] **Step 6:** 確かめる: `cargo clippy --target x86_64-pc-windows-msvc --all-targets -- -D warnings` が通り、`cargo xwin build`
  の exe を起こすと窓が出る (見た目だけ見て閉じる)。
- [ ] **Step 7:** commit。

## Task 7: プレビューと枠と「クリックで選ぶ」

**Files:** Create: `src/win/preview.rs`, `src/win/frame.rs`, `src/win/hooks.rs`; Modify: `src/win/app.rs`

**Interfaces:**
- Consumes: `geometry::fit_within` (Task 5)、`target::visible_bounds` (Task 6)
- Produces:
  - `preview::Preview::new(dest: HWND)`、`.show(source: Option<HWND>)`、`.place(area: geometry::Rect)`、
    `.source_is_minimized() -> bool`
  - `frame::Frame::new() -> Result<Frame>`、`.show_around(target: HWND, color: COLORREF)`、`.hide()`
  - `hooks::HookMode { Pick, Typing }`、`hooks::install(mode, notify: HWND, abort: Arc<AtomicU8>) -> Result<HookGuard>`
    (Drop で外す)、`hooks::INPUT_MARK: usize` (自分が送る入力の `dwExtraInfo`)、`hooks::last_pick_point() -> POINT`

- [ ] **Step 1:** プレビュー: 見せる窓 = 候補か、無ければ打つ先。替わったら `DwmUnregisterThumbnail` → `DwmRegisterThumbnail`。
  欄の下に札の帯を空け、残りに `fit_within` で収める。枠の色は候補が橙 (#F59E0B)、打つ先が青 (#2563EB)、空は灰の破線と
  「打つ先を選ぶと、ここに映る」。札は左下に「候補」(橙) と「打つ先 ●」(青)。最小化した窓は thumbnail を隠して
  「最小化しているので映りません」。下の題は「題 (exe)」。
- [ ] **Step 2:** 枠: `WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE` の popup に、
  外の矩形から内の矩形を抜いた region を付ける。位置は `DWMWA_EXTENDED_FRAME_BOUNDS`。
- [ ] **Step 3:** 一覧をなぞる: ListView を `SetWindowSubclass` で包み、`WM_MOUSEMOVE` の `LVM_HITTEST` で候補を出し
  (橙の枠)、`WM_MOUSELEAVE` で消す。選ぶ (`LVN_ITEMCHANGED`) と打つ先にし、青の枠を 700 ms だけ出す。
- [ ] **Step 4:** 「クリックで選ぶ」: `hooks::install(Pick, ..)`。mouse の移動で `WindowFromPoint` → `GetAncestor(GA_ROOT)` →
  `is_pickable` なら候補。左クリックは hook で握りつぶして (相手に届けない) 打つ先に決める。自分の窓や選べない所の
  クリック・右クリック・Esc はやめる。状態の行は「選びたい窓をクリック」/「Esc でやめる」。
- [ ] **Step 5:** 確かめる: clippy が通る。exe を起こし、自分で起こしたメモ帳を一覧から選ぶ操作を、ListView への
  `LVM_SETITEMSTATE` で外から与え (キーやマウスは送らない)、プレビューに映るのを見て閉じる。
- [ ] **Step 6:** commit。

## Task 8: 打つ (`typer`)

**Files:** Create: `src/win/typer.rs`; Modify: `src/win/app.rs`, `src/win/hooks.rs`

**Interfaces:**
- Consumes: `typing::{plan, Strokes, Stroke, Outcome, TEST_SAMPLE}` (Task 3)、`status::*` (Task 5)、`hooks::*` (Task 7)、
  `target::elevation` (Task 6)、`clipboard::read` (Task 6)
- Produces: `typer::Job { target: isize, strokes: Strokes, interval_ms: u32, notify: isize, abort: Arc<AtomicU8> }`、
  `typer::spawn(job: Job) -> std::thread::JoinHandle<()>`、`typer::modifiers_held() -> bool`、`typer::caps_lock_on() -> bool`、
  `typer::ime_open(target: HWND) -> Option<bool>`

- [ ] **Step 1:** ボタンを押したとき (UI の thread): 打つ先が無い・閉じた → 断る。`elevation` が `Higher` → 「打つ先は管理者
  として動いているので打てません」/「このアプリも管理者として起こせば打てます」。文字を読む (試し打ちは `TEST_SAMPLE`) →
  `plan` → 断るなら `status::refusal`。修飾キーが押されたまま → 断る。キーの位置の方式で CapsLock が on → 断る。
  通ったら打つ先を前に出す (`IsIconic` なら `SW_RESTORE`、`SetForegroundWindow`)、`hooks::install(Typing, ..)`、
  コントロールを止め、`typer::spawn`。
- [ ] **Step 2:** thread: 500 ms 待つ (その間も止める合図を見る) → 前面が打つ先でなければ `NotForeground` → `ime_open` が
  `Some(true)` なら `ImeOn` → 修飾キーが押されたままなら `ModifierHeld` → 1 文字ずつ: 止める合図 (`StoppedByInput`) と前面
  (`StoppedByForeground`) を見てから、key down の列を `SendInput` で送り、5 ms 置いて key up の列を送る (Shift は文字の前後で
  押して離す。Unicode は UTF-16 の単位ごとに down/up)。`dwExtraInfo` に `INPUT_MARK`。送れなければ `SendFailed`。
  1 文字ごとに `WM_APP_PROGRESS`、終わりに `WM_APP_DONE` を `PostMessageW`。間隔は設定の ms。
- [ ] **Step 3:** hook の Typing: 自分の印の無いキーの押下 (握りつぶす) と、印の無いマウスのボタンの押下で止める合図を立てる。
- [ ] **Step 4:** `WM_APP_DONE` で hook を外し、コントロールを戻し、`status::outcome` を出す。進み具合のバーは完了で通常、
  止めたら `PBST_PAUSED`。`Strokes` は thread の終わりで Drop され消える。
- [ ] **Step 5:** 確かめる: clippy が通る (打つ処理は手元の画面では動かさない。手で確かめるのは PR の「手で確かめる」)。
- [ ] **Step 6:** commit。

## Task 9: CI と Release

**Files:** Create: `.github/workflows/ci.yml`, `.github/workflows/release.yml`

- [ ] **Step 1:** `ci.yml` (`pull_request` と main への `push`、`permissions: contents: read`):
  job `build` (windows-latest): checkout (`persist-credentials: false`) → `rustup toolchain install` → `cargo fmt --all --check` →
  `cargo clippy --all-targets --locked -- -D warnings` → `cargo test --locked` → `cargo build --release --locked` →
  upload-artifact (名前 `type-clipboard`、`target/release/type-clipboard.exe`、`if-no-files-found: error`)。
  job `lock-age` (ubuntu-latest): checkout → nix-installer-action → `nix run 'github:pollenjp/dotfiles?dir=nix#flake-lock-age' -- check`。
- [ ] **Step 2:** `release.yml` (tag `v*`): job `build` (windows-latest、`contents: read`) が exe と `.sha256` を artifact に上げ、
  job `release` (ubuntu-latest、`contents: write`、`needs: build`) が落として `gh release create "$TAG" --verify-tag --generate-notes` で置く。
- [ ] **Step 3:** Actions は猶予 (7 日) を守った release の SHA で固定し、行末に `# vX.Y.Z`。
- [ ] **Step 4:** commit。

## Task 10: 文書

**Files:** Create: `LICENSE`; Modify: `README.md`

- [ ] **Step 1:** `LICENSE` は MIT、著作者の行は `Copyright (c) 2026 pollenjp`。
- [ ] **Step 2:** README (日本語): 何をするか・使い方・2 つの打ち方の違い・配列の選び方 (RDP・VMConnect は接続先の OS の配列、
  VNC はサーバーがキーの位置を受け取るか (noVNC と QEMU の拡張キーイベントなど) で変わり、試し打ちで確かめる)・試し打ち・
  断るとき・止まるとき・昇格した窓・SmartScreen の警告・exe の取り方 (Release と CI の artifact)・手元のビルド・設定の場所・
  低レベルの hook を使う理由・ライセンス。例は vm01・example.com。
- [ ] **Step 3:** commit。

## Task 11: 仕上げ

- [ ] **Step 1:** `cargo fmt --check`・`cargo clippy --target x86_64-pc-windows-msvc --all-targets -- -D warnings`・`cargo test` を通す。
- [ ] **Step 2:** 禁止語の一覧 (repo の外) と `git diff origin/main...HEAD`・`git log --format=%B origin/main..HEAD` を突き合わせる。
- [ ] **Step 3:** push して PR を作り、CI が通るまで直す。PR の本文に、決めたこと・手で確かめること (spec §6)・CI の artifact
  から exe を取る手順を書く。
