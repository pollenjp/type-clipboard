//! OS に触らない部分。Windows でなくても test が通る。
//!
//! Win32 を呼ぶ部分は bin の側 (`src/win/`) に置く。

pub mod keymap;
pub mod secret;
pub mod settings;
pub mod typing;
