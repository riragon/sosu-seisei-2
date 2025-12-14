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
    /// Explore タブ専用の進捗（0.0〜1.0）
    pub explore_progress: f32,
    /// Gap タブ専用の進捗（0.0〜1.0）
    pub gap_progress: f32,
    /// Density タブ専用の進捗（0.0〜1.0）
    pub density_progress: f32,

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
    pub memory_usage_percent_input: String,
    pub use_timestamp_prefix: bool,

    pub show_advanced_options: bool,

    // 教育モード（Explore / Gap）用
    pub current_tab: AppTab,
    pub explore_running: bool,
    pub explore_data: Vec<(f64, f64, f64)>, // (x, pi_x, x_log_x)
    pub explore_speed: f32,
    pub explore_current_x: u64,
    pub explore_min_input: String,
    pub explore_max_input: String,
    pub explore_processed: u64,
    pub explore_total: u64,
    pub explore_graph_mode: ExploreGraphMode,
    pub explore_follow_mode: bool,
    pub explore_window_size: usize, // 追跡モードで表示するデータポイント数
    /// Explore グラフ用のズーム・パン状態
    pub explore_view: ZoomPanState,

    // ギャップモード（Gap）用
    pub gap_running: bool,
    pub gap_data: HashMap<u64, u64>, // gap_size -> count
    pub gap_min_input: String,
    pub gap_max_input: String,
    pub gap_speed: f32,
    pub gap_current_x: u64,
    pub gap_last_prime: u64,
    pub gap_processed: u64,
    pub gap_total: u64,
    pub gap_prime_count: u64,
    pub gap_max_gap_value: u64,
    pub gap_max_gap_prev_prime: u64,
    pub gap_max_gap_prime: u64,
    /// Gap ヒストグラム用のズーム・パン状態
    pub gap_view: ZoomPanState,
    /// Gap ヒストグラムで対数スケールを使用するか
    pub gap_log_scale: bool,

    // 密度モード（Density）用
    pub density_running: bool,
    pub density_data: Vec<(u64, u64)>, // (interval_start, prime_count)
    pub density_min_input: String,
    pub density_max_input: String,
    pub density_interval_input: String,
    pub density_speed: f32,
    pub density_current_interval: u64,
    pub density_processed: u64,
    pub density_total: u64,
    pub density_total_primes: u64,
    /// Density グラフの横方向バー幅スケール（1.0 が標準）
    pub density_bar_width_scale: f32,
    /// Density グラフ用のズーム・パン状態
    pub density_view: ZoomPanState,

    // スパイラルモード（Spiral）用
    pub spiral_running: bool,
    pub spiral_center: u64,
    pub spiral_size: usize,
    pub spiral_center_input: String,
    pub spiral_size_input: String,
    /// Spiral モード用素数フラグ
    ///
    /// - `spiral_primes.len()` はおおむね `spiral_size * spiral_size`。
    /// - インデックス `k` は整数値 `n = spiral_center + k` に対応し、
    ///   その値が素数なら `spiral_primes[k] == true` になる。
    pub spiral_primes: Vec<bool>,
    pub spiral_generated: bool,
    pub spiral_speed: f32,
    pub spiral_processed: u64,
    pub spiral_total: u64,
    // ズーム・パン用
    pub spiral_zoom: f32,  // 1.0 = 100%, 2.0 = 200% など
    pub spiral_pan_x: f32, // パン（移動）のオフセット X
    pub spiral_pan_y: f32, // パン（移動）のオフセット Y
    /// スパイラルの描画形状（正方 or ハニカム）
    pub spiral_grid_shape: SpiralGridShape,
    /// 螺旋パス（セル中心を結ぶ線）を表示するかどうか
    pub spiral_show_path: bool,
}

/// Explore グラフの表示モード
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExploreGraphMode {
    #[default]
    PiVsXLogX, // π(x) vs x/log x
    Ratio,     // π(x) / (x/log x)
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
        let memory_usage_percent_input = config.memory_usage_percent.to_string();
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
            explore_progress: 0.0,
            gap_progress: 0.0,
            density_progress: 0.0,
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
            memory_usage_percent_input,
            use_timestamp_prefix,

            show_advanced_options: false,

            // 教育モード（Explore / Gap）用
            current_tab: AppTab::default(),
            explore_running: false,
            explore_data: Vec::new(),
            // speed は 0.0, 1.0, 2.0 の 3段階インデックス（1x / 3x / MAX）として扱う
            explore_speed: 0.0,
            explore_current_x: 0,
            explore_min_input: "2".to_string(),
            explore_max_input: "1000000".to_string(),
            explore_processed: 0,
            explore_total: 0,
            explore_graph_mode: ExploreGraphMode::default(),
            explore_follow_mode: true,
            explore_window_size: 50,
            explore_view: ZoomPanState::default(),

            gap_running: false,
            gap_data: HashMap::new(),
            gap_min_input: "2".to_string(),
            gap_max_input: "1000000".to_string(),
            gap_speed: 0.0,
            gap_current_x: 0,
            gap_last_prime: 0,
            gap_processed: 0,
            gap_total: 0,
            gap_prime_count: 0,
            gap_max_gap_value: 0,
            gap_max_gap_prev_prime: 0,
            gap_max_gap_prime: 0,
            gap_view: ZoomPanState::default(),
            gap_log_scale: false,

            density_running: false,
            density_data: Vec::new(),
            density_min_input: "2".to_string(),
            density_max_input: "1000000".to_string(),
            density_interval_input: "1000".to_string(),
            density_speed: 0.0,
            density_current_interval: 0,
            density_processed: 0,
            density_total: 0,
            density_total_primes: 0,
            density_bar_width_scale: 1.0,
            density_view: ZoomPanState::default(),

            spiral_running: false,
            spiral_center: 1,
            spiral_size: 201,
            spiral_center_input: "1".to_string(),
            spiral_size_input: "201".to_string(),
            spiral_primes: Vec::new(),
            spiral_generated: false,
            spiral_speed: 0.0,
            spiral_processed: 0,
            spiral_total: 0,
            spiral_zoom: 1.0,
            spiral_pan_x: 0.0,
            spiral_pan_y: 0.0,
            spiral_grid_shape: SpiralGridShape::default(),
            // 初期状態ではパス線を非表示（ユーザーが明示的に有効化できるようにする）
            spiral_show_path: false,
        }
    }
}


