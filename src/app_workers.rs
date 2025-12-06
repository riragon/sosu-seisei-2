//! `MyApp` のワーカー起動ロジック（Run ボタンで動く処理）をまとめたモジュール。
//!
//!
//! - Generator の素数生成 (`start_worker`)
//! - 区間の素数個数を primecount で数える (`start_prime_pi`)
//! - 教育タブ用アニメーション (`start_explore`, `start_gap`, `start_density`, `start_spiral`)

use std::sync::atomic::Ordering;
use std::sync::mpsc;

use chrono::Local;
use sysinfo::System;

use crate::config::save_config;
use crate::cpu_engine::generate_primes_cpu;
use crate::engine_types::{PrimeResult, Progress};
use crate::output::{FilePrimeWriter, LastPrimeWriter, OutputMetadata};
use crate::prime_pi_engine::{
    compute_prime_count_in_range, compute_prime_pi, PRIMECOUNT_MODE, PRIMECOUNT_VERSION,
};
use crate::verify::{verify_primes_file, LogCallback};
use crate::worker_message::{format_eta, WorkerMessage};

use crate::app_state::MyApp;

impl MyApp {
    /// 共通の u64 入力パースヘルパー（失敗時はログにメッセージを追記して None を返す）
    fn parse_u64_input(&mut self, input: &str, error_msg: &str) -> Option<u64> {
        match input.trim().parse::<u64>() {
            Ok(v) => Some(v),
            Err(_) => {
                self.append_log_line(error_msg);
                None
            }
        }
    }

    /// 正の u64 入力パースヘルパー（0 や負値はエラー扱い）
    fn parse_positive_u64_input(&mut self, input: &str, error_msg: &str) -> Option<u64> {
        match input.trim().parse::<u64>() {
            Ok(v) if v > 0 => Some(v),
            _ => {
                self.append_log_line(error_msg);
                None
            }
        }
    }

    /// 共通の usize 入力パースヘルパー
    fn parse_usize_input(&mut self, input: &str, error_msg: &str) -> Option<usize> {
        match input.trim().parse::<usize>() {
            Ok(v) => Some(v),
            Err(_) => {
                self.append_log_line(error_msg);
                None
            }
        }
    }

    /// Explore モードのアニメーションを開始する
    pub fn start_explore(&mut self) {
        if self.is_running || self.explore.running || self.gap.running || self.density.running {
            self.append_log_line("Cannot start while a computation is running.");
            return;
        }

        // Explore 専用の Range を使用
        // 借用チェッカー対策: 入力値を先にクローンしてから parse
        let min_input = self.explore.min_input.clone();
        let max_input = self.explore.max_input.clone();

        let mut explore_min =
            match self.parse_u64_input(&min_input, "Explore min is not a valid u64 integer.") {
                Some(v) => v,
                None => return,
            };

        let explore_max =
            match self.parse_u64_input(&max_input, "Explore max is not a valid u64 integer.") {
                Some(v) => v,
                None => return,
            };

        // 可視化の都合上、x < 2 はすべて x = 2 に丸める
        if explore_min < 2 {
            self.append_log_line("Explore min < 2, clamped to 2 for visualization.");
            explore_min = 2;
            self.explore.min_input = "2".to_string();
        }

        if explore_min >= explore_max {
            self.append_log_line("Explore min must be less than max.");
            return;
        }

        // 状態をリセット
        self.explore.data.clear();
        self.explore.current_x = explore_min;
        self.explore.running = true;
        self.is_running = true;
        self.progress = 0.0;
        self.explore.progress = 0.0;
        self.explore.processed = 0;
        self.explore.total = 0;
        self.stop_flag.store(false, Ordering::SeqCst);
        self.clear_log();

        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);

        let stop_flag = self.stop_flag.clone();
        let speed = self.explore.speed;

        crate::explore_engine::start_explore_animation(
            explore_min,
            explore_max,
            speed,
            stop_flag,
            sender,
        );
    }

    /// Gap モードのアニメーションを開始する
    pub fn start_gap(&mut self) {
        if self.is_running || self.explore.running || self.gap.running || self.density.running {
            self.append_log_line("Cannot start while a computation is running.");
            return;
        }

        // 借用チェッカー対策: 入力値を先にクローンしてから parse
        let min_input = self.gap.min_input.clone();
        let max_input = self.gap.max_input.clone();

        let mut gap_min =
            match self.parse_u64_input(&min_input, "Gap min is not a valid u64 integer.") {
                Some(v) => v,
                None => return,
            };

        let gap_max = match self.parse_u64_input(&max_input, "Gap max is not a valid u64 integer.")
        {
            Some(v) => v,
            None => return,
        };

        // ギャップの性質上、2 未満は意味がないので 2 に丸める
        if gap_min < 2 {
            self.append_log_line("Gap min < 2, clamped to 2 for visualization.");
            gap_min = 2;
            self.gap.min_input = "2".to_string();
        }

        if gap_min >= gap_max {
            self.append_log_line("Gap min must be less than max.");
            return;
        }

        // 状態をリセット
        self.gap.data.clear();
        self.gap.history.clear();
        self.gap.running = true;
        self.is_running = true;
        self.progress = 0.0;
        self.gap.progress = 0.0;
        self.gap.current_x = gap_min;
        self.gap.last_prime = 0;
        self.gap.processed = 0;
        self.gap.total = 0;
        self.gap.prime_count = 0;
        self.gap.max_gap_value = 0;
        self.gap.max_gap_prev_prime = 0;
        self.gap.max_gap_prime = 0;
        self.stop_flag.store(false, Ordering::SeqCst);
        self.clear_log();

        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);

        let stop_flag = self.stop_flag.clone();
        let speed = self.gap.speed;

        crate::explore_engine::start_gap_animation(gap_min, gap_max, speed, stop_flag, sender);
    }

    /// Density モードのアニメーションを開始する
    pub fn start_density(&mut self) {
        if self.is_running || self.explore.running || self.gap.running || self.density.running {
            self.append_log_line("Cannot start while a computation is running.");
            return;
        }

        // 借用チェッカー対策: 入力値を先にクローンしてから parse
        let min_input = self.density.min_input.clone();
        let max_input = self.density.max_input.clone();
        let interval_input = self.density.interval_input.clone();

        let mut density_min =
            match self.parse_u64_input(&min_input, "Density min is not a valid u64 integer.") {
                Some(v) => v,
                None => return,
            };

        let density_max =
            match self.parse_u64_input(&max_input, "Density max is not a valid u64 integer.") {
                Some(v) => v,
                None => return,
            };

        let interval_size = match self.parse_positive_u64_input(
            &interval_input,
            "Density interval is not a valid positive u64 integer.",
        ) {
            Some(v) => v,
            None => return,
        };

        if density_min < 2 {
            self.append_log_line("Density min < 2, clamped to 2 for visualization.");
            density_min = 2;
            self.density.min_input = "2".to_string();
        }

        if density_min >= density_max {
            self.append_log_line("Density min must be less than max.");
            return;
        }

        // 状態をリセット
        self.density.data.clear();
        self.density.running = true;
        self.is_running = true;
        self.progress = 0.0;
        self.density.progress = 0.0;
        self.density.current_interval = density_min;
        self.density.processed = 0;
        self.density.total = 0;
        self.density.total_primes = 0;
        self.stop_flag.store(false, Ordering::SeqCst);
        self.clear_log();

        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);

        let stop_flag = self.stop_flag.clone();
        let speed = self.density.speed;

        crate::explore_engine::start_density_animation(
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
            || self.explore.running
            || self.gap.running
            || self.density.running
            || self.spiral.running
        {
            self.append_log_line("Cannot start while a computation is running.");
            return;
        }

        // 借用チェッカー対策: 入力値を先にクローンしてから parse
        let center_input = self.spiral.center_input.clone();
        let size_input = self.spiral.size_input.clone();

        let center = match self
            .parse_u64_input(&center_input, "Spiral center is not a valid u64 integer.")
        {
            Some(v) => v,
            None => return,
        };

        let mut size = match self
            .parse_usize_input(&size_input, "Spiral size is not a valid usize integer.")
        {
            Some(v) => v,
            None => return,
        };

        // グリッドサイズは下限のみ設定し（>=5）、奇数に揃える（中心を明確にするため）
        if size < 5 {
            size = 5;
            self.spiral.size_input = "5".to_string();
        }
        if size % 2 == 0 {
            size += 1;
            self.spiral.size_input = size.to_string();
        }

        // 状態をリセット
        self.spiral.center = center;
        self.spiral.size = size;
        self.spiral.primes = bitvec::bitvec![0; size * size];
        self.spiral.running = true;
        self.spiral.generated = false;
        self.is_running = true;
        self.progress = 0.0;
        self.spiral.processed = 0;
        self.spiral.total = (size as u64).saturating_mul(size as u64);
        self.spiral.zoom = 1.0;
        self.spiral.pan_x = 0.0;
        self.spiral.pan_y = 0.0;
        self.stop_flag.store(false, Ordering::SeqCst);
        self.clear_log();

        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);

        let stop_flag = self.stop_flag.clone();
        let speed = self.spiral.speed;

        crate::explore_engine::start_spiral_generation(center, size, speed, stop_flag, sender);
    }

    pub fn start_prime_pi(&mut self) {
        if self.is_running {
            self.append_log_line("Cannot run π(x) while a computation is running.");
            return;
        }

        // prime_min / prime_max の現在の入力値を使用して区間 [prime_min, prime_max] の素数個数を数える。
        // 借用チェッカー対策: 入力値を先にクローンしてから parse
        let min_input = self.prime_min_input.clone();
        let max_input = self.prime_max_input.clone();

        let prime_min =
            match self.parse_u64_input(&min_input, "prime_min is not a valid u64 integer.") {
                Some(v) => v,
                None => return,
            };

        let prime_max =
            match self.parse_u64_input(&max_input, "prime_max is not a valid u64 integer.") {
                Some(v) => v,
                None => return,
            };

        if prime_min >= prime_max {
            self.append_log_line("prime_min must be less than prime_max.");
            return;
        }

        self.clear_log();
        // 設定としても現在のレンジを保存しておく
        self.config.prime_min = prime_min;
        self.config.prime_max = prime_max;

        if let Err(e) = save_config(&self.config) {
            self.append_log_line(&format!("Failed to save settings: {e}"));
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
            let monitor_handle = start_resource_monitor(sender.clone());

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
                let count = compute_prime_count_in_range(prime_min, prime_max)?;
                Ok((pi_max, pi_before_min, count))
            })();

            match result {
                Ok((pi_max, pi_before_min, count)) => {
                    sender
                        .send(WorkerMessage::Log(format!(
                            "pi({prime_max}) = {pi_max}, pi({}-1) = {pi_before_min}",
                            prime_min
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

        if prime_min >= prime_max {
            errors.push("prime_min must be less than prime_max.");
        }

        if !errors.is_empty() {
            for e in errors {
                self.append_log_line(e);
            }
            return;
        }

        self.clear_log();
        self.config.prime_min = prime_min;
        self.config.prime_max = prime_max;
        self.config.segment_size = segment_size;
        self.config.writer_buffer_size = writer_buffer_size;
        self.config.output_format = self.selected_format;
        self.config.output_dir = self.output_dir_input.clone();
        self.config.split_count = split_count;
        self.config.wheel_type = self.selected_wheel_type;
        self.config.last_prime_only = self.last_prime_only;
        self.config.use_timestamp_prefix = self.use_timestamp_prefix;

        if let Err(e) = save_config(&self.config) {
            self.append_log_line(&format!("Failed to save settings: {e}"));
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
            let monitor_handle = start_resource_monitor(sender.clone());

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
                    match compute_prime_count_in_range(cfg.prime_min, cfg.prime_max) {
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
                                        "Verification: MISMATCH - sieve={}, π(x)={}",
                                        total_primes, pi_count
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
                    match compute_prime_count_in_range(cfg.prime_min, cfg.prime_max) {
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
                                        "Verification: MISMATCH - sieve={}, π(x)={}",
                                        total_primes, pi_count
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
                    "Total elapsed time: {:.3} ms",
                    elapsed_ms
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

/// メモリ使用量を 500ms ごとにポーリングし、`WorkerMessage::MemUsage` として送信する。
///
/// - このスレッドはメインの計算とは独立して動作し、UI の「Memory Usage」表示を更新します。
/// - sender 側がドロップされた場合（計算終了・画面クローズなど）はループを終了します。
fn start_resource_monitor(sender: mpsc::Sender<WorkerMessage>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut sys = System::new_all();
        sys.refresh_memory();

        loop {
            std::thread::sleep(std::time::Duration::from_millis(500));
            sys.refresh_memory();

            let mem_usage = sys.used_memory();

            if sender.send(WorkerMessage::MemUsage(mem_usage)).is_err() {
                break;
            }
        }
    })
}
