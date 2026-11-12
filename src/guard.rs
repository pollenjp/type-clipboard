//! 打っている間に hook が受けた入力を、止めるか (キーなら、打つ先に届けないか) 決める。hook から呼ぶが、
//! OS には触らない。
//!
//! VMConnect (Hyper-V の拡張セッション) は前にいる間、届いた修飾キー (少なくとも Shift と Ctrl) を自分の低レベルの
//! hook でいったん握りつぶし、印を付け替えて打ち直す (写しを送る)。文字のキーは打ち直さない。前に来た直後と、
//! 約 0.5 秒後 (hook を入れ直すとき) に、文字を打たない合成のキー (vk 0xFF) も流す。
//!
//! hook は 2 つの段で使い分ける ([`Stage`])。
//!
//! - 待つ段: 打つ先を前に出す前から、前に来て落ち着くまで。手で押したキーと手でしたクリックで止める。ほかのアプリが
//!   送った (合成の) キーは通す。
//! - 打つ段: 1 文字ごとに hook を入れ直してから、打ち終えるまで。このアプリが送ったキーと、その写し (直前に通した
//!   このアプリのキーと形が同じで、印だけが違うキー) だけを通す。ほかのキーの押下は、手で押したものも、ほかのアプリが
//!   送ったものも止める。
//!
//! どちらの段でも、合成の vk 0xFF は通す (文字を打たないので、通してもパスワードは変わらない)。
//!
//! 入れ直すのは、Windows が後から入れた低レベルの hook から先に呼ぶため。このアプリの hook が先に元のキーを見て、
//! 後から写しを見るようにする。VMConnect は hook を入れ直すことがあるので、1 文字ごとに入れ直して先頭を取り返す。

use crate::secret::{LockFailed, SecretCell};

/// このアプリが SendInput で送る入力の印 (`dwExtraInfo`)。
pub const INPUT_MARK: usize = 0x5459_4350;

// 低レベルの hook が受ける flags のビット (Windows の LLKHF_*・LLMHF_INJECTED)。lib は OS に触らないので、値を置く
const LLKHF_EXTENDED: u32 = 0x01;
const LLKHF_INJECTED: u32 = 0x10;
const LLKHF_UP: u32 = 0x80;
const LLMHF_INJECTED: u32 = 0x01;
/// 割り当ての無い仮想キー。押しても文字を打たない
const VK_UNASSIGNED: u32 = 0xFF;

/// キーの形: 仮想キー・scancode・拡張キーか・離す方か。印 (`dwExtraInfo`) と、合成かどうかは含めない。
/// このアプリのキーは打つ文字から作る値なので、Debug で中身を出さない。
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct KeyShape {
    pub vk: u32,
    pub scan: u32,
    pub extended: bool,
    pub up: bool,
}

impl std::fmt::Debug for KeyShape {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("KeyShape(..)")
    }
}

/// hook が受けたキーの入力。
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    pub shape: KeyShape,
    /// 合成の入力 (SendInput などで送られたもの) か。手で押したキーには立たない
    pub injected: bool,
    /// このアプリの印 ([`INPUT_MARK`]) が付いているか
    pub ours: bool,
}

impl KeyEvent {
    /// 低レベルのキーボードの hook が受けた値 (KBDLLHOOKSTRUCT の vkCode・scanCode・flags・dwExtraInfo) から作る。
    pub fn from_hook(vk: u32, scan: u32, flags: u32, extra: usize) -> KeyEvent {
        KeyEvent {
            shape: KeyShape {
                vk,
                scan,
                extended: flags & LLKHF_EXTENDED != 0,
                up: flags & LLKHF_UP != 0,
            },
            injected: flags & LLKHF_INJECTED != 0,
            ours: extra == INPUT_MARK,
        }
    }
}

/// hook が受けたマウスのボタンの押下。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Click {
    /// 合成の入力か。手でしたクリックには立たない
    pub injected: bool,
    /// このアプリの印 ([`INPUT_MARK`]) が付いているか (このアプリはクリックを送らない)
    pub ours: bool,
}

impl Click {
    /// 低レベルのマウスの hook が受けたボタンの押下の値 (MSLLHOOKSTRUCT の flags・dwExtraInfo) から作る。
    pub fn from_hook(flags: u32, extra: usize) -> Click {
        Click {
            injected: flags & LLMHF_INJECTED != 0,
            ours: extra == INPUT_MARK,
        }
    }
}

/// 止めたわけ。キーの値は持たない (画面に出さないため)。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interruption {
    /// 手で押したキー
    Key,
    /// ほかのアプリが送ったキーで、照らす覚えが無いとき (hook を入れ直した直後か、写しを通した後)
    OtherAppKey,
    /// ほかのアプリが送ったキーで、覚えている直前のこのアプリのキーと形が違うもの
    UnmatchedKey,
    /// クリック
    Click,
}

/// hook が入力をどう扱うか。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// 次の hook へ渡す
    Pass,
    /// 止める合図を立てる。キーなら握りつぶす (打つ先に届けない)。クリックは握りつぶさない
    Stop(Interruption),
}

/// 打っている間の hook の段。
pub enum Stage {
    /// 待つ段: 打つ先を前に出す前から、前に来て落ち着くまで
    Waiting,
    /// 打つ段: hook を入れ直してから、打ち終えるまで
    Typing(LastOwnKey),
}

impl Stage {
    /// 打つ段。直前に通したこのアプリのキーを覚える場所を、物理メモリに留めて用意する。
    pub fn typing() -> Result<Stage, LockFailed> {
        Ok(Stage::Typing(LastOwnKey(SecretCell::new(None)?)))
    }

    /// キーの入力を判定する。離す方は止めない (打つ段では、写しと照らすために覚えは更新する)。
    pub fn key(&mut self, key: KeyEvent) -> Verdict {
        if !key.injected {
            // 手で押したキー: 押す方で止め、打つ先に届けない
            return if key.shape.up {
                Verdict::Pass
            } else {
                Verdict::Stop(Interruption::Key)
            };
        }
        if !key.ours && key.shape.vk == VK_UNASSIGNED {
            // 文字を打たない合成のキー (VMConnect が前に来た直後と、hook を入れ直すときに流す)。段によらず通し、
            // 写しの照合にも使わない
            return Verdict::Pass;
        }
        match self {
            // 合成のキーは、印が何でも通す
            Stage::Waiting => Verdict::Pass,
            Stage::Typing(last) => last.judge(key),
        }
    }

    /// マウスのボタンの押下を判定する (止めても握りつぶさない)。
    pub fn click(&self, click: Click) -> Verdict {
        let stop = match self {
            // 手でしたクリックで止める。ほかのアプリが送ったクリックは通す
            Stage::Waiting => !click.injected,
            // このアプリはクリックを送らないので、自分の印でないクリックは全部止める
            Stage::Typing(_) => !click.ours,
        };
        if stop {
            Verdict::Stop(Interruption::Click)
        } else {
            Verdict::Pass
        }
    }
}

/// 打つ段で、直前に通したこのアプリのキーの形 (写しが来たら消す)。打つ文字から作る値なので、秘密専用のページに
/// 置いて物理メモリに留め、手放すときに上書きして消す。
pub struct LastOwnKey(SecretCell<Option<KeyShape>>);

impl LastOwnKey {
    /// 合成のキーを判定する。
    fn judge(&mut self, key: KeyEvent) -> Verdict {
        if key.ours {
            // このアプリのキー: 形を覚えて通す (写しが来ないまま次のキーが来たら、覚えを置き換える)
            self.0.set(Some(key.shape));
            return Verdict::Pass;
        }
        match self.0.get() {
            Some(last) if last == key.shape => {
                // 直前のこのアプリのキーの写し: 通して覚えを消す (1 つの元に写しは 1 つまで)
                self.0.set(None);
                Verdict::Pass
            }
            _ if key.shape.up => Verdict::Pass,
            Some(_) => Verdict::Stop(Interruption::UnmatchedKey),
            None => Verdict::Stop(Interruption::OtherAppKey),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 診断で vmconnect の前に送った印 (0x44494147) は、このアプリでは `INPUT_MARK` にあたる。
    const VMCONNECT_MARK: usize = 0x4321_DCBA;

    const fn shape(vk: u32, scan: u32, extended: bool, up: bool) -> KeyShape {
        KeyShape {
            vk,
            scan,
            extended,
            up,
        }
    }

    /// 「A」(US の配列の位置) を押す・離す。
    const A_DOWN: KeyShape = shape(0x41, 0x1E, false, false);
    const A_UP: KeyShape = shape(0x41, 0x1E, false, true);
    /// 「B」を押す。
    const B_DOWN: KeyShape = shape(0x42, 0x30, false, false);

    fn own(shape: KeyShape) -> KeyEvent {
        KeyEvent {
            shape,
            injected: true,
            ours: true,
        }
    }

    /// ほかのアプリが送った (合成の) キー。打つ段では、形が同じなら写しとして扱う。
    fn injected(shape: KeyShape) -> KeyEvent {
        KeyEvent {
            shape,
            injected: true,
            ours: false,
        }
    }

    fn by_hand(shape: KeyShape) -> KeyEvent {
        KeyEvent {
            shape,
            injected: false,
            ours: false,
        }
    }

    fn typing() -> Stage {
        Stage::typing().unwrap()
    }

    const CLICK_BY_HAND: Click = Click {
        injected: false,
        ours: false,
    };
    const CLICK_FROM_OTHER_APP: Click = Click {
        injected: true,
        ours: false,
    };

    // ---- 入力の値の読み方 ----

    #[test]
    fn key_from_hook_reads_shape_injection_and_mark() {
        // 右 Shift を離す、このアプリの印つきの合成のキー (flags 0x91 = 合成・拡張・離す)
        let key = KeyEvent::from_hook(0xA1, 0x36, 0x91, INPUT_MARK);
        assert_eq!(key.shape.vk, 0xA1);
        assert_eq!(key.shape.scan, 0x36);
        assert!(key.shape.extended);
        assert!(key.shape.up);
        assert!(key.injected);
        assert!(key.ours);
        // 手で押した「A」(flags 0x00、印なし)
        let key = KeyEvent::from_hook(0x41, 0x1E, 0x00, 0);
        assert_eq!(key.shape, A_DOWN);
        assert!(!key.injected);
        assert!(!key.ours);
    }

    #[test]
    fn key_shape_leaves_out_the_mark_and_other_flags() {
        // 写しは印だけが違う。Alt (0x20)・低い整合性レベルからの合成 (0x02) のビットも形に入れない
        let original = KeyEvent::from_hook(0x41, 0x1E, 0x10, INPUT_MARK);
        let copy = KeyEvent::from_hook(0x41, 0x1E, 0x10 | 0x20 | 0x02, VMCONNECT_MARK);
        assert_eq!(copy.shape, original.shape);
        assert!(copy.injected);
        assert!(!copy.ours);
    }

    #[test]
    fn click_from_hook_reads_injection_and_mark() {
        assert_eq!(Click::from_hook(0, 0), CLICK_BY_HAND);
        assert_eq!(Click::from_hook(0x01, VMCONNECT_MARK), CLICK_FROM_OTHER_APP);
        assert_eq!(
            Click::from_hook(0x01, INPUT_MARK),
            Click {
                injected: true,
                ours: true
            }
        );
    }

    #[test]
    fn debug_of_a_key_shape_hides_the_values() {
        assert_eq!(format!("{A_DOWN:?}"), "KeyShape(..)");
    }

    /// Windows では、置いた値が windows crate の値と同じか。
    #[cfg(windows)]
    #[test]
    fn flag_bits_match_windows() {
        use windows::Win32::UI::WindowsAndMessaging::{
            LLKHF_EXTENDED as EXTENDED, LLKHF_INJECTED as INJECTED, LLKHF_UP as UP,
            LLMHF_INJECTED as MOUSE_INJECTED,
        };
        assert_eq!(LLKHF_EXTENDED, EXTENDED.0);
        assert_eq!(LLKHF_INJECTED, INJECTED.0);
        assert_eq!(LLKHF_UP, UP.0);
        assert_eq!(LLMHF_INJECTED, MOUSE_INJECTED);
    }

    // ---- 待つ段 ----

    #[test]
    fn waiting_stops_and_swallows_a_key_pressed_by_hand() {
        let mut stage = Stage::Waiting;
        assert_eq!(stage.key(by_hand(A_DOWN)), Verdict::Stop(Interruption::Key));
    }

    #[test]
    fn waiting_passes_a_key_released_by_hand() {
        let mut stage = Stage::Waiting;
        assert_eq!(stage.key(by_hand(A_UP)), Verdict::Pass);
    }

    #[test]
    fn waiting_passes_injected_keys_whatever_the_mark() {
        let mut stage = Stage::Waiting;
        // vmconnect が前に来たときに流す vk 0xFF (押す・離す。診断のログの値)
        assert_eq!(
            stage.key(KeyEvent::from_hook(0xFF, 0x00, 0x10, VMCONNECT_MARK)),
            Verdict::Pass
        );
        assert_eq!(
            stage.key(KeyEvent::from_hook(0xFF, 0x00, 0x90, VMCONNECT_MARK)),
            Verdict::Pass
        );
        assert_eq!(stage.key(injected(A_DOWN)), Verdict::Pass);
        assert_eq!(stage.key(own(A_DOWN)), Verdict::Pass);
    }

    #[test]
    fn waiting_stops_on_a_click_by_hand_but_passes_injected_clicks() {
        let stage = Stage::Waiting;
        assert_eq!(
            stage.click(CLICK_BY_HAND),
            Verdict::Stop(Interruption::Click)
        );
        assert_eq!(stage.click(CLICK_FROM_OTHER_APP), Verdict::Pass);
    }

    // ---- 打つ段 ----

    #[test]
    fn typing_passes_own_key_and_then_one_copy_of_it() {
        let mut stage = typing();
        assert_eq!(stage.key(own(A_DOWN)), Verdict::Pass);
        assert_eq!(stage.key(injected(A_DOWN)), Verdict::Pass);
        // 1 つの元に写しは 1 つまで
        assert_eq!(
            stage.key(injected(A_DOWN)),
            Verdict::Stop(Interruption::OtherAppKey)
        );
    }

    #[test]
    fn typing_stops_on_an_injected_key_before_any_own_key() {
        let mut stage = typing();
        assert_eq!(
            stage.key(injected(A_DOWN)),
            Verdict::Stop(Interruption::OtherAppKey)
        );
    }

    #[test]
    fn typing_stops_on_an_injected_key_of_another_shape() {
        let mut stage = typing();
        assert_eq!(stage.key(own(A_DOWN)), Verdict::Pass);
        assert_eq!(
            stage.key(injected(B_DOWN)),
            Verdict::Stop(Interruption::UnmatchedKey)
        );
    }

    #[test]
    fn typing_tells_the_extended_key_from_the_plain_one() {
        let mut stage = typing();
        // 右 Shift (拡張) を送った後に、拡張の印の無い同じ値のキーが来た
        assert_eq!(
            stage.key(own(shape(0xA1, 0x36, true, false))),
            Verdict::Pass
        );
        assert_eq!(
            stage.key(injected(shape(0xA1, 0x36, false, false))),
            Verdict::Stop(Interruption::UnmatchedKey)
        );
    }

    #[test]
    fn typing_tells_the_release_from_the_press() {
        let mut stage = typing();
        // 離す方を送った後に、同じキーを押す合成のキーが来た
        assert_eq!(stage.key(own(A_UP)), Verdict::Pass);
        assert_eq!(
            stage.key(injected(A_DOWN)),
            Verdict::Stop(Interruption::UnmatchedKey)
        );
    }

    #[test]
    fn typing_stops_and_swallows_a_key_pressed_by_hand_even_if_it_matches() {
        let mut stage = typing();
        assert_eq!(stage.key(own(A_DOWN)), Verdict::Pass);
        assert_eq!(stage.key(by_hand(A_DOWN)), Verdict::Stop(Interruption::Key));
    }

    #[test]
    fn typing_passes_releases_that_are_not_copies() {
        let mut stage = typing();
        assert_eq!(stage.key(by_hand(A_UP)), Verdict::Pass);
        assert_eq!(stage.key(injected(A_UP)), Verdict::Pass);
    }

    #[test]
    fn typing_remembers_releases_too() {
        let mut stage = typing();
        assert_eq!(stage.key(own(A_UP)), Verdict::Pass);
        assert_eq!(stage.key(injected(A_UP)), Verdict::Pass);
        // 写しを通したので覚えは消えている
        assert_eq!(
            stage.key(injected(A_DOWN)),
            Verdict::Stop(Interruption::OtherAppKey)
        );
    }

    #[test]
    fn typing_replaces_the_pending_key_when_no_copy_comes() {
        // 打ち直さない相手 (メモ帳など) では写しが来ないまま、次のキーを送る
        let mut stage = typing();
        assert_eq!(stage.key(own(A_DOWN)), Verdict::Pass);
        assert_eq!(stage.key(own(B_DOWN)), Verdict::Pass);
        assert_eq!(stage.key(injected(B_DOWN)), Verdict::Pass);

        let mut stage = typing();
        assert_eq!(stage.key(own(A_DOWN)), Verdict::Pass);
        assert_eq!(stage.key(own(B_DOWN)), Verdict::Pass);
        assert_eq!(
            stage.key(injected(A_DOWN)),
            Verdict::Stop(Interruption::UnmatchedKey)
        );
    }

    /// 診断で、vmconnect を前に出してから hook を入れ、[左 Shift を押す, 右 Shift を押す] と
    /// [右 Shift を離す, 左 Shift を離す] を 1 回の SendInput ずつ送ったときに届いた並び (元 → 写し の順)。
    #[test]
    fn typing_lets_through_the_order_seen_with_vmconnect() {
        let seen = [
            (0xA0, 0x2A, 0x10, INPUT_MARK),
            (0xA0, 0x2A, 0x10, VMCONNECT_MARK),
            (0xA1, 0x36, 0x11, INPUT_MARK),
            (0xA1, 0x36, 0x11, VMCONNECT_MARK),
            (0xA1, 0x36, 0x91, INPUT_MARK),
            (0xA1, 0x36, 0x91, VMCONNECT_MARK),
            (0xA0, 0x2A, 0x90, INPUT_MARK),
            (0xA0, 0x2A, 0x90, VMCONNECT_MARK),
        ];
        let mut stage = typing();
        for (i, (vk, scan, flags, extra)) in seen.into_iter().enumerate() {
            assert_eq!(
                stage.key(KeyEvent::from_hook(vk, scan, flags, extra)),
                Verdict::Pass,
                "{i} 番目"
            );
        }
    }

    /// vmconnect が写しを送るのは修飾キーだけで、文字のキーには写しが来ない (試し打ちの見本で見た形)。
    #[test]
    fn typing_lets_through_a_shifted_char_when_only_shift_is_copied() {
        let seen = [
            (0xA0, 0x2A, 0x10, INPUT_MARK),     // 左 Shift を押す
            (0xA0, 0x2A, 0x10, VMCONNECT_MARK), // その写し
            (0x41, 0x1E, 0x10, INPUT_MARK),     // 「A」を押す (写しは来ない)
            (0x41, 0x1E, 0x90, INPUT_MARK),     // 「A」を離す
            (0xA0, 0x2A, 0x90, INPUT_MARK),     // 左 Shift を離す
            (0xA0, 0x2A, 0x90, VMCONNECT_MARK), // その写し
        ];
        let mut stage = typing();
        for (i, (vk, scan, flags, extra)) in seen.into_iter().enumerate() {
            assert_eq!(
                stage.key(KeyEvent::from_hook(vk, scan, flags, extra)),
                Verdict::Pass,
                "{i} 番目"
            );
        }
    }

    /// vmconnect が前に来た直後と、約 0.5 秒後に流す vk 0xFF (文字を打たない) は、どちらの段でも通す。
    #[test]
    fn injected_vk_ff_passes_in_either_stage() {
        let ff_down = KeyEvent::from_hook(0xFF, 0x00, 0x10, VMCONNECT_MARK);
        let ff_up = KeyEvent::from_hook(0xFF, 0x00, 0x90, VMCONNECT_MARK);
        for mut stage in [Stage::Waiting, typing()] {
            assert_eq!(stage.key(ff_down), Verdict::Pass);
            assert_eq!(stage.key(ff_up), Verdict::Pass);
        }
    }

    #[test]
    fn injected_vk_ff_leaves_the_pending_key_alone() {
        let mut stage = typing();
        assert_eq!(stage.key(own(A_DOWN)), Verdict::Pass);
        assert_eq!(
            stage.key(KeyEvent::from_hook(0xFF, 0x00, 0x10, VMCONNECT_MARK)),
            Verdict::Pass
        );
        // 「A」の写しは、まだ照らせる
        assert_eq!(stage.key(injected(A_DOWN)), Verdict::Pass);
    }

    #[test]
    fn vk_ff_pressed_by_hand_still_stops() {
        let ff = shape(0xFF, 0x00, false, false);
        assert_eq!(
            Stage::Waiting.key(by_hand(ff)),
            Verdict::Stop(Interruption::Key)
        );
        assert_eq!(typing().key(by_hand(ff)), Verdict::Stop(Interruption::Key));
    }

    #[test]
    fn typing_stops_on_any_click_that_is_not_ours() {
        let stage = typing();
        assert_eq!(
            stage.click(CLICK_BY_HAND),
            Verdict::Stop(Interruption::Click)
        );
        assert_eq!(
            stage.click(CLICK_FROM_OTHER_APP),
            Verdict::Stop(Interruption::Click)
        );
    }
}
