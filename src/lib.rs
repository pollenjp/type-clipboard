//! OS に触らない部分。Windows でなくても test が通る。
//!
//! Win32 を呼ぶ部分は bin の側 (`src/win/`) に置く。例外は秘密の写しを置くメモリ (`secret`) で、Windows では
//! VirtualAlloc と VirtualLock を使う (ほかの OS では test のために、ふつうに確保する)。

// test の失敗の文には Debug の書式を使ってよい (Cargo.toml の [lints.clippy] の use_debug)
#![cfg_attr(test, allow(clippy::use_debug))]

pub mod candidate;
pub mod geometry;
pub mod keymap;
pub mod pick;
pub mod secret;
pub mod settings;
pub mod status;
pub mod typing;
