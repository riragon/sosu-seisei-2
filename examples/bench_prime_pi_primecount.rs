use std::time::Instant;

use sosu_seisei_main2::prime_pi_engine::compute_prime_pi;

/// primecount ベースの π(x) の簡易ベンチマーク。
///
/// 使い方:
/// ```bash
/// cargo run --example bench_prime_pi_primecount --release
/// ```
///
/// ※ 実行には primecount ライブラリのインストールとリンク設定が必要です。
fn main() {
    env_logger::init();

    // 負荷と所要時間のバランスを見て適宜変更してください。
    let test_points: &[u64] = &[
        10_000,
        1_000_000,
        100_000_000,
        1_000_000_000,
        // 10^11 は環境によっては時間がかかる可能性があるためコメントアウトしています。
        // 100_000_000_000,
    ];

    println!("=== primecount-based π(x) benchmark ===");
    for &x in test_points {
        println!("Computing pi({x}) ...");
        let start = Instant::now();
        match compute_prime_pi(x) {
            Ok(pi) => {
                let elapsed = start.elapsed();
                println!(
                    "  pi({x}) = {pi}  (elapsed: {:.3?})",
                    elapsed
                );
            }
            Err(e) => {
                println!("  Error while computing pi({x}): {e}");
            }
        }
        println!();
    }
}


