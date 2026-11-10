//! 設定: 打ち方・配列・間隔・常に手前・窓の位置。打つ先とクリップボードの中身は持たない。
//!
//! `%APPDATA%\type-clipboard\settings.ini` に `key=value` の行で書く。

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::keymap::Layout;
use crate::typing::Mode;

/// 1 文字ごとの間隔の既定 (ms)。
pub const DEFAULT_INTERVAL_MS: u32 = 30;
/// 1 文字ごとの間隔の上限 (ms)。
pub const MAX_INTERVAL_MS: u32 = 1000;
/// 打つ先の配列の既定 (設定に無いとき)。手元の Windows の配列にはよらない。
pub const DEFAULT_LAYOUT: Layout = Layout::Us;

/// 窓の位置と大きさ (画面の座標・物理 pixel)。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub mode: Mode,
    pub layout: Layout,
    pub interval_ms: u32,
    pub always_on_top: bool,
    pub window: Option<WindowRect>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            mode: Mode::KeyPosition,
            layout: DEFAULT_LAYOUT,
            interval_ms: DEFAULT_INTERVAL_MS,
            always_on_top: true,
            window: None,
        }
    }
}

impl Settings {
    /// 設定のファイルの中身を読む。読めない行・値は飛ばして既定のままにする (落ちない)。
    pub fn parse(text: &str) -> Settings {
        let mut s = Settings::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "mode" => {
                    if let Some(mode) = Mode::from_key(value) {
                        s.mode = mode;
                    }
                }
                "layout" => {
                    if let Some(layout) = Layout::from_key(value) {
                        s.layout = layout;
                    }
                }
                "interval_ms" => {
                    if let Ok(ms) = value.parse::<u32>() {
                        s.interval_ms = ms.min(MAX_INTERVAL_MS);
                    }
                }
                "always_on_top" => {
                    if let Ok(on) = value.parse::<bool>() {
                        s.always_on_top = on;
                    }
                }
                "window" => s.window = parse_rect(value),
                _ => {}
            }
        }
        s
    }

    pub fn to_text(&self) -> String {
        let mut text = String::new();
        let _ = writeln!(text, "mode={}", self.mode.key());
        let _ = writeln!(text, "layout={}", self.layout.key());
        let _ = writeln!(text, "interval_ms={}", self.interval_ms);
        let _ = writeln!(text, "always_on_top={}", self.always_on_top);
        if let Some(r) = self.window {
            let _ = writeln!(text, "window={},{},{},{}", r.x, r.y, r.width, r.height);
        }
        text
    }
}

fn parse_rect(value: &str) -> Option<WindowRect> {
    let numbers: Vec<i32> = value
        .split(',')
        .map(|part| part.trim().parse().ok())
        .collect::<Option<_>>()?;
    let [x, y, width, height] = numbers[..] else {
        return None;
    };
    (width > 0 && height > 0).then_some(WindowRect {
        x,
        y,
        width,
        height,
    })
}

/// `%APPDATA%\type-clipboard\settings.ini`。APPDATA が無ければ `None` (保存しない)。
pub fn default_path() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(
        PathBuf::from(appdata)
            .join("type-clipboard")
            .join("settings.ini"),
    )
}

/// 無い・読めないときは既定。
pub fn load(path: &Path) -> Settings {
    std::fs::read_to_string(path)
        .map(|text| Settings::parse(&text))
        .unwrap_or_default()
}

/// 親の dir が無ければ作ってから書く。
#[expect(
    clippy::disallowed_methods,
    reason = "ディスクに書くのは、この設定のファイル (打ち方・配列・間隔・常に手前・窓の位置) だけ"
)]
pub fn save(path: &Path, settings: &Settings) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, settings.to_text())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::Layout;
    use crate::typing::Mode;

    #[test]
    fn round_trip() {
        let s = Settings {
            mode: Mode::Unicode,
            layout: Layout::Jis,
            interval_ms: 80,
            always_on_top: false,
            window: Some(WindowRect {
                x: -1200,
                y: 40,
                width: 380,
                height: 700,
            }),
        };
        assert_eq!(Settings::parse(&s.to_text()), s);
        assert_eq!(
            Settings::parse(&Settings::default().to_text()),
            Settings::default()
        );
    }

    #[test]
    fn defaults_follow_the_spec() {
        let s = Settings::default();
        assert_eq!(s.mode, Mode::KeyPosition);
        assert_eq!(s.layout, Layout::Us);
        assert_eq!(s.interval_ms, 30);
        assert!(s.always_on_top);
        assert_eq!(s.window, None);
    }

    #[test]
    fn saved_layout_wins_over_the_default() {
        assert_eq!(Settings::parse("").layout, Layout::Us);
        assert_eq!(Settings::parse("layout=jis\n").layout, Layout::Jis);
        assert_eq!(Settings::parse("layout=us\n").layout, Layout::Us);
    }

    #[test]
    fn broken_values_fall_back() {
        let s = Settings::parse(
            "mode=fast\nlayout=dvorak\ninterval_ms=abc\nalways_on_top=maybe\nwindow=1,2\nunknown=1\nno equals sign\n",
        );
        assert_eq!(s, Settings::default());
        assert_eq!(
            Settings::parse("interval_ms=999999").interval_ms,
            MAX_INTERVAL_MS
        );
        assert_eq!(
            Settings::parse("interval_ms=-5").interval_ms,
            DEFAULT_INTERVAL_MS
        );
        assert_eq!(Settings::parse("window=1,2,0,100").window, None);
        assert_eq!(Settings::parse("window=a,b,c,d").window, None);
    }

    #[test]
    fn load_missing_file_gives_default() {
        let path = std::env::temp_dir().join("type-clipboard-test-missing/settings.ini");
        assert_eq!(load(&path), Settings::default());
    }

    #[test]
    fn save_then_load() {
        let dir = std::env::temp_dir().join(format!("type-clipboard-test-{}", std::process::id()));
        let path = dir.join("nested").join("settings.ini");
        let s = Settings {
            interval_ms: 120,
            ..Settings::default()
        };
        save(&path, &s).unwrap();
        assert_eq!(load(&path), s);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn only_known_keys_are_written() {
        let s = Settings {
            layout: Layout::Jis,
            window: Some(WindowRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            }),
            ..Settings::default()
        };
        for line in s.to_text().lines() {
            let key = line.split_once('=').unwrap().0;
            assert!(
                ["mode", "layout", "interval_ms", "always_on_top", "window"].contains(&key),
                "{key}"
            );
        }
    }
}
