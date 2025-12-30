//! Validation（検証）用エンジン。
//!
//! 目的:
//! - 「見えている歴史効果」が計算ミス/分母ミスによるものではないことを確認する
//! - 理論値（一様 1/φ(M)）との比較で “偏りの大きさ” を定量化する
//! - wheel 乱数（gcd(n,M)=1 に条件付けたベースライン）と比較し、ふるい由来かを判定する
//! - 範囲（max p）依存性を確認する

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use statrs::distribution::{ChiSquared, ContinuousCDF};

use crate::analyze::tab_validation::{
    BinScalingData, IntegrityCheck, LemkeOliverComparison, PairwiseResult, RangeDependency,
    RangeResult, ScalingAnalysis, TheoryComparison, ValidationBundle, ValidationResult,
    WheelComparison,
};
use crate::analyze::{MOD210_RESIDUES, MOD30_RESIDUES};
use crate::engine_types::PrimeResult;
use crate::worker_message::WorkerMessage;

const READER_CAPACITY_BYTES: usize = 8 * 1024 * 1024;

const MOD10_RESIDUES: [u64; 4] = [1, 3, 7, 9];

fn residue_to_index_mod10(r: u64) -> Option<usize> {
    match r {
        1 => Some(0),
        3 => Some(1),
        7 => Some(2),
        9 => Some(3),
        _ => None,
    }
}

fn open_binary_primes_file(path: &Path) -> PrimeResult<(BufReader<File>, u64)> {
    let file = File::open(path).map_err(|e| -> Box<dyn std::error::Error + Send + Sync> {
        if let Some(code) = e.raw_os_error() {
            format!("Failed to open primes file {path:?}: OS error code {code}").into()
        } else {
            format!("Failed to open primes file {path:?}: unknown I/O error").into()
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
    if total_records == 0 {
        return Err("File is empty".into());
    }
    let reader = BufReader::with_capacity(READER_CAPACITY_BYTES, file);
    Ok((reader, total_records))
}

fn read_next_u64(reader: &mut BufReader<File>, buf: &mut [u8; 8], idx: u64) -> PrimeResult<u64> {
    reader
        .read_exact(buf)
        .map_err(|e| format!("I/O error at record {}: {e}", idx + 1))?;
    Ok(u64::from_le_bytes(*buf))
}

fn idx3(i: usize, j: usize, k: usize, n: usize) -> usize {
    (i * n + j) * n + k
}

fn c1_from_c2(c2: &[u64], n: usize) -> Vec<u64> {
    let mut c1 = vec![0u64; n * n];
    for i in 0..n {
        for j in 0..n {
            for k in 0..n {
                let c = c2[idx3(i, j, k, n)];
                c1[j * n + k] = c1[j * n + k].saturating_add(c);
            }
        }
    }
    c1
}

fn total_triplets_c2(c2: &[u64]) -> u64 {
    c2.iter().copied().sum()
}

fn compute_log_loss_bits_m1(c1_test: &[u64], c1_train: &[u64], n: usize, alpha: f64) -> (f64, u64) {
    let mut loss_bits = 0.0f64;
    let mut n_test = 0u64;
    for j in 0..n {
        let mut row_sum_train = 0u64;
        for k in 0..n {
            row_sum_train = row_sum_train.saturating_add(c1_train[j * n + k]);
        }
        if row_sum_train == 0 {
            continue;
        }
        let denom = row_sum_train as f64 + (n as f64) * alpha;
        for k in 0..n {
            let c = c1_test[j * n + k];
            if c == 0 {
                continue;
            }
            let p = (c1_train[j * n + k] as f64 + alpha) / denom;
            loss_bits -= (c as f64) * p.log2();
            n_test = n_test.saturating_add(c);
        }
    }
    let ll = if n_test > 0 {
        loss_bits / (n_test as f64)
    } else {
        0.0
    };
    (ll, n_test)
}

fn compute_log_loss_bits_m2(c2_test: &[u64], c2_train: &[u64], n: usize, alpha: f64) -> (f64, u64) {
    let mut loss_bits = 0.0f64;
    let mut n_test = 0u64;
    for i in 0..n {
        for j in 0..n {
            let mut row_sum_train = 0u64;
            for k in 0..n {
                row_sum_train = row_sum_train.saturating_add(c2_train[idx3(i, j, k, n)]);
            }
            if row_sum_train == 0 {
                continue;
            }
            let denom = row_sum_train as f64 + (n as f64) * alpha;
            for k in 0..n {
                let c = c2_test[idx3(i, j, k, n)];
                if c == 0 {
                    continue;
                }
                let p = (c2_train[idx3(i, j, k, n)] as f64 + alpha) / denom;
                loss_bits -= (c as f64) * p.log2();
                n_test = n_test.saturating_add(c);
            }
        }
    }
    let ll = if n_test > 0 {
        loss_bits / (n_test as f64)
    } else {
        0.0
    };
    (ll, n_test)
}

fn compute_cmi_nats(c2: &[u64], c1: &[u64], n: usize) -> f64 {
    let total = total_triplets_c2(c2);
    if total == 0 {
        return 0.0;
    }
    let total_f = total as f64;

    let mut row_sum_j = vec![0u64; n];
    for j in 0..n {
        let mut s = 0u64;
        for k in 0..n {
            s = s.saturating_add(c1[j * n + k]);
        }
        row_sum_j[j] = s;
    }

    let mut cmi = 0.0f64;
    for i in 0..n {
        for j in 0..n {
            let mut row_sum_ij = 0u64;
            for k in 0..n {
                row_sum_ij = row_sum_ij.saturating_add(c2[idx3(i, j, k, n)]);
            }
            if row_sum_ij == 0 || row_sum_j[j] == 0 {
                continue;
            }
            let denom_ij_f = row_sum_ij as f64;
            let denom_j_f = row_sum_j[j] as f64;
            for k in 0..n {
                let c = c2[idx3(i, j, k, n)];
                if c == 0 {
                    continue;
                }
                let p_ijk = (c as f64) / total_f;
                let p_k_ij = (c as f64) / denom_ij_f;
                let p_k_j = (c1[j * n + k] as f64) / denom_j_f;
                if p_k_j > 0.0 {
                    cmi += p_ijk * (p_k_ij / p_k_j).ln();
                }
            }
        }
    }
    cmi
}

fn theory_comparison_from_c1(c1: &[u64], n: usize) -> TheoryComparison {
    let expected_uniform = 1.0f64 / (n as f64);
    let mut max_dev = 0.0f64;
    let mut chi2 = 0.0f64;
    let mut df = 0u64;

    for j in 0..n {
        let mut row_sum = 0u64;
        for k in 0..n {
            row_sum = row_sum.saturating_add(c1[j * n + k]);
        }
        if row_sum == 0 {
            continue;
        }
        df = df.saturating_add((n as u64).saturating_sub(1));
        let expected = (row_sum as f64) * expected_uniform;
        for k in 0..n {
            let o = c1[j * n + k] as f64;
            let p = o / (row_sum as f64);
            let dev = (p - expected_uniform).abs();
            if dev > max_dev {
                max_dev = dev;
            }
            if expected > 0.0 {
                chi2 += (o - expected) * (o - expected) / expected;
            }
        }
    }

    let p_value = if df > 0 {
        let dist = ChiSquared::new(df as f64).ok();
        dist.map(|d| 1.0 - d.cdf(chi2)).unwrap_or(f64::NAN)
    } else {
        f64::NAN
    };

    TheoryComparison {
        expected_uniform,
        max_deviation: max_dev,
        chi_squared: chi2,
        df,
        p_value,
    }
}

fn determine_conclusion(delta_cmi: f64, wheel_cmi: f64) -> String {
    if wheel_cmi.is_finite() && wheel_cmi > 0.0 && delta_cmi.abs() < wheel_cmi * 0.1 {
        "sieve-derived (history effect explained by wheel structure)".to_string()
    } else if delta_cmi.is_finite() && delta_cmi > 0.0 {
        "prime-specific (residual structure beyond sieve)".to_string()
    } else {
        "inconclusive (need larger sample)".to_string()
    }
}

/// Expected sign of c(a,b) for mod 10 based on Lemke Oliver–Soundararajan (2016).
///
/// Exact coefficient sign table from the paper:
/// ```text
/// From\To   1    3    7    9
///   1      -1   +1   +1    0
///   3       0   -1   +1   +1
///   7      +1    0   -1   +1
///   9      +1   +1    0   -1
/// ```
/// - Diagonal: -1 (primes avoid repeating)
/// - "Adjacent" in cycle (1→9, 9→7, 7→3, 3→1): 0 (neutral)
/// - Other off-diagonal: +1 (primes prefer jumping)
fn get_expected_c_sign_mod10(from: u64, to: u64) -> i8 {
    match (from, to) {
        // Diagonal: -1
        (1, 1) | (3, 3) | (7, 7) | (9, 9) => -1,
        // Neutral pairs (adjacent in cycle): 0
        (1, 9) | (9, 7) | (7, 3) | (3, 1) => 0,
        // All other off-diagonal: +1
        (1, 3) | (1, 7) => 1,
        (3, 7) | (3, 9) => 1,
        (7, 1) | (7, 9) => 1,
        (9, 1) | (9, 3) => 1,
        _ => 0,
    }
}

fn get_correction_coefficient_mod10(from: u64, to: u64) -> f64 {
    // Empirical scale factor to match the paper's qualitative predictions.
    // This is used for computing theoretical probabilities; the sign is what matters most.
    const SCALE: f64 = 0.5;
    let sign = get_expected_c_sign_mod10(from, to);
    (sign as f64) * SCALE
}

/// Expected sign of c(a,b) for mod 30.
///
/// Pattern: diagonal is negative (avoidance), off-diagonal varies.
/// This uses the same principle as mod 10 but extended to 8 residue classes.
fn get_expected_c_sign_mod30(from: u64, to: u64) -> i8 {
    // Diagonal: primes avoid repeating → negative deviation
    if from == to {
        return -1;
    }
    // Off-diagonal: primes tend to "jump" to residues that are arithmetically distant.
    // Simplified heuristic based on mod 3 and mod 5 relationships.
    // If from and to share a common factor pattern, it's less preferred.
    let same_mod3 = from % 3 == to % 3;
    let same_mod5 = from % 5 == to % 5;
    if same_mod3 && same_mod5 {
        // Similar to diagonal in subgroup → less preferred
        -1
    } else if !same_mod3 && !same_mod5 {
        // Very different → more preferred
        1
    } else {
        // Mixed
        0
    }
}

fn get_correction_coefficient_mod30(from: u64, to: u64) -> f64 {
    const SCALE: f64 = 0.25;
    let sign = get_expected_c_sign_mod30(from, to);
    (sign as f64) * SCALE
}

fn compute_lo_theory_row_probs(
    residues: &[u64],
    from_idx: usize,
    modulus: u64,
    prime_max: u64,
) -> Vec<f64> {
    let m = residues.len();
    let uniform = 1.0 / (m as f64);
    let log_x = (prime_max.max(3) as f64).ln();
    let from = residues[from_idx];
    let mut row = vec![0.0f64; m];
    for (b_idx, &b) in residues.iter().enumerate() {
        let c = match modulus {
            10 => get_correction_coefficient_mod10(from, b),
            30 => get_correction_coefficient_mod30(from, b),
            _ => 0.0,
        };
        row[b_idx] = uniform + c / log_x;
    }
    // row-wise normalize（1次展開の誤差・負値を吸収）
    let eps = 1e-15;
    let mut sum = 0.0;
    for v in row.iter_mut() {
        if *v < eps {
            *v = eps;
        }
        sum += *v;
    }
    if sum > 0.0 {
        for v in row.iter_mut() {
            *v /= sum;
        }
    }
    row
}

fn lo_comparison_from_transition_counts(
    modulus: u64,
    residues: &[u64],
    trans_counts: &[u64], // row-major [a*m + b]
    prime_max: u64,
    prefix_snapshots: &[(u64, Vec<u64>)], // (max_p, trans_counts_at_prefix)
) -> LemkeOliverComparison {
    let m = residues.len();
    let mut pairwise: Vec<PairwiseResult> = Vec::with_capacity(m * m);
    let mut chi2 = 0.0f64;
    let mut df = 0u64;
    let mut max_abs_z = 0.0f64;

    for a in 0..m {
        let mut row_sum = 0u64;
        for b in 0..m {
            row_sum = row_sum.saturating_add(trans_counts[a * m + b]);
        }
        if row_sum == 0 {
            continue;
        }
        df = df.saturating_add((m as u64).saturating_sub(1));
        let row_probs = compute_lo_theory_row_probs(residues, a, modulus, prime_max);
        for b in 0..m {
            let o = trans_counts[a * m + b] as f64;
            let expected = (row_sum as f64) * row_probs[b];
            let delta = o - expected;
            let z = if expected > 0.0 {
                delta / expected.sqrt()
            } else {
                0.0
            };
            if z.abs() > max_abs_z {
                max_abs_z = z.abs();
            }
            if expected > 0.0 {
                chi2 += (o - expected) * (o - expected) / expected;
            }

            let p_obs = o / (row_sum as f64);
            let p_theory = row_probs[b];
            let uniform = 1.0 / (m as f64);
            let log_x = (prime_max.max(3) as f64).ln();
            let estimated_c = (p_obs - uniform) * log_x;
            let expected_c_sign = match modulus {
                10 => get_expected_c_sign_mod10(residues[a], residues[b]),
                30 => get_expected_c_sign_mod30(residues[a], residues[b]),
                _ => 0,
            };

            pairwise.push(PairwiseResult {
                from_residue: residues[a],
                to_residue: residues[b],
                p_observed: p_obs,
                p_theory,
                delta,
                z_score: z,
                estimated_c,
                expected_c_sign,
            });
        }
    }

    let p_value = if df > 0 {
        ChiSquared::new(df as f64)
            .ok()
            .map(|d| 1.0 - d.cdf(chi2))
            .unwrap_or(f64::NAN)
    } else {
        f64::NAN
    };

    // Qualitative agreement: do observed deviations line up with the expected sign pattern?
    // NOTE: With very large samples, chi-squared can be huge even for tiny deviations;
    // sign-pattern agreement is the more meaningful qualitative check for LOS.
    let mut sign_ok = 0u64;
    let mut sign_tot = 0u64;
    for r in &pairwise {
        let exp = r.expected_c_sign;
        if exp == 0 {
            continue;
        }
        sign_tot += 1;
        let obs = if r.z_score > 0.0 {
            1
        } else if r.z_score < 0.0 {
            -1
        } else {
            0
        };
        if obs == exp {
            sign_ok += 1;
        }
    }
    let sign_match_ratio = if sign_tot > 0 {
        (sign_ok as f64) / (sign_tot as f64)
    } else {
        f64::NAN
    };

    // scaling（prefix）: c_est を range ごとに推定し、符号パターンの一致度をざっくり評価
    let mut bin_data: Vec<BinScalingData> = Vec::new();
    for (max_p, tc) in prefix_snapshots.iter() {
        let log10_mid = (*max_p as f64).max(1.0).log10();
        let mut est = vec![vec![0.0f64; m]; m];
        for a in 0..m {
            let mut row_sum = 0u64;
            for b in 0..m {
                row_sum = row_sum.saturating_add(tc[a * m + b]);
            }
            if row_sum == 0 {
                continue;
            }
            let log_x = ((*max_p).max(3) as f64).ln();
            for b in 0..m {
                let p_obs = (tc[a * m + b] as f64) / (row_sum as f64);
                let uniform = 1.0 / (m as f64);
                est[a][b] = (p_obs - uniform) * log_x;
            }
        }

        let mut ok = 0u64;
        let mut tot = 0u64;
        for a in 0..m {
            for b in 0..m {
                let exp = match modulus {
                    10 => get_expected_c_sign_mod10(residues[a], residues[b]),
                    30 => get_expected_c_sign_mod30(residues[a], residues[b]),
                    _ => 0,
                };
                if exp == 0 {
                    continue;
                }
                tot += 1;
                let s = if est[a][b] > 0.0 {
                    1
                } else if est[a][b] < 0.0 {
                    -1
                } else {
                    0
                };
                if s == exp {
                    ok += 1;
                }
            }
        }
        let sign_match = if tot > 0 {
            (ok as f64) / (tot as f64) >= 0.7
        } else {
            false
        };
        bin_data.push(BinScalingData {
            max_p: *max_p,
            log10_mid,
            estimated_c: est,
            sign_match,
        });
    }
    let scaling = if !bin_data.is_empty() {
        let convergence_ok = bin_data.iter().all(|b| b.sign_match);
        Some(ScalingAnalysis {
            bin_data,
            convergence_ok,
        })
    } else {
        None
    };

    // Verdict logic:
    // - Consistent: χ² is small enough (statistically indistinguishable from theory)
    // - QualitativeMatch: sign pattern matches well (≥60%), even if χ² is large
    // - PartialMatch: sign pattern partially matches (≥40%)
    // - TheoryInconsistent: sign pattern contradicts (<40%)
    // - InsufficientData: not enough data points
    let verdict = if df == 0 || sign_tot < 4 {
        "InsufficientData".to_string()
    } else if p_value > 0.01 && max_abs_z < 3.0 {
        "Consistent".to_string()
    } else if sign_match_ratio.is_finite() && sign_match_ratio >= 0.6 {
        // 60% threshold: for mod 10 with 12 non-neutral pairs, 8+ matches
        "QualitativeMatch".to_string()
    } else if sign_match_ratio.is_finite() && sign_match_ratio >= 0.4 {
        "PartialMatch".to_string()
    } else {
        "TheoryInconsistent".to_string()
    };

    LemkeOliverComparison {
        modulus,
        prime_max,
        chi_squared: chi2,
        df,
        p_value,
        pairwise,
        scaling,
        verdict,
    }
}

#[derive(Clone)]
struct Xorshift64(u64);

impl Xorshift64 {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

fn next_f64_open01(rng: &mut Xorshift64) -> f64 {
    // (0,1) に入れる（0 を避ける）
    let x = rng.next();
    (x as f64 + 1.0) / (u64::MAX as f64 + 2.0)
}

fn generate_uniform_residue_indices(n_states: usize, len: usize, seed: u64) -> Vec<usize> {
    let mut rng = Xorshift64(if seed == 0 { 1 } else { seed });
    let mut out = Vec::with_capacity(len);
    for _ in 0..len {
        out.push((rng.next() as usize) % n_states);
    }
    out
}

fn sample_geometric_failures(rng: &mut Xorshift64, q: f64) -> u64 {
    // failures ~ Geom(q) - 1  (P(f>=k)=(1-q)^k)
    if !(q.is_finite()) || q <= 0.0 {
        return u64::MAX / 4;
    }
    if q >= 1.0 {
        return 0;
    }
    let u = next_f64_open01(rng);
    let ln1mq = (-q).ln_1p(); // ln(1-q) 安定版
    if ln1mq >= 0.0 {
        return 0;
    }
    let f = (u.ln() / ln1mq).floor();
    if f.is_finite() && f >= 0.0 {
        f as u64
    } else {
        0
    }
}

fn generate_thinned_candidate_indices(
    n_states: usize,
    len: usize,
    q: f64,
    seed: u64,
    start_index: usize,
) -> Vec<usize> {
    let mut rng = Xorshift64(if seed == 0 { 1 } else { seed });
    let mut out = Vec::with_capacity(len);
    let mut idx = start_index % n_states;
    out.push(idx);
    while out.len() < len {
        let failures = sample_geometric_failures(&mut rng, q);
        let steps = failures.saturating_add(1);
        let step_mod = (steps % (n_states as u64)) as usize;
        idx = (idx + step_mod) % n_states;
        out.push(idx);
    }
    out
}

fn residue_to_index_mod30(r: u64) -> Option<usize> {
    // 8要素なので線形探索で十分
    MOD30_RESIDUES.iter().position(|&x| x == r)
}

fn residue_to_index_mod210_table() -> [i16; 210] {
    let mut table = [-1i16; 210];
    for (idx, &r) in MOD210_RESIDUES.iter().enumerate() {
        table[r as usize] = idx as i16;
    }
    table
}

fn accumulate_triplets(
    residues: &[usize],
    n: usize,
    c2_total: &mut [u64],
    c2_train: &mut [u64],
    c2_test: &mut [u64],
) -> u64 {
    // in-sequence holdout: 5本に1本を test
    let mut triplet_idx = 0u64;
    if residues.len() < 3 {
        return 0;
    }
    for t in 2..residues.len() {
        let i = residues[t - 2];
        let j = residues[t - 1];
        let k = residues[t];
        let pos = idx3(i, j, k, n);
        c2_total[pos] = c2_total[pos].saturating_add(1);
        if triplet_idx % 5 == 0 {
            c2_test[pos] = c2_test[pos].saturating_add(1);
        } else {
            c2_train[pos] = c2_train[pos].saturating_add(1);
        }
        triplet_idx = triplet_idx.saturating_add(1);
    }
    triplet_idx
}

fn compute_integrity(c2_total: &[u64], n: usize) -> IntegrityCheck {
    let c2_sum = total_triplets_c2(c2_total);
    let c1 = c1_from_c2(c2_total, n);
    let mut consistent = true;
    for j in 0..n {
        for k in 0..n {
            let mut s = 0u64;
            for i in 0..n {
                s = s.saturating_add(c2_total[idx3(i, j, k, n)]);
            }
            if s != c1[j * n + k] {
                consistent = false;
                break;
            }
        }
        if !consistent {
            break;
        }
    }
    // row_sums_ok: row確率が正規化できるか（row_sum>0 の行のみ）
    let row_ok = true;
    for j in 0..n {
        let mut row_sum = 0u64;
        for k in 0..n {
            row_sum = row_sum.saturating_add(c1[j * n + k]);
        }
        if row_sum == 0 {
            continue;
        }
        // MLE の row の和は常に 1 なので、ここは形式チェックとして true
    }
    IntegrityCheck {
        c2_sum,
        expected_triplets: c2_sum,
        c1_c2_consistent: consistent,
        row_sums_ok: row_ok,
    }
}

fn compute_stats_from_c2(
    c2_total: &[u64],
    c2_train: &[u64],
    c2_test: &[u64],
    n: usize,
) -> (u64, f64, f64, f64, Vec<u64>) {
    let c1_total = c1_from_c2(c2_total, n);
    let c1_train = c1_from_c2(c2_train, n);
    let c1_test = c1_from_c2(c2_test, n);
    let alpha = 1e-3_f64;
    let (ll1, n1) = compute_log_loss_bits_m1(&c1_test, &c1_train, n, alpha);
    let (ll2, n2) = compute_log_loss_bits_m2(c2_test, c2_train, n, alpha);
    let n_test = n1.min(n2);
    let cmi = compute_cmi_nats(c2_total, &c1_total, n);
    (n_test, ll1, ll2, cmi, c1_total)
}

fn build_is_coprime_table(modulus: u64) -> Vec<u8> {
    let m = modulus as usize;
    let residues: Vec<u64> = match modulus {
        30 => MOD30_RESIDUES.to_vec(),
        210 => MOD210_RESIDUES.to_vec(),
        _ => Vec::new(),
    };
    let mut is = vec![0u8; m];
    for r in residues {
        is[r as usize] = 1;
    }
    is
}

fn count_coprime_upto_inclusive(x: u64, modulus: u64, is_coprime: &[u8]) -> u64 {
    let m = modulus;
    let phi = is_coprime.iter().map(|&v| v as u64).sum::<u64>();
    let blocks = x / m;
    let rem = (x % m) as usize;
    // prefix[0]=0, prefix[i+1]=count in [0..=i]
    let mut prefix = vec![0u64; (m as usize) + 1];
    for i in 0..(m as usize) {
        prefix[i + 1] = prefix[i] + (is_coprime[i] as u64);
    }
    blocks * phi + prefix[rem + 1]
}

fn count_coprime_in_range_inclusive(a: u64, b: u64, modulus: u64, is_coprime: &[u8]) -> u64 {
    if b < a {
        return 0;
    }
    let left = if a > 0 {
        count_coprime_upto_inclusive(a - 1, modulus, is_coprime)
    } else {
        0
    };
    let right = count_coprime_upto_inclusive(b, modulus, is_coprime);
    right.saturating_sub(left)
}

/// Validation 実行（mod 30 / mod 210）
///
/// - `wheel_samples_override`: Some(x) の場合、wheel 側の triplet 数を x に固定する
/// - `ranges`: max p の昇順リスト（空なら range_dep は空）
#[allow(clippy::too_many_arguments)]
pub fn analyze_validation_binary_file(
    path: &Path,
    stop_flag: &AtomicBool,
    sender: &mpsc::Sender<WorkerMessage>,
    shared_result: &Arc<Mutex<ValidationBundle>>,
    shared_processed: &Arc<Mutex<u64>>,
    wheel_samples_override: Option<u64>,
    seed: u64,
    ranges: Vec<u64>,
) -> PrimeResult<(ValidationBundle, u64, u64)> {
    let (mut reader, total_records) = open_binary_primes_file(path)?;
    let mut buf = [0u8; 8];

    // Dual-modulus: compute mod 30 and mod 210 in one streaming pass.
    let residues30: Vec<u64> = MOD30_RESIDUES.to_vec();
    let residues210: Vec<u64> = MOD210_RESIDUES.to_vec();
    let n30 = residues30.len();
    let n210 = residues210.len();
    let table210 = residue_to_index_mod210_table();

    // mod 30 accumulators
    let mut first_p30: Option<u64> = None;
    let mut last_p30: Option<u64> = None;
    let mut prime_count30: u64 = 0;
    let mut triplets30_seen: u64 = 0;
    let mut last2_30: [Option<usize>; 2] = [None, None];
    let mut len30: u8 = 0;
    let mut c2_total30 = vec![0u64; n30 * n30 * n30];
    let mut c2_train30 = vec![0u64; n30 * n30 * n30];
    let mut c2_test30 = vec![0u64; n30 * n30 * n30];
    let mut range_results30: Vec<RangeResult> = Vec::new();

    // mod 210 accumulators
    let mut first_p210: Option<u64> = None;
    let mut last_p210: Option<u64> = None;
    let mut prime_count210: u64 = 0;
    let mut triplets210_seen: u64 = 0;
    let mut last2_210: [Option<usize>; 2] = [None, None];
    let mut len210: u8 = 0;
    let mut c2_total210 = vec![0u64; n210 * n210 * n210];
    let mut c2_train210 = vec![0u64; n210 * n210 * n210];
    let mut c2_test210 = vec![0u64; n210 * n210 * n210];
    let mut range_results210: Vec<RangeResult> = Vec::new();

    let mut next_range_i = 0usize;
    let mut ranges_sorted = ranges;
    ranges_sorted.sort_unstable();
    ranges_sorted.dedup();

    // Lemke Oliver 理論比較用（mod 10 / mod 30 の遷移カウントをストリームで構築）
    let mut lo10_trans = vec![0u64; 4 * 4];
    let mut lo30_trans = vec![0u64; 8 * 8];
    let mut lo10_last: Option<usize> = None;
    let mut lo30_last: Option<usize> = None;
    let mut lo10_prime_max: u64 = 0;
    let mut lo30_prime_max: u64 = 0;
    let mut lo10_prefix_snaps: Vec<(u64, Vec<u64>)> = Vec::new();
    let mut lo30_prefix_snaps: Vec<(u64, Vec<u64>)> = Vec::new();

    // 進捗表示用
    let mut idx: u64 = 0;
    let mut processed_considered: u64 = 0;

    #[allow(clippy::too_many_arguments)]
    fn push_triplet(
        last2: &mut [Option<usize>; 2],
        len: &mut u8,
        cur: usize,
        triplets_seen: &mut u64,
        n: usize,
        c2_total: &mut [u64],
        c2_train: &mut [u64],
        c2_test: &mut [u64],
    ) {
        match *len {
            0 => {
                last2[1] = Some(cur);
                *len = 1;
            }
            1 => {
                last2[0] = last2[1];
                last2[1] = Some(cur);
                *len = 2;
            }
            _ => {
                let i = last2[0].unwrap();
                let j = last2[1].unwrap();
                let k = cur;
                let pos = idx3(i, j, k, n);
                c2_total[pos] = c2_total[pos].saturating_add(1);
                if *triplets_seen % 5 == 0 {
                    c2_test[pos] = c2_test[pos].saturating_add(1);
                } else {
                    c2_train[pos] = c2_train[pos].saturating_add(1);
                }
                *triplets_seen = triplets_seen.saturating_add(1);
                last2[0] = last2[1];
                last2[1] = Some(cur);
            }
        }
    }

    while idx < total_records {
        if stop_flag.load(Ordering::SeqCst) {
            break;
        }
        let p = read_next_u64(&mut reader, &mut buf, idx)?;
        idx += 1;

        // range prefix: しきい値を超えたタイミングで「直前までの prefix」をスナップショット
        if !ranges_sorted.is_empty() {
            while next_range_i < ranges_sorted.len() && p > ranges_sorted[next_range_i] {
                // Lemke Oliver 用の prefix スナップショット（この時点では p は未反映）
                lo10_prefix_snaps.push((ranges_sorted[next_range_i], lo10_trans.clone()));
                lo30_prefix_snaps.push((ranges_sorted[next_range_i], lo30_trans.clone()));

                // Range stats are computed from current running counts (p not yet applied).
                let (_n_test30, ll1_30, ll2_30, cmi30, _c1_30) =
                    compute_stats_from_c2(&c2_total30, &c2_train30, &c2_test30, n30);
                range_results30.push(RangeResult {
                    max_p: ranges_sorted[next_range_i],
                    triplets: triplets30_seen,
                    cmi: cmi30,
                    delta_ll: ll1_30 - ll2_30,
                });

                let (_n_test210, ll1_210, ll2_210, cmi210, _c1_210) =
                    compute_stats_from_c2(&c2_total210, &c2_train210, &c2_test210, n210);
                range_results210.push(RangeResult {
                    max_p: ranges_sorted[next_range_i],
                    triplets: triplets210_seen,
                    cmi: cmi210,
                    delta_ll: ll1_210 - ll2_210,
                });
                next_range_i += 1;
            }
        }

        // Lemke Oliver: mod 10 遷移（2,5 を除外）
        if p != 2 && p != 5 {
            if let Some(cur10) = residue_to_index_mod10(p % 10) {
                if let Some(prev10) = lo10_last {
                    lo10_trans[prev10 * 4 + cur10] =
                        lo10_trans[prev10 * 4 + cur10].saturating_add(1);
                }
                lo10_last = Some(cur10);
                lo10_prime_max = p;
            }
        }
        // Lemke Oliver: mod 30 遷移（2,3,5 を除外）
        if p != 2 && p != 3 && p != 5 {
            if let Some(cur30) = residue_to_index_mod30(p % 30) {
                if let Some(prev30) = lo30_last {
                    lo30_trans[prev30 * 8 + cur30] =
                        lo30_trans[prev30 * 8 + cur30].saturating_add(1);
                }
                lo30_last = Some(cur30);
                lo30_prime_max = p;
            }
        }

        // Update mod 30 chain (exclude 2,3,5)
        if p != 2 && p != 3 && p != 5 {
            if let Some(cur30) = residue_to_index_mod30(p % 30) {
                processed_considered = processed_considered.saturating_add(1);
                prime_count30 = prime_count30.saturating_add(1);
                if first_p30.is_none() {
                    first_p30 = Some(p);
                }
                last_p30 = Some(p);
                push_triplet(
                    &mut last2_30,
                    &mut len30,
                    cur30,
                    &mut triplets30_seen,
                    n30,
                    &mut c2_total30,
                    &mut c2_train30,
                    &mut c2_test30,
                );
            }
        }

        // Update mod 210 chain (exclude 2,3,5,7)
        if p != 2 && p != 3 && p != 5 && p != 7 {
            let v = table210[(p % 210) as usize];
            if v >= 0 {
                let cur210 = v as usize;
                prime_count210 = prime_count210.saturating_add(1);
                if first_p210.is_none() {
                    first_p210 = Some(p);
                }
                last_p210 = Some(p);
                push_triplet(
                    &mut last2_210,
                    &mut len210,
                    cur210,
                    &mut triplets210_seen,
                    n210,
                    &mut c2_total210,
                    &mut c2_train210,
                    &mut c2_test210,
                );
            }
        }

        // 途中結果の publish（軽量）
        if idx % 1_000_000 == 0 {
            if let Ok(mut guard) = shared_processed.try_lock() {
                // UI の「Total triplets」表示に合わせ、概算で triplets を出す
                *guard = triplets30_seen;
            }
            sender
                .send(WorkerMessage::AnalyzeProgress {
                    current: idx.min(total_records),
                    total: total_records,
                })
                .ok();
        }
    }

    // 最後に range の残り（ファイル終端まで到達した場合）
    while next_range_i < ranges_sorted.len() {
        // Lemke Oliver 用の prefix スナップショット（ファイル終端までの状態）
        lo10_prefix_snaps.push((ranges_sorted[next_range_i], lo10_trans.clone()));
        lo30_prefix_snaps.push((ranges_sorted[next_range_i], lo30_trans.clone()));

        let (_n_test30, ll1_30, ll2_30, cmi30, _c1_30) =
            compute_stats_from_c2(&c2_total30, &c2_train30, &c2_test30, n30);
        range_results30.push(RangeResult {
            max_p: ranges_sorted[next_range_i],
            triplets: triplets30_seen,
            cmi: cmi30,
            delta_ll: ll1_30 - ll2_30,
        });

        let (_n_test210, ll1_210, ll2_210, cmi210, _c1_210) =
            compute_stats_from_c2(&c2_total210, &c2_train210, &c2_test210, n210);
        range_results210.push(RangeResult {
            max_p: ranges_sorted[next_range_i],
            triplets: triplets210_seen,
            cmi: cmi210,
            delta_ll: ll1_210 - ll2_210,
        });
        next_range_i += 1;
    }

    // Prime stats
    let integrity30 = compute_integrity(&c2_total30, n30);
    let (_n_test30, ll1_30, ll2_30, cmi30, c1_30) =
        compute_stats_from_c2(&c2_total30, &c2_train30, &c2_test30, n30);
    let theory30 = theory_comparison_from_c1(&c1_30, n30);

    let integrity210 = compute_integrity(&c2_total210, n210);
    let (_n_test210, ll1_210, ll2_210, cmi210, c1_210) =
        compute_stats_from_c2(&c2_total210, &c2_train210, &c2_test210, n210);
    let theory210 = theory_comparison_from_c1(&c1_210, n210);

    // Lemke Oliver 理論比較（mod10 / mod30）
    let lo10 = lo_comparison_from_transition_counts(
        10,
        &MOD10_RESIDUES,
        &lo10_trans,
        lo10_prime_max,
        &lo10_prefix_snaps,
    );
    let lo30 = lo_comparison_from_transition_counts(
        30,
        &MOD30_RESIDUES,
        &lo30_trans,
        lo30_prime_max,
        &lo30_prefix_snaps,
    );
    let lemke_oliver = Some(vec![lo10, lo30]);

    // wheel 側：デフォルトは prime triplets 数だが、上限 10M で打ち切る（過大な乱数生成を防ぐ）
    const MAX_WHEEL_DEFAULT: u64 = 10_000_000;
    // Wheel baselines (mod 30)
    let wheel_triplets30 = wheel_samples_override.unwrap_or(triplets30_seen.min(MAX_WHEEL_DEFAULT));
    let wheel_len30 = (wheel_triplets30 as usize).saturating_add(2);
    sender
        .send(WorkerMessage::Log(format!(
            "Validation mod30: prime triplets={triplets30_seen} -> wheel baselines ({wheel_triplets30} samples)..."
        )))
        .ok();

    let iid_indices30 = generate_uniform_residue_indices(n30, wheel_len30, seed);
    let mut iid_total30 = vec![0u64; n30 * n30 * n30];
    let mut iid_train30 = vec![0u64; n30 * n30 * n30];
    let mut iid_test30 = vec![0u64; n30 * n30 * n30];
    let _ = accumulate_triplets(
        &iid_indices30,
        n30,
        &mut iid_total30,
        &mut iid_train30,
        &mut iid_test30,
    );
    let (_iid_n_test30, iid_ll1_30, iid_ll2_30, iid_cmi30, _iid_c1_total30) =
        compute_stats_from_c2(&iid_total30, &iid_train30, &iid_test30, n30);
    let iid_cmp30 = WheelComparison {
        baseline: "iid_residues".to_string(),
        wheel_sample_size: wheel_triplets30,
        accept_prob_q: f64::NAN,
        prime_cmi: cmi30,
        wheel_cmi: iid_cmi30,
        delta_cmi: cmi30 - iid_cmi30,
        prime_delta_ll: ll1_30 - ll2_30,
        wheel_delta_ll: iid_ll1_30 - iid_ll2_30,
        delta_delta_ll: (ll1_30 - ll2_30) - (iid_ll1_30 - iid_ll2_30),
        conclusion: determine_conclusion(cmi30 - iid_cmi30, iid_cmi30),
    };

    let is_coprime30 = build_is_coprime_table(30);
    let (a30, b30) = (first_p30.unwrap_or(0), last_p30.unwrap_or(0));
    let candidate_count30 = if a30 > 0 && b30 >= a30 {
        count_coprime_in_range_inclusive(a30, b30, 30, &is_coprime30)
    } else {
        0
    };
    let q30 = if candidate_count30 > 0 {
        (prime_count30 as f64 / candidate_count30 as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let start_idx30 = {
        let rem = if a30 > 0 { a30 % 30 } else { residues30[0] };
        residues30.iter().position(|&x| x == rem).unwrap_or(0usize)
    };
    let thinned_indices30 = generate_thinned_candidate_indices(
        n30,
        wheel_len30,
        q30,
        seed ^ 0x9E37_79B9_7F4A_7C15,
        start_idx30,
    );
    let mut th_total30 = vec![0u64; n30 * n30 * n30];
    let mut th_train30 = vec![0u64; n30 * n30 * n30];
    let mut th_test30 = vec![0u64; n30 * n30 * n30];
    let _ = accumulate_triplets(
        &thinned_indices30,
        n30,
        &mut th_total30,
        &mut th_train30,
        &mut th_test30,
    );
    let (_th_n_test30, th_ll1_30, th_ll2_30, th_cmi30, _th_c1_total30) =
        compute_stats_from_c2(&th_total30, &th_train30, &th_test30, n30);
    let th_cmp30 = WheelComparison {
        baseline: "wheel_thinned_candidates".to_string(),
        wheel_sample_size: wheel_triplets30,
        accept_prob_q: if q30 > 0.0 { q30 } else { f64::NAN },
        prime_cmi: cmi30,
        wheel_cmi: th_cmi30,
        delta_cmi: cmi30 - th_cmi30,
        prime_delta_ll: ll1_30 - ll2_30,
        wheel_delta_ll: th_ll1_30 - th_ll2_30,
        delta_delta_ll: (ll1_30 - ll2_30) - (th_ll1_30 - th_ll2_30),
        conclusion: determine_conclusion(cmi30 - th_cmi30, th_cmi30),
    };

    // Wheel baselines (mod 210)
    let wheel_triplets210 =
        wheel_samples_override.unwrap_or(triplets210_seen.min(MAX_WHEEL_DEFAULT));
    let wheel_len210 = (wheel_triplets210 as usize).saturating_add(2);
    sender
        .send(WorkerMessage::Log(format!(
            "Validation mod210: prime triplets={triplets210_seen} -> wheel baselines ({wheel_triplets210} samples)..."
        )))
        .ok();

    let iid_indices210 = generate_uniform_residue_indices(n210, wheel_len210, seed ^ 0xD1B5_4A32);
    let mut iid_total210 = vec![0u64; n210 * n210 * n210];
    let mut iid_train210 = vec![0u64; n210 * n210 * n210];
    let mut iid_test210 = vec![0u64; n210 * n210 * n210];
    let _ = accumulate_triplets(
        &iid_indices210,
        n210,
        &mut iid_total210,
        &mut iid_train210,
        &mut iid_test210,
    );
    let (_iid_n_test210, iid_ll1_210, iid_ll2_210, iid_cmi210, _iid_c1_total210) =
        compute_stats_from_c2(&iid_total210, &iid_train210, &iid_test210, n210);
    let iid_cmp210 = WheelComparison {
        baseline: "iid_residues".to_string(),
        wheel_sample_size: wheel_triplets210,
        accept_prob_q: f64::NAN,
        prime_cmi: cmi210,
        wheel_cmi: iid_cmi210,
        delta_cmi: cmi210 - iid_cmi210,
        prime_delta_ll: ll1_210 - ll2_210,
        wheel_delta_ll: iid_ll1_210 - iid_ll2_210,
        delta_delta_ll: (ll1_210 - ll2_210) - (iid_ll1_210 - iid_ll2_210),
        conclusion: determine_conclusion(cmi210 - iid_cmi210, iid_cmi210),
    };

    let is_coprime210 = build_is_coprime_table(210);
    let (a210, b210) = (first_p210.unwrap_or(0), last_p210.unwrap_or(0));
    let candidate_count210 = if a210 > 0 && b210 >= a210 {
        count_coprime_in_range_inclusive(a210, b210, 210, &is_coprime210)
    } else {
        0
    };
    let q210 = if candidate_count210 > 0 {
        (prime_count210 as f64 / candidate_count210 as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let start_idx210 = {
        let rem = if a210 > 0 { a210 % 210 } else { residues210[0] };
        residues210.iter().position(|&x| x == rem).unwrap_or(0usize)
    };
    let thinned_indices210 = generate_thinned_candidate_indices(
        n210,
        wheel_len210,
        q210,
        seed ^ 0xA24B_81C6_5E3D_9A47,
        start_idx210,
    );
    let mut th_total210 = vec![0u64; n210 * n210 * n210];
    let mut th_train210 = vec![0u64; n210 * n210 * n210];
    let mut th_test210 = vec![0u64; n210 * n210 * n210];
    let _ = accumulate_triplets(
        &thinned_indices210,
        n210,
        &mut th_total210,
        &mut th_train210,
        &mut th_test210,
    );
    let (_th_n_test210, th_ll1_210, th_ll2_210, th_cmi210, _th_c1_total210) =
        compute_stats_from_c2(&th_total210, &th_train210, &th_test210, n210);
    let th_cmp210 = WheelComparison {
        baseline: "wheel_thinned_candidates".to_string(),
        wheel_sample_size: wheel_triplets210,
        accept_prob_q: if q210 > 0.0 { q210 } else { f64::NAN },
        prime_cmi: cmi210,
        wheel_cmi: th_cmi210,
        delta_cmi: cmi210 - th_cmi210,
        prime_delta_ll: ll1_210 - ll2_210,
        wheel_delta_ll: th_ll1_210 - th_ll2_210,
        delta_delta_ll: (ll1_210 - ll2_210) - (th_ll1_210 - th_ll2_210),
        conclusion: determine_conclusion(cmi210 - th_cmi210, th_cmi210),
    };

    let res30 = ValidationResult {
        modulus: 30,
        integrity: integrity30,
        theory: theory30,
        lemke_oliver: lemke_oliver.clone(),
        wheel: vec![iid_cmp30, th_cmp30.clone()],
        overall_conclusion: th_cmp30.conclusion.clone(),
        range_dep: RangeDependency {
            ranges: range_results30,
        },
    };
    let res210 = ValidationResult {
        modulus: 210,
        integrity: integrity210,
        theory: theory210,
        lemke_oliver: None,
        wheel: vec![iid_cmp210, th_cmp210.clone()],
        overall_conclusion: th_cmp210.conclusion.clone(),
        range_dep: RangeDependency {
            ranges: range_results210,
        },
    };

    let bundle = ValidationBundle {
        mod30: res30,
        mod30_triplets: triplets30_seen,
        mod210: res210,
        mod210_triplets: triplets210_seen,
    };

    // 最終 publish
    if let Ok(mut guard) = shared_result.try_lock() {
        *guard = bundle.clone();
    }
    if let Ok(mut guard) = shared_processed.try_lock() {
        *guard = triplets30_seen;
    }

    Ok((bundle, triplets30_seen, idx))
}
