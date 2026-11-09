//! 一覧に出す窓・クリックで選べる窓の判定と、打つ先が管理者として動いているかの判定。

/// 窓の判定に使う性質 (Win32 から読んだもの)。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowFacts {
    pub visible: bool,
    /// DWM で隠されている (別の仮想デスクトップ・停止中のストアアプリなど)
    pub cloaked: bool,
    pub title_len: usize,
    /// WS_EX_TOOLWINDOW
    pub tool_window: bool,
    /// WS_EX_APPWINDOW
    pub app_window: bool,
    /// WS_EX_NOACTIVATE
    pub no_activate: bool,
    /// owner の窓がある (ダイアログなど)
    pub has_owner: bool,
    /// このアプリ自身の窓
    pub is_self: bool,
    /// デスクトップやタスクバー
    pub shell: bool,
}

/// デスクトップとタスクバーの窓の class。
const SHELL_CLASSES: [&str; 4] = [
    "Progman",
    "WorkerW",
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
];

pub fn is_shell_class(class: &str) -> bool {
    SHELL_CLASSES.contains(&class)
}

/// 一覧に出す窓: Alt+Tab に出るような top-level の窓で、題があり、このアプリでないもの。
pub fn is_listed(f: &WindowFacts) -> bool {
    let alt_tab = f.app_window || (!f.tool_window && !f.no_activate && !f.has_owner);
    f.visible && !f.cloaked && f.title_len > 0 && !f.is_self && !f.shell && alt_tab
}

/// 「クリックで選ぶ」で選べる窓: 見えている窓なら、owner のある窓や題の無い窓も選べる。
pub fn is_pickable(f: &WindowFacts) -> bool {
    f.visible && !f.cloaked && !f.is_self && !f.shell
}

/// 打つ先が、このアプリより高い整合性のレベル (管理者など) で動いているか。
/// 高ければ UIPI で入力が届かない。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Elevation {
    Higher,
    NotHigher,
    /// 判定できない (止めない)
    Unknown,
}

/// 打つ先の整合性のレベルを読めなかったわけ。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LevelError {
    /// process か token を開けなかった (アクセス拒否)
    AccessDenied,
    Other,
}

/// High の整合性のレベル (SECURITY_MANDATORY_HIGH_RID)。
const HIGH_RID: u32 = 0x3000;

/// 整合性のレベル (mandatory label の RID) から判定する。
///
/// 同じ利用者の同じレベルの process の token は開けるので、こちらが High より低いのに
/// 開けなければ、相手は管理者として動いているとみなす。
pub fn elevation_from_levels(ours: Option<u32>, target: Result<u32, LevelError>) -> Elevation {
    match (ours, target) {
        (Some(ours), Ok(target)) if target > ours => Elevation::Higher,
        (Some(_), Ok(_)) => Elevation::NotHigher,
        (Some(ours), Err(LevelError::AccessDenied)) if ours < HIGH_RID => Elevation::Higher,
        _ => Elevation::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: WindowFacts = WindowFacts {
        visible: true,
        cloaked: false,
        title_len: 5,
        tool_window: false,
        app_window: false,
        no_activate: false,
        has_owner: false,
        is_self: false,
        shell: false,
    };

    #[test]
    fn listed_follows_alt_tab_rules() {
        assert!(is_listed(&BASE));
        assert!(!is_listed(&WindowFacts {
            visible: false,
            ..BASE
        }));
        assert!(!is_listed(&WindowFacts {
            cloaked: true,
            ..BASE
        }));
        assert!(!is_listed(&WindowFacts {
            title_len: 0,
            ..BASE
        }));
        assert!(!is_listed(&WindowFacts {
            tool_window: true,
            ..BASE
        }));
        assert!(is_listed(&WindowFacts {
            tool_window: true,
            app_window: true,
            ..BASE
        }));
        assert!(!is_listed(&WindowFacts {
            no_activate: true,
            ..BASE
        }));
        assert!(!is_listed(&WindowFacts {
            has_owner: true,
            ..BASE
        }));
        assert!(is_listed(&WindowFacts {
            has_owner: true,
            app_window: true,
            ..BASE
        }));
        assert!(!is_listed(&WindowFacts {
            is_self: true,
            ..BASE
        }));
        assert!(!is_listed(&WindowFacts {
            shell: true,
            ..BASE
        }));
    }

    #[test]
    fn pickable_allows_owned_and_untitled_windows() {
        assert!(is_pickable(&WindowFacts {
            has_owner: true,
            title_len: 0,
            ..BASE
        }));
        assert!(!is_pickable(&WindowFacts {
            shell: true,
            ..BASE
        }));
        assert!(!is_pickable(&WindowFacts {
            is_self: true,
            ..BASE
        }));
        assert!(!is_pickable(&WindowFacts {
            cloaked: true,
            ..BASE
        }));
        assert!(!is_pickable(&WindowFacts {
            visible: false,
            ..BASE
        }));
    }

    #[test]
    fn shell_classes() {
        for class in [
            "Progman",
            "WorkerW",
            "Shell_TrayWnd",
            "Shell_SecondaryTrayWnd",
        ] {
            assert!(is_shell_class(class), "{class}");
        }
        assert!(!is_shell_class("Notepad"));
    }

    #[test]
    fn elevation_from_levels() {
        use super::elevation_from_levels as e;
        assert_eq!(e(Some(0x2000), Ok(0x3000)), Elevation::Higher);
        assert_eq!(e(Some(0x2000), Ok(0x2000)), Elevation::NotHigher);
        assert_eq!(e(Some(0x3000), Ok(0x2000)), Elevation::NotHigher);
        assert_eq!(
            e(Some(0x2000), Err(LevelError::AccessDenied)),
            Elevation::Higher
        );
        assert_eq!(
            e(Some(0x3000), Err(LevelError::AccessDenied)),
            Elevation::Unknown
        );
        assert_eq!(e(Some(0x2000), Err(LevelError::Other)), Elevation::Unknown);
        assert_eq!(e(None, Ok(0x3000)), Elevation::Unknown);
    }
}
