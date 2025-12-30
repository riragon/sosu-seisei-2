//! Sosu-Seisei（Generator + 教育タブ）関連のエントリポイント。
//!
//! - `mod.rs` は使わず、`src/seisei.rs` + `src/seisei/*.rs` の形で整理する。

pub mod cpu_engine;
pub mod explore_engine;
pub mod memory;
pub mod output;
pub mod prime_pi_engine;
pub mod sieve_math;
pub mod ui_density;
pub mod ui_explore;
pub mod ui_gap;
pub mod ui_generator;
pub mod ui_spiral;
pub mod verify;

// 外部から頻繁に使うものだけ「入口」として再公開する。
//
// - 呼び出し側は `crate::seisei::generate_primes_cpu` のように短いパスでアクセスできる。
// - 内部実装（細かい補助関数等）は各 submodule に閉じ込めたままにできる。

// 主要エンジン
pub use cpu_engine::generate_primes_cpu;
pub use prime_pi_engine::{compute_prime_pi, PRIMECOUNT_MODE, PRIMECOUNT_VERSION};
pub use verify::{is_probable_prime, verify_primes_file, LogCallback, VerifyReport};

// 出力
pub use output::{FilePrimeWriter, LastPrimeWriter, OutputMetadata, PrimeWriter};

// UI パネル
pub use ui_density::render_density_panel;
pub use ui_explore::render_explore_panel;
pub use ui_gap::render_gap_panel;
pub use ui_generator::render_generator_panel;
pub use ui_spiral::render_spiral_panel;
