use crate::engine_types::PrimeResult;

/// primecount クレートに関するメタ情報。
///
/// 現在の依存バージョン（`Cargo.toml` / `Cargo.lock` 時点）と、
/// π(x) 計算に用いているモードを人間可読な文字列として保持する。
/// ここでは primecount クレートのデフォルト（自動アルゴリズム選択）を利用している。
pub const PRIMECOUNT_VERSION: &str = "primecount crate 0.2.1 (C++ primecount auto mode)";
pub const PRIMECOUNT_MODE: &str = "pi(x) default (automatic algorithm selection)";

/// primecount クレートを用いた Prime counting function π(x) エンジン。
///
/// - 入力: `x`（x 以下の素数の個数を求める）
/// - 戻り値: `PrimeResult<u64>`（成功時は π(x)）
///
/// 現状、この関数自体は進捗情報を返さず、一発計算のみを行います。
/// 長時間計算時の UI 連携（ログやプログレスバー更新）は、呼び出し側
///（例えば `app.rs` のワーカースレッド）がこの関数をラップして行います。
pub fn compute_prime_pi(x: u64) -> PrimeResult<u64> {
    // primecount::pi は i64 を受け取って i64 を返すため、u64 からの変換を行う。
    // 非常に大きな x（i64::MAX を超える）に対しては panic させるシンプルな方針とする。
    let x_i64: i64 = x
        .try_into()
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> {
            format!("x is too large for primecount::pi (x={x}): {e}").into()
        })?;
    let pi_i64 = primecount::pi(x_i64);
    Ok(pi_i64 as u64)
}

/// 区間 [min, max] に含まれる素数の個数を primecount で計算するヘルパー。
///
/// - min > max の場合はエラーを返す。
/// - 計算自体は `compute_prime_pi` を2回呼ぶだけの薄いラッパー。
pub fn compute_prime_count_in_range(min: u64, max: u64) -> PrimeResult<u64> {
    if min > max {
        return Err("min must be <= max".into());
    }

    let pi_max = compute_prime_pi(max)?;
    let pi_before_min = if min > 0 {
        compute_prime_pi(min - 1)?
    } else {
        0
    };

    Ok(pi_max.saturating_sub(pi_before_min))
}