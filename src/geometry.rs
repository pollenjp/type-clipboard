//! プレビューの欄に、縦横比を保って収める計算。

/// 矩形 (left・top を含み、right・bottom を含まない)。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
}

/// `src_w`x`src_h` を、縦横比を保って `area` に収め、中央に置いた矩形。
/// 大きくはしない (小さい窓は等倍)。どちらかが空なら `None`。
pub fn fit_within(src_w: i32, src_h: i32, area: Rect) -> Option<Rect> {
    if src_w <= 0 || src_h <= 0 || area.width() <= 0 || area.height() <= 0 {
        return None;
    }
    let (sw, sh) = (i64::from(src_w), i64::from(src_h));
    let (aw, ah) = (i64::from(area.width()), i64::from(area.height()));
    let (w, h) = if sw <= aw && sh <= ah {
        (sw, sh)
    } else if sw * ah >= sh * aw {
        // 横で詰まる
        (aw, (sh * aw / sw).max(1))
    } else {
        // 縦で詰まる
        ((sw * ah / sh).max(1), ah)
    };
    // w <= aw・h <= ah なので i32 に収まる
    let left = area.left + ((aw - w) / 2) as i32;
    let top = area.top + ((ah - h) / 2) as i32;
    Some(Rect {
        left,
        top,
        right: left + w as i32,
        bottom: top + h as i32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Rect = Rect {
        left: 10,
        top: 20,
        right: 210,
        bottom: 120,
    }; // 200x100

    #[test]
    fn large_source_shrinks_to_fit() {
        assert_eq!(
            fit_within(400, 400, AREA),
            Some(Rect {
                left: 60,
                top: 20,
                right: 160,
                bottom: 120
            })
        );
        assert_eq!(
            fit_within(800, 200, AREA),
            Some(Rect {
                left: 10,
                top: 45,
                right: 210,
                bottom: 95
            })
        );
    }

    #[test]
    fn small_source_is_not_upscaled() {
        assert_eq!(
            fit_within(100, 50, AREA),
            Some(Rect {
                left: 60,
                top: 45,
                right: 160,
                bottom: 95
            })
        );
    }

    #[test]
    fn extreme_aspect_stays_visible_and_inside() {
        for (w, h) in [(3000, 10), (10, 3000), (1, 1), (100_000, 100_000)] {
            let r = fit_within(w, h, AREA).unwrap();
            assert!(r.width() >= 1 && r.height() >= 1, "{w}x{h}: {r:?}");
            assert!(
                r.left >= AREA.left && r.right <= AREA.right,
                "{w}x{h}: {r:?}"
            );
            assert!(
                r.top >= AREA.top && r.bottom <= AREA.bottom,
                "{w}x{h}: {r:?}"
            );
        }
    }

    #[test]
    fn empty_source_or_area_gives_none() {
        assert_eq!(fit_within(0, 100, AREA), None);
        assert_eq!(fit_within(100, -1, AREA), None);
        assert_eq!(
            fit_within(
                100,
                100,
                Rect {
                    left: 0,
                    top: 0,
                    right: 0,
                    bottom: 50
                }
            ),
            None
        );
    }
}
