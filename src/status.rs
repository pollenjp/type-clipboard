//! 状態の行の文言 (上の行と、下の小さい行)。クリップボードの中身は出さない。

use crate::typing::{Mode, Outcome, Refusal, TEST_SAMPLE};

/// 上の行の色分け。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Info,
    Busy,
    Ok,
    Warn,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub kind: Kind,
    pub text: String,
    pub sub: String,
}

impl Status {
    pub fn new(kind: Kind, text: impl Into<String>, sub: impl Into<String>) -> Status {
        Status {
            kind,
            text: text.into(),
            sub: sub.into(),
        }
    }
}

/// 起こした直後・打つ先が無いとき。
pub fn choose_target(kind: Kind) -> Status {
    Status::new(
        kind,
        "打つ先を選んでください",
        "一覧から選ぶか「クリックで選ぶ」",
    )
}

pub fn picking() -> Status {
    Status::new(Kind::Info, "選びたい窓をクリック", "Esc でやめる")
}

pub fn pick_cancelled() -> Status {
    Status::new(Kind::Info, "選ぶのをやめました", "")
}

pub fn target_decided() -> Status {
    Status::new(
        Kind::Info,
        "打つ先を決めた",
        "欄を出し、コピーしてから「打つ」",
    )
}

pub fn target_closed() -> Status {
    Status::new(
        Kind::Warn,
        "打つ先の窓が閉じました",
        "一覧から選び直してください",
    )
}

pub fn elevated() -> Status {
    Status::new(
        Kind::Warn,
        "打つ先は管理者として動いているので打てません",
        "このアプリも管理者として起こせば打てます",
    )
}

pub fn caps_lock() -> Status {
    Status::new(
        Kind::Warn,
        "CapsLock がオンなので打ちません",
        "キーの位置の方式では大文字と小文字が逆になります。CapsLock を切ってから押してください",
    )
}

pub fn clipboard_busy() -> Status {
    Status::new(
        Kind::Warn,
        "クリップボードを読めませんでした",
        "ほかのアプリが使っています。もう一度押してください",
    )
}

/// 打たずに断るとき。どの文字かは出さず、数だけを出す。
pub fn refusal(refusal: &Refusal, mode: Mode) -> Status {
    match refusal {
        Refusal::Empty => Status::new(
            Kind::Warn,
            "クリップボードに文字がありません",
            "打つ文字をコピーしてから押してください",
        ),
        Refusal::LineBreakOrTab => Status::new(
            Kind::Warn,
            "改行かタブを含むので打ちません",
            "Enter や Tab を押すことになるため",
        ),
        Refusal::Unsupported { count } => Status::new(
            Kind::Warn,
            format!("打てない文字が {count} 個あるので打ちません"),
            match mode {
                Mode::KeyPosition => {
                    "選んだ配列に無い文字 (全角・絵文字など) です。配列か打ち方を見直してください"
                }
                Mode::Unicode => "制御文字などは打てません",
            },
        ),
        Refusal::CannotLockMemory => cannot_lock_memory(),
    }
}

/// 打つ列か、打つ thread の stack を、物理メモリに留められなかった (打たずに止める)。
fn cannot_lock_memory() -> Status {
    Status::new(
        Kind::Warn,
        "打つ文字をメモリに留められないので打ちません",
        "ページファイルに書き出させないためです。長すぎる文字は打てません",
    )
}

/// 打っている最中。
pub fn typing(typed: usize, total: usize) -> Status {
    Status::new(
        Kind::Busy,
        format!("打っています {typed} / {total}"),
        "キーを押すか、クリックすると止まる",
    )
}

/// 打ち終わり・止まったとき。`trial` は試し打ち。
pub fn outcome(outcome: Outcome, typed: usize, total: usize, trial: bool) -> Status {
    let stopped =
        |why: &str| Status::new(Kind::Warn, format!("止めました ({typed} / {total})"), why);
    match outcome {
        Outcome::Completed if trial => Status::new(
            Kind::Ok,
            format!("試し打ちしました ({total} 文字)"),
            format!("見本: {TEST_SAMPLE}"),
        ),
        Outcome::Completed => Status::new(
            Kind::Ok,
            format!("{total} 文字を打ちました"),
            "Enter は打つ先で自分で押す",
        ),
        Outcome::StoppedByInput => stopped("キー入力かクリックがあったため"),
        Outcome::StoppedByForeground => stopped("ほかの窓が前に来たため"),
        Outcome::NotForeground => Status::new(
            Kind::Warn,
            "打つ先を前に出せませんでした",
            "打つ先の窓を一度クリックしてから、もう一度押してください",
        ),
        Outcome::ImeOn => Status::new(
            Kind::Warn,
            "打つ先で IME がオンなので打ちません",
            "IME をオフにしてから、もう一度押してください",
        ),
        Outcome::ModifierHeld => Status::new(
            Kind::Warn,
            "Shift・Ctrl・Alt・Win が押されたままなので打ちません",
            "キーを離してから、もう一度押してください",
        ),
        Outcome::SendFailed => Status::new(
            Kind::Warn,
            format!("キーを送れませんでした ({typed} / {total})"),
            "打つ先が入力を受け付けませんでした",
        ),
        Outcome::CannotLockMemory => cannot_lock_memory(),
        Outcome::Crashed => Status::new(
            Kind::Warn,
            format!("打つ処理が異常終了しました ({typed} / {total})"),
            "途中まで入った文字を消してから、打ち直してください",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finished_and_stopped_texts() {
        let done = outcome(Outcome::Completed, 60, 60, false);
        assert_eq!(done.kind, Kind::Ok);
        assert_eq!(done.text, "60 文字を打ちました");
        assert_eq!(done.sub, "Enter は打つ先で自分で押す");
        let stopped = outcome(Outcome::StoppedByForeground, 12, 60, false);
        assert_eq!(stopped.kind, Kind::Warn);
        assert_eq!(stopped.text, "止めました (12 / 60)");
        assert_eq!(stopped.sub, "ほかの窓が前に来たため");
        assert_eq!(
            outcome(Outcome::StoppedByInput, 3, 60, false).text,
            "止めました (3 / 60)"
        );
    }

    #[test]
    fn trial_shows_the_sample() {
        let trial = outcome(Outcome::Completed, 57, 57, true);
        assert_eq!(trial.kind, Kind::Ok);
        assert!(trial.sub.contains(TEST_SAMPLE));
    }

    #[test]
    fn every_outcome_has_text() {
        for o in Outcome::ALL {
            let s = outcome(o, 1, 2, false);
            assert!(!s.text.is_empty(), "{o:?}");
        }
    }

    #[test]
    fn typing_progress_text() {
        let s = typing(3, 60);
        assert_eq!(s.kind, Kind::Busy);
        assert_eq!(s.text, "打っています 3 / 60");
    }

    #[test]
    fn refusal_texts_give_counts_only() {
        let s = refusal(&Refusal::Unsupported { count: 2 }, Mode::KeyPosition);
        assert_eq!(s.kind, Kind::Warn);
        assert_eq!(s.text, "打てない文字が 2 個あるので打ちません");
        assert_eq!(
            refusal(&Refusal::LineBreakOrTab, Mode::Unicode).text,
            "改行かタブを含むので打ちません"
        );
        assert_eq!(
            refusal(&Refusal::Empty, Mode::Unicode).text,
            "クリップボードに文字がありません"
        );
    }

    #[test]
    fn memory_lock_and_crash_texts() {
        let refused = refusal(&Refusal::CannotLockMemory, Mode::Unicode);
        assert_eq!(refused.kind, Kind::Warn);
        assert_eq!(refused.text, "打つ文字をメモリに留められないので打ちません");
        assert_eq!(outcome(Outcome::CannotLockMemory, 0, 60, false), refused);
        let crashed = outcome(Outcome::Crashed, 7, 60, false);
        assert_eq!(crashed.kind, Kind::Warn);
        assert_eq!(crashed.text, "打つ処理が異常終了しました (7 / 60)");
    }
}
