//! アプリケーション状態 (`MyApp`) と初期化ロジックをまとめたモジュール。
//!
//! - タブ種別（`AppTab`）やスパイラル設定などの enum 定義
//! - `MyApp` 構造体
//! - `MyApp::new` による初期化

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use bitvec::vec::BitVec;
use eframe::CreationContext;
use sysinfo::System;

use crate::app_style::setup_style;
use crate::config::{load_or_create_config, Config, OutputFormat, WheelType};
use crate::ui_components::ZoomPanState;

/// アプリケーションのタブ（Generator / Explore / Gap / Density / Spiral）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppTab {
    #[default]
    Generator,
    Explore,
    Gap,
    Density,
    Spiral,
}

/// Spiral ビューのグリッド形状（通常のウラム螺旋 or 六角形ハニカム螺旋）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpiralGridShape {
    /// 通常の正方グリッド上の Ulam spiral
    #[default]
    Square,
    /// 六角形セルによるハニカム螺旋
    Hex,
}

/// Explore タブ用の状態
#[derive(Debug)]
pub struct ExploreState {
    pub running: bool,
    pub data: Vec<(f64, f64, f64)>, // (x, pi_x, x_log_x)
    pub speed: f32,
    pub current_x: u64,
    pub min_input: String,
    pub max_input: String,
    pub processed: u64,
    pub total: u64,
    pub graph_mode: ExploreGraphMode,
    pub follow_mode: bool,
    pub window_size: usize,
    pub view: ZoomPanState,
    /// Explore タブ専用の進捗（0.0〜1.0）
    pub progress: f32,
}

impl Default for ExploreState {
    fn default() -> Self {
        Self {
            running: false,
            data: Vec::new(),
            // speed は 0.0, 1.0, 2.0 の 3段階インデックス（1x / 3x / MAX）として扱う
            speed: 0.0,
            current_x: 0,
            min_input: "2".to_string(),
            max_input: "1000000".to_string(),
            processed: 0,
            total: 0,
            graph_mode: ExploreGraphMode::default(),
            follow_mode: true,
            window_size: 50,
            view: ZoomPanState::default(),
            progress: 0.0,
        }
    }
}

/// Gap タブ用の状態
#[derive(Debug)]
pub struct GapState {
    pub running: bool,
    pub data: HashMap<u64, u64>, // gap_size -> count
    pub history: VecDeque<(u64, u64, u64)>,
    pub min_input: String,
    pub max_input: String,
    pub speed: f32,
    pub current_x: u64,
    pub last_prime: u64,
    pub processed: u64,
    pub total: u64,
    pub prime_count: u64,
    pub max_gap_value: u64,
    pub max_gap_prev_prime: u64,
    pub max_gap_prime: u64,
    pub view: ZoomPanState,
    /// Gap ヒストグラムで対数スケールを使用するか
    pub log_scale: bool,
    /// Gap タブ専用の進捗（0.0〜1.0）
    pub progress: f32,
}

impl Default for GapState {
    fn default() -> Self {
        Self {
            running: false,
            data: HashMap::new(),
            history: VecDeque::new(),
            min_input: "2".to_string(),
            max_input: "1000000".to_string(),
            speed: 0.0,
            current_x: 0,
            last_prime: 0,
            processed: 0,
            total: 0,
            prime_count: 0,
            max_gap_value: 0,
            max_gap_prev_prime: 0,
            max_gap_prime: 0,
            view: ZoomPanState::default(),
            log_scale: false,
            progress: 0.0,
        }
    }
}

/// Density タブ用の状態
#[derive(Debug)]
pub struct DensityState {
    pub running: bool,
    pub data: Vec<(u64, u64)>, // (interval_start, prime_count)
    pub min_input: String,
    pub max_input: String,
    pub interval_input: String,
    pub speed: f32,
    pub current_interval: u64,
    pub processed: u64,
    pub total: u64,
    pub total_primes: u64,
    /// Density グラフの横方向バー幅スケール（1.0 が標準）
    pub bar_width_scale: f32,
    /// Density グラフ用のズーム・パン状態
    pub view: ZoomPanState,
    /// Density タブ専用の進捗（0.0〜1.0）
    pub progress: f32,
}

impl Default for DensityState {
    fn default() -> Self {
        Self {
            running: false,
            data: Vec::new(),
            min_input: "2".to_string(),
            max_input: "1000000".to_string(),
            interval_input: "1000".to_string(),
            speed: 0.0,
            current_interval: 0,
            processed: 0,
            total: 0,
            total_primes: 0,
            bar_width_scale: 1.0,
            view: ZoomPanState::default(),
            progress: 0.0,
        }
    }
}

/// Spiral タブ用の状態
#[derive(Debug)]
pub struct SpiralState {
    pub running: bool,
    pub center: u64,
    pub size: usize,
    pub center_input: String,
    pub size_input: String,
    /// Spiral モード用素数フラグ
    ///
    /// - `primes.len()` はおおむね `size * size`。
    /// - インデックス `k` は整数値 `n = center + k` に対応し、
    ///   その値が素数なら `primes[k] == true` になる。
    pub primes: BitVec,
    pub generated: bool,
    pub speed: f32,
    pub processed: u64,
    pub total: u64,
    // ズーム・パン用
    pub zoom: f32,  // 1.0 = 100%, 2.0 = 200% など
    pub pan_x: f32, // パン（移動）のオフセット X
    pub pan_y: f32, // パン（移動）のオフセット Y
    /// スパイラルの描画形状（正方 or ハニカム）
    pub grid_shape: SpiralGridShape,
    /// 螺旋パス（セル中心を結ぶ線）を表示するかどうか
    pub show_path: bool,
}

impl Default for SpiralState {
    fn default() -> Self {
        Self {
            running: false,
            center: 1,
            size: 201,
            center_input: "1".to_string(),
            size_input: "201".to_string(),
            primes: BitVec::new(),
            generated: false,
            speed: 0.0,
            processed: 0,
            total: 0,
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
            grid_shape: SpiralGridShape::default(),
            // 初期状態ではパス線を非表示（ユーザーが明示的に有効化できるようにする）
            show_path: false,
        }
    }
}

pub struct MyApp {
    pub config: Config,
    pub is_running: bool,
    pub log: VecDeque<String>,
    pub receiver: Option<std::sync::mpsc::Receiver<crate::worker_message::WorkerMessage>>,

    pub prime_min_input: String,
    pub prime_max_input: String,
    pub split_count_input: String,
    pub segment_size_input: String,
    pub writer_buffer_size_input: String,

    /// Generator / π(x) 用の進捗（0.0〜1.0）
    pub progress: f32,

    pub eta: String,
    pub mem_usage: u64,
    pub stop_flag: Arc<AtomicBool>,

    pub total_mem: u64,
    pub current_processed: u64,
    pub total_range: u64,

    pub selected_format: OutputFormat,
    pub output_dir_input: String,
    pub last_prime_only: bool,

    pub selected_wheel_type: WheelType,
    pub use_timestamp_prefix: bool,

    pub show_advanced_options: bool,

    // 教育モード（Explore / Gap）用
    pub current_tab: AppTab,
    pub explore: ExploreState,
    pub gap: GapState,
    pub density: DensityState,
    pub spiral: SpiralState,
}

/// Explore グラフの表示モード
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExploreGraphMode {
    #[default]
    PiVsXLogX, // π(x) vs x/log x
    Ratio, // π(x) / (x/log x)
}

impl MyApp {
    /// いずれかのタブまたはメインが実行中かどうか
    pub fn is_any_running(&self) -> bool {
        self.is_running
            || self.explore.running
            || self.gap.running
            || self.density.running
            || self.spiral.running
    }

    /// すべての running フラグを停止状態にリセット
    pub fn reset_all_running(&mut self) {
        self.is_running = false;
        self.explore.running = false;
        self.gap.running = false;
        self.density.running = false;
        self.spiral.running = false;
    }

    pub fn new(cc: &CreationContext<'_>) -> Self {
        let config = load_or_create_config().unwrap_or_default();

        let mut sys = System::new_all();
        sys.refresh_all();
        let total_mem = sys.total_memory(); // KB

        let selected_format = config.output_format;
        let output_dir_input = config.output_dir.clone();
        let last_prime_only = config.last_prime_only;
        let selected_wheel_type = config.wheel_type;
        let use_timestamp_prefix = config.use_timestamp_prefix;

        // Apple 風のミニマルなダークモード UI
        setup_style(&cc.egui_ctx);

        MyApp {
            prime_min_input: config.prime_min.to_string(),
            prime_max_input: config.prime_max.to_string(),
            split_count_input: config.split_count.to_string(),
            segment_size_input: config.segment_size.to_string(),
            writer_buffer_size_input: config.writer_buffer_size.to_string(),

            config,
            is_running: false,
            log: VecDeque::new(),
            receiver: None,

            progress: 0.0,
            eta: "N/A".to_string(),
            mem_usage: 0,
            stop_flag: Arc::new(AtomicBool::new(false)),

            total_mem,
            current_processed: 0,
            total_range: 0,

            selected_format,
            output_dir_input,
            last_prime_only,

            selected_wheel_type,
            use_timestamp_prefix,

            show_advanced_options: false,

            // 教育モード（Explore / Gap）用
            current_tab: AppTab::default(),
            explore: ExploreState::default(),
            gap: GapState::default(),
            density: DensityState::default(),
            spiral: SpiralState::default(),
        }
    }

impl MyApp {
    pub fn append_log_line(&mut self, msg: &str) {
        if msg.is_empty() {
            return;
        }

        for line in msg.split('\n') {
            if !line.is_empty() {
                self.log.push_back(line.to_string());
            }
        }

        self.trim_logs();
    }

    pub fn clear_log(&mut self) {
        self.log.clear();
    }

    pub fn trim_logs(&mut self) {
        while self.log.len() > self.config.max_log_lines {
            self.log.pop_front();
        }
    }

    pub fn push_explore_point(&mut self, point: (f64, f64, f64)) {
        self.explore.data.push(point);

        self.enforce_explore_limit();
    }

    pub fn enforce_explore_limit(&mut self) {
        let max_points = self.config.max_explore_points;
        if self.explore.data.len() > max_points {
            let overflow = self.explore.data.len() - max_points;
            self.explore.data.drain(0..overflow);
        }
    }

    pub fn push_density_point(&mut self, interval_start: u64, count: u64) {
        self.density
            .data
            .push((interval_start, count.min(u64::MAX)));
        self.density.total_primes = self.density.total_primes.saturating_add(count);

        self.enforce_density_limit();
    }

    pub fn enforce_density_limit(&mut self) {
        let max_points = self.config.max_density_points;
        if self.density.data.len() > max_points {
            let overflow = self.density.data.len() - max_points;
            let mut removed_total = 0u64;
            for (_, removed_count) in self.density.data.drain(0..overflow) {
                removed_total = removed_total.saturating_add(removed_count);
            }
            self.density.total_primes = self.density.total_primes.saturating_sub(removed_total);
        }
    }

    pub fn enforce_gap_limit(&mut self) {
        self.trim_gap_history();
    }

    pub fn push_gap_entry(&mut self, prime: u64, prev_prime: u64, gap: u64) {
        *self.gap.data.entry(gap).or_insert(0) += 1;
        self.gap.history.push_back((gap, prev_prime, prime));
        self.gap.prime_count = self.gap.history.len() as u64;

        if gap > self.gap.max_gap_value {
            self.gap.max_gap_value = gap;
            self.gap.max_gap_prev_prime = prev_prime;
            self.gap.max_gap_prime = prime;
        }

        self.trim_gap_history();
    }

    fn trim_gap_history(&mut self) {
        let max_events = self.config.max_gap_events;
        while self.gap.history.len() > max_events {
            if let Some((old_gap, _, _)) = self.gap.history.pop_front() {
                if let Some(count) = self.gap.data.get_mut(&old_gap) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        self.gap.data.remove(&old_gap);
                    }
                }

                self.gap.prime_count = self.gap.prime_count.saturating_sub(1);

                if old_gap == self.gap.max_gap_value && !self.gap.data.contains_key(&old_gap) {
                    self.recompute_gap_max();
                }
            }
        }

        self.gap.prime_count = self.gap.history.len() as u64;
    }

    fn recompute_gap_max(&mut self) {
        if let Some((gap, prev, prime)) = self.gap.history.iter().max_by_key(|entry| entry.0) {
            self.gap.max_gap_value = *gap;
            self.gap.max_gap_prev_prime = *prev;
            self.gap.max_gap_prime = *prime;
        } else {
            self.gap.max_gap_value = 0;
            self.gap.max_gap_prev_prime = 0;
            self.gap.max_gap_prime = 0;
        }
    }

    pub fn apply_spiral_data(&mut self, primes: Vec<bool>, size: usize) {
        let mut compact: BitVec = primes.into_iter().collect();

        if compact.len() > self.config.max_spiral_cells {
            let overflow = compact.len() - self.config.max_spiral_cells;
            compact.truncate(self.config.max_spiral_cells);
            self.append_log_line(&format!(
                "Spiral data truncated to {} cells ({} were dropped to respect the limit).",
                self.config.max_spiral_cells, overflow
            ));
        }

        self.spiral.primes = compact;
        self.spiral.size = size;
        self.spiral.generated = true;
    }

    pub fn enforce_spiral_limit(&mut self) {
        if self.spiral.primes.len() > self.config.max_spiral_cells {
            let overflow = self.spiral.primes.len() - self.config.max_spiral_cells;
            self.spiral.primes.truncate(self.config.max_spiral_cells);
            self.append_log_line(&format!(
                "Existing spiral data trimmed by {} cells to satisfy the current limit.",
                overflow
            ));
        }
    }
    /// 全タブ共通で使用する `running` / `progress` をまとめてリセットする。
    ///
    /// 新しいタブを追加した際も、このメソッド内のリストを更新するだけで
    /// 停止時のリセット漏れを防げるようにしている。
    pub fn reset_running_and_progress(&mut self) {
        self.is_running = false;
        self.progress = 0.0;
        self.current_processed = 0;
        self.total_range = 0;
        self.eta = "N/A".to_string();

        for target in self.tab_reset_targets() {
            *target.running = false;
            if let Some(progress) = target.progress {
                *progress = 0.0;
            }
        }

        // Spiral は進捗率を processed / total から算出するため、ここで 0 に戻す。
        self.spiral.processed = 0;
        self.spiral.total = 0;
    }

    /// 停止時にリセットしたいタブの `running` / `progress` フィールドへの参照をまとめる。
    fn tab_reset_targets(&mut self) -> [TabResetTarget<'_>; 4] {
        [
            TabResetTarget {
                running: &mut self.explore.running,
                progress: Some(&mut self.explore.progress),
            },
            TabResetTarget {
                running: &mut self.gap.running,
                progress: Some(&mut self.gap.progress),
            },
            TabResetTarget {
                running: &mut self.density.running,
                progress: Some(&mut self.density.progress),
            },
            TabResetTarget {
                running: &mut self.spiral.running,
                progress: None,
            },
        ]
    }
}

struct TabResetTarget<'a> {
    running: &'a mut bool,
    progress: Option<&'a mut f32>,
}
