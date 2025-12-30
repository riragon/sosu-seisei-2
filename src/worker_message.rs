//! ワーカースレッドと UI 間でやり取りするメッセージ型と、その補助関数。
//!
//! - 元々は `app.rs` に定義されていたものを切り出し、CPU/GPU エンジンや
//!   教育モード用のワーカーからも共有しやすくしています。

use serde::{Deserialize, Serialize};

use crate::analyze::tab_last_digit::LastDigitResult;
use crate::analyze::tab_mod30::Mod30Result;
use crate::analyze::tab_prhs::{PRHS210Result, PRHSResult};
use crate::analyze::tab_validation::ValidationBundle;

/// ワーカースレッド（CPU/GPU エンジンや検証処理）から UI へ送られるメッセージ。
///
/// この列挙型は「進捗・ログの契約」の中核です。バリアントの意味を変えると
/// ログパネル／プログレスバー／ETA／メモリ表示に直接影響するため、
/// 変更する場合は UI 表示との整合性を必ず確認してください。
///
/// - `Log`      : 任意のテキストログ。下部ログパネルに *新しいものが上* になるよう表示されます。
/// - `Progress` : 全体に対する処理済み件数と総件数。プログレスバーと「Processed:」表示に使用されます。
/// - `Eta`      : 人間に読みやすい ETA 文字列（例: `"12 min 3 sec"`）。`format_eta` で生成されます。
/// - `MemUsage` : 現在のメモリ使用量（KB）。500ms ごとに `start_resource_monitor` から送信されます。
/// - `Done`     : 正常完了を表し、UI 側で `is_running` を false にし、receiver を破棄します。
/// - `Stopped`  : ユーザー操作による停止を表し、「Process stopped by user。」ログを残して終了します。
#[allow(clippy::large_enum_variant)]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum WorkerMessage {
    Log(String),
    Progress {
        current: u64,
        total: u64,
    },
    Eta(String),
    MemUsage(u64),
    Done,
    Stopped,
    /// Explore モード用: (x, π(x)) のデータポイント
    ExploreData {
        x: u64,
        pi_x: u64,
    },
    /// Gap モード用: 新しい素数とその直前の素数との差（ギャップ）
    GapData {
        prime: u64,
        prev_prime: u64,
        gap: u64,
    },
    /// Density モード用: 区間の開始位置と素数個数
    DensityData {
        interval_start: u64,
        count: u64,
    },
    /// Spiral モード用: 素数フラグ配列（ステップ順一次元列）
    ///
    /// - `primes.len()` は通常 `size * size` 以上（生成時に上限サイズで確保）。
    /// - インデックス `k` は「スパイラル上の連番セル（step）」に対応します。
    ///   実際に判定した整数値 `n` は UI の数列モード（All / Candidates）に応じて決まり、
    ///   UI の判定モード（Prime / Random）に応じて「素数」または「ランダムでマーク」
    ///   された場合に `primes[k] == true` になります。
    /// - グリッド上のどのセルに配置するかは UI 側（スクエア / ハニカム等）が
    ///   この一次元列をそれぞれの座標系にマッピングして決めます。
    SpiralData {
        primes: Vec<bool>,
        size: usize,
    },

    /// Sosu-Analyze モード用: LastDigit 分析結果（高次分析含む）と総数
    AnalyzeLastDigitResult {
        result: LastDigitResult,
        total: u64,
    },

    /// Sosu-Analyze モード用: mod 30（8種類）分析結果と総数
    AnalyzeMod30Result {
        result: Mod30Result,
        total: u64,
    },

    /// Sosu-Analyze モード用: mod 30 PRHS 分析結果と総数（トリプレット数）
    AnalyzePRHSResult {
        result: PRHSResult,
        total: u64,
    },

    /// Sosu-Analyze モード用: mod 210 PRHS 分析結果と総数（トリプレット数）
    AnalyzePRHS210Result {
        result: PRHS210Result,
        total: u64,
    },

    /// Sosu-Analyze モード用: Validation（検証）結果と総数（トリプレット数）
    AnalyzeValidationResult {
        result: ValidationBundle,
        total: u64,
    },

    /// Sosu-Analyze モード用: バイナリ読み込み進捗（処理済みレコード数 / 総レコード数）
    AnalyzeProgress {
        current: u64,
        total: u64,
    },
}

/// ETA（残り時間の秒数）を人間が読みやすい文字列にフォーマットするヘルパー。
///
/// - `None` の場合は「Calculating...」として表示されます（まだ統計が安定していない状態）。
/// - 数十秒〜数分〜数時間といったオーダーに応じて単位を切り替えます。
///
/// この関数の戻り値は `WorkerMessage::Eta` 経由で UI に渡されるため、
/// ユーザーが進捗を直感的に把握できる形を維持するようにしてください。
/// 例:
/// - `None`  → `"Calculating..."`
/// - `Some(45)` → `"45 sec"`
/// - `Some(125)` → `"2 min 5 sec"`
/// - `Some(3670)` → `"1 h 1 min"`
pub fn format_eta(eta_secs: Option<u64>) -> String {
    match eta_secs {
        None => "Calculating...".to_string(),
        Some(secs) => {
            if secs < 60 {
                format!("{secs} sec")
            } else if secs < 3600 {
                let minutes = secs / 60;
                let seconds = secs % 60;
                if seconds == 0 {
                    format!("{minutes} min")
                } else {
                    format!("{minutes} min {seconds} sec")
                }
            } else {
                let hours = secs / 3600;
                let minutes = (secs % 3600) / 60;
                if minutes == 0 {
                    format!("{hours} h")
                } else {
                    format!("{hours} h {minutes} min")
                }
            }
        }
    }
}
