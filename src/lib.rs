//! OS に触らない部分。Windows でなくても test が通る。
//!
//! Win32 を呼ぶ部分は bin の側 (`src/win/`) に置く。

pub mod candidate;
pub mod geometry;
pub mod keymap;
pub mod secret;
pub mod settings;
pub mod status;
pub mod typing;
