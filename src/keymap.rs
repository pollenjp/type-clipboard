//! 配列 (日本語 (JIS)・英語 (US)) の表: 文字を、そのキーの位置 (scancode) と Shift に直す。
//!
//! RDP・VMConnect・VNC は押したキーの位置を相手へ送り、相手の OS が自分の配列で文字に直す。
//! そのため、相手の配列でその文字を出すキーの位置を送る。scancode は Set 1 (Windows の scancode)。

/// 打つ先 (相手の OS) のキーボードの配列。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Layout {
    /// 日本語 (JIS) の 106/109 キー
    Jis,
    /// 英語 (US) の 101/104 キー
    Us,
}

/// 1 文字を打つキーの位置と、Shift を押すか。打つ文字と同じ値になるので、Debug で中身を出さない。
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct KeyStroke {
    pub scancode: u16,
    pub shift: bool,
}

impl std::fmt::Debug for KeyStroke {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("KeyStroke(..)")
    }
}

/// 左の Shift の scancode。
pub const SHIFT_SCANCODE: u16 = 0x2A;

const fn plain(scancode: u16) -> Option<KeyStroke> {
    Some(KeyStroke {
        scancode,
        shift: false,
    })
}

const fn shifted(scancode: u16) -> Option<KeyStroke> {
    Some(KeyStroke {
        scancode,
        shift: true,
    })
}

impl Layout {
    pub const ALL: [Layout; 2] = [Layout::Jis, Layout::Us];

    /// 画面に出す名前。
    pub fn label(self) -> &'static str {
        match self {
            Layout::Jis => "日本語 (JIS)",
            Layout::Us => "英語 (US)",
        }
    }

    /// 設定のファイルに書く名前。
    pub fn key(self) -> &'static str {
        match self {
            Layout::Jis => "jis",
            Layout::Us => "us",
        }
    }

    pub fn from_key(key: &str) -> Option<Layout> {
        Self::ALL.into_iter().find(|layout| layout.key() == key)
    }

    /// `c` を打つキーの位置。この配列に無い文字 (ASCII の外・制御文字) は `None`。
    pub fn stroke_for(self, c: char) -> Option<KeyStroke> {
        if c.is_ascii_alphabetic() {
            let scancode = letter_scancode(c.to_ascii_lowercase())?;
            return Some(KeyStroke {
                scancode,
                shift: c.is_ascii_uppercase(),
            });
        }
        match c {
            ' ' => plain(0x39),
            '1'..='9' => plain(0x02 + (c as u16 - '1' as u16)),
            '0' => plain(0x0B),
            _ => match self {
                Layout::Jis => jis_symbol(c),
                Layout::Us => us_symbol(c),
            },
        }
    }
}

/// 英字のキーの位置 (JIS も US も同じ QWERTY)。
fn letter_scancode(c: char) -> Option<u16> {
    const TOP: &str = "qwertyuiop";
    const HOME: &str = "asdfghjkl";
    const BOTTOM: &str = "zxcvbnm";
    for (row, start) in [(TOP, 0x10), (HOME, 0x1E), (BOTTOM, 0x2C)] {
        if let Some(i) = row.find(c) {
            return Some(start + i as u16);
        }
    }
    None
}

fn jis_symbol(c: char) -> Option<KeyStroke> {
    match c {
        '!' => shifted(0x02),
        '"' => shifted(0x03),
        '#' => shifted(0x04),
        '$' => shifted(0x05),
        '%' => shifted(0x06),
        '&' => shifted(0x07),
        '\'' => shifted(0x08),
        '(' => shifted(0x09),
        ')' => shifted(0x0A),
        '-' => plain(0x0C),
        '=' => shifted(0x0C),
        '^' => plain(0x0D),
        '~' => shifted(0x0D),
        // ¥ のキー: Shift なしは \ だが、\ はろのキーで打つ
        '|' => shifted(0x7D),
        '@' => plain(0x1A),
        '`' => shifted(0x1A),
        '[' => plain(0x1B),
        '{' => shifted(0x1B),
        ';' => plain(0x27),
        '+' => shifted(0x27),
        ':' => plain(0x28),
        '*' => shifted(0x28),
        ']' => plain(0x2B),
        '}' => shifted(0x2B),
        ',' => plain(0x33),
        '<' => shifted(0x33),
        '.' => plain(0x34),
        '>' => shifted(0x34),
        '/' => plain(0x35),
        '?' => shifted(0x35),
        // ろのキー
        '\\' => plain(0x73),
        '_' => shifted(0x73),
        _ => None,
    }
}

fn us_symbol(c: char) -> Option<KeyStroke> {
    match c {
        '`' => plain(0x29),
        '~' => shifted(0x29),
        '!' => shifted(0x02),
        '@' => shifted(0x03),
        '#' => shifted(0x04),
        '$' => shifted(0x05),
        '%' => shifted(0x06),
        '^' => shifted(0x07),
        '&' => shifted(0x08),
        '*' => shifted(0x09),
        '(' => shifted(0x0A),
        ')' => shifted(0x0B),
        '-' => plain(0x0C),
        '_' => shifted(0x0C),
        '=' => plain(0x0D),
        '+' => shifted(0x0D),
        '[' => plain(0x1A),
        '{' => shifted(0x1A),
        ']' => plain(0x1B),
        '}' => shifted(0x1B),
        '\\' => plain(0x2B),
        '|' => shifted(0x2B),
        ';' => plain(0x27),
        ':' => shifted(0x27),
        '\'' => plain(0x28),
        '"' => shifted(0x28),
        ',' => plain(0x33),
        '<' => shifted(0x33),
        '.' => plain(0x34),
        '>' => shifted(0x34),
        '/' => plain(0x35),
        '?' => shifted(0x35),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_printable_ascii_maps_in_both_layouts() {
        for layout in Layout::ALL {
            for b in 0x20u8..=0x7E {
                assert!(
                    layout.stroke_for(b as char).is_some(),
                    "{layout:?} {:?}",
                    b as char
                );
            }
        }
    }

    #[test]
    fn layouts_differ_where_keycaps_differ() {
        assert_eq!(
            Layout::Jis.stroke_for('@'),
            Some(KeyStroke {
                scancode: 0x1A,
                shift: false
            })
        );
        assert_eq!(
            Layout::Us.stroke_for('@'),
            Some(KeyStroke {
                scancode: 0x03,
                shift: true
            })
        );
        assert_eq!(
            Layout::Jis.stroke_for('_'),
            Some(KeyStroke {
                scancode: 0x73,
                shift: true
            })
        );
        assert_eq!(
            Layout::Jis.stroke_for('\\'),
            Some(KeyStroke {
                scancode: 0x73,
                shift: false
            })
        );
        assert_eq!(
            Layout::Jis.stroke_for('|'),
            Some(KeyStroke {
                scancode: 0x7D,
                shift: true
            })
        );
        assert_eq!(
            Layout::Us.stroke_for('A'),
            Some(KeyStroke {
                scancode: 0x1E,
                shift: true
            })
        );
        assert_eq!(
            Layout::Us.stroke_for(' '),
            Some(KeyStroke {
                scancode: 0x39,
                shift: false
            })
        );
    }

    #[test]
    fn non_ascii_and_controls_are_not_mapped() {
        for c in ['あ', 'Ａ', '€', '¥', '\t', '\n', '\u{7F}'] {
            assert_eq!(Layout::Jis.stroke_for(c), None, "{c:?}");
            assert_eq!(Layout::Us.stroke_for(c), None, "{c:?}");
        }
    }

    #[test]
    fn each_key_and_shift_gives_one_char() {
        for layout in Layout::ALL {
            let mut seen = std::collections::HashMap::new();
            for b in 0x20u8..=0x7E {
                let stroke = layout.stroke_for(b as char).unwrap();
                let key = (stroke.scancode, stroke.shift);
                if let Some(prev) = seen.insert(key, b as char) {
                    panic!("{layout:?}: {prev:?} と {:?} が同じキー {key:?}", b as char);
                }
            }
        }
    }

    #[test]
    fn layout_keys_round_trip() {
        for layout in Layout::ALL {
            assert_eq!(Layout::from_key(layout.key()), Some(layout));
        }
        assert_eq!(Layout::from_key("dvorak"), None);
    }
}
