use std::error::Error;

// エンジン層（CPU / GPU / 検証）で共有するエラー型と進捗情報の定義。
//
// - このモジュールの型は UI 層（`app.rs`）との「進捗・ETA 契約」の一部です。
// - 特に `Progress` のフィールド意味は UI 側の表示に直結するため、互換性を壊さないようにしてください。

/// エンジン共通の結果型。
///
/// - すべての長時間実行タスク（CPU/GPU の素数生成、検証処理など）はこの型を返します。
/// - エラーは `Send + Sync` な Box でラップされ、ワーカースレッドから安全に伝播できる想定です。
pub type PrimeResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// 素数生成処理や検証処理の進捗情報。
///
/// UI 層とは次の「契約」を満たす必要があります:
/// - `processed` と `total` は単調に増加する（逆戻りしない）こと
/// - `processed <= total` を維持すること（ETA 計算で使用）
/// - `eta_secs` は「残り時間の概算」であり、`None` の場合は「まだ計算できない」ことを意味すること
///
/// この構造体の意味を変える場合、`app.rs` のプログレスバー表示ロジックも含めた見直しが必要になります。
#[derive(Clone, Copy, Debug)]
pub struct Progress {
    /// これまでに処理した値の個数。
    pub processed: u64,
    /// 全体として処理する予定の値の個数。
    pub total: u64,
    /// 推定残り時間（秒）。まだ計算できない場合は None。
    pub eta_secs: Option<u64>,
}

/// 現在の進捗と経過時間から ETA（残り時間の秒数）を推定するユーティリティ。
///
/// - `processed` / `total` は 0 以上で、`processed <= total` を想定しています。
/// - 進捗 0% の間は `None` を返し、ある程度進んでから ETA を表示する前提です。
/// - CPU / GPU エンジン双方から呼び出され、UI に渡す `Progress::eta_secs` の元になります。
pub fn compute_eta(processed: u64, total: u64, elapsed_secs: f64) -> Option<u64> {
    if total == 0 {
        return None;
    }
    let progress = processed.min(total) as f64 / total as f64;
    if progress > 0.0 {
        let total_time = elapsed_secs / progress;
        Some(((total_time - elapsed_secs).max(0.0)).round() as u64)
    } else {
        None
    }
}


