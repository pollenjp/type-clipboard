//! 「クリックで選ぶ」の間のマウスのボタンの扱い。hook から呼ぶが、OS には触らない。
//!
//! ボタンを押す方を握りつぶしたら (相手の窓に届けなかったら)、離す方も握りつぶす。
//! 離す方だけが相手に届くと、右ボタンなら相手の窓で右クリックのメニューが出てしまう。

/// マウスのボタン。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
    Middle,
    /// 戻る・進むなどの X ボタン
    X,
}

impl Button {
    fn bit(self) -> u8 {
        match self {
            Button::Left => 1,
            Button::Right => 2,
            Button::Middle => 4,
            Button::X => 8,
        }
    }
}

/// hook が受けたマウスの入力。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseEvent {
    Move,
    Down(Button),
    Up(Button),
    /// ホイールなど
    Other,
}

/// 主窓に知らせること。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickNotice {
    /// マウスが動いた (指している窓を候補にする)
    Move,
    /// 左クリック (指している窓を打つ先に決める)
    Click,
    /// 左以外のボタン (やめる)
    Cancel,
    /// 握りつぶしたボタンが全部離された (hook を外してよい)
    Released,
}

/// 押す方を握りつぶしたボタンを覚えておき、離す方も握りつぶす。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PickMouse {
    pending_up: u8,
}

impl PickMouse {
    /// 握りつぶすか (相手の窓に届けないか) と、主窓に知らせること。
    pub fn handle(&mut self, event: MouseEvent) -> (bool, Option<PickNotice>) {
        match event {
            MouseEvent::Move => (false, Some(PickNotice::Move)),
            MouseEvent::Down(button) => {
                self.pending_up |= button.bit();
                let notice = if button == Button::Left {
                    PickNotice::Click
                } else {
                    PickNotice::Cancel
                };
                (true, Some(notice))
            }
            MouseEvent::Up(button) if self.pending_up & button.bit() != 0 => {
                self.pending_up &= !button.bit();
                let notice = (self.pending_up == 0).then_some(PickNotice::Released);
                (true, notice)
            }
            MouseEvent::Up(_) | MouseEvent::Other => (false, None),
        }
    }

    /// 押す方を握りつぶして、まだ離されていないボタンがあるか。
    pub fn has_pending_up(&self) -> bool {
        self.pending_up != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_is_passed_and_reported() {
        let mut mouse = PickMouse::default();
        assert_eq!(
            mouse.handle(MouseEvent::Move),
            (false, Some(PickNotice::Move))
        );
    }

    #[test]
    fn left_click_is_swallowed_down_and_up() {
        let mut mouse = PickMouse::default();
        assert_eq!(
            mouse.handle(MouseEvent::Down(Button::Left)),
            (true, Some(PickNotice::Click))
        );
        assert!(mouse.has_pending_up());
        assert_eq!(
            mouse.handle(MouseEvent::Up(Button::Left)),
            (true, Some(PickNotice::Released))
        );
        assert!(!mouse.has_pending_up());
    }

    #[test]
    fn other_buttons_cancel_and_their_up_is_swallowed_too() {
        for button in [Button::Right, Button::Middle, Button::X] {
            let mut mouse = PickMouse::default();
            assert_eq!(
                mouse.handle(MouseEvent::Down(button)),
                (true, Some(PickNotice::Cancel)),
                "{button:?}"
            );
            assert!(mouse.has_pending_up(), "{button:?}");
            assert_eq!(
                mouse.handle(MouseEvent::Up(button)),
                (true, Some(PickNotice::Released)),
                "{button:?}"
            );
        }
    }

    #[test]
    fn up_without_swallowed_down_passes() {
        let mut mouse = PickMouse::default();
        assert_eq!(mouse.handle(MouseEvent::Up(Button::Right)), (false, None));
        assert_eq!(mouse.handle(MouseEvent::Other), (false, None));
    }

    #[test]
    fn released_only_after_every_swallowed_button_is_up() {
        let mut mouse = PickMouse::default();
        mouse.handle(MouseEvent::Down(Button::Left));
        mouse.handle(MouseEvent::Down(Button::Right));
        assert_eq!(mouse.handle(MouseEvent::Up(Button::Left)), (true, None));
        assert_eq!(
            mouse.handle(MouseEvent::Up(Button::Right)),
            (true, Some(PickNotice::Released))
        );
    }
}
