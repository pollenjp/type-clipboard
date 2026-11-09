//! クリップボードの写しを、使い終えたら上書きしてから手放す。

use std::sync::atomic::{Ordering, compiler_fence};

/// `buf` の要素を全部 `fill` で上書きする。最適化で書き込みが消されないよう volatile に書く。
pub fn wipe<T: Copy>(buf: &mut [T], fill: T) {
    for x in buf.iter_mut() {
        // SAFETY: `x` は有効な `&mut T` で、`T: Copy` なので drop の要るものを上書きしない
        unsafe { std::ptr::write_volatile(x, fill) };
    }
    compiler_fence(Ordering::SeqCst);
}

/// クリップボードから読んだ文字 (UTF-16) の写し。Drop で上書きして消す。
/// 中身を Debug などで出さない。
pub struct SecretText(Vec<u16>);

impl SecretText {
    pub fn from_utf16(units: Vec<u16>) -> Self {
        Self(units)
    }

    pub fn as_slice(&self) -> &[u16] {
        &self.0
    }
}

impl Drop for SecretText {
    fn drop(&mut self) {
        wipe(&mut self.0, 0);
    }
}

impl std::fmt::Debug for SecretText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SecretText({} units)", self.0.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wipe_overwrites_every_element() {
        let mut v = vec![1u16, 2, 3];
        wipe(&mut v, 0);
        assert_eq!(v, [0, 0, 0]);
    }

    #[test]
    fn secret_text_keeps_its_units() {
        let text = SecretText::from_utf16(vec![0x61, 0x62]);
        assert_eq!(text.as_slice(), &[0x61, 0x62]);
    }
}
