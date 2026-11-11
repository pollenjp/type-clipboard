//! 秘密の写し (クリップボードの文字から作る打つ列と、1 文字ずつ送るときの値) を、外に残さずに扱う。
//!
//! - [`SecretBuf`]: 秘密専用にページ単位で確保した、大きさの決まった buffer。ほかの確保とページを分け合わない。
//!   Windows では VirtualLock で物理メモリに留め、ページファイルに書き出させない。クラッシュのときに Windows の
//!   エラー報告 (WER) が取る dump からも外す。Drop で上書きしてから、ロックを外して手放す。
//! - [`SecretCell`]: 秘密の値を 1 つだけ置く、[`SecretBuf`] と同じ扱いの場所。置き換えられる。
//! - [`StackLock`]: 今の thread の stack のうち、呼んだ所から下 (これから呼ぶ関数が使う側) と上の全部を、
//!   同じく物理メモリに留める。1 文字ずつの写し (送る INPUT の並び・decode した文字など) が stack に乗るため。
//!   Drop で、呼んだ所より下を上書きしてからロックを外す。
//! - [`wipe`]・[`scrub_stack`]: 上書きして消す。
//!
//! Windows でないとき (test) は、ページ単位で確保するだけで、物理メモリに留めない (このアプリは Windows でだけ動く)。

use std::cell::Cell;
use std::marker::PhantomData;
use std::ops::Deref;
use std::ptr::NonNull;
use std::sync::atomic::{Ordering, compiler_fence};

/// `buf` の要素を全部 `fill` で上書きする。最適化で書き込みが消されないよう volatile に書く。
pub fn wipe<T: Copy>(buf: &mut [T], fill: T) {
    for x in buf.iter_mut() {
        // SAFETY: `x` は有効な `&mut T` で、`T: Copy` なので drop の要るものを上書きしない
        unsafe { std::ptr::write_volatile(x, fill) };
    }
    compiler_fence(Ordering::SeqCst);
}

/// [`scrub_stack`] が上書きする stack の大きさ。[`STACK_LOCK_BELOW`] より大きくする
/// (上書きで使ったページは commit されるので、[`StackLock`] はその内側を留められる)。
pub const STACK_SCRUB_BYTES: usize = 48 * 1024;

/// [`StackLock`] が、呼んだ所から下に留める stack の大きさ。打つ処理 (SendInput まで) と、打つ列を作る処理が
/// 使う深さより十分に大きくする。
pub const STACK_LOCK_BELOW: usize = 32 * 1024;

/// 呼んだ所より下 (呼び終えた関数の frame があった側) の stack を、[`STACK_SCRUB_BYTES`] だけ 0 で上書きする。
/// 呼び終えた関数の frame に残った 1 文字ぶんの写しを消すため。
#[inline(never)]
pub fn scrub_stack() {
    let mut area = [0u64; STACK_SCRUB_BYTES / 8];
    wipe(&mut area, 0);
    std::hint::black_box(&area);
}

/// 秘密のためのメモリを確保できなかった・物理メモリに留められなかった。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LockFailed;

/// 秘密専用の buffer。容量は確保するときに決め、伸ばさない (伸ばし直しで写しを残さないため)。
pub struct SecretBuf<T: Copy> {
    pages: Pages,
    len: usize,
    cap: usize,
    _items: PhantomData<T>,
}

// SAFETY: 中身は `T` の並びだけで、確保したページはこの値だけが持つ
unsafe impl<T: Copy + Send> Send for SecretBuf<T> {}
// SAFETY: `&SecretBuf` からは読むことしかできない
unsafe impl<T: Copy + Sync> Sync for SecretBuf<T> {}

impl<T: Copy> SecretBuf<T> {
    /// `cap` 個ぶんを、秘密専用のページに確保して物理メモリに留める。
    pub fn with_capacity(cap: usize) -> Result<Self, LockFailed> {
        const { assert!(align_of::<T>() <= sys::PAGE_ALIGN) };
        let bytes = cap.checked_mul(size_of::<T>()).ok_or(LockFailed)?;
        let pages = Pages::alloc_locked(bytes.max(1))?;
        Ok(SecretBuf {
            pages,
            len: 0,
            cap,
            _items: PhantomData,
        })
    }

    /// 末尾に足す。
    ///
    /// # Panics
    ///
    /// 容量を超えるとき (容量は確保するときに、要る数を数えてから決める)。
    pub fn push(&mut self, value: T) {
        assert!(self.len < self.cap, "SecretBuf の容量を超えた");
        // SAFETY: len < cap なので、確保した範囲の中。ページの先頭は T の整列より大きい単位で揃っている
        unsafe { self.items().add(self.len).write(value) };
        self.len += 1;
    }

    pub fn capacity(&self) -> usize {
        self.cap
    }

    fn items(&self) -> *mut T {
        self.pages.base.as_ptr().cast()
    }
}

impl<T: Copy> Deref for SecretBuf<T> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        // SAFETY: 先頭の len 個は push で書いた
        unsafe { std::slice::from_raw_parts(self.items(), self.len) }
    }
}

impl<T: Copy> std::fmt::Debug for SecretBuf<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SecretBuf({} items)", self.len)
    }
}

/// 秘密の値を 1 つだけ置く場所。[`SecretBuf`] と同じく、秘密専用のページに置いて物理メモリに留め、Drop で上書き
/// してから手放す。
pub struct SecretCell<T: Copy>(SecretBuf<T>);

impl<T: Copy> SecretCell<T> {
    /// `value` を置く場所を、秘密専用のページに確保して物理メモリに留める。
    pub fn new(value: T) -> Result<Self, LockFailed> {
        let mut buf = SecretBuf::with_capacity(1)?;
        buf.push(value);
        Ok(SecretCell(buf))
    }

    pub fn get(&self) -> T {
        self.0[0]
    }

    /// 置いている値を `value` で上書きする。最適化で書き込みが消されないよう volatile に書く。
    pub fn set(&mut self, value: T) {
        // SAFETY: new で 1 つ書いた、確保した範囲の先頭。T の整列より大きい単位で揃っている
        unsafe { self.0.items().write_volatile(value) };
        compiler_fence(Ordering::SeqCst);
    }
}

impl<T: Copy> std::fmt::Debug for SecretCell<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretCell(..)")
    }
}

/// 秘密専用に確保したページ。Drop で全部を上書きしてから、ロックを外して手放す。
struct Pages {
    base: NonNull<u8>,
    bytes: usize,
}

impl Pages {
    fn alloc_locked(bytes: usize) -> Result<Pages, LockFailed> {
        let base = sys::alloc_locked(bytes).ok_or(LockFailed)?;
        Ok(Pages { base, bytes })
    }
}

impl Drop for Pages {
    fn drop(&mut self) {
        // 中の値の padding (初期化されていない byte) に参照を作らないよう、pointer で 1 byte ずつ書く
        for i in 0..self.bytes {
            // SAFETY: base から bytes は alloc_locked で確保した範囲で、この値だけが持つ
            unsafe { self.base.as_ptr().add(i).write_volatile(0) };
        }
        compiler_fence(Ordering::SeqCst);
        // SAFETY: alloc_locked で確保した範囲を、上書きしてから手放す
        unsafe { sys::free(self.base, self.bytes) };
    }
}

thread_local! {
    /// この thread で [`StackLock`] を持っているか。VirtualLock はロックを数えないので、入れ子にすると内側の
    /// Drop が外側のロックも外してしまう。入れ子は断る。
    static STACK_LOCKED: Cell<bool> = const { Cell::new(false) };
}

/// 今の thread の stack のうち、呼んだ所から下 [`STACK_LOCK_BELOW`] と、上 (呼び出し元の側) の全部を、
/// 物理メモリに留めている間の印。Drop で、呼んだ所より下を上書きしてからロックを外す。
/// 留めた thread で手放す (thread をまたげない)。1 つの thread で同時に持てるのは 1 つだけ。
pub struct StackLock {
    start: usize,
    len: usize,
    _same_thread: PhantomData<*const ()>,
}

impl StackLock {
    pub fn here() -> Result<StackLock, LockFailed> {
        if STACK_LOCKED.get() {
            return Err(LockFailed);
        }
        // stack は使った所までしか commit されないので、留める範囲より下まで先に使っておく
        scrub_stack();
        let marker = 0u8;
        let here = std::hint::black_box(&marker) as *const u8 as usize;
        let (start, len) =
            sys::lock_stack(here.saturating_sub(STACK_LOCK_BELOW)).ok_or(LockFailed)?;
        STACK_LOCKED.set(true);
        Ok(StackLock {
            start,
            len,
            _same_thread: PhantomData,
        })
    }
}

impl Drop for StackLock {
    fn drop(&mut self) {
        scrub_stack();
        sys::unlock_stack(self.start, self.len);
        STACK_LOCKED.set(false);
    }
}

#[cfg(windows)]
mod sys {
    use std::ffi::c_void;
    use std::ptr::NonNull;

    use windows::Win32::System::ErrorReporting::{
        WerRegisterExcludedMemoryBlock, WerUnregisterExcludedMemoryBlock,
    };
    use windows::Win32::System::Memory::{
        MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE, VirtualAlloc, VirtualFree,
        VirtualLock, VirtualUnlock,
    };
    use windows::Win32::System::Threading::GetCurrentThreadStackLimits;

    /// VirtualAlloc が返す先頭の揃え (少なくともページの大きさ)。
    pub const PAGE_ALIGN: usize = 4096;

    pub fn alloc_locked(bytes: usize) -> Option<NonNull<u8>> {
        // WER に登録できる大きさ
        let size = u32::try_from(bytes).ok()?;
        // SAFETY: 新しい領域を確保するだけ。VirtualAlloc はページ単位 (予約は 64 KB 単位) で確保するので、
        // ほかの確保とページを分け合わない (VirtualUnlock でほかのロックを外さない)
        let base = unsafe { VirtualAlloc(None, bytes, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE) };
        let base = NonNull::new(base.cast::<u8>())?;
        let ptr = base.as_ptr().cast::<c_void>();
        // SAFETY: 確保したばかりの範囲を留め、WER の dump から外すよう登録する (このプロセスの中だけの登録)。
        // どちらかができなければ、戻してから手放す
        unsafe {
            if VirtualLock(ptr, bytes).is_err() {
                let _ = VirtualFree(ptr, 0, MEM_RELEASE);
                return None;
            }
            if WerRegisterExcludedMemoryBlock(ptr, size).is_err() {
                let _ = VirtualUnlock(ptr, bytes);
                let _ = VirtualFree(ptr, 0, MEM_RELEASE);
                return None;
            }
        }
        Some(base)
    }

    /// # Safety
    ///
    /// `base` と `bytes` は `alloc_locked` が返した範囲で、この後は使わない。
    pub unsafe fn free(base: NonNull<u8>, bytes: usize) {
        let ptr = base.as_ptr().cast::<c_void>();
        // SAFETY: 呼び手が範囲を保証する
        unsafe {
            let _ = WerUnregisterExcludedMemoryBlock(ptr);
            let _ = VirtualUnlock(ptr, bytes);
            let _ = VirtualFree(ptr, 0, MEM_RELEASE);
        }
    }

    /// 今の thread の stack の `from` から上の端までを留める。
    pub fn lock_stack(from: usize) -> Option<(usize, usize)> {
        let (mut low, mut high) = (0usize, 0usize);
        // SAFETY: 今の thread の stack の範囲を読むだけ
        unsafe { GetCurrentThreadStackLimits(&mut low, &mut high) };
        if from <= low || from >= high {
            return None;
        }
        let len = high - from;
        // SAFETY: from から high は今の thread の stack で、呼び手が先に使ったので commit されている
        unsafe { VirtualLock(from as *const c_void, len) }.ok()?;
        Some((from, len))
    }

    pub fn unlock_stack(start: usize, len: usize) {
        // SAFETY: lock_stack が留めた範囲 (今の thread の stack) のロックを外すだけ
        let _ = unsafe { VirtualUnlock(start as *const c_void, len) };
    }
}

#[cfg(not(windows))]
mod sys {
    //! test のため: ページ単位で確保するだけで、物理メモリに留めない。

    use std::alloc::{Layout, alloc_zeroed, dealloc};
    use std::ptr::NonNull;

    pub const PAGE_ALIGN: usize = 4096;

    fn layout(bytes: usize) -> Option<Layout> {
        Layout::from_size_align(bytes.div_ceil(PAGE_ALIGN) * PAGE_ALIGN, PAGE_ALIGN).ok()
    }

    pub fn alloc_locked(bytes: usize) -> Option<NonNull<u8>> {
        let layout = layout(bytes)?;
        // SAFETY: 大きさが 0 でない layout で確保する
        NonNull::new(unsafe { alloc_zeroed(layout) })
    }

    /// # Safety
    ///
    /// `base` と `bytes` は `alloc_locked` が返した範囲で、この後は使わない。
    pub unsafe fn free(base: NonNull<u8>, bytes: usize) {
        if let Some(layout) = layout(bytes) {
            // SAFETY: alloc_locked と同じ layout で手放す
            unsafe { dealloc(base.as_ptr(), layout) };
        }
    }

    pub fn lock_stack(_from: usize) -> Option<(usize, usize)> {
        Some((0, 0))
    }

    pub fn unlock_stack(_start: usize, _len: usize) {}
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
    fn secret_buf_keeps_what_was_pushed() {
        let mut buf = SecretBuf::with_capacity(3).unwrap();
        buf.push(0x61u16);
        buf.push(0x62);
        assert_eq!(&buf[..], &[0x61, 0x62]);
        assert_eq!(buf.capacity(), 3);
    }

    #[test]
    fn secret_buf_starts_on_its_own_page() {
        let buf = SecretBuf::<u64>::with_capacity(1).unwrap();
        assert_eq!(buf.items() as usize % sys::PAGE_ALIGN, 0);
    }

    #[test]
    fn secret_buf_of_zero_capacity_is_empty() {
        let buf = SecretBuf::<u16>::with_capacity(0).unwrap();
        assert!(buf.is_empty());
    }

    #[test]
    #[should_panic(expected = "SecretBuf の容量を超えた")]
    fn secret_buf_does_not_grow() {
        let mut buf = SecretBuf::with_capacity(1).unwrap();
        buf.push(1u8);
        buf.push(2);
    }

    #[test]
    fn secret_buf_refuses_a_size_that_overflows() {
        assert_eq!(
            SecretBuf::<u64>::with_capacity(usize::MAX).err(),
            Some(LockFailed)
        );
    }

    #[test]
    fn debug_shows_only_the_length() {
        let mut buf = SecretBuf::with_capacity(2).unwrap();
        buf.push('p');
        assert_eq!(format!("{buf:?}"), "SecretBuf(1 items)");
    }

    #[test]
    fn secret_cell_keeps_the_value_last_set() {
        let mut cell = SecretCell::new(Some(0x1Eu32)).unwrap();
        assert_eq!(cell.get(), Some(0x1E));
        cell.set(Some(0x30));
        assert_eq!(cell.get(), Some(0x30));
        cell.set(None);
        assert_eq!(cell.get(), None);
    }

    #[test]
    fn secret_cell_starts_on_its_own_page_and_hides_the_value() {
        let cell = SecretCell::new(0x1Eu32).unwrap();
        assert_eq!(cell.0.items() as usize % sys::PAGE_ALIGN, 0);
        assert_eq!(format!("{cell:?}"), "SecretCell(..)");
    }

    #[test]
    fn stack_lock_can_be_taken_and_released() {
        let lock = StackLock::here().unwrap();
        drop(lock);
        scrub_stack();
    }

    #[test]
    fn nested_stack_lock_is_refused() {
        let outer = StackLock::here().unwrap();
        assert!(StackLock::here().is_err());
        drop(outer);
        // 外した後は、また持てる
        drop(StackLock::here().unwrap());
    }

    /// 物理メモリに留めたかを、ページごとの状態 (QueryWorkingSetEx の Locked) で確かめる。
    #[cfg(windows)]
    mod windows_only {
        use std::ffi::c_void;

        use windows::Win32::System::ErrorReporting::{
            WerRegisterExcludedMemoryBlock, WerUnregisterExcludedMemoryBlock,
        };
        use windows::Win32::System::ProcessStatus::{
            K32QueryWorkingSetEx, PSAPI_WORKING_SET_EX_INFORMATION,
        };
        use windows::Win32::System::Threading::GetCurrentProcess;

        use super::super::*;

        /// `addr` を含むページが、物理メモリにあって留められているか。
        fn locked(addr: usize) -> bool {
            let mut info = PSAPI_WORKING_SET_EX_INFORMATION {
                VirtualAddress: addr as *mut c_void,
                ..Default::default()
            };
            // SAFETY: info に書いてもらうだけ
            let ok = unsafe {
                K32QueryWorkingSetEx(
                    GetCurrentProcess(),
                    (&raw mut info).cast(),
                    size_of::<PSAPI_WORKING_SET_EX_INFORMATION>() as u32,
                )
            };
            // SAFETY: union の Flags は全部のビットをそのまま読む
            let flags = unsafe { info.VirtualAttributes.Flags };
            // 0 ビット目が Valid (物理メモリにある)、22 ビット目が Locked
            ok.as_bool() && flags & 1 == 1 && (flags >> 22) & 1 == 1
        }

        #[test]
        fn every_page_of_a_secret_buf_is_locked() {
            // 3 ページにまたがる大きさ
            let buf = SecretBuf::<u64>::with_capacity(3 * 4096 / 8).unwrap();
            let base = buf.items() as usize;
            for page in 0..3 {
                assert!(locked(base + page * 4096), "page {page}");
            }
        }

        #[test]
        fn the_page_of_a_secret_cell_is_locked() {
            let cell = SecretCell::new(0u64).unwrap();
            assert!(locked(cell.0.items() as usize));
        }

        #[test]
        fn secret_buf_is_excluded_from_crash_reports_while_alive() {
            let buf = SecretBuf::<u64>::with_capacity(16).unwrap();
            let base = buf.items() as *const c_void;
            // 登録されていれば外せる。外したら登録し直して、元に戻す
            // SAFETY: このプロセスの WER の登録を変えるだけ
            unsafe {
                WerUnregisterExcludedMemoryBlock(base).unwrap();
                WerRegisterExcludedMemoryBlock(base, 16 * 8).unwrap();
            }
            drop(buf);
            // 手放した後は登録が無い
            // SAFETY: 同上
            assert!(unsafe { WerUnregisterExcludedMemoryBlock(base) }.is_err());
        }

        #[test]
        fn stack_lock_keeps_the_stack_around_here_locked() {
            let lock = StackLock::here().unwrap();
            let marker = 0u8;
            let here = std::hint::black_box(&marker) as *const u8 as usize;
            assert!(locked(here));
            assert!(locked(here - STACK_LOCK_BELOW / 2));
            drop(lock);
            assert!(!locked(here));
        }
    }
}
