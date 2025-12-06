//! アプリケーション状態 (`MyApp`) と初期化ロジックをまとめたモジュール。
//!
//! - タブ種別（`AppTab`）やスパイラル設定などの enum 定義
//! - `MyApp` 構造体
//! - `MyApp::new` による初期化

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

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
    pub primes: Vec<bool>,
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
            primes: Vec::new(),
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
    pub log: String,
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
            log: String::new(),
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
