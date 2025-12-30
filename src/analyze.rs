//! Sosu-Analyze（Analyze Tabs）関連のエントリポイント。
//!
//! - `mod.rs` は使わず、`src/analyze.rs` + `src/analyze/*.rs` の形で整理する。

pub mod common;
pub mod tab_last_digit;
pub mod tab_mod30;
pub mod tab_prhs;
pub mod tab_validation;
pub mod ui_analyze;
pub mod ui_tab_prhs;
pub mod ui_tab_prhs210;
pub mod ui_tab_validation;
mod validation_engine;

pub use common::*;

// 外部から頻繁に使うものだけ「入口」として再公開する。
pub use tab_validation::analyze_validation_binary_file;
pub use ui_analyze::render_analyze_panel;
