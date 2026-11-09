//! 打つ文字の下ごしらえ: 末尾の改行を落とし、打たずに断る条件を見て、打つ列 (`Strokes`) を作る。

use std::ops::Deref;

use crate::keymap::{KeyStroke, Layout};
use crate::secret::wipe;

/// 打ち方。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// 文字を、選んだ配列でのキーの位置 (scancode) と Shift に直して送る (RDP・VMConnect・VNC 向け)
    KeyPosition,
    /// 文字そのものを送る (KEYEVENTF_UNICODE。ふつうのアプリ向け)
    Unicode,
}

impl Mode {
    /// 設定のファイルに書く名前。
    pub fn key(self) -> &'static str {
        match self {
            Mode::KeyPosition => "key",
            Mode::Unicode => "unicode",
        }
    }

    pub fn from_key(key: &str) -> Option<Mode> {
        [Mode::KeyPosition, Mode::Unicode]
            .into_iter()
            .find(|mode| mode.key() == key)
    }
}

/// 1 文字ぶんの送り方。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stroke {
    /// このキーの位置を押す (Shift も)
    Key(KeyStroke),
    /// この文字を Unicode で送る (送るときに UTF-16 に直す)
    Unicode(char),
}

/// 打つ列。Drop で上書きして消す (中身はクリップボードの文字と同じ値になるため)。
pub struct Strokes(Vec<Stroke>);

impl Deref for Strokes {
    type Target = [Stroke];

    fn deref(&self) -> &[Stroke] {
        &self.0
    }
}

impl Drop for Strokes {
    fn drop(&mut self) {
        wipe(&mut self.0, Stroke::Unicode('\0'));
    }
}

impl std::fmt::Debug for Strokes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Strokes({} chars)", self.0.len())
    }
}

/// 打たずに断るわけ。どの文字かは持たない (画面に出さないため)。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// 打つ文字が無い (空か、末尾の改行だけ)
    Empty,
    /// 途中に改行かタブがある (Enter・Tab を押すことになる)
    LineBreakOrTab,
    /// 打てない文字がある (選んだ配列に無い・制御文字・壊れた UTF-16)
    Unsupported { count: usize },
}

/// 試し打ちの見本: 英字の大文字と小文字・数字・ASCII の記号の一通り。
pub const TEST_SAMPLE: &str = "ABCXYZ abcxyz 0123456789 !\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~";

const CR: u16 = 0x0D;
const LF: u16 = 0x0A;
const TAB: u16 = 0x09;

/// 末尾の改行 (CR・LF の並び) を落とす。
pub fn trim_trailing_newlines(text: &[u16]) -> &[u16] {
    let mut end = text.len();
    while end > 0 && matches!(text[end - 1], CR | LF) {
        end -= 1;
    }
    &text[..end]
}

/// 打つ文字の数 (末尾の改行を落とした後)。surrogate の対は 1 文字、対の無い surrogate も 1 文字と数える。
pub fn typeable_len(text: &[u16]) -> usize {
    char::decode_utf16(trim_trailing_newlines(text).iter().copied()).count()
}

/// `text` を打つ列にする。打てないときは、打たずに断るわけを返す。
pub fn plan(text: &[u16], mode: Mode, layout: Layout) -> Result<Strokes, Refusal> {
    let text = trim_trailing_newlines(text);
    if text.is_empty() {
        return Err(Refusal::Empty);
    }
    if text.iter().any(|&unit| matches!(unit, CR | LF | TAB)) {
        return Err(Refusal::LineBreakOrTab);
    }
    // UTF-16 の単位の数 >= 文字の数なので、伸ばし直さない (写しを残さない)
    let mut strokes = Strokes(Vec::with_capacity(text.len()));
    let mut unsupported = 0;
    for decoded in char::decode_utf16(text.iter().copied()) {
        let stroke = match (decoded, mode) {
            (Ok(c), Mode::KeyPosition) => layout.stroke_for(c).map(Stroke::Key),
            (Ok(c), Mode::Unicode) if !c.is_control() => Some(Stroke::Unicode(c)),
            _ => None,
        };
        match stroke {
            Some(stroke) => strokes.0.push(stroke),
            None => unsupported += 1,
        }
    }
    if unsupported > 0 {
        return Err(Refusal::Unsupported { count: unsupported });
    }
    Ok(strokes)
}

/// 打ち終わり・止まったわけ。打つ thread から UI の thread へ WPARAM に載せて送る。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// 全部打った
    Completed,
    /// 利用者のキー入力かクリックで止めた
    StoppedByInput,
    /// ほかの窓が前に来たので止めた
    StoppedByForeground,
    /// 打つ先を前に出せなかった
    NotForeground,
    /// 打つ先で IME がオンだった
    ImeOn,
    /// 打ち始めに修飾キーが押されたままだった
    ModifierHeld,
    /// キーを送れなかった (SendInput が受け付けなかった)
    SendFailed,
}

impl Outcome {
    pub const ALL: [Outcome; 7] = [
        Outcome::Completed,
        Outcome::StoppedByInput,
        Outcome::StoppedByForeground,
        Outcome::NotForeground,
        Outcome::ImeOn,
        Outcome::ModifierHeld,
        Outcome::SendFailed,
    ];

    pub fn code(self) -> usize {
        self as usize
    }

    pub fn from_code(code: usize) -> Option<Outcome> {
        Self::ALL.into_iter().find(|outcome| outcome.code() == code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::{KeyStroke, Layout};

    fn u(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    #[test]
    fn trailing_crlf_variants_are_trimmed() {
        for s in ["pw\r\n", "pw\n", "pw\r", "pw\r\n\r\n", "pw\n\n"] {
            assert_eq!(trim_trailing_newlines(&u(s)), &u("pw")[..], "{s:?}");
            assert_eq!(typeable_len(&u(s)), 2, "{s:?}");
        }
    }

    #[test]
    fn inner_line_break_or_tab_is_refused() {
        for s in ["a\nb", "a\r\nb", "a\tb", "\tab"] {
            assert_eq!(
                plan(&u(s), Mode::KeyPosition, Layout::Us).err(),
                Some(Refusal::LineBreakOrTab),
                "{s:?}"
            );
            assert_eq!(
                plan(&u(s), Mode::Unicode, Layout::Us).err(),
                Some(Refusal::LineBreakOrTab),
                "{s:?}"
            );
        }
    }

    #[test]
    fn empty_after_trim_is_refused() {
        for s in ["", "\r\n", "\n\n"] {
            assert_eq!(
                plan(&u(s), Mode::Unicode, Layout::Us).err(),
                Some(Refusal::Empty),
                "{s:?}"
            );
        }
    }

    #[test]
    fn unsupported_chars_are_counted_not_shown() {
        assert_eq!(
            plan(&u("aあbＡ"), Mode::KeyPosition, Layout::Jis).err(),
            Some(Refusal::Unsupported { count: 2 })
        );
    }

    #[test]
    fn astral_char_counts_as_one() {
        assert_eq!(typeable_len(&u("a😀")), 2);
        assert_eq!(
            plan(&u("a😀"), Mode::KeyPosition, Layout::Us).err(),
            Some(Refusal::Unsupported { count: 1 })
        );
    }

    #[test]
    fn unicode_mode_keeps_astral_char() {
        let s = plan(&u("aあ😀"), Mode::Unicode, Layout::Us).unwrap();
        assert_eq!(
            &s[..],
            &[
                Stroke::Unicode('a'),
                Stroke::Unicode('あ'),
                Stroke::Unicode('😀')
            ]
        );
    }

    #[test]
    fn unicode_mode_refuses_controls() {
        assert_eq!(
            plan(&u("a\u{7}b"), Mode::Unicode, Layout::Us).err(),
            Some(Refusal::Unsupported { count: 1 })
        );
    }

    #[test]
    fn unpaired_surrogate_is_unsupported() {
        let text = vec![0x61, 0xD800, 0x62];
        assert_eq!(typeable_len(&text), 3);
        assert_eq!(
            plan(&text, Mode::Unicode, Layout::Us).err(),
            Some(Refusal::Unsupported { count: 1 })
        );
    }

    #[test]
    fn key_mode_maps_shifted_chars() {
        let s = plan(&u("aA"), Mode::KeyPosition, Layout::Us).unwrap();
        assert_eq!(
            &s[..],
            &[
                Stroke::Key(KeyStroke {
                    scancode: 0x1E,
                    shift: false
                }),
                Stroke::Key(KeyStroke {
                    scancode: 0x1E,
                    shift: true
                })
            ]
        );
    }

    #[test]
    fn test_sample_is_typeable_everywhere() {
        for layout in Layout::ALL {
            for mode in [Mode::KeyPosition, Mode::Unicode] {
                let strokes = plan(&u(TEST_SAMPLE), mode, layout).unwrap();
                assert_eq!(strokes.len(), TEST_SAMPLE.chars().count());
            }
        }
    }

    #[test]
    fn test_sample_covers_every_ascii_symbol() {
        for b in 0x21u8..=0x7E {
            let c = b as char;
            if c.is_ascii_punctuation() {
                assert!(TEST_SAMPLE.contains(c), "{c:?}");
            }
        }
    }

    #[test]
    fn mode_keys_round_trip() {
        for mode in [Mode::KeyPosition, Mode::Unicode] {
            assert_eq!(Mode::from_key(mode.key()), Some(mode));
        }
        assert_eq!(Mode::from_key("fast"), None);
    }

    #[test]
    fn outcome_codes_round_trip() {
        for outcome in Outcome::ALL {
            assert_eq!(Outcome::from_code(outcome.code()), Some(outcome));
        }
        assert_eq!(Outcome::from_code(usize::MAX), None);
    }
}
