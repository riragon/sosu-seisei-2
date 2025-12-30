//! Analyze Tab: PRHS (mod 30 / mod 210)
//!
//! NOTE:
//! - types + engine + markdown + UI をこのファイルに集約する。

#![allow(clippy::needless_range_loop)]

use crate::analyze::PRHSBinMode;

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use crate::analyze::MOD210_RESIDUES;
use crate::engine_types::PrimeResult;
use crate::worker_message::WorkerMessage;

const READER_CAPACITY_BYTES: usize = 8 * 1024 * 1024;
const LOG_INTERVAL: u64 = 1_000_000;

/// バイナリ primes ファイルを開き、`BufReader` と総レコード数を返す。
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

fn publish_realtime<T: Clone>(
    idx: u64,
    total_records: u64,
    sender: &mpsc::Sender<WorkerMessage>,
    shared_result: &Arc<Mutex<T>>,
    shared_processed: &Arc<Mutex<u64>>,
    result: &T,
    total_considered: u64,
) {
    if let Ok(mut guard) = shared_result.try_lock() {
        *guard = result.clone();
    }
    if let Ok(mut guard) = shared_processed.try_lock() {
        *guard = total_considered;
    }
    sender
        .send(WorkerMessage::AnalyzeProgress {
            current: idx.min(total_records),
            total: total_records,
        })
        .ok();
}

fn mod30_residue_to_index(r: u64) -> Option<usize> {
    match r {
        1 => Some(0),
        7 => Some(1),
        11 => Some(2),
        13 => Some(3),
        17 => Some(4),
        19 => Some(5),
        23 => Some(6),
        29 => Some(7),
        _ => None,
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct PrhsCounts {
    c2: [[[u64; 8]; 8]; 8],
}

impl PrhsCounts {
    fn add_triplet(&mut self, i: usize, j: usize, k: usize) {
        self.c2[i][j][k] = self.c2[i][j][k].saturating_add(1);
    }

    fn total_triplets(&self) -> u64 {
        let mut s = 0u64;
        for i in 0..8 {
            for j in 0..8 {
                s = s.saturating_add(self.c2[i][j].iter().sum::<u64>());
            }
        }
        s
    }

    fn c1_from_c2(&self) -> [[u64; 8]; 8] {
        let mut c1 = [[0u64; 8]; 8];
        for i in 0..8 {
            for j in 0..8 {
                for k in 0..8 {
                    c1[j][k] = c1[j][k].saturating_add(self.c2[i][j][k]);
                }
            }
        }
        c1
    }
}

fn safe_log2(x: f64) -> f64 {
    x.ln() / std::f64::consts::LN_2
}

fn idx3(n: usize, i: usize, j: usize, k: usize) -> usize {
    (i * n + j) * n + k
}

fn mod210_residue_to_index(r: u64) -> Option<usize> {
    match r {
        1 => Some(0),
        11 => Some(1),
        13 => Some(2),
        17 => Some(3),
        19 => Some(4),
        23 => Some(5),
        29 => Some(6),
        31 => Some(7),
        37 => Some(8),
        41 => Some(9),
        43 => Some(10),
        47 => Some(11),
        53 => Some(12),
        59 => Some(13),
        61 => Some(14),
        67 => Some(15),
        71 => Some(16),
        73 => Some(17),
        79 => Some(18),
        83 => Some(19),
        89 => Some(20),
        97 => Some(21),
        101 => Some(22),
        103 => Some(23),
        107 => Some(24),
        109 => Some(25),
        113 => Some(26),
        121 => Some(27),
        127 => Some(28),
        131 => Some(29),
        137 => Some(30),
        139 => Some(31),
        143 => Some(32),
        149 => Some(33),
        151 => Some(34),
        157 => Some(35),
        163 => Some(36),
        167 => Some(37),
        169 => Some(38),
        173 => Some(39),
        179 => Some(40),
        181 => Some(41),
        187 => Some(42),
        191 => Some(43),
        193 => Some(44),
        197 => Some(45),
        199 => Some(46),
        209 => Some(47),
        _ => None,
    }
}

fn total_triplets_c2(n: usize, c2: &[u64]) -> u64 {
    let mut s = 0u64;
    for v in c2.iter().copied() {
        s = s.saturating_add(v);
    }
    let _ = n;
    s
}

fn c1_from_c2_48(c2: &[u64]) -> [[u64; 48]; 48] {
    let n = 48usize;
    let mut c1 = [[0u64; 48]; 48];
    for i in 0..n {
        for j in 0..n {
            for k in 0..n {
                let idx = idx3(n, i, j, k);
                c1[j][k] = c1[j][k].saturating_add(c2[idx]);
            }
        }
    }
    c1
}

fn compute_log_loss_bits_m1_48(
    test_c1: &[[u64; 48]; 48],
    train_c1: &[[u64; 48]; 48],
    alpha: f64,
) -> (f64, u64) {
    let mut total = 0u64;
    let mut log_l = 0.0f64;
    for j in 0..48usize {
        let row_sum_train: u64 = train_c1[j].iter().sum();
        let denom = row_sum_train as f64 + 48.0 * alpha;
        for k in 0..48usize {
            let c = test_c1[j][k];
            if c == 0 {
                continue;
            }
            total = total.saturating_add(c);
            let p = (train_c1[j][k] as f64 + alpha) / denom;
            log_l += (c as f64) * safe_log2(p);
        }
    }
    let ll = if total > 0 {
        -(log_l / total as f64)
    } else {
        0.0
    };
    (ll, total)
}

fn compute_log_loss_bits_m2_48(test_c2: &[u64], train_c2: &[u64], alpha: f64) -> (f64, u64) {
    let n = 48usize;
    let mut total = 0u64;
    let mut log_l = 0.0f64;
    for i in 0..n {
        for j in 0..n {
            let mut row_sum_train = 0u64;
            for k in 0..n {
                row_sum_train = row_sum_train.saturating_add(train_c2[idx3(n, i, j, k)]);
            }
            let denom = row_sum_train as f64 + (n as f64) * alpha;
            for k in 0..n {
                let c = test_c2[idx3(n, i, j, k)];
                if c == 0 {
                    continue;
                }
                total = total.saturating_add(c);
                let p = (train_c2[idx3(n, i, j, k)] as f64 + alpha) / denom;
                log_l += (c as f64) * safe_log2(p);
            }
        }
    }
    let ll = if total > 0 {
        -(log_l / total as f64)
    } else {
        0.0
    };
    (ll, total)
}

fn compute_log_likelihood_mle_m1_48(c1: &[[u64; 48]; 48]) -> f64 {
    let mut ll = 0.0f64;
    for j in 0..48usize {
        let row_sum: u64 = c1[j].iter().sum();
        if row_sum == 0 {
            continue;
        }
        let denom = row_sum as f64;
        for k in 0..48usize {
            let c = c1[j][k];
            if c == 0 {
                continue;
            }
            let p = (c as f64) / denom;
            ll += (c as f64) * p.ln();
        }
    }
    ll
}

fn compute_log_likelihood_mle_m2_48(c2: &[u64]) -> f64 {
    let n = 48usize;
    let mut ll = 0.0f64;
    for i in 0..n {
        for j in 0..n {
            let mut row_sum = 0u64;
            for k in 0..n {
                row_sum = row_sum.saturating_add(c2[idx3(n, i, j, k)]);
            }
            if row_sum == 0 {
                continue;
            }
            let denom = row_sum as f64;
            for k in 0..n {
                let c = c2[idx3(n, i, j, k)];
                if c == 0 {
                    continue;
                }
                let p = (c as f64) / denom;
                ll += (c as f64) * p.ln();
            }
        }
    }
    ll
}

fn compute_cmi_nats_48(c2: &[u64], c1: &[[u64; 48]; 48]) -> f64 {
    let n = 48usize;
    let total = total_triplets_c2(n, c2);
    if total == 0 {
        return 0.0;
    }
    let n_total = total as f64;

    let mut row_sum_j = [0u64; 48];
    for j in 0..n {
        row_sum_j[j] = c1[j].iter().sum();
    }

    let mut cmi = 0.0f64;
    for i in 0..n {
        for j in 0..n {
            let mut row_sum_ij = 0u64;
            for k in 0..n {
                row_sum_ij = row_sum_ij.saturating_add(c2[idx3(n, i, j, k)]);
            }
            if row_sum_ij == 0 || row_sum_j[j] == 0 {
                continue;
            }
            let denom_ij = row_sum_ij as f64;
            let denom_j = row_sum_j[j] as f64;
            for k in 0..n {
                let c = c2[idx3(n, i, j, k)];
                if c == 0 {
                    continue;
                }
                let p_ijk = (c as f64) / n_total;
                let p_k_ij = (c as f64) / denom_ij;
                let p_k_j = (c1[j][k] as f64) / denom_j;
                if p_k_j > 0.0 {
                    cmi += p_ijk * (p_k_ij / p_k_j).ln();
                }
            }
        }
    }
    cmi
}

fn compute_kl_matrix_nats_48(c2: &[u64], c1: &[[u64; 48]; 48], alpha: f64) -> Vec<Vec<f64>> {
    let n = 48usize;
    let mut out = vec![vec![0.0f64; n]; n];
    let mut row_sum_j = [0u64; 48];
    for j in 0..n {
        row_sum_j[j] = c1[j].iter().sum();
    }
    for i in 0..n {
        for j in 0..n {
            let mut row_sum_ij = 0u64;
            for k in 0..n {
                row_sum_ij = row_sum_ij.saturating_add(c2[idx3(n, i, j, k)]);
            }
            if row_sum_ij == 0 || row_sum_j[j] == 0 {
                out[i][j] = 0.0;
                continue;
            }
            let denom_q = row_sum_ij as f64 + (n as f64) * alpha;
            let denom_p = row_sum_j[j] as f64 + (n as f64) * alpha;
            let mut kl = 0.0f64;
            for k in 0..n {
                let q = (c2[idx3(n, i, j, k)] as f64 + alpha) / denom_q;
                let p = (c1[j][k] as f64 + alpha) / denom_p;
                kl += q * (q / p).ln();
            }
            out[i][j] = kl;
        }
    }
    out
}

fn normalize_row_probs_48(counts: &[u64], alpha: f64) -> Vec<f64> {
    let n = 48usize;
    let mut sum = 0u64;
    for &c in counts {
        sum = sum.saturating_add(c);
    }
    let denom = sum as f64 + (n as f64) * alpha;
    let mut out = vec![0.0f64; n];
    for k in 0..n {
        out[k] = (counts[k] as f64 + alpha) / denom;
    }
    out
}

fn compute_prhs210_stats(
    total_c2: &[u64],
    train_c2: &[u64],
    test_c2: &[u64],
) -> (PRHS210Stats, [[u64; 48]; 48]) {
    let c1_total = c1_from_c2_48(total_c2);
    let c1_train = c1_from_c2_48(train_c2);
    let c1_test = c1_from_c2_48(test_c2);

    let alpha_ll = 1e-3_f64;
    let (ll1, n_test_1) = compute_log_loss_bits_m1_48(&c1_test, &c1_train, alpha_ll);
    let (ll2, n_test_2) = compute_log_loss_bits_m2_48(test_c2, train_c2, alpha_ll);
    let test_count = n_test_1.min(n_test_2);

    let cmi = compute_cmi_nats_48(total_c2, &c1_total);

    let sample_count = total_triplets_c2(48, total_c2);
    let n = sample_count.max(1) as f64;
    let log_l1 = compute_log_likelihood_mle_m1_48(&c1_total);
    let log_l2 = compute_log_likelihood_mle_m2_48(total_c2);
    let k1 = (48 * (48 - 1)) as f64;
    let k2 = (48 * 48 * (48 - 1)) as f64;
    let aic_m1 = 2.0 * k1 - 2.0 * log_l1;
    let aic_m2 = 2.0 * k2 - 2.0 * log_l2;
    let bic_m1 = k1 * n.ln() - 2.0 * log_l1;
    let bic_m2 = k2 * n.ln() - 2.0 * log_l2;

    let kl_matrix = compute_kl_matrix_nats_48(total_c2, &c1_total, 1e-9);

    let stats = PRHS210Stats {
        sample_count,
        test_count,
        log_loss_m1: ll1,
        log_loss_m2: ll2,
        delta_ll: ll1 - ll2,
        cmi,
        aic_m1,
        aic_m2,
        bic_m1,
        bic_m2,
        kl_matrix,
    };
    (stats, c1_total)
}

fn p1_prob_from_c1_48(c1: &[[u64; 48]; 48]) -> Vec<Vec<f64>> {
    let n = 48usize;
    let mut p1 = vec![vec![0.0f64; n]; n];
    for j in 0..n {
        let row_sum: u64 = c1[j].iter().sum();
        if row_sum == 0 {
            continue;
        }
        let denom = row_sum as f64;
        for k in 0..n {
            p1[j][k] = (c1[j][k] as f64) / denom;
        }
    }
    p1
}

fn compute_log_loss_bits_m1(
    test_c1: &[[u64; 8]; 8],
    train_c1: &[[u64; 8]; 8],
    alpha: f64,
) -> (f64, u64) {
    let mut total = 0u64;
    let mut log_l = 0.0f64;
    for j in 0..8usize {
        let row_sum_train: u64 = train_c1[j].iter().sum();
        let denom = row_sum_train as f64 + 8.0 * alpha;
        for k in 0..8usize {
            let c = test_c1[j][k];
            if c == 0 {
                continue;
            }
            total = total.saturating_add(c);
            let p = (train_c1[j][k] as f64 + alpha) / denom;
            log_l += (c as f64) * safe_log2(p);
        }
    }
    let ll = if total > 0 {
        -(log_l / total as f64)
    } else {
        0.0
    };
    (ll, total)
}

fn compute_log_loss_bits_m2(
    test_c2: &[[[u64; 8]; 8]; 8],
    train_c2: &[[[u64; 8]; 8]; 8],
    alpha: f64,
) -> (f64, u64) {
    let mut total = 0u64;
    let mut log_l = 0.0f64;
    for i in 0..8usize {
        for j in 0..8usize {
            let row_sum_train: u64 = train_c2[i][j].iter().sum();
            let denom = row_sum_train as f64 + 8.0 * alpha;
            for k in 0..8usize {
                let c = test_c2[i][j][k];
                if c == 0 {
                    continue;
                }
                total = total.saturating_add(c);
                let p = (train_c2[i][j][k] as f64 + alpha) / denom;
                log_l += (c as f64) * safe_log2(p);
            }
        }
    }
    let ll = if total > 0 {
        -(log_l / total as f64)
    } else {
        0.0
    };
    (ll, total)
}

fn compute_log_likelihood_mle_m1(c1: &[[u64; 8]; 8]) -> f64 {
    let mut ll = 0.0f64;
    for j in 0..8usize {
        let row_sum: u64 = c1[j].iter().sum();
        if row_sum == 0 {
            continue;
        }
        let denom = row_sum as f64;
        for k in 0..8usize {
            let c = c1[j][k];
            if c == 0 {
                continue;
            }
            let p = (c as f64) / denom;
            ll += (c as f64) * p.ln();
        }
    }
    ll
}

fn compute_log_likelihood_mle_m2(c2: &[[[u64; 8]; 8]; 8]) -> f64 {
    let mut ll = 0.0f64;
    for i in 0..8usize {
        for j in 0..8usize {
            let row_sum: u64 = c2[i][j].iter().sum();
            if row_sum == 0 {
                continue;
            }
            let denom = row_sum as f64;
            for k in 0..8usize {
                let c = c2[i][j][k];
                if c == 0 {
                    continue;
                }
                let p = (c as f64) / denom;
                ll += (c as f64) * p.ln();
            }
        }
    }
    ll
}

fn compute_cmi_nats(c2: &[[[u64; 8]; 8]; 8], c1: &[[u64; 8]; 8]) -> f64 {
    let mut total = 0u64;
    let mut row_sum_ij = [[0u64; 8]; 8];
    for i in 0..8usize {
        for j in 0..8usize {
            let s = c2[i][j].iter().sum::<u64>();
            row_sum_ij[i][j] = s;
            total = total.saturating_add(s);
        }
    }
    if total == 0 {
        return 0.0;
    }

    let mut row_sum_j = [0u64; 8];
    for j in 0..8usize {
        row_sum_j[j] = c1[j].iter().sum();
    }

    let total_f = total as f64;
    let mut cmi = 0.0f64;
    for i in 0..8usize {
        for j in 0..8usize {
            let denom_ij = row_sum_ij[i][j];
            if denom_ij == 0 {
                continue;
            }
            let denom_j = row_sum_j[j];
            if denom_j == 0 {
                continue;
            }
            let denom_ij_f = denom_ij as f64;
            let denom_j_f = denom_j as f64;
            for k in 0..8usize {
                let c = c2[i][j][k];
                if c == 0 {
                    continue;
                }
                let p_ijk = (c as f64) / total_f;
                let p_k_ij = (c as f64) / denom_ij_f;
                let p_k_j = (c1[j][k] as f64) / denom_j_f;
                if p_k_j > 0.0 {
                    cmi += p_ijk * (p_k_ij / p_k_j).ln();
                }
            }
        }
    }
    cmi
}

fn compute_kl_matrix_nats(
    c2: &[[[u64; 8]; 8]; 8],
    c1: &[[u64; 8]; 8],
    alpha: f64,
) -> [[f64; 8]; 8] {
    let mut out = [[0.0f64; 8]; 8];
    let mut row_sum_j = [0u64; 8];
    for j in 0..8usize {
        row_sum_j[j] = c1[j].iter().sum();
    }
    for i in 0..8usize {
        for j in 0..8usize {
            let row_sum_ij: u64 = c2[i][j].iter().sum();
            if row_sum_ij == 0 || row_sum_j[j] == 0 {
                out[i][j] = 0.0;
                continue;
            }
            let denom_q = row_sum_ij as f64 + 8.0 * alpha;
            let denom_p = row_sum_j[j] as f64 + 8.0 * alpha;
            let mut kl = 0.0f64;
            for k in 0..8usize {
                let q = (c2[i][j][k] as f64 + alpha) / denom_q;
                let p = (c1[j][k] as f64 + alpha) / denom_p;
                kl += q * (q / p).ln();
            }
            out[i][j] = kl;
        }
    }
    out
}

fn compute_prhs_stats(
    global_total: &PrhsCounts,
    train: &PrhsCounts,
    test: &PrhsCounts,
) -> PRHSStats {
    let c1_total = global_total.c1_from_c2();
    let c1_train = train.c1_from_c2();
    let c1_test = test.c1_from_c2();

    let alpha = 1e-3_f64;
    let (ll1, n_test_1) = compute_log_loss_bits_m1(&c1_test, &c1_train, alpha);
    let (ll2, n_test_2) = compute_log_loss_bits_m2(&test.c2, &train.c2, alpha);
    let test_count = n_test_1.min(n_test_2);

    let cmi = compute_cmi_nats(&global_total.c2, &c1_total);

    let n = global_total.total_triplets().max(1) as f64;
    let log_l1 = compute_log_likelihood_mle_m1(&c1_total);
    let log_l2 = compute_log_likelihood_mle_m2(&global_total.c2);
    let k1 = 56.0;
    let k2 = 448.0;
    let aic_m1 = 2.0 * k1 - 2.0 * log_l1;
    let aic_m2 = 2.0 * k2 - 2.0 * log_l2;
    let bic_m1 = k1 * n.ln() - 2.0 * log_l1;
    let bic_m2 = k2 * n.ln() - 2.0 * log_l2;

    let kl_matrix = compute_kl_matrix_nats(&global_total.c2, &c1_total, 1e-9);

    PRHSStats {
        sample_count: global_total.total_triplets(),
        test_count,
        log_loss_m1: ll1,
        log_loss_m2: ll2,
        delta_ll: ll1 - ll2,
        cmi,
        aic_m1,
        aic_m2,
        bic_m1,
        bic_m2,
        kl_matrix,
    }
}

#[derive(Debug, Default, Clone)]
struct PrhsBinAcc {
    total: PrhsCounts,
    train: PrhsCounts,
    test: PrhsCounts,
    label: String,
    sample_count: u64,
}

fn ensure_bin(vec: &mut Vec<PrhsBinAcc>, idx: usize) -> &mut PrhsBinAcc {
    if vec.len() <= idx {
        vec.resize_with(idx + 1, PrhsBinAcc::default);
    }
    &mut vec[idx]
}

/// mod 30 PRHS（Prime Residue History Study）
#[allow(clippy::too_many_arguments)]
pub fn analyze_prhs_binary_file(
    path: &Path,
    stop_flag: &AtomicBool,
    sender: &mpsc::Sender<WorkerMessage>,
    shared_result: &Arc<Mutex<PRHSResult>>,
    shared_processed: &Arc<Mutex<u64>>,
    log10_bin_width: f64,
    equal_bin_primes: u64,
    train_ratio: f64,
) -> PrimeResult<(PRHSResult, u64, u64)> {
    let (mut reader, total_records) = open_binary_primes_file(path)?;
    let mut buf = [0u8; 8];

    let mut result = PRHSResult::default();

    let mut global_c1 = [[0u64; 8]; 8];
    let mut global_total = PrhsCounts::default();
    let mut train_total = PrhsCounts::default();
    let mut test_total = PrhsCounts::default();

    let mut bins_log: Vec<PrhsBinAcc> = Vec::new();
    let mut bins_eq: Vec<PrhsBinAcc> = Vec::new();

    let mut idx: u64 = 0;
    let mut skipped_2_3_5: u64 = 0;
    let mut unexpected_residue: u64 = 0;

    let mut filtered_prime_idx: u64 = 0;
    let mut prev2_state: Option<usize> = None;
    let mut prev1_state: Option<usize> = None;
    let mut prev1_p: u64 = 0;
    let mut prev1_filtered_idx: u64 = 0;

    while idx < total_records {
        if stop_flag.load(Ordering::SeqCst) {
            break;
        }

        let p = read_next_u64(&mut reader, &mut buf, idx)?;
        idx += 1;

        if p == 2 || p == 3 || p == 5 {
            skipped_2_3_5 += 1;
            continue;
        }

        let cur = mod30_residue_to_index(p % 30);
        let Some(cur_i) = cur else {
            unexpected_residue += 1;
            prev2_state = None;
            prev1_state = None;
            continue;
        };

        let pos = (idx as f64) / (total_records.max(1) as f64);
        let is_test = pos >= train_ratio;

        if let Some(j) = prev1_state {
            global_c1[j][cur_i] = global_c1[j][cur_i].saturating_add(1);
        }

        if let (Some(i2), Some(j)) = (prev2_state, prev1_state) {
            global_total.add_triplet(i2, j, cur_i);
            if is_test {
                test_total.add_triplet(i2, j, cur_i);
            } else {
                train_total.add_triplet(i2, j, cur_i);
            }

            let w = if log10_bin_width > 0.0 {
                log10_bin_width
            } else {
                0.2
            };
            let log10p = (prev1_p as f64).log10();
            let b = (log10p / w).floor().max(0.0) as usize;
            let acc = ensure_bin(&mut bins_log, b);
            acc.total.add_triplet(i2, j, cur_i);
            let is_test_bin = (acc.sample_count % 5) == 0;
            acc.sample_count = acc.sample_count.saturating_add(1);
            if is_test_bin {
                acc.test.add_triplet(i2, j, cur_i);
            } else {
                acc.train.add_triplet(i2, j, cur_i);
            }
            if acc.label.is_empty() {
                let lo = (b as f64) * w;
                let hi = lo + w;
                acc.label = format!("log10 {lo:.3}-{hi:.3}");
            }

            let step = if equal_bin_primes > 0 {
                equal_bin_primes
            } else {
                5_000_000
            };
            let be = (prev1_filtered_idx / step) as usize;
            let acc2 = ensure_bin(&mut bins_eq, be);
            acc2.total.add_triplet(i2, j, cur_i);
            let is_test_bin2 = (acc2.sample_count % 5) == 0;
            acc2.sample_count = acc2.sample_count.saturating_add(1);
            if is_test_bin2 {
                acc2.test.add_triplet(i2, j, cur_i);
            } else {
                acc2.train.add_triplet(i2, j, cur_i);
            }
            if acc2.label.is_empty() {
                let lo = (be as u64).saturating_mul(step);
                let hi = lo.saturating_add(step);
                acc2.label = format!("idx {lo}-{hi}");
            }
        }

        prev2_state = prev1_state;
        prev1_state = Some(cur_i);
        prev1_p = p;
        prev1_filtered_idx = filtered_prime_idx;
        filtered_prime_idx = filtered_prime_idx.saturating_add(1);

        if idx % LOG_INTERVAL == 0 || idx == total_records {
            result.c1 = global_c1;
            result.c2 = global_total.c2;
            result.global = compute_prhs_stats(&global_total, &train_total, &test_total);

            let total_triplets = global_total.total_triplets();
            publish_realtime(
                idx,
                total_records,
                sender,
                shared_result,
                shared_processed,
                &result,
                total_triplets,
            );
        }
    }

    result.c1 = global_c1;
    result.c2 = global_total.c2;
    result.global = compute_prhs_stats(&global_total, &train_total, &test_total);

    let mut bins_out: Vec<PRHSBinStats> = Vec::new();

    for (bi, acc) in bins_log.iter().enumerate() {
        if acc.total.total_triplets() == 0 {
            continue;
        }
        let stats = compute_prhs_stats(&acc.total, &acc.train, &acc.test);
        bins_out.push(PRHSBinStats {
            mode: PRHSBinMode::Log10,
            bin_index: bi,
            label: acc.label.clone(),
            c1: acc.total.c1_from_c2(),
            c2: acc.total.c2,
            stats,
        });
    }
    for (bi, acc) in bins_eq.iter().enumerate() {
        if acc.total.total_triplets() == 0 {
            continue;
        }
        let stats = compute_prhs_stats(&acc.total, &acc.train, &acc.test);
        bins_out.push(PRHSBinStats {
            mode: PRHSBinMode::EqualCount,
            bin_index: bi,
            label: acc.label.clone(),
            c1: acc.total.c1_from_c2(),
            c2: acc.total.c2,
            stats,
        });
    }

    bins_out.sort_by(|a, b| {
        let ma = match a.mode {
            PRHSBinMode::Log10 => 0u8,
            PRHSBinMode::EqualCount => 1u8,
        };
        let mb = match b.mode {
            PRHSBinMode::Log10 => 0u8,
            PRHSBinMode::EqualCount => 1u8,
        };
        ma.cmp(&mb).then_with(|| a.bin_index.cmp(&b.bin_index))
    });
    result.bins = bins_out;

    let total_triplets = global_total.total_triplets();
    publish_realtime(
        idx,
        total_records,
        sender,
        shared_result,
        shared_processed,
        &result,
        total_triplets,
    );

    if skipped_2_3_5 > 0 {
        sender
            .send(WorkerMessage::Log(format!(
                "Analyze(PRHS): skipped {skipped_2_3_5} record(s) for p=2,3,5 (excluded)."
            )))
            .ok();
    }
    if unexpected_residue > 0 {
        sender
            .send(WorkerMessage::Log(format!(
                "Analyze(PRHS): found {unexpected_residue} record(s) with unexpected p%30."
            )))
            .ok();
    }

    Ok((result, total_triplets, idx))
}

/// mod 210 PRHS（Prime Residue History Study）
#[allow(clippy::too_many_arguments)]
pub fn analyze_prhs210_binary_file(
    path: &Path,
    stop_flag: &AtomicBool,
    sender: &mpsc::Sender<WorkerMessage>,
    shared_result: &Arc<Mutex<PRHS210Result>>,
    shared_processed: &Arc<Mutex<u64>>,
    log10_bin_width: f64,
    equal_bin_primes: u64,
    train_ratio: f64,
    exclude_diagonal: bool,
    min_nij: u64,
) -> PrimeResult<(PRHS210Result, u64, u64)> {
    const N: usize = 48;
    const N3: usize = N * N * N;

    let (mut reader, total_records) = open_binary_primes_file(path)?;
    let mut buf = [0u8; 8];

    let mut result = PRHS210Result::default();

    let mut global_total = vec![0u64; N3];
    let mut train_total = vec![0u64; N3];
    let mut test_total = vec![0u64; N3];

    let mut bins_out: Vec<PRHS210BinStats> = Vec::new();

    let mut log_bin_idx: Option<usize> = None;
    let mut log_bin_label = String::new();
    let mut log_bin_sample_count: u64 = 0;
    let mut log_total = vec![0u64; N3];
    let mut log_train = vec![0u64; N3];
    let mut log_test = vec![0u64; N3];

    let mut eq_bin_idx: Option<usize> = None;
    let mut eq_bin_label = String::new();
    let mut eq_bin_sample_count: u64 = 0;
    let mut eq_total = vec![0u64; N3];
    let mut eq_train = vec![0u64; N3];
    let mut eq_test = vec![0u64; N3];

    let finalize_bin = |mode: PRHSBinMode,
                        bin_index: usize,
                        label: &str,
                        total_c2: &Vec<u64>,
                        train_c2: &Vec<u64>,
                        test_c2: &Vec<u64>,
                        out: &mut Vec<PRHS210BinStats>| {
        if total_triplets_c2(N, total_c2) == 0 {
            return;
        }
        let (stats, _c1_total) = compute_prhs210_stats(total_c2, train_c2, test_c2);
        out.push(PRHS210BinStats {
            mode,
            bin_index,
            label: label.to_string(),
            stats,
        });
    };

    let mut idx: u64 = 0;
    let mut skipped_2_3_5_7: u64 = 0;
    let mut unexpected_residue: u64 = 0;

    let mut filtered_prime_idx: u64 = 0;
    let mut prev2_state: Option<usize> = None;
    let mut prev1_state: Option<usize> = None;
    let mut prev1_p: u64 = 0;
    let mut prev1_filtered_idx: u64 = 0;

    while idx < total_records {
        if stop_flag.load(Ordering::SeqCst) {
            break;
        }

        let p = read_next_u64(&mut reader, &mut buf, idx)?;
        idx += 1;

        if p == 2 || p == 3 || p == 5 || p == 7 {
            skipped_2_3_5_7 += 1;
            continue;
        }

        let cur = mod210_residue_to_index(p % 210);
        let Some(cur_i) = cur else {
            unexpected_residue += 1;
            prev2_state = None;
            prev1_state = None;
            continue;
        };

        let pos = (idx as f64) / (total_records.max(1) as f64);
        let is_test_global = pos >= train_ratio;

        if let (Some(i2), Some(j)) = (prev2_state, prev1_state) {
            let id = idx3(N, i2, j, cur_i);
            global_total[id] = global_total[id].saturating_add(1);
            if is_test_global {
                test_total[id] = test_total[id].saturating_add(1);
            } else {
                train_total[id] = train_total[id].saturating_add(1);
            }

            // Log10 bin
            let w = if log10_bin_width > 0.0 {
                log10_bin_width
            } else {
                0.2
            };
            let log10p = (prev1_p as f64).log10();
            let b = (log10p / w).floor().max(0.0) as usize;
            if log_bin_idx != Some(b) {
                if let Some(old) = log_bin_idx {
                    finalize_bin(
                        PRHSBinMode::Log10,
                        old,
                        &log_bin_label,
                        &log_total,
                        &log_train,
                        &log_test,
                        &mut bins_out,
                    );
                }
                log_bin_idx = Some(b);
                let lo = (b as f64) * w;
                let hi = lo + w;
                log_bin_label = format!("log10 {lo:.3}-{hi:.3}");
                log_bin_sample_count = 0;
                log_total.fill(0);
                log_train.fill(0);
                log_test.fill(0);
            }
            let is_test_bin = (log_bin_sample_count % 5) == 0;
            log_bin_sample_count = log_bin_sample_count.saturating_add(1);
            log_total[id] = log_total[id].saturating_add(1);
            if is_test_bin {
                log_test[id] = log_test[id].saturating_add(1);
            } else {
                log_train[id] = log_train[id].saturating_add(1);
            }

            // Equal-count bin
            let step = if equal_bin_primes > 0 {
                equal_bin_primes
            } else {
                5_000_000
            };
            let be = (prev1_filtered_idx / step) as usize;
            if eq_bin_idx != Some(be) {
                if let Some(old) = eq_bin_idx {
                    finalize_bin(
                        PRHSBinMode::EqualCount,
                        old,
                        &eq_bin_label,
                        &eq_total,
                        &eq_train,
                        &eq_test,
                        &mut bins_out,
                    );
                }
                eq_bin_idx = Some(be);
                let lo = (be as u64).saturating_mul(step);
                let hi = lo.saturating_add(step);
                eq_bin_label = format!("idx {lo}-{hi}");
                eq_bin_sample_count = 0;
                eq_total.fill(0);
                eq_train.fill(0);
                eq_test.fill(0);
            }
            let is_test_bin2 = (eq_bin_sample_count % 5) == 0;
            eq_bin_sample_count = eq_bin_sample_count.saturating_add(1);
            eq_total[id] = eq_total[id].saturating_add(1);
            if is_test_bin2 {
                eq_test[id] = eq_test[id].saturating_add(1);
            } else {
                eq_train[id] = eq_train[id].saturating_add(1);
            }
        }

        prev2_state = prev1_state;
        prev1_state = Some(cur_i);
        prev1_p = p;
        prev1_filtered_idx = filtered_prime_idx;
        filtered_prime_idx = filtered_prime_idx.saturating_add(1);

        if idx % LOG_INTERVAL == 0 || idx == total_records {
            let (global_stats, c1_total) =
                compute_prhs210_stats(&global_total, &train_total, &test_total);
            let p1 = p1_prob_from_c1_48(&c1_total);

            let nij_min = min_nij.max(1);
            let mut entries: Vec<(f64, usize, usize)> = Vec::with_capacity(N * N);
            for i in 0..N {
                for j in 0..N {
                    if exclude_diagonal && i == j {
                        continue;
                    }
                    let mut nij = 0u64;
                    for k in 0..N {
                        nij = nij.saturating_add(global_total[idx3(N, i, j, k)]);
                    }
                    if nij < nij_min {
                        continue;
                    }
                    entries.push((global_stats.kl_matrix[i][j], i, j));
                }
            }
            entries.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

            let alpha_disp = 1e-9_f64;
            let mut top_contexts: Vec<PRHS210TopContext> = Vec::new();
            for (rank, (klv, i, j)) in entries.into_iter().take(20).enumerate() {
                let mut nij = 0u64;
                let mut row_counts = vec![0u64; N];
                for k in 0..N {
                    let c = global_total[idx3(N, i, j, k)];
                    row_counts[k] = c;
                    nij = nij.saturating_add(c);
                }
                let total_triplets = total_triplets_c2(N, &global_total);
                let support_pct = if total_triplets > 0 {
                    (nij as f64 / total_triplets as f64) * 100.0
                } else {
                    0.0
                };

                let p2 = normalize_row_probs_48(&row_counts, alpha_disp);
                let mut c1_row_counts = vec![0u64; N];
                c1_row_counts[..N].copy_from_slice(&c1_total[j][..N]);
                let p1s = normalize_row_probs_48(&c1_row_counts, alpha_disp);

                let mut deltas: Vec<(f64, usize)> =
                    (0..N).map(|k| ((p2[k] - p1s[k]).abs(), k)).collect();
                deltas.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

                let mut delta_top: Vec<PRHS210DeltaTop> = Vec::new();
                for (_dabs, k) in deltas.into_iter().take(3) {
                    delta_top.push(PRHS210DeltaTop {
                        k,
                        residue_k: MOD210_RESIDUES[k],
                        p2: p2[k],
                        p1: p1s[k],
                        delta: p2[k] - p1s[k],
                    });
                }

                top_contexts.push(PRHS210TopContext {
                    rank: rank + 1,
                    i,
                    j,
                    residue_i: MOD210_RESIDUES[i],
                    residue_j: MOD210_RESIDUES[j],
                    kl_nats: klv,
                    n_ij: nij,
                    support_pct,
                    delta_top,
                });
            }

            result.p1 = p1;
            result.global = global_stats;
            result.top_contexts = top_contexts;
            result.bins.clear();

            let total_triplets = total_triplets_c2(N, &global_total);
            publish_realtime(
                idx,
                total_records,
                sender,
                shared_result,
                shared_processed,
                &result,
                total_triplets,
            );
        }
    }

    if let Some(old) = log_bin_idx {
        finalize_bin(
            PRHSBinMode::Log10,
            old,
            &log_bin_label,
            &log_total,
            &log_train,
            &log_test,
            &mut bins_out,
        );
    }
    if let Some(old) = eq_bin_idx {
        finalize_bin(
            PRHSBinMode::EqualCount,
            old,
            &eq_bin_label,
            &eq_total,
            &eq_train,
            &eq_test,
            &mut bins_out,
        );
    }

    bins_out.sort_by(|a, b| {
        let ma = match a.mode {
            PRHSBinMode::Log10 => 0u8,
            PRHSBinMode::EqualCount => 1u8,
        };
        let mb = match b.mode {
            PRHSBinMode::Log10 => 0u8,
            PRHSBinMode::EqualCount => 1u8,
        };
        ma.cmp(&mb).then_with(|| a.bin_index.cmp(&b.bin_index))
    });

    let (global_stats, c1_total) = compute_prhs210_stats(&global_total, &train_total, &test_total);
    let p1 = p1_prob_from_c1_48(&c1_total);

    let nij_min = min_nij.max(1);
    let mut entries: Vec<(f64, usize, usize)> = Vec::with_capacity(N * N);
    for i in 0..N {
        for j in 0..N {
            if exclude_diagonal && i == j {
                continue;
            }
            let mut nij = 0u64;
            for k in 0..N {
                nij = nij.saturating_add(global_total[idx3(N, i, j, k)]);
            }
            if nij < nij_min {
                continue;
            }
            entries.push((global_stats.kl_matrix[i][j], i, j));
        }
    }
    entries.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let alpha_disp = 1e-9_f64;
    let mut top_contexts: Vec<PRHS210TopContext> = Vec::new();
    for (rank, (klv, i, j)) in entries.into_iter().take(20).enumerate() {
        let mut nij = 0u64;
        let mut row_counts = vec![0u64; N];
        for k in 0..N {
            let c = global_total[idx3(N, i, j, k)];
            row_counts[k] = c;
            nij = nij.saturating_add(c);
        }
        let total_triplets = total_triplets_c2(N, &global_total);
        let support_pct = if total_triplets > 0 {
            (nij as f64 / total_triplets as f64) * 100.0
        } else {
            0.0
        };

        let p2 = normalize_row_probs_48(&row_counts, alpha_disp);
        let mut c1_row_counts = vec![0u64; N];
        c1_row_counts[..N].copy_from_slice(&c1_total[j][..N]);
        let p1s = normalize_row_probs_48(&c1_row_counts, alpha_disp);

        let mut deltas: Vec<(f64, usize)> = (0..N).map(|k| ((p2[k] - p1s[k]).abs(), k)).collect();
        deltas.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut delta_top: Vec<PRHS210DeltaTop> = Vec::new();
        for (_dabs, k) in deltas.into_iter().take(3) {
            delta_top.push(PRHS210DeltaTop {
                k,
                residue_k: MOD210_RESIDUES[k],
                p2: p2[k],
                p1: p1s[k],
                delta: p2[k] - p1s[k],
            });
        }

        top_contexts.push(PRHS210TopContext {
            rank: rank + 1,
            i,
            j,
            residue_i: MOD210_RESIDUES[i],
            residue_j: MOD210_RESIDUES[j],
            kl_nats: klv,
            n_ij: nij,
            support_pct,
            delta_top,
        });
    }

    result.p1 = p1;
    result.global = global_stats;
    result.top_contexts = top_contexts;
    result.bins = bins_out;

    let total_triplets = total_triplets_c2(N, &global_total);
    publish_realtime(
        idx,
        total_records,
        sender,
        shared_result,
        shared_processed,
        &result,
        total_triplets,
    );

    if skipped_2_3_5_7 > 0 {
        sender
            .send(WorkerMessage::Log(format!(
                "Analyze(PRHS210): skipped {skipped_2_3_5_7} record(s) for p=2,3,5,7 (excluded)."
            )))
            .ok();
    }
    if unexpected_residue > 0 {
        sender
            .send(WorkerMessage::Log(format!(
                "Analyze(PRHS210): unexpected residue count = {unexpected_residue}."
            )))
            .ok();
    }

    Ok((result, total_triplets, idx))
}

/// PRHS の統計（全体またはビン別）。
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct PRHSStats {
    /// トリプレット数（X_{n-1}, X_n, X_{n+1} を観測できた数）
    pub sample_count: u64,
    /// ホールドアウト（test）に入ったトリプレット数
    pub test_count: u64,
    /// ホールドアウト上の log-loss（bits / sample）
    pub log_loss_m1: f64,
    /// ホールドアウト上の log-loss（bits / sample）
    pub log_loss_m2: f64,
    /// 改善量（LogLoss(M1) - LogLoss(M2)）。正なら二次化で改善。
    pub delta_ll: f64,
    /// 条件付き相互情報量 I(X_{n-1}; X_{n+1} | X_n)（nats）
    pub cmi: f64,
    /// AIC（自然対数）
    pub aic_m1: f64,
    pub aic_m2: f64,
    /// BIC（自然対数）
    pub bic_m1: f64,
    pub bic_m2: f64,
    /// KL(i,j): P2(·|i,j) と P1(·|j) の乖離（nats）
    pub kl_matrix: [[f64; 8]; 8],
}

/// PRHS のビン別統計。
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct PRHSBinStats {
    pub mode: PRHSBinMode,
    pub bin_index: usize,
    /// 表示用レンジラベル（例: "log10 9.0-9.2", "idx 0-5,000,000"）
    pub label: String,
    /// 一次遷移カウント（from=j, to=k）
    pub c1: [[u64; 8]; 8],
    /// 二次遷移カウント（prev2=i, prev1=j, next=k）
    pub c2: [[[u64; 8]; 8]; 8],
    pub stats: PRHSStats,
}

/// PRHS（Prime Residue History Study）の分析結果。
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct PRHSResult {
    /// 全体: 一次遷移カウント
    pub c1: [[u64; 8]; 8],
    /// 全体: 二次遷移カウント
    pub c2: [[[u64; 8]; 8]; 8],
    /// 全体統計
    pub global: PRHSStats,
    /// ビン別統計（Log10 / EqualCount の両方を同一Vecに格納）
    pub bins: Vec<PRHSBinStats>,
}

/// mod 210 PRHS の統計（全体またはビン別）。
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct PRHS210Stats {
    /// トリプレット数（X_{n-1}, X_n, X_{n+1} を観測できた数）
    pub sample_count: u64,
    /// ホールドアウト（test）に入ったトリプレット数
    pub test_count: u64,
    /// ホールドアウト上の log-loss（bits / sample）
    pub log_loss_m1: f64,
    /// ホールドアウト上の log-loss（bits / sample）
    pub log_loss_m2: f64,
    /// 改善量（LogLoss(M1) - LogLoss(M2)）。正なら二次化で改善。
    pub delta_ll: f64,
    /// 条件付き相互情報量 I(X_{n-1}; X_{n+1} | X_n)（nats）
    pub cmi: f64,
    /// AIC（自然対数）
    pub aic_m1: f64,
    pub aic_m2: f64,
    /// BIC（自然対数）
    pub bic_m1: f64,
    pub bic_m2: f64,
    /// KL(i,j): P2(·|i,j) と P1(·|j) の乖離（nats）
    ///
    /// 注: Rust/serde の配列実装制約を避けるため Vec<Vec<..>> で保持する（48×48）。
    pub kl_matrix: Vec<Vec<f64>>,
}

/// mod 210 PRHS のビン別統計（メモリ節約のため counts は保持しない）。
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct PRHS210BinStats {
    pub mode: PRHSBinMode,
    pub bin_index: usize,
    /// 表示用レンジラベル（例: "log10 9.0-9.2", "idx 0-5,000,000"）
    pub label: String,
    pub stats: PRHS210Stats,
}

/// mod 210 PRHS: Top context の差分要約（48状態の全分布は出力が巨大になるため、上位のみ保持）
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct PRHS210DeltaTop {
    pub k: usize,
    pub residue_k: u64,
    pub p2: f64,
    pub p1: f64,
    pub delta: f64,
}

/// mod 210 PRHS: Top context（i,j）要約
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct PRHS210TopContext {
    pub rank: usize,
    pub i: usize,
    pub j: usize,
    pub residue_i: u64,
    pub residue_j: u64,
    pub kl_nats: f64,
    pub n_ij: u64,
    pub support_pct: f64,
    /// |Δ| 上位（デフォルトは3件）
    pub delta_top: Vec<PRHS210DeltaTop>,
}

/// mod 210 PRHS（Prime Residue History Study）の分析結果。
///
/// - 48状態の C2 をそのまま保持すると UI 側の clone が重くなるため、
///   UI 出力に必要な「統計・P1・KL・Top contexts・bin stats」に圧縮して保持する。
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct PRHS210Result {
    /// P1: P(X_{n+1}=k | X_n=j)（行=j, 列=k）
    ///
    /// 注: Rust/serde の配列実装制約を避けるため Vec<Vec<..>> で保持する（48×48）。
    pub p1: Vec<Vec<f64>>,
    /// 全体統計（KL 行列も含む）
    pub global: PRHS210Stats,
    /// Top contexts by KL（上位のみ）
    pub top_contexts: Vec<PRHS210TopContext>,
    /// ビン別統計（Log10 / EqualCount の両方を同一Vecに格納）
    pub bins: Vec<PRHS210BinStats>,
}
