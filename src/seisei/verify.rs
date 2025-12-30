use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

use crate::engine_types::PrimeResult;

#[derive(Debug, Clone)]
pub struct VerifyReport {
    /// 検証した値の個数（テキスト時は行数、バイナリ時はレコード数）。
    pub line_count: u64,
    /// Miller-Rabin によって末尾から実際にチェックした件数。
    pub checked_tail: usize,
    /// 最初の値。
    pub min: u64,
    /// 最後の値。
    pub max: u64,
}

/// 検証中のログコールバック用
pub type LogCallback = Box<dyn FnMut(String) + Send>;

/// 64bit 整数に対する決定的 Miller-Rabin 素数判定。
///
/// この関数は検証処理の「末尾サンプル」の素数性チェックに使われます。
/// 長時間の IO の中で計算コストが支配的になることはほぼないため、
/// 可読性と安全性（既知の基数セットによる決定的判定）を優先しています。
///
/// 参考: https://miller-rabin.appspot.com/ （64bit 用の既知の基数セット）
pub fn is_probable_prime(n: u64) -> bool {
    // 小さいケース
    if n < 2 {
        return false;
    }
    if n == 2 || n == 3 {
        return true;
    }
    if n % 2 == 0 {
        return false;
    }

    // n-1 = d * 2^s を求める
    let mut d = n - 1;
    let mut s = 0u32;
    while d % 2 == 0 {
        d /= 2;
        s += 1;
    }

    // 64bit 決定的テスト用の基数
    const BASES: [u64; 7] = [2, 325, 9375, 28178, 450775, 9780504, 1795265022];

    for &a in &BASES {
        if a % n == 0 {
            continue;
        }
        if !miller_rabin_round(n, d, s, a) {
            return false;
        }
    }
    true
}

fn miller_rabin_round(n: u64, d: u64, s: u32, a: u64) -> bool {
    let mut x = mod_pow(a % n, d, n);
    if x == 1 || x == n - 1 {
        return true;
    }

    for _ in 1..s {
        x = mod_mul(x, x, n);
        if x == n - 1 {
            return true;
        }
    }
    false
}

fn mod_mul(a: u64, b: u64, m: u64) -> u64 {
    ((a as u128 * b as u128) % m as u128) as u64
}

fn mod_pow(mut base: u64, mut exp: u64, m: u64) -> u64 {
    let mut res = 1u64;
    base %= m;
    while exp > 0 {
        if exp & 1 == 1 {
            res = mod_mul(res, base, m);
        }
        base = mod_mul(base, base, m);
        exp >>= 1;
    }
    res
}

/// primes ファイルを検証する（テキスト or バイナリ）。
///
/// - `.txt` / 拡張子なしなど: 1行1素数のテキストとして扱う
/// - `.bin`: little-endian `u64` の連続バイナリとして扱う
///
/// 進捗・ログの契約:
/// - `log_cb` が与えられている場合、テキスト/バイナリともに「約 100万件ごと」に進捗ログを出します。
/// - 検証完了前には「末尾サンプルの Miller-Rabin チェック開始」を必ず 1 回ログします。
/// - ログの頻度を極端に下げると、大きなファイル検証時に「止まっているように見える」ため、
///   ログ間隔を変更する場合は十分に注意してください。
pub fn verify_primes_file<P: AsRef<Path>>(
    path: P,
    sample_tail: usize,
    log_cb: Option<LogCallback>,
) -> PrimeResult<VerifyReport> {
    let path_ref = path.as_ref();
    match path_ref.extension().and_then(|e| e.to_str()) {
        Some("bin") => verify_primes_binary_file(path_ref, sample_tail, log_cb),
        _ => verify_primes_text_file(path_ref, sample_tail, log_cb),
    }
}

fn verify_primes_text_file(
    path: &Path,
    sample_tail: usize,
    mut log_cb: Option<LogCallback>,
) -> PrimeResult<VerifyReport> {
    let file = File::open(path).map_err(|e| {
        // OS 固有メッセージは環境によっては文字化けすることがあるため、
        // ログとしては英語のみの簡潔なメッセージに統一する。
        if let Some(code) = e.raw_os_error() {
            format!("Failed to open primes file {path:?}: OS error code {code}")
        } else {
            format!("Failed to open primes file {path:?}: unknown I/O error")
        }
    })?;
    let reader = BufReader::with_capacity(8 * 1024 * 1024, file); // 8MB buffer

    let mut prev: Option<u64> = None;
    let mut line_no: u64 = 0;
    let mut min_val: Option<u64> = None;
    let mut max_val: Option<u64> = None;
    let mut tail: VecDeque<(u64, u64)> = VecDeque::with_capacity(sample_tail.max(1));

    const LOG_INTERVAL: u64 = 1_000_000; // 100万行ごとにログ

    for line_res in reader.lines() {
        line_no += 1;
        let line = line_res.map_err(|e| format!("I/O error at line {line_no}: {e}"))?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Err(format!("Empty line at {line_no}").into());
        }
        let n: u64 = trimmed
            .parse()
            .map_err(|e| format!("Parse error at line {line_no}: {e}"))?;

        if let Some(p) = prev {
            if n <= p {
                return Err(format!(
                    "Non-increasing sequence at line {line_no}: prev={p}, current={n}",
                )
                .into());
            }
        }

        if n != 2 && n % 2 == 0 {
            return Err(format!("Even composite candidate at line {line_no}: {n}").into());
        }

        if min_val.is_none() {
            min_val = Some(n);
        }
        max_val = Some(n);
        prev = Some(n);

        // 末尾サンプルを保持
        if sample_tail > 0 {
            if tail.len() == sample_tail {
                tail.pop_front();
            }
            tail.push_back((line_no, n));
        }

        // 進捗ログ（100万行ごと）
        if line_no % LOG_INTERVAL == 0 {
            if let Some(ref mut cb) = log_cb {
                cb(format!("Verified {line_no} lines (current value: {n})..."));
            }
        }
    }

    let line_count = line_no;
    if line_count == 0 {
        return Err("File is empty".into());
    }

    // 末尾サンプルの素数判定
    if let Some(ref mut cb) = log_cb {
        cb(format!(
            "Checking last {} values with Miller-Rabin...",
            tail.len()
        ));
    }

    for (ln, n) in tail.iter() {
        if !is_probable_prime(*n) {
            return Err(format!("Composite detected among tail sample at line {ln}: {n}",).into());
        }
    }

    Ok(VerifyReport {
        line_count,
        checked_tail: tail.len(),
        min: min_val.unwrap(),
        max: max_val.unwrap(),
    })
}

fn verify_primes_binary_file(
    path: &Path,
    sample_tail: usize,
    mut log_cb: Option<LogCallback>,
) -> PrimeResult<VerifyReport> {
    let file = File::open(path).map_err(|e| {
        if let Some(code) = e.raw_os_error() {
            format!("Failed to open primes file {path:?}: OS error code {code}")
        } else {
            format!("Failed to open primes file {path:?}: unknown I/O error")
        }
    })?;
    let metadata = file
        .metadata()
        .map_err(|e| format!("Failed to read metadata: {e}"))?;

    if metadata.len() % 8 != 0 {
        return Err(format!(
            "Binary primes file size is not a multiple of 8 bytes: {}",
            metadata.len()
        )
        .into());
    }

    let total_records = metadata.len() / 8;
    let mut reader = BufReader::with_capacity(8 * 1024 * 1024, file);

    let mut prev: Option<u64> = None;
    let mut index: u64 = 0;
    let mut min_val: Option<u64> = None;
    let mut max_val: Option<u64> = None;
    let mut tail: VecDeque<(u64, u64)> = VecDeque::with_capacity(sample_tail.max(1));

    const LOG_INTERVAL: u64 = 1_000_000; // 100万レコードごとにログ

    let mut buf = [0u8; 8];

    while index < total_records {
        reader
            .read_exact(&mut buf)
            .map_err(|e| format!("I/O error at record {}: {e}", index + 1))?;
        index += 1;

        let n = u64::from_le_bytes(buf);

        if let Some(p) = prev {
            if n <= p {
                return Err(format!(
                    "Non-increasing sequence at record {index}: prev={p}, current={n}",
                )
                .into());
            }
        }

        if n != 2 && n % 2 == 0 {
            return Err(format!("Even composite candidate at record {index}: {n}",).into());
        }

        if min_val.is_none() {
            min_val = Some(n);
        }
        max_val = Some(n);
        prev = Some(n);

        // 末尾サンプルを保持
        if sample_tail > 0 {
            if tail.len() == sample_tail {
                tail.pop_front();
            }
            tail.push_back((index, n));
        }

        // 進捗ログ（100万レコードごと）
        if index % LOG_INTERVAL == 0 {
            if let Some(ref mut cb) = log_cb {
                cb(format!("Verified {index} records (current value: {n})...",));
            }
        }
    }

    let record_count = index;
    if record_count == 0 {
        return Err("File is empty".into());
    }

    // 末尾サンプルの素数判定
    if let Some(ref mut cb) = log_cb {
        cb(format!(
            "Checking last {} values with Miller-Rabin...",
            tail.len()
        ));
    }

    for (idx, n) in tail.iter() {
        if !is_probable_prime(*n) {
            return Err(
                format!("Composite detected among tail sample at record {idx}: {n}",).into(),
            );
        }
    }

    Ok(VerifyReport {
        line_count: record_count,
        checked_tail: tail.len(),
        min: min_val.unwrap(),
        max: max_val.unwrap(),
    })
}
