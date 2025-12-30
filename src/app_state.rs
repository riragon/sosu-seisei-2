//! アプリケーション状態 (`MyApp`) と初期化ロジックをまとめたモジュール。
//!
//! - タブ種別（`AppTab`）やスパイラル設定などの enum 定義
//! - `MyApp` 構造体
//! - `MyApp::new` による初期化

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use eframe::CreationContext;
use sysinfo::System;

use crate::analyze::tab_last_digit::LastDigitResult;
use crate::analyze::tab_mod30::Mod30Result;
use crate::analyze::tab_prhs::{PRHS210Result, PRHSResult};
use crate::analyze::tab_validation::ValidationBundle;
use crate::analyze::{AnalyzeTab, PRHSBinMode};
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

/// アプリケーションのモード（生成アプリ / 分析アプリ）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppMode {
    /// Sosu-Seisei（素数生成）
    #[default]
    Seisei,
    /// Sosu-Analyze（素数分析）
    Analyze,
}

/// Sosu-Analyze モードの状態をまとめた構造体。
#[derive(Debug, Default, Clone)]
pub struct AnalyzeState {
    // === 共通 ===
    /// 入力ファイル（バイナリ primes）パス
    pub file_path: String,
    /// 分析が実行中かどうか
    pub running: bool,
    /// 進捗（0.0〜1.0）
    pub progress: f32,
    /// 入力ファイルが選択済みかどうか
    pub file_loaded: bool,
    /// 読み込んだ素数（レコード）総数（集計対象はタブ定義に従う）
    pub total_primes: u64,
    /// Analyze 内の現在タブ
    pub current_tab: AnalyzeTab,

    // === All Run（全タブ一括実行） ===
    /// All Run 実行中フラグ（Analyze の全タブを順次実行して自動保存）
    pub all_run_mode: bool,
    /// All Run: 未実行（キュー）
    pub all_run_pending: Vec<AnalyzeTab>,
    /// All Run: 完了済み
    pub all_run_completed: Vec<AnalyzeTab>,

    // === 各分析結果（サブ構造体） ===
    /// 末尾 1/3/7/9 の出現率 + 遷移行列
    pub last_digit: LastDigitResult,
    /// mod 30（{1,7,11,13,17,19,23,29}）の分析結果
    pub mod30: Mod30Result,
    /// mod 30 PRHS（一次 vs 二次マルコフ）分析結果
    pub prhs: PRHSResult,
    /// mod 210 PRHS（一次 vs 二次マルコフ）分析結果
    pub prhs210: PRHS210Result,
    /// Validation（整合性・理論値・wheel比較）: mod30 + mod210 を同時計算
    pub validation: ValidationBundle,

    // === PRHS 表示/設定（UI用） ===
    /// PRHS のビン表示モード（ログ / 等量）
    pub prhs_view_bin_mode: PRHSBinMode,
    /// PRHS で選択中のビン（表示用）
    pub prhs_selected_bin: usize,
    /// 対数ビン幅（log10(p) の幅）入力
    pub prhs_log10_bin_width_input: String,
    /// 等量ビン: 1ビンあたりの素数数（フィルタ後）入力
    pub prhs_equal_bin_primes_input: String,
    /// 学習比率（train_ratio）入力
    pub prhs_train_ratio_input: String,

    // === リアルタイム更新（共有メモリ） ===
    /// リアルタイム更新用の共有結果（ワーカーが更新、UI が読み取り）
    pub shared_result: Arc<Mutex<LastDigitResult>>,
    /// リアルタイム表示用の「累計（集計対象）」カウント
    pub shared_processed: Arc<Mutex<u64>>,
    /// mod 30 リアルタイム更新用の共有結果
    pub shared_mod30: Arc<Mutex<Mod30Result>>,
    /// mod 30 PRHS リアルタイム更新用の共有結果
    pub shared_prhs: Arc<Mutex<PRHSResult>>,
    /// mod 210 PRHS リアルタイム更新用の共有結果
    pub shared_prhs210: Arc<Mutex<PRHS210Result>>,
    /// Validation リアルタイム更新用の共有結果
    pub shared_validation: Arc<Mutex<ValidationBundle>>,

    // === Validation 設定（UI用） ===
    /// Wheel 比較のサンプル数（空/不正は「素数側の triplets と同数」）
    pub validation_wheel_samples_input: String,
    /// 乱数シード（空/不正はデフォルト）
    pub validation_seed_input: String,
    /// 範囲依存性：max p のリスト（例: "1e6,1e7,1e8" / "1000000,10000000"）
    pub validation_ranges_input: String,

    // === PRHS 表示オプション ===
    /// Top Contexts 表示で対角成分（i==j）を除外する
    pub prhs_exclude_diagonal: bool,
    /// Top Contexts の最小サンプル数 N_ij（空/不正は 0）
    pub prhs_min_context_nij_input: String,
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

/// Spiral ビューの「数列モード」。
///
/// - `All`: 通常のウラム螺旋（`center, center+1, ...`）
/// - `Candidates1379`: 末尾が 1/3/7/9 の数だけを連番として使う（2/4/5/6/8/0 末尾は存在しない）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpiralNumberMode {
    /// 通常の連番（`center, center+1, ...`）
    #[default]
    All,
    /// 末尾が 1/3/7/9 の候補数列（2 と 5 の倍数を除外）
    Candidates1379,
}

/// Spiral ビューの「判定モード」。
///
/// - `Prime`: 実際に素数判定して塗る
/// - `Random`: 素数定理の密度（約 `factor/ln(n)`）に従ってランダムに塗る
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpiralPrimeMode {
    /// 実際の素数判定
    #[default]
    Prime,
    /// 素数密度に従うランダム（理論分布）
    Random,
}

/// `n` が「末尾 1/3/7/9」の候補かどうか。
pub fn is_candidate_1379(n: u64) -> bool {
    matches!(n % 10, 1 | 3 | 7 | 9)
}

/// `n` 以上の最小の「末尾 1/3/7/9」候補へ丸める。
///
/// 例: 1→1, 2→3, 4→7, 8→9, 10→11
pub fn next_candidate_1379(n: u64) -> u64 {
    let q = n / 10;
    let r = n % 10;
    match r {
        0 | 1 => q.saturating_mul(10).saturating_add(1),
        2 | 3 => q.saturating_mul(10).saturating_add(3),
        4..=7 => q.saturating_mul(10).saturating_add(7),
        8 | 9 => q.saturating_mul(10).saturating_add(9),
        _ => n, // unreachable
    }
}

/// 末尾 1/3/7/9 の候補 `n` を 0-based の rank に変換する。
///
/// - `n = 10q + r (r∈{1,3,7,9})` のとき `rank = 4q + idx(r)`。
pub fn rank_of_candidate_1379(n: u64) -> u64 {
    debug_assert!(is_candidate_1379(n));
    let q = n / 10;
    let r = n % 10;
    let idx = match r {
        1 => 0,
        3 => 1,
        7 => 2,
        9 => 3,
        _ => 0, // debug_assert により到達しない想定
    };
    q.saturating_mul(4).saturating_add(idx)
}

/// 0-based の rank から候補値（末尾 1/3/7/9）へ変換する。
///
/// - `rank = 4q + i` のとき `value = 10q + residues[i]`。
pub fn candidate_1379_of_rank(rank: u64) -> u64 {
    const RESIDUES: [u64; 4] = [1, 3, 7, 9];
    let q = rank / 4;
    let i = (rank % 4) as usize;
    q.saturating_mul(10).saturating_add(RESIDUES[i])
}

/// Spiral の `step`（0-based, 連番セル）に対応する実際の値を返す。
pub fn spiral_value_at_step(mode: SpiralNumberMode, center: u64, step: u64) -> u64 {
    match mode {
        SpiralNumberMode::All => center.saturating_add(step),
        SpiralNumberMode::Candidates1379 => {
            let base = next_candidate_1379(center);
            let base_rank = rank_of_candidate_1379(base);
            candidate_1379_of_rank(base_rank.saturating_add(step))
        }
    }
}

/// 素数定理の密度（約 `factor / ln(n)`）に従ってランダムに「塗る」かどうかを返す。
///
/// - `All`: factor = 1.0（密度 ≈ 1/ln(n)）
/// - `Candidates1379`: factor = 2.5（候補集合に条件付けた密度 ≈ 2.5/ln(n)）
///
/// 注意:
/// - 小さい n では 1/ln(n) が 1 を超えるため、確率は [0,1] にクランプします。
/// - n < 2 は ln が定義できないので常に false にします。
pub fn random_by_prime_density(n: u64, number_mode: SpiralNumberMode) -> bool {
    if n < 2 {
        return false;
    }
    let nf = n as f64;
    let ln = nf.ln();
    if ln <= 0.0 {
        return false;
    }
    let factor = match number_mode {
        SpiralNumberMode::All => 1.0,
        SpiralNumberMode::Candidates1379 => 10.0 / 4.0, // = 2.5
    };
    let p = (factor / ln).clamp(0.0, 1.0);
    fastrand::f64() < p
}

pub struct MyApp {
    pub config: Config,
    pub is_running: bool,
    pub log: String,
    pub receiver: Option<std::sync::mpsc::Receiver<crate::worker_message::WorkerMessage>>,

    /// アプリ全体のモード（Sosu-Seisei / Sosu-Analyze）
    pub analyze_mode: AppMode,
    /// Sosu-Analyze の状態
    pub analyze: AnalyzeState,

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
    /// - インデックス `k` は「スパイラル上の連番セル（step）」に対応する。
    /// - 実際の整数値 `n` は `spiral_number_mode`（All / Candidates）に応じて決まり、
    ///   `spiral_prime_mode`（Prime / Random）に応じて「素数」または「ランダムでマーク」
    ///   された場合に `spiral_primes[k] == true` になる。
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
    /// スパイラルの数列モード（全マス or 末尾1379候補のみ）
    pub spiral_number_mode: SpiralNumberMode,
    /// スパイラルの判定モード（素数判定 or 理論密度ランダム）
    pub spiral_prime_mode: SpiralPrimeMode,
    /// 螺旋パス（セル中心を結ぶ線）を表示するかどうか
    pub spiral_show_path: bool,
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
        let memory_usage_percent_input = config.memory_usage_percent.to_string();
        let use_timestamp_prefix = config.use_timestamp_prefix;

        // Apple 風のミニマルなダークモード UI
        setup_style(&cc.egui_ctx);

        MyApp {
            analyze_mode: AppMode::Seisei,
            analyze: AnalyzeState::default(),

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
            spiral_number_mode: SpiralNumberMode::default(),
            spiral_prime_mode: SpiralPrimeMode::default(),
            // 初期状態ではパス線を非表示（ユーザーが明示的に有効化できるようにする）
            spiral_show_path: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_sequence_starts_correctly() {
        let center = 1u64;
        let mode = SpiralNumberMode::Candidates1379;
        let got: Vec<u64> = (0u64..12)
            .map(|s| spiral_value_at_step(mode, center, s))
            .collect();
        let expected = vec![1, 3, 7, 9, 11, 13, 17, 19, 21, 23, 27, 29];
        assert_eq!(got, expected);
    }

    #[test]
    fn next_candidate_rounds_up() {
        let cases = [
            (1, 1),
            (2, 3),
            (3, 3),
            (4, 7),
            (5, 7),
            (6, 7),
            (7, 7),
            (8, 9),
            (9, 9),
            (10, 11),
            (11, 11),
            (12, 13),
            (14, 17),
            (18, 19),
            (20, 21),
        ];
        for (n, exp) in cases {
            assert_eq!(next_candidate_1379(n), exp);
        }
    }

    #[test]
    fn rank_value_roundtrip() {
        // 0..100 くらいの範囲で往復確認
        for rank in 0u64..200 {
            let v = candidate_1379_of_rank(rank);
            assert!(is_candidate_1379(v));
            assert_eq!(rank_of_candidate_1379(v), rank);
        }
    }
}
