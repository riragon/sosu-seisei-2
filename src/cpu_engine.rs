use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use bitvec::prelude::*;
use rayon::prelude::*;

use crate::config::{Config, WheelType};
use crate::engine_types::{compute_eta, Progress, PrimeResult};
use crate::output::PrimeWriter;
use crate::sieve_math::{integer_sqrt, simple_sieve};

/// CPU ベースの分割エラトステネスの篩で素数を生成する。
///
/// 進捗・ログに関する契約:
/// - この関数は長時間実行されることを前提としており、`progress_cb` を通じて
///   「全体に対する進捗（`Progress::processed` / `Progress::total`）と ETA」を定期的に報告します。
/// - 少なくとも「各セグメントグループ（`group_size` 件）」の処理後には 1 回以上 `progress_cb` を呼びます。
/// - 呼び出し側（`app.rs` のワーカースレッド）は受け取った進捗をそのまま UI に流すため、
///   コール頻度を極端に落とすと「進捗が止まったように見える」ことに注意してください。
///
/// シグネチャ（引数と戻り値の型）は UI 層・GPU エンジンとの共通 API として扱うため、
/// 変更する場合は呼び出し元すべての確認が必要です。
pub fn generate_primes_cpu(
    cfg: &Config,
    stop_flag: &AtomicBool,
    writer: &mut dyn PrimeWriter,
    mut progress_cb: impl FnMut(Progress),
) -> PrimeResult<()> {
    let prime_min = cfg.prime_min;
    let prime_max = cfg.prime_max;
    if prime_min > prime_max {
        return Err("prime_min must be <= prime_max".into());
    }

    let start_time = Instant::now();
    let total_range = prime_max - prime_min + 1;
    let wheel_type = cfg.wheel_type;

    if stop_flag.load(Ordering::SeqCst) {
        return Ok(());
    }

    // メモリ制限に基づいてセグメントサイズを調整
    use crate::memory;
    let num_threads = rayon::current_num_threads();
    let optimal_segment_size = memory::calculate_optimal_segment_size(
        cfg.memory_usage_percent,
        num_threads,
        wheel_type,
    );
    let segment_size = if cfg.segment_size > 0 {
        cfg.segment_size.min(optimal_segment_size)
    } else {
        optimal_segment_size
    };

    // メモリ情報をログ出力
    let mem_info = memory::get_memory_info(segment_size, num_threads, wheel_type);
    log::info!("ホイールタイプ: {:?}, セグメントサイズ: {}", wheel_type, segment_size);
    log::info!("{}", mem_info.format());

    // small primes up to sqrt(max)
    let root = integer_sqrt(prime_max) + 1;
    let small_primes = simple_sieve(root)?;

    // ホイールタイプに応じた小さい素数の特別処理
    let wheel_excluded_primes: Vec<u64> = match wheel_type {
        WheelType::Odd => vec![2],
        WheelType::Mod6 => vec![2, 3],
        WheelType::Mod30 => vec![2, 3, 5],
    };

    for &p in &wheel_excluded_primes {
        if prime_min <= p && p <= prime_max {
            writer.write_prime(p)?;
        }
    }

    // セグメントの開始位置は、ホイールで除外された素数より大きい値から
    let sieve_start = wheel_excluded_primes
        .last()
        .map(|&p| p + 1)
        .unwrap_or(prime_min);

    // これ以上篩う範囲がなければ終了
    if sieve_start > prime_max {
        progress_cb(Progress {
            processed: total_range,
            total: total_range,
            eta_secs: Some(0),
        });
        writer.finish()?;
        return Ok(());
    }

    // 篩い開始位置を決定
    let mut seg_start = sieve_start.max(prime_min);
    if seg_start > prime_max {
        writer.finish()?;
        return Ok(());
    }

    // 全セグメント数を概算（ベクタには保持しない）
    let remaining = prime_max - seg_start + 1;
    let total_segments = (remaining.div_ceil(segment_size)) as usize;

    // グループサイズはスレッド数に合わせる（過剰な同時結果保持を防ぐ）
    let group_size = num_threads.max(1);

    let total_groups = total_segments.div_ceil(group_size);
    log::info!(
        "Processing ~{} segments in groups of {} (total {} groups)",
        total_segments,
        group_size,
        total_groups
    );

    // 進捗カウンタ（seg_start より前の範囲は既に処理済みとみなす）
    let mut processed = seg_start.saturating_sub(prime_min);

    #[derive(Clone)]
    struct SegmentResult {
        low: u64,
        high: u64,
        primes: Vec<u64>,
    }

    let mut group_index = 0usize;

    // セグメントを逐次生成しつつ、グループ単位で並列処理
    while seg_start <= prime_max {
        if stop_flag.load(Ordering::SeqCst) {
            writer.finish()?;
            return Ok(());
        }

        group_index += 1;

        // このグループで処理するセグメント境界を生成（メモリに保持するのはこのグループ分だけ）
        let mut group_bounds: Vec<(u64, u64)> = Vec::with_capacity(group_size);
        for _ in 0..group_size {
            if seg_start > prime_max {
                break;
            }
            let seg_end = seg_start
                .saturating_add(segment_size - 1)
                .min(prime_max);
            group_bounds.push((seg_start, seg_end));
            seg_start = seg_end.saturating_add(1);
        }

        if group_bounds.is_empty() {
            break;
        }

        log::info!(
            "Processing group {}/{} ({} segments)...",
            group_index,
            total_groups,
            group_bounds.len()
        );

        // グループ内を並列処理
        let mut results: Vec<SegmentResult> = group_bounds
            .par_iter()
            .map(|&(low, high)| {
                if stop_flag.load(Ordering::SeqCst) {
                    SegmentResult {
                        low,
                        high,
                        primes: Vec::new(),
                    }
                } else {
                    let primes =
                        sieve_segment_collect(low, high, &small_primes, stop_flag, wheel_type);
                    SegmentResult { low, high, primes }
                }
            })
            .collect();

        // セグメント開始値でソートして順序を保証
        results.sort_by_key(|r| r.low);

        // 即座に出力（グループごとにメモリを解放）
        for res in results {
            if stop_flag.load(Ordering::SeqCst) {
                writer.finish()?;
                return Ok(());
            }

            for p in res.primes {
                writer.write_prime(p)?;
            }

            processed = processed.saturating_add(res.high - res.low + 1);
        }

        // グループ処理完了後に進捗を更新（リアルタイム）
        let elapsed = start_time.elapsed().as_secs_f64();
        let eta_secs = compute_eta(processed.min(total_range), total_range, elapsed);

        progress_cb(Progress {
            processed: processed.min(total_range),
            total: total_range,
            eta_secs,
        });

        log::info!(
            "Group {}/{} completed. Overall progress: {:.1}%",
            group_index,
            total_groups,
            (processed.min(total_range) as f64 / total_range as f64) * 100.0
        );
    }

    log::info!("All processing completed");

    writer.finish()?;
    Ok(())
}

fn sieve_segment_collect(
    low_inclusive: u64,
    high_inclusive: u64,
    small_primes: &[u64],
    stop_flag: &AtomicBool,
    wheel_type: WheelType,
) -> Vec<u64> {
    if low_inclusive > high_inclusive {
        return Vec::new();
    }

    // ホイールに応じた最小値の調整
    let low = adjust_low(low_inclusive, wheel_type);
    if low > high_inclusive {
        return Vec::new();
    }
    let high = high_inclusive;

    // ビット配列のサイズを計算
    let len = calculate_bitvec_size(low, high, wheel_type);
    let mut is_prime = bitvec![1; len];

    // 篩処理
    for &p in small_primes {
        if stop_flag.load(Ordering::SeqCst) {
            return Vec::new();
        }
        
        // ホイールで既に除外されている素数はスキップ
        match wheel_type {
            WheelType::Odd => {
                if p == 2 {
                    continue;
                }
            }
            WheelType::Mod6 => {
                if p == 2 || p == 3 {
                    continue;
                }
            }
            WheelType::Mod30 => {
                if p == 2 || p == 3 || p == 5 {
                    continue;
                }
            }
        }
        
        if p * p > high {
            break;
        }

        // 開始位置を計算
        let mut start = if low % p == 0 {
            low
        } else {
            low + (p - (low % p))
        };
        if start < p * p {
            start = p * p;
        }

        // ホイールの候補に合わせて調整
        while start <= high {
            if n_to_index(start, low, wheel_type).is_some() {
                break;
            }
            start += p;
        }

        // マーク処理
        let mut n = start;
        while n <= high {
            if stop_flag.load(Ordering::SeqCst) {
                return Vec::new();
            }
            
            if let Some(idx) = n_to_index(n, low, wheel_type) {
                if idx < len {
                    is_prime.set(idx, false);
                }
            }
            
            // 次の候補を探す
            n += p;
            while n <= high && n_to_index(n, low, wheel_type).is_none() {
                n += p;
            }
        }
    }

    // 素数を収集
    let mut primes = Vec::new();
    for (i, bit) in is_prime.iter().by_vals().enumerate() {
        if bit {
            let n = index_to_n(i, low, wheel_type);
            if n <= high {
                primes.push(n);
            }
        }
    }

    primes
}

// ========== ホイール構造関連の関数 ==========

/// mod 30 ホイールの候補パターン (30で割った余り)
const MOD30_PATTERN: [u64; 8] = [1, 7, 11, 13, 17, 19, 23, 29];

/// mod 30 での余りからインデックスへのマッピング
/// 候補でない数は 255 を返す
const MOD30_TO_INDEX: [u8; 30] = [
    255, 0, 255, 255, 255, 255, 255, 1,   // 0-7
    255, 255, 255, 2, 255, 3, 255, 255,   // 8-15
    255, 4, 255, 5, 255, 255, 255, 6,     // 16-23
    255, 255, 255, 255, 255, 7            // 24-29
];

/// ホイールタイプに応じた数値nからインデックスへの変換
/// low: セグメントの開始位置（調整済み）
/// 戻り値: Some(index) または None（候補でない場合）
fn n_to_index(n: u64, low: u64, wheel_type: WheelType) -> Option<usize> {
    if n < low {
        return None;
    }
    
    match wheel_type {
        WheelType::Odd => {
            // 奇数のみ
            if n % 2 == 1 && low % 2 == 1 {
                Some(((n - low) / 2) as usize)
            } else {
                None
            }
        }
        WheelType::Mod6 => {
            // mod 6: 6k+1, 6k+5 のみ
            let n_rem = n % 6;
            let low_rem = low % 6;
            
            if (n_rem != 1 && n_rem != 5) || (low_rem != 1 && low_rem != 5) {
                return None;
            }
            
            // lowとnがどちらも候補の場合
            // lowからnまでの候補の個数を数える
            let low_in_period = (low / 6) * 2 + if low_rem == 5 { 1 } else { 0 };
            let n_in_period = (n / 6) * 2 + if n_rem == 5 { 1 } else { 0 };
            
            Some((n_in_period - low_in_period) as usize)
        }
        WheelType::Mod30 => {
            // mod 30: パターンテーブルを使用
            let n_rem = (n % 30) as usize;
            let low_rem = (low % 30) as usize;
            
            let n_idx = MOD30_TO_INDEX[n_rem];
            let low_idx = MOD30_TO_INDEX[low_rem];
            
            if n_idx == 255 || low_idx == 255 {
                return None;
            }
            
            // lowからnまでの候補の個数を数える
            let low_in_period = (low / 30) * 8 + low_idx as u64;
            let n_in_period = (n / 30) * 8 + n_idx as u64;
            
            Some((n_in_period - low_in_period) as usize)
        }
    }
}

/// インデックスから数値nへの変換
/// low: セグメントの開始位置（調整済み、候補値）
/// idx: ビット配列のインデックス（0から始まる）
fn index_to_n(idx: usize, low: u64, wheel_type: WheelType) -> u64 {
    match wheel_type {
        WheelType::Odd => {
            // 奇数のみ
            low + (idx as u64) * 2
        }
        WheelType::Mod6 => {
            // mod 6: 6k+1, 6k+5
            let low_rem = low % 6;
            debug_assert!(low_rem == 1 || low_rem == 5, "low must be a candidate");
            
            // lowが何番目の候補か計算
            let low_in_period = (low / 6) * 2 + if low_rem == 5 { 1 } else { 0 };
            
            // idx番目の候補を計算
            let target_in_period = low_in_period + idx as u64;
            let period = target_in_period / 2;
            let offset = target_in_period % 2;
            
            if offset == 0 {
                period * 6 + 1
            } else {
                period * 6 + 5
            }
        }
        WheelType::Mod30 => {
            // mod 30: パターンテーブルを使用
            let low_rem = (low % 30) as usize;
            let low_idx = MOD30_TO_INDEX[low_rem];
            debug_assert!(low_idx != 255, "low must be a candidate");
            
            // lowが何番目の候補か計算
            let low_in_period = (low / 30) * 8 + low_idx as u64;
            
            // idx番目の候補を計算
            let target_in_period = low_in_period + idx as u64;
            let period = target_in_period / 8;
            let offset = (target_in_period % 8) as usize;
            
            period * 30 + MOD30_PATTERN[offset]
        }
    }
}

/// ホイールタイプに応じたビット配列のサイズを計算
fn calculate_bitvec_size(low: u64, high: u64, wheel_type: WheelType) -> usize {
    match wheel_type {
        WheelType::Odd => {
            // 奇数のみ: (high - low) / 2 + 1
            ((high - low) / 2 + 1) as usize
        }
        WheelType::Mod6 => {
            // mod 6: 6 周期あたり 2 個
            let range = high - low + 1;
            ((range / 6) * 2 + 2) as usize  // 余裕を持たせる
        }
        WheelType::Mod30 => {
            // mod 30: 30 周期あたり 8 個
            let range = high - low + 1;
            ((range / 30) * 8 + 8) as usize  // 余裕を持たせる
        }
    }
}

/// low を次の候補値に調整
fn adjust_low(low: u64, wheel_type: WheelType) -> u64 {
    match wheel_type {
        WheelType::Odd => {
            if low % 2 == 0 {
                low + 1
            } else {
                low
            }
        }
        WheelType::Mod6 => {
            let rem = low % 6;
            match rem {
                1 | 5 => low,  // 既に候補
                0 => low + 1,  // 6k → 6k+1
                2 => low + 3,  // 6k+2 → 6k+5
                3 => low + 2,  // 6k+3 → 6k+5
                4 => low + 1,  // 6k+4 → 6k+5
                _ => unreachable!(),
            }
        }
        WheelType::Mod30 => {
            let rem = low % 30;
            if MOD30_TO_INDEX[rem as usize] != 255 {
                return low;  // 既に候補
            }
            
            // 次の候補に調整
            for offset in 1..30 {
                let new_rem = ((rem + offset) % 30) as usize;
                if MOD30_TO_INDEX[new_rem] != 255 {
                    return low + offset;
                }
            }
            low  // ここには到達しないはず
        }
    }
}


