//! 教育モード（Explore）用のワーカーエンジン。
//!
//! - π(x) vs x/log x のアニメーショングラフを描画するためのデータを生成します。
//! - primecount を使って π(x) を計算し、UI にデータポイントを送信します。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};

use crate::prime_pi_engine::compute_prime_pi;
use crate::sieve_math::simple_sieve;
use crate::verify::is_probable_prime;
use crate::worker_message::WorkerMessage;

/// Explore モードのアニメーションを開始する。
///
/// - `prime_min` から `prime_max` まで、段階的に x を増やしながら
///   π(x) を計算し、`WorkerMessage::ExploreData` として送信します。
/// - `speed` はアニメーション速度（1.0 = 標準、2.0 = 2倍速）
pub fn start_explore_animation(
    prime_min: u64,
    prime_max: u64,
    speed: f32,
    stop_flag: Arc<AtomicBool>,
    sender: mpsc::Sender<WorkerMessage>,
) {
    std::thread::spawn(move || {
        sender
            .send(WorkerMessage::Log(format!(
                "Starting π(x) visualization for range [{}, {}]...",
                prime_min, prime_max
            )))
            .ok();

        // ステップ数を決定（最大500ポイント程度）
        let range = prime_max.saturating_sub(prime_min);
        let num_steps = 200.min(range as usize).max(10);
        let step_size = range / num_steps as u64;

        // 速度インデックスに応じたスリープ時間（ms）
        // speed: 0.0 => 1x, 1.0 => 3x, 2.0 => MAX(0ms)
        let base_delay_ms: u64 = if speed < 0.5 {
            50 // 1x
        } else if speed < 1.5 {
            (50.0 / 3.0) as u64 // 約 3x
        } else {
            0 // MAX（待ち時間なし）
        };

        let mut x = prime_min;
        let mut step = 0;

        while x <= prime_max && !stop_flag.load(Ordering::SeqCst) {
            // π(x) を計算
            match compute_prime_pi(x) {
                Ok(pi_x) => {
                    if sender.send(WorkerMessage::ExploreData { x, pi_x }).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    sender
                        .send(WorkerMessage::Log(format!(
                            "Error computing π({}): {}",
                            x, e
                        )))
                        .ok();
                    break;
                }
            }

            // 進捗を送信
            let progress = step as f32 / num_steps as f32;
            sender
                .send(WorkerMessage::Progress {
                    current: (progress * 100.0) as u64,
                    total: 100,
                })
                .ok();

            // 次のステップへ
            step += 1;
            if x == prime_max {
                break;
            }
            x = (x + step_size).min(prime_max);

            // アニメーション用のディレイ
            if base_delay_ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(base_delay_ms));
            }
        }

        if stop_flag.load(Ordering::SeqCst) {
            sender.send(WorkerMessage::Stopped).ok();
        } else {
            sender
                .send(WorkerMessage::Log(format!(
                    "Visualization complete. {} data points generated.",
                    step
                )))
                .ok();
            sender.send(WorkerMessage::Done).ok();
        }
    });
}

/// Gap モードのアニメーションを開始する。
///
/// - [prime_min, prime_max] の範囲で素数を列挙し、隣接素数のギャップを計算して
///   `WorkerMessage::GapData` として送信する。
/// - `speed` はアニメーション速度（1.0 = 標準、2.0 = 2倍速）で、送信間隔に反映される。
pub fn start_gap_animation(
    prime_min: u64,
    prime_max: u64,
    speed: f32,
    stop_flag: Arc<AtomicBool>,
    sender: mpsc::Sender<WorkerMessage>,
) {
    std::thread::spawn(move || {
        let prime_min = prime_min.max(2);

        sender
            .send(WorkerMessage::Log(format!(
                "Starting prime gap visualization for range [{}, {}]...",
                prime_min, prime_max
            )))
            .ok();

        if prime_min >= prime_max {
            sender
                .send(WorkerMessage::Log(
                    "Invalid range: min must be less than max.".to_string(),
                ))
                .ok();
            let _ = sender.send(WorkerMessage::Done);
            return;
        }

        // 素数を事前に列挙（simple_sieve は [2, prime_max] の素数を返す）
        let primes_res = simple_sieve(prime_max);
        let primes = match primes_res {
            Ok(p) => p,
            Err(e) => {
                sender
                    .send(WorkerMessage::Log(format!(
                        "Error while generating primes for gap visualization: {}",
                        e
                    )))
                    .ok();
                let _ = sender.send(WorkerMessage::Done);
                return;
            }
        };

        // prime_min 以上の最初の素数の位置を探しつつ、隣接素数ペアごとにギャップを生成
        let mut prev_prime: Option<u64> = None;
        let mut gaps: Vec<(u64, u64, u64)> = Vec::new(); // (prev, prime, gap)

        for &p in primes.iter() {
            if p < prime_min {
                prev_prime = Some(p);
                continue;
            }
            if p > prime_max {
                break;
            }
            if let Some(prev) = prev_prime {
                let gap = p.saturating_sub(prev);
                gaps.push((prev, p, gap));
            }
            prev_prime = Some(p);
        }

        let total_gaps = gaps.len() as u64;
        if total_gaps == 0 {
            sender
                .send(WorkerMessage::Log(
                    "No prime gaps found in the selected range.".to_string(),
                ))
                .ok();
            let _ = sender.send(WorkerMessage::Done);
            return;
        }

        // 速度インデックスに応じたスリープ時間（ms）
        let base_delay_ms: u64 = if speed < 0.5 {
            50 // 1x
        } else if speed < 1.5 {
            (50.0 / 3.0) as u64 // 約 3x
        } else {
            0 // MAX
        };

        for (idx, (prev, prime, gap)) in gaps.into_iter().enumerate() {
            if stop_flag.load(Ordering::SeqCst) {
                sender.send(WorkerMessage::Stopped).ok();
                return;
            }

            if sender
                .send(WorkerMessage::GapData {
                    prime,
                    prev_prime: prev,
                    gap,
                })
                .is_err()
            {
                return;
            }

            // 進捗を送信
            let current = (idx + 1) as u64;
            sender
                .send(WorkerMessage::Progress {
                    current,
                    total: total_gaps,
                })
                .ok();

            if base_delay_ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(base_delay_ms));
            }
        }

        sender
            .send(WorkerMessage::Log(
                "Gap visualization complete.".to_string(),
            ))
            .ok();
        let _ = sender.send(WorkerMessage::Done);
    });
}

/// Density モードのアニメーションを開始する。
///
/// - [prime_min, prime_max] の範囲で素数を列挙し、区間ごとの素数個数を計算して
///   `WorkerMessage::DensityData` として送信する。
/// - `interval_size` は区間幅、`speed` はアニメーション速度。
pub fn start_density_animation(
    prime_min: u64,
    prime_max: u64,
    interval_size: u64,
    speed: f32,
    stop_flag: Arc<AtomicBool>,
    sender: mpsc::Sender<WorkerMessage>,
) {
    std::thread::spawn(move || {
        let prime_min = prime_min.max(2);
        let interval_size = interval_size.max(1);

        sender
            .send(WorkerMessage::Log(format!(
                "Starting prime density visualization for range [{}, {}] (interval = {})...",
                prime_min, prime_max, interval_size
            )))
            .ok();

        if prime_min >= prime_max {
            sender
                .send(WorkerMessage::Log(
                    "Invalid range: min must be less than max.".to_string(),
                ))
                .ok();
            let _ = sender.send(WorkerMessage::Done);
            return;
        }

        // 素数を事前に列挙
        let primes_res = simple_sieve(prime_max);
        let primes = match primes_res {
            Ok(p) => p,
            Err(e) => {
                sender
                    .send(WorkerMessage::Log(format!(
                        "Error while generating primes for density visualization: {}",
                        e
                    )))
                    .ok();
                let _ = sender.send(WorkerMessage::Done);
                return;
            }
        };

        // 区間数を計算
        let mut intervals: Vec<(u64, u64)> = Vec::new(); // (start, count)
        let mut idx = 0usize;
        let mut start = prime_min;

        while start <= prime_max {
            if stop_flag.load(Ordering::SeqCst) {
                sender.send(WorkerMessage::Stopped).ok();
                return;
            }

            let end = start.saturating_add(interval_size - 1).min(prime_max);

            // idx を現在の start まで進める
            while idx < primes.len() && primes[idx] < start {
                idx += 1;
            }

            let mut count = 0u64;
            let mut j = idx;
            while j < primes.len() && primes[j] <= end {
                count += 1;
                j += 1;
            }

            intervals.push((start, count));
            start = end.saturating_add(1);
        }

        let total_intervals = intervals.len() as u64;
        if total_intervals == 0 {
            sender
                .send(WorkerMessage::Log(
                    "No intervals found in the selected range.".to_string(),
                ))
                .ok();
            let _ = sender.send(WorkerMessage::Done);
            return;
        }

        let base_delay_ms: u64 = if speed < 0.5 {
            50 // 1x
        } else if speed < 1.5 {
            (50.0 / 3.0) as u64 // 約 3x
        } else {
            0 // MAX
        };

        for (i, (start, count)) in intervals.into_iter().enumerate() {
            if stop_flag.load(Ordering::SeqCst) {
                sender.send(WorkerMessage::Stopped).ok();
                return;
            }

            if sender
                .send(WorkerMessage::DensityData {
                    interval_start: start,
                    count,
                })
                .is_err()
            {
                return;
            }

            let current = (i + 1) as u64;
            sender
                .send(WorkerMessage::Progress {
                    current,
                    total: total_intervals,
                })
                .ok();

            if base_delay_ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(base_delay_ms));
            }
        }

        sender
            .send(WorkerMessage::Log(
                "Density visualization complete.".to_string(),
            ))
            .ok();
        let _ = sender.send(WorkerMessage::Done);
    });
}

/// Spiral モード（Ulam Spiral / ハニカム螺旋共通）のアニメーションを開始する。
///
/// - `center` を起点として、整数列 `center, center+1, ...` について素数判定を行います。
/// - 判定結果は「ステップ順一次元列」として `Vec<bool>` に格納され、
///   `k` 番目の要素は `n = center + k` が素数かどうかを表します。
/// - グリッド形状（スクエア / ハニカム）は UI 側でこの一次元列をそれぞれの座標系に
///   マッピングして描画します（エンジン側は形状非依存）。
/// - 途中経過を `WorkerMessage::SpiralData` として何度も送り、UI 側で順次描画します。
pub fn start_spiral_generation(
    center: u64,
    size: usize,
    speed: f32,
    stop_flag: Arc<AtomicBool>,
    sender: mpsc::Sender<WorkerMessage>,
) {
    std::thread::spawn(move || {
        if size == 0 {
            let _ = sender.send(WorkerMessage::Done);
            return;
        }

        sender
            .send(WorkerMessage::Log(format!(
                "Starting Ulam spiral visualization (center = {}, size = {}x{})...",
                center, size, size
            )))
            .ok();

        let total_cells = (size as u64).saturating_mul(size as u64);
        let mut primes = vec![false; total_cells as usize];

        // 速度インデックスに応じたスリープ時間（ms）
        let base_delay_ms: u64 = if speed < 0.5 {
            30 // 1x
        } else if speed < 1.5 {
            (30.0 / 3.0) as u64 // 約 3x
        } else {
            0 // MAX
        };

        // ステップ順一次元配列として、center, center+1, ... の素数判定を行う
        let update_every: u64 = (size as u64).max(1);
        for step in 0..total_cells {
            if stop_flag.load(Ordering::SeqCst) {
                sender.send(WorkerMessage::Stopped).ok();
                return;
            }

            let n = center.saturating_add(step);
            if is_probable_prime(n) {
                primes[step as usize] = true;
            }

            let cells_done = step + 1;

            // 一定ステップごとに UI へ送信
            if cells_done % update_every == 0 || cells_done >= total_cells {
                let _ = sender.send(WorkerMessage::SpiralData {
                    primes: primes.clone(),
                    size,
                });
                let _ = sender.send(WorkerMessage::Progress {
                    current: cells_done.min(total_cells),
                    total: total_cells,
                });

                if base_delay_ms > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(base_delay_ms));
                }
            }
        }

        sender
            .send(WorkerMessage::Log(
                "Spiral visualization complete.".to_string(),
            ))
            .ok();
        let _ = sender.send(WorkerMessage::Done);
    });
}
