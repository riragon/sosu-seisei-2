//! `MyApp` のワーカー起動ロジック（Run ボタンで動く処理）をまとめたモジュール。
//!
//!
//! - Generator の素数生成 (`start_worker`)
//! - 区間の素数個数を primecount で数える (`start_prime_pi`)
//! - 教育タブ用アニメーション (`start_explore`, `start_gap`, `start_density`, `start_spiral`)

use std::sync::atomic::Ordering;
use std::sync::mpsc;

use chrono::Local;

use crate::analyze::tab_last_digit::{analyze_last_digits_binary_file, LastDigitResult};
use crate::analyze::tab_mod30::{analyze_mod30_binary_file, Mod30Result};
use crate::analyze::tab_prhs::{
    analyze_prhs210_binary_file, analyze_prhs_binary_file, PRHS210Result, PRHSResult,
};
use crate::analyze::tab_validation::analyze_validation_binary_file;
use crate::config::save_config;
use crate::engine_types::{PrimeResult, Progress};
use crate::seisei::cpu_engine::generate_primes_cpu;
use crate::seisei::output::{FilePrimeWriter, LastPrimeWriter, OutputMetadata};
use crate::seisei::prime_pi_engine::{compute_prime_pi, PRIMECOUNT_MODE, PRIMECOUNT_VERSION};
use crate::seisei::verify::{verify_primes_file, LogCallback};
use crate::worker_message::{format_eta, WorkerMessage};

use crate::analyze::AnalyzeTab;
use crate::app_state::MyApp;

impl MyApp {
    /// Sosu-Analyze モード: バイナリ primes ファイルを読み込み、末尾 1/3/7/9 の出現率を集計する
    pub fn start_analyze(&mut self) {
        if self.is_running {
            self.log
                .push_str("Cannot start analysis while a computation is running.\n");
            return;
        }

        let tab = self.analyze.current_tab;

        let path_str = self.analyze.file_path.trim();
        if path_str.is_empty() {
            self.log.push_str("Analyze file path is empty.\n");
            return;
        }

        // 状態をリセット
        self.analyze.running = true;
        self.is_running = true;
        self.analyze.progress = 0.0;
        self.analyze.total_primes = 0;
        self.analyze.file_loaded = true;
        // リアルタイム表示用の共有カウントをリセット
        if let Ok(mut g) = self.analyze.shared_processed.lock() {
            *g = 0;
        }
        // タブごとの結果をリセット
        match tab {
            AnalyzeTab::LastDigit => {
                self.analyze.last_digit = LastDigitResult::default();
                if let Ok(mut g) = self.analyze.shared_result.lock() {
                    *g = LastDigitResult::default();
                }
            }
            AnalyzeTab::Mod30 => {
                self.analyze.mod30 = Mod30Result::default();
                if let Ok(mut g) = self.analyze.shared_mod30.lock() {
                    *g = Mod30Result::default();
                }
            }
            AnalyzeTab::Mod30PRHS => {
                self.analyze.prhs = PRHSResult::default();
                if let Ok(mut g) = self.analyze.shared_prhs.lock() {
                    *g = PRHSResult::default();
                }
            }
            AnalyzeTab::Mod210PRHS => {
                self.analyze.prhs210 = PRHS210Result::default();
                if let Ok(mut g) = self.analyze.shared_prhs210.lock() {
                    *g = PRHS210Result::default();
                }
            }
            AnalyzeTab::Validation => {
                self.analyze.validation =
                    crate::analyze::tab_validation::ValidationBundle::default();
                if let Ok(mut g) = self.analyze.shared_validation.lock() {
                    *g = crate::analyze::tab_validation::ValidationBundle::default();
                }
            }
        }
        self.stop_flag.store(false, Ordering::SeqCst);
        self.log.clear();

        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);

        let stop_flag = self.stop_flag.clone();
        let path = std::path::PathBuf::from(path_str);
        let shared_processed = self.analyze.shared_processed.clone();
        let shared_last_digit = self.analyze.shared_result.clone();
        let shared_mod30 = self.analyze.shared_mod30.clone();
        let shared_prhs = self.analyze.shared_prhs.clone();
        let shared_prhs210 = self.analyze.shared_prhs210.clone();
        let shared_validation = self.analyze.shared_validation.clone();

        // PRHS パラメータ（UI入力→パース。空/不正はデフォルト）
        let prhs_log10_bin_width = self
            .analyze
            .prhs_log10_bin_width_input
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|v| *v > 0.0 && *v <= 2.0)
            .unwrap_or(0.2_f64);
        let prhs_equal_bin_primes = self
            .analyze
            .prhs_equal_bin_primes_input
            .trim()
            .parse::<u64>()
            .ok()
            .filter(|v| *v > 0)
            .unwrap_or(5_000_000_u64);
        let prhs_train_ratio = self
            .analyze
            .prhs_train_ratio_input
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|v| *v >= 0.1 && *v <= 0.95)
            .unwrap_or(0.7_f64);
        let prhs_exclude_diagonal = self.analyze.prhs_exclude_diagonal;
        let prhs_min_nij = self
            .analyze
            .prhs_min_context_nij_input
            .trim()
            .parse::<u64>()
            .ok()
            .unwrap_or(0);

        // Validation パラメータ（mod30 + mod210 を同時計算）
        let validation_wheel_samples = self
            .analyze
            .validation_wheel_samples_input
            .trim()
            .parse::<u64>()
            .ok()
            .filter(|v| *v > 0);
        let validation_seed = self
            .analyze
            .validation_seed_input
            .trim()
            .parse::<u64>()
            .ok()
            .filter(|v| *v > 0)
            .unwrap_or(1u64);
        let validation_ranges = {
            fn parse_token(tok: &str) -> Option<u64> {
                let t = tok.trim();
                if t.is_empty() {
                    return None;
                }
                // "10^6" 形式
                if let Some((a, b)) = t.split_once('^') {
                    let base = a.trim().parse::<u64>().ok()?;
                    let exp = b.trim().parse::<u32>().ok()?;
                    return base.checked_pow(exp);
                }
                // "1e6" 形式
                if t.contains('e') || t.contains('E') {
                    let v = t.parse::<f64>().ok()?;
                    if v.is_finite() && v > 0.0 {
                        return Some(v.round() as u64);
                    }
                }
                t.parse::<u64>().ok()
            }

            let mut out: Vec<u64> = self
                .analyze
                .validation_ranges_input
                .split([',', ' ', '\n', '\t'])
                .filter_map(parse_token)
                .filter(|v| *v > 0)
                .collect();
            if out.is_empty() {
                // デフォルト（軽量な3点）
                out = vec![1_000_000, 10_000_000, 100_000_000];
            }
            out.sort_unstable();
            out.dedup();
            if out.len() > 10 {
                out.truncate(10);
            }
            out
        };

        std::thread::spawn(move || {
            let monitor_handle = crate::worker_jobs::start_resource_monitor(sender.clone());

            macro_rules! run_analysis {
                ($label:expr, $call:expr, $variant:ident) => {{
                    sender
                        .send(WorkerMessage::Log(format!(
                            "Analyzing {} from: {}",
                            $label,
                            path.display()
                        )))
                        .ok();

                    match $call {
                        Ok((result_value, total_considered, processed_records)) => {
                            sender
                                .send(WorkerMessage::$variant {
                                    result: result_value,
                                    total: total_considered,
                                })
                                .ok();
                            sender
                                .send(WorkerMessage::Log(format!(
                                    "Analyze complete: processed {processed_records} record(s)."
                                )))
                                .ok();
                        }
                        Err(e) => {
                            sender
                                .send(WorkerMessage::Log(format!("Analyze error: {e}")))
                                .ok();
                        }
                    }
                }};
            }

            match tab {
                AnalyzeTab::LastDigit => run_analysis!(
                    "last-digit distribution",
                    analyze_last_digits_binary_file(
                        &path,
                        stop_flag.as_ref(),
                        &sender,
                        &shared_last_digit,
                        &shared_processed,
                    ),
                    AnalyzeLastDigitResult
                ),
                AnalyzeTab::Mod30 => run_analysis!(
                    "mod30 distribution",
                    analyze_mod30_binary_file(
                        &path,
                        stop_flag.as_ref(),
                        &sender,
                        &shared_mod30,
                        &shared_processed,
                    ),
                    AnalyzeMod30Result
                ),
                AnalyzeTab::Mod30PRHS => run_analysis!(
                    "mod30 PRHS (order-1 vs order-2 Markov)",
                    analyze_prhs_binary_file(
                        &path,
                        stop_flag.as_ref(),
                        &sender,
                        &shared_prhs,
                        &shared_processed,
                        prhs_log10_bin_width,
                        prhs_equal_bin_primes,
                        prhs_train_ratio,
                    ),
                    AnalyzePRHSResult
                ),
                AnalyzeTab::Mod210PRHS => run_analysis!(
                    "mod210 PRHS (order-1 vs order-2 Markov)",
                    analyze_prhs210_binary_file(
                        &path,
                        stop_flag.as_ref(),
                        &sender,
                        &shared_prhs210,
                        &shared_processed,
                        prhs_log10_bin_width,
                        prhs_equal_bin_primes,
                        prhs_train_ratio,
                        prhs_exclude_diagonal,
                        prhs_min_nij,
                    ),
                    AnalyzePRHS210Result
                ),
                AnalyzeTab::Validation => run_analysis!(
                    "validation (integrity/theory/wheel/range-dependency)",
                    analyze_validation_binary_file(
                        &path,
                        stop_flag.as_ref(),
                        &sender,
                        &shared_validation,
                        &shared_processed,
                        validation_wheel_samples,
                        validation_seed,
                        validation_ranges,
                    ),
                    AnalyzeValidationResult
                ),
            }

            if stop_flag.load(Ordering::SeqCst) {
                let _ = sender.send(WorkerMessage::Stopped);
            } else {
                let _ = sender.send(WorkerMessage::Done);
            }

            drop(monitor_handle);
        });
    }

    /// Sosu-Analyze モード: 全 Analyze タブを順次実行し、各結果を自動保存する（All Run）。
    ///
    /// 実行順: LastDigit → Mod30 → Mod30PRHS → Mod210PRHS → Validation
    pub fn start_all_run(&mut self) {
        if self.is_running {
            self.log
                .push_str("Cannot start All Run while a computation is running.\n");
            return;
        }

        let path_str = self.analyze.file_path.trim();
        if path_str.is_empty() {
            self.log.push_str("Analyze file path is empty.\n");
            return;
        }

        self.analyze.all_run_mode = true;
        self.analyze.all_run_completed.clear();
        self.analyze.all_run_pending = vec![
            AnalyzeTab::LastDigit,
            AnalyzeTab::Mod30,
            AnalyzeTab::Mod30PRHS,
            AnalyzeTab::Mod210PRHS,
            AnalyzeTab::Validation,
        ];

        // 先頭タブを開始
        if !self.analyze.all_run_pending.is_empty() {
            let first = self.analyze.all_run_pending.remove(0);
            self.analyze.current_tab = first;
            self.start_analyze();
        } else {
            // 念のため（到達しない想定）
            self.analyze.all_run_mode = false;
        }
    }

    /// Explore モードのアニメーションを開始する
    pub fn start_explore(&mut self) {
        if self.is_running || self.explore_running || self.gap_running || self.density_running {
            self.log
                .push_str("Cannot start while a computation is running.\n");
            return;
        }

        // Explore 専用の Range を使用
        let mut explore_min = match self.explore_min_input.trim().parse::<u64>() {
            Ok(v) => v,
            Err(_) => {
                self.log
                    .push_str("Explore min is not a valid u64 integer.\n");
                return;
            }
        };

        let explore_max = match self.explore_max_input.trim().parse::<u64>() {
            Ok(v) => v,
            Err(_) => {
                self.log
                    .push_str("Explore max is not a valid u64 integer.\n");
                return;
            }
        };

        // 可視化の都合上、x < 2 はすべて x = 2 に丸める
        if explore_min < 2 {
            self.log
                .push_str("Explore min < 2, clamped to 2 for visualization.\n");
            explore_min = 2;
            self.explore_min_input = "2".to_string();
        }

        if explore_min >= explore_max {
            self.log.push_str("Explore min must be less than max.\n");
            return;
        }

        // 状態をリセット
        self.explore_data.clear();
        self.explore_current_x = explore_min;
        self.explore_running = true;
        self.is_running = true;
        self.progress = 0.0;
        self.explore_progress = 0.0;
        self.explore_processed = 0;
        self.explore_total = 0;
        self.stop_flag.store(false, Ordering::SeqCst);
        self.log.clear();

        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);

        let stop_flag = self.stop_flag.clone();
        let speed = self.explore_speed;

        crate::seisei::explore_engine::start_explore_animation(
            explore_min,
            explore_max,
            speed,
            stop_flag,
            sender,
        );
    }

    /// Gap モードのアニメーションを開始する
    pub fn start_gap(&mut self) {
        if self.is_running || self.explore_running || self.gap_running || self.density_running {
            self.log
                .push_str("Cannot start while a computation is running.\n");
            return;
        }

        let mut gap_min = match self.gap_min_input.trim().parse::<u64>() {
            Ok(v) => v,
            Err(_) => {
                self.log.push_str("Gap min is not a valid u64 integer.\n");
                return;
            }
        };

        let gap_max = match self.gap_max_input.trim().parse::<u64>() {
            Ok(v) => v,
            Err(_) => {
                self.log.push_str("Gap max is not a valid u64 integer.\n");
                return;
            }
        };

        // ギャップの性質上、2 未満は意味がないので 2 に丸める
        if gap_min < 2 {
            self.log
                .push_str("Gap min < 2, clamped to 2 for visualization.\n");
            gap_min = 2;
            self.gap_min_input = "2".to_string();
        }

        if gap_min >= gap_max {
            self.log.push_str("Gap min must be less than max.\n");
            return;
        }

        // 状態をリセット
        self.gap_data.clear();
        self.gap_running = true;
        self.is_running = true;
        self.progress = 0.0;
        self.gap_progress = 0.0;
        self.gap_current_x = gap_min;
        self.gap_last_prime = 0;
        self.gap_processed = 0;
        self.gap_total = 0;
        self.gap_prime_count = 0;
        self.gap_max_gap_value = 0;
        self.gap_max_gap_prev_prime = 0;
        self.gap_max_gap_prime = 0;
        self.stop_flag.store(false, Ordering::SeqCst);
        self.log.clear();

        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);

        let stop_flag = self.stop_flag.clone();
        let speed = self.gap_speed;

        crate::seisei::explore_engine::start_gap_animation(
            gap_min, gap_max, speed, stop_flag, sender,
        );
    }

    /// Density モードのアニメーションを開始する
    pub fn start_density(&mut self) {
        if self.is_running || self.explore_running || self.gap_running || self.density_running {
            self.log
                .push_str("Cannot start while a computation is running.\n");
            return;
        }

        let mut density_min = match self.density_min_input.trim().parse::<u64>() {
            Ok(v) => v,
            Err(_) => {
                self.log
                    .push_str("Density min is not a valid u64 integer.\n");
                return;
            }
        };

        let density_max = match self.density_max_input.trim().parse::<u64>() {
            Ok(v) => v,
            Err(_) => {
                self.log
                    .push_str("Density max is not a valid u64 integer.\n");
                return;
            }
        };

        let interval_size = match self.density_interval_input.trim().parse::<u64>() {
            Ok(v) if v > 0 => v,
            _ => {
                self.log
                    .push_str("Density interval is not a valid positive u64 integer.\n");
                return;
            }
        };

        if density_min < 2 {
            self.log
                .push_str("Density min < 2, clamped to 2 for visualization.\n");
            density_min = 2;
            self.density_min_input = "2".to_string();
        }

        if density_min >= density_max {
            self.log.push_str("Density min must be less than max.\n");
            return;
        }

        // 状態をリセット
        self.density_data.clear();
        self.density_running = true;
        self.is_running = true;
        self.progress = 0.0;
        self.density_progress = 0.0;
        self.density_current_interval = density_min;
        self.density_processed = 0;
        self.density_total = 0;
        self.density_total_primes = 0;
        self.stop_flag.store(false, Ordering::SeqCst);
        self.log.clear();

        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);

        let stop_flag = self.stop_flag.clone();
        let speed = self.density_speed;

        crate::seisei::explore_engine::start_density_animation(
            density_min,
            density_max,
            interval_size,
            speed,
            stop_flag,
            sender,
        );
    }

    /// Spiral モード（Ulam Spiral）のアニメーションを開始する
    pub fn start_spiral(&mut self) {
        if self.is_running
            || self.explore_running
            || self.gap_running
            || self.density_running
            || self.spiral_running
        {
            self.log
                .push_str("Cannot start while a computation is running.\n");
            return;
        }

        let mut center = match self.spiral_center_input.trim().parse::<u64>() {
            Ok(v) => v,
            Err(_) => {
                self.log
                    .push_str("Spiral center is not a valid u64 integer.\n");
                return;
            }
        };

        let mut size = match self.spiral_size_input.trim().parse::<usize>() {
            Ok(v) => v,
            Err(_) => {
                self.log
                    .push_str("Spiral size is not a valid usize integer.\n");
                return;
            }
        };

        // グリッドサイズは下限のみ設定し（>=5）、奇数に揃える（中心を明確にするため）
        if size < 5 {
            size = 5;
            self.spiral_size_input = "5".to_string();
        }
        if size % 2 == 0 {
            size += 1;
            self.spiral_size_input = size.to_string();
        }

        // 数列モードに応じて center を丸める（Candidates1379 は次の候補へ）
        if self.spiral_number_mode == crate::app_state::SpiralNumberMode::Candidates1379 {
            center = crate::app_state::next_candidate_1379(center);
            self.spiral_center_input = center.to_string();
        }

        // 状態をリセット
        self.spiral_center = center;
        self.spiral_size = size;
        self.spiral_primes = vec![false; size * size];
        self.spiral_running = true;
        self.spiral_generated = false;
        self.is_running = true;
        self.progress = 0.0;
        self.spiral_processed = 0;
        self.spiral_total = (size as u64).saturating_mul(size as u64);
        self.spiral_zoom = 1.0;
        self.spiral_pan_x = 0.0;
        self.spiral_pan_y = 0.0;
        self.stop_flag.store(false, Ordering::SeqCst);
        self.log.clear();

        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);

        let stop_flag = self.stop_flag.clone();
        let speed = self.spiral_speed;
        let mode = self.spiral_number_mode;
        let prime_mode = self.spiral_prime_mode;

        crate::seisei::explore_engine::start_spiral_generation(
            center, size, speed, mode, prime_mode, stop_flag, sender,
        );
    }

    pub fn start_prime_pi(&mut self) {
        if self.is_running {
            self.log
                .push_str("Cannot run π(x) while a computation is running.\n");
            return;
        }

        // prime_min / prime_max の現在の入力値を使用して区間 [prime_min, prime_max] の素数個数を数える。
        let prime_min = match self.prime_min_input.trim().parse::<u64>() {
            Ok(v) => v,
            Err(_) => {
                self.log.push_str("prime_min is not a valid u64 integer.\n");
                return;
            }
        };

        let prime_max = match self.prime_max_input.trim().parse::<u64>() {
            Ok(v) => v,
            Err(_) => {
                self.log.push_str("prime_max is not a valid u64 integer.\n");
                return;
            }
        };

        if prime_min >= prime_max {
            self.log
                .push_str("prime_min must be less than prime_max.\n");
            return;
        }

        self.log.clear();
        // 設定としても現在のレンジを保存しておく
        self.config.prime_min = prime_min;
        self.config.prime_max = prime_max;

        if let Err(e) = save_config(&self.config) {
            self.log
                .push_str(&format!("Failed to save settings: {e}\n"));
        }

        self.is_running = true;
        self.progress = 0.0;
        self.eta = "Calculating π(x)...".to_string();
        self.stop_flag.store(false, Ordering::SeqCst);
        self.current_processed = 0;
        self.total_range = 0;

        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);

        let stop_flag = self.stop_flag.clone();

        std::thread::spawn(move || {
            let monitor_handle = crate::worker_jobs::start_resource_monitor(sender.clone());

            sender
                .send(WorkerMessage::Log(format!(
                    "Computing number of primes in [{prime_min}, {prime_max}] using primecount (primecount_pi)..."
                )))
                .ok();

            let result: PrimeResult<(u64, u64, u64)> = (|| {
                let pi_max = compute_prime_pi(prime_max)?;
                let pi_before_min = if prime_min > 0 {
                    compute_prime_pi(prime_min - 1)?
                } else {
                    0
                };
                let count = pi_max.saturating_sub(pi_before_min);
                Ok((pi_max, pi_before_min, count))
            })();

            match result {
                Ok((pi_max, pi_before_min, count)) => {
                    sender
                        .send(WorkerMessage::Log(format!(
                            "pi({prime_max}) = {pi_max}, pi({prime_min}-1) = {pi_before_min}"
                        )))
                        .ok();
                    sender
                        .send(WorkerMessage::Log(format!(
                            "#primes in [{prime_min}, {prime_max}] = {count}"
                        )))
                        .ok();
                }
                Err(e) => {
                    sender
                        .send(WorkerMessage::Log(format!(
                            "Error while computing prime count in [{prime_min}, {prime_max}]: {e}"
                        )))
                        .ok();
                }
            }

            if stop_flag.load(Ordering::SeqCst) {
                let _ = sender.send(WorkerMessage::Stopped);
            } else {
                let _ = sender.send(WorkerMessage::Done);
            }

            drop(monitor_handle);
        });
    }

    pub fn start_worker(&mut self) {
        let mut errors = Vec::new();

        let prime_min = match self.prime_min_input.trim().parse::<u64>() {
            Ok(v) => v,
            Err(_) => {
                errors.push("prime_min is not a valid u64 integer.");
                1
            }
        };

        let prime_max = match self.prime_max_input.trim().parse::<u64>() {
            Ok(v) => v,
            Err(_) => {
                errors.push("prime_max is not a valid u64 integer.");
                10_000_000_000
            }
        };

        let split_count = match self.split_count_input.trim().parse::<u64>() {
            Ok(v) => v,
            Err(_) => {
                errors.push("split_count is not a valid u64 integer.");
                0
            }
        };

        let segment_size = match self.segment_size_input.trim().parse::<u64>() {
            Ok(v) => v,
            Err(_) => {
                errors.push("segment_size is not a valid u64 integer.");
                10_000_000
            }
        };

        let writer_buffer_size = match self.writer_buffer_size_input.trim().parse::<usize>() {
            Ok(v) => v,
            Err(_) => {
                errors.push("writer_buffer_size is not a valid usize integer.");
                8 * 1024 * 1024
            }
        };

        let memory_usage_percent = match self.memory_usage_percent_input.trim().parse::<f64>() {
            Ok(v) => {
                if !(10.0..=90.0).contains(&v) {
                    errors.push("memory_usage_percent must be between 10.0 and 90.0.");
                    50.0
                } else {
                    v
                }
            }
            Err(_) => {
                errors.push("memory_usage_percent is not a valid number.");
                50.0
            }
        };

        if prime_min >= prime_max {
            errors.push("prime_min must be less than prime_max.");
        }

        if !errors.is_empty() {
            for e in errors {
                self.log.push_str(&format!("{e}\n"));
            }
            return;
        }

        self.log.clear();
        self.config.prime_min = prime_min;
        self.config.prime_max = prime_max;
        self.config.segment_size = segment_size;
        self.config.writer_buffer_size = writer_buffer_size;
        self.config.output_format = self.selected_format;
        self.config.output_dir = self.output_dir_input.clone();
        self.config.split_count = split_count;
        self.config.wheel_type = self.selected_wheel_type;
        self.config.memory_usage_percent = memory_usage_percent;
        self.config.last_prime_only = self.last_prime_only;
        self.config.use_timestamp_prefix = self.use_timestamp_prefix;

        if let Err(e) = save_config(&self.config) {
            self.log
                .push_str(&format!("Failed to save settings: {e}\n"));
        }

        self.is_running = true;
        self.progress = 0.0;
        self.eta = "Calculating...".to_string();
        self.stop_flag.store(false, Ordering::SeqCst);
        self.current_processed = 0;
        self.total_range = 0;

        let cfg = self.config.clone();
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        let stop_flag = self.stop_flag.clone();

        std::thread::spawn(move || {
            let monitor_handle = crate::worker_jobs::start_resource_monitor(sender.clone());

            let run = || -> PrimeResult<()> {
                if cfg.last_prime_only {
                    // 最後の素数だけモード: ファイル書き出し無し（CPU 専用）
                    let mut writer = LastPrimeWriter::new();

                    let mut last_progress = 0u64;
                    let mut last_total = 0u64;
                    let mut eta_history: Vec<u64> = Vec::new();

                    let progress_cb = |p: Progress| {
                        last_progress = p.processed;
                        last_total = p.total;

                        let eta_str = if let Some(eta) = p.eta_secs {
                            // 簡易スムージング（直近5回の移動平均）
                            eta_history.push(eta);
                            if eta_history.len() > 5 {
                                eta_history.remove(0);
                            }
                            let avg_eta =
                                eta_history.iter().sum::<u64>() / eta_history.len() as u64;
                            format_eta(Some(avg_eta))
                        } else {
                            format_eta(None)
                        };

                        sender.send(WorkerMessage::Eta(eta_str)).ok();
                        sender
                            .send(WorkerMessage::Progress {
                                current: p.processed,
                                total: p.total,
                            })
                            .ok();
                    };

                    if stop_flag.load(Ordering::SeqCst) {
                        return Ok(());
                    }

                    sender
                        .send(WorkerMessage::Log(
                            "Using CPU engine (Rayon segmented sieve) - Last Prime Only Mode"
                                .to_string(),
                        ))
                        .ok();
                    generate_primes_cpu(&cfg, &stop_flag, &mut writer, progress_cb)?;

                    if last_total > 0 {
                        sender
                            .send(WorkerMessage::Progress {
                                current: last_total,
                                total: last_total,
                            })
                            .ok();
                        sender.send(WorkerMessage::Eta(format_eta(Some(0)))).ok();
                    }

                    // 最後の素数を表示
                    if let Some(last) = writer.get_last_prime() {
                        sender
                            .send(WorkerMessage::Log(format!("Last prime found: {last}")))
                            .ok();
                    } else {
                        sender
                            .send(WorkerMessage::Log("No primes found in range.".to_string()))
                            .ok();
                    }

                    // 検出した素数の総数と prime_pi によるカウントをログ出力・検証
                    let total_primes = writer.total_primes_written();
                    sender
                        .send(WorkerMessage::Log(format!(
                            "total primes found : {total_primes}"
                        )))
                        .ok();

                    // primecount (prime_pi) による区間 [prime_min, prime_max] の素数個数
                    match (|| -> PrimeResult<u64> {
                        let pi_max = compute_prime_pi(cfg.prime_max)?;
                        let pi_before_min = if cfg.prime_min > 0 {
                            compute_prime_pi(cfg.prime_min - 1)?
                        } else {
                            0
                        };
                        Ok(pi_max.saturating_sub(pi_before_min))
                    })() {
                        Ok(pi_count) => {
                            sender
                                .send(WorkerMessage::Log(format!("#primes π(x) = {pi_count}")))
                                .ok();
                            // π(x) 一致チェック
                            if total_primes == pi_count {
                                sender
                                    .send(WorkerMessage::Log(
                                        "Verification: OK - count matches π(x)".to_string(),
                                    ))
                                    .ok();
                            } else {
                                sender
                                    .send(WorkerMessage::Log(format!(
                                        "Verification: MISMATCH - sieve={total_primes}, π(x)={pi_count}"
                                    )))
                                    .ok();
                            }
                        }
                        Err(e) => {
                            sender
                                .send(WorkerMessage::Log(format!(
                                    "Error while computing π(x): {e}"
                                )))
                                .ok();
                        }
                    }
                } else {
                    // 通常モード: ファイルに書き出す（CPU 専用）
                    let sieve_start = std::time::Instant::now();

                    // ファイル名プレフィックス（オプション）
                    let timestamp_prefix = if cfg.use_timestamp_prefix {
                        Some(Local::now().format("%Y%m%d_%H%M%S_").to_string())
                    } else {
                        None
                    };

                    let mut writer = FilePrimeWriter::new(
                        &cfg.output_dir,
                        cfg.output_format,
                        cfg.split_count,
                        cfg.writer_buffer_size,
                        timestamp_prefix.clone(),
                    )?;

                    let mut last_progress = 0u64;
                    let mut last_total = 0u64;
                    let mut eta_history: Vec<u64> = Vec::new();

                    let progress_cb = |p: Progress| {
                        last_progress = p.processed;
                        last_total = p.total;

                        let eta_str = if let Some(eta) = p.eta_secs {
                            // 簡易スムージング（直近5回の移動平均）
                            eta_history.push(eta);
                            if eta_history.len() > 5 {
                                eta_history.remove(0);
                            }
                            let avg_eta =
                                eta_history.iter().sum::<u64>() / eta_history.len() as u64;
                            format_eta(Some(avg_eta))
                        } else {
                            format_eta(None)
                        };

                        sender.send(WorkerMessage::Eta(eta_str)).ok();
                        sender
                            .send(WorkerMessage::Progress {
                                current: p.processed,
                                total: p.total,
                            })
                            .ok();
                    };

                    if stop_flag.load(Ordering::SeqCst) {
                        return Ok(());
                    }

                    sender
                        .send(WorkerMessage::Log(
                            "Using CPU engine (Rayon segmented sieve)".to_string(),
                        ))
                        .ok();
                    generate_primes_cpu(&cfg, &stop_flag, &mut writer, progress_cb)?;

                    if last_total > 0 {
                        sender
                            .send(WorkerMessage::Progress {
                                current: last_total,
                                total: last_total,
                            })
                            .ok();
                        sender.send(WorkerMessage::Eta(format_eta(Some(0)))).ok();
                    }

                    // ファイルに書き出した素数の総数と prime_pi によるカウントをログ出力・検証
                    let total_primes = writer.total_primes_written();
                    sender
                        .send(WorkerMessage::Log(format!(
                            "total primes found : {total_primes}"
                        )))
                        .ok();

                    // primecount (prime_pi) による区間 [prime_min, prime_max] の素数個数
                    let mut pi_x_verified = false;
                    match (|| -> PrimeResult<u64> {
                        let pi_max = compute_prime_pi(cfg.prime_max)?;
                        let pi_before_min = if cfg.prime_min > 0 {
                            compute_prime_pi(cfg.prime_min - 1)?
                        } else {
                            0
                        };
                        Ok(pi_max.saturating_sub(pi_before_min))
                    })() {
                        Ok(pi_count) => {
                            sender
                                .send(WorkerMessage::Log(format!("#primes π(x) = {pi_count}")))
                                .ok();
                            // π(x) 一致チェック
                            if total_primes == pi_count {
                                pi_x_verified = true;
                                sender
                                    .send(WorkerMessage::Log(
                                        "Verification: OK - count matches π(x)".to_string(),
                                    ))
                                    .ok();
                            } else {
                                sender
                                    .send(WorkerMessage::Log(format!(
                                        "Verification: MISMATCH - sieve={total_primes}, π(x)={pi_count}"
                                    )))
                                    .ok();
                            }
                        }
                        Err(e) => {
                            sender
                                .send(WorkerMessage::Log(format!(
                                    "Error while computing π(x): {e}"
                                )))
                                .ok();
                        }
                    }

                    // メタデータファイルを出力
                    let elapsed_ms = sieve_start.elapsed().as_millis() as u64;

                    let output_files: Vec<String> = writer
                        .output_file_paths()
                        .iter()
                        .map(|p| p.to_string_lossy().to_string())
                        .collect();

                    let metadata = OutputMetadata::new(
                        (cfg.prime_min, cfg.prime_max),
                        total_primes,
                        pi_x_verified,
                        elapsed_ms,
                        output_files,
                        Some(PRIMECOUNT_VERSION.to_string()),
                        Some(PRIMECOUNT_MODE.to_string()),
                    );
                    match metadata.write_to_file(&cfg.output_dir, &cfg, timestamp_prefix.as_deref())
                    {
                        Ok(meta_path) => {
                            sender
                                .send(WorkerMessage::Log(format!(
                                    "Metadata written to: {}",
                                    meta_path.display()
                                )))
                                .ok();
                        }
                        Err(e) => {
                            sender
                                .send(WorkerMessage::Log(format!("Failed to write metadata: {e}")))
                                .ok();
                        }
                    }

                    // 自動ファイル検証
                    match cfg.output_format {
                        crate::config::OutputFormat::Binary => {
                            if let Some(first_path) = writer.output_file_paths().first() {
                                let path_str = first_path.to_string_lossy().to_string();

                                sender
                                    .send(WorkerMessage::Log(format!(
                                        "Auto-verifying (binary): {path_str}"
                                    )))
                                    .ok();

                                let sender_clone = sender.clone();
                                let log_cb: LogCallback = Box::new(move |msg: String| {
                                    sender_clone.send(WorkerMessage::Log(msg)).ok();
                                });

                                match verify_primes_file(&path_str, 100, Some(log_cb)) {
                                    Ok(report) => {
                                        sender
                                            .send(WorkerMessage::Log(format!(
                                                "File verification OK: lines={}, min={}, max={}, tail_checked={}",
                                                report.line_count, report.min, report.max, report.checked_tail
                                            )))
                                            .ok();
                                    }
                                    Err(e) => {
                                        sender
                                            .send(WorkerMessage::Log(format!(
                                                "File verification FAILED: {e}"
                                            )))
                                            .ok();
                                    }
                                }
                            } else {
                                sender
                                    .send(WorkerMessage::Log(
                                        "Skipping file verification: no binary output file found"
                                            .to_string(),
                                    ))
                                    .ok();
                            }
                        }
                        _ => {
                            sender
                                .send(WorkerMessage::Log(
                                    "Skipping file verification (only supported for Binary format)"
                                        .to_string(),
                                ))
                                .ok();
                        }
                    }
                }

                Ok(())
            };

            let wall_start = std::time::Instant::now();
            let result = run();
            let elapsed = wall_start.elapsed();
            let elapsed_ms = elapsed.as_secs_f64() * 1000.0;

            sender
                .send(WorkerMessage::Log(format!(
                    "Total elapsed time: {elapsed_ms:.3} ms"
                )))
                .ok();

            if let Err(e) = result {
                let _ = sender.send(WorkerMessage::Log(format!("An error occurred: {e}\n")));
            }

            if stop_flag.load(Ordering::SeqCst) {
                let _ = sender.send(WorkerMessage::Stopped);
            } else {
                let _ = sender.send(WorkerMessage::Done);
            }
            drop(monitor_handle);
        });
    }
}
