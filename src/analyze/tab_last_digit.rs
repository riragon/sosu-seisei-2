//! Analyze Tab: LastDigit (1/3/7/9)
//!
//! このファイルは **LastDigit タブを自己完結**させるための実装です。
//! 他タブ開発の影響を受けないよう、必要な型・エンジン・Markdown/UI 出力をここに集約します。

#![allow(clippy::needless_range_loop)]

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use eframe::egui;

use crate::engine_types::PrimeResult;
use crate::ui_components::{field_label, section_title};
use crate::ui_theme::{colors, font_sizes};
use crate::worker_message::WorkerMessage;

const READER_CAPACITY_BYTES: usize = 8 * 1024 * 1024;
const LOG_INTERVAL: u64 = 1_000_000;

fn encode_4gram(a: usize, b: usize, c: usize, d: usize) -> usize {
    (((a * 4) + b) * 4 + c) * 4 + d
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

fn publish_realtime(
    idx: u64,
    total_records: u64,
    sender: &mpsc::Sender<WorkerMessage>,
    shared_result: &Arc<Mutex<LastDigitResult>>,
    shared_processed: &Arc<Mutex<u64>>,
    result: &LastDigitResult,
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

/// 連続同一パターン（例: 11→?, 111→?）の統計。
#[derive(Debug, Default, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct ConsecutiveStats {
    /// このパターンが出現した回数（= 次の末尾を観測できた回数）
    pub occurrences: u64,
    /// 次に 1/3/7/9 が来た回数（[1,3,7,9] の順）
    pub next_counts: [u64; 4],
}

/// サイクルパターン（例: 1379→?）の統計。
#[derive(Debug, Default, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct CycleStats {
    /// このパターンが出現した回数（= 次の末尾を観測できた回数）
    pub occurrences: u64,
    /// 次に 1/3/7/9 が来た回数（[1,3,7,9] の順）
    pub next_counts: [u64; 4],
}

/// 末尾 1/3/7/9 の分析結果。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LastDigitResult {
    /// 末尾 1/3/7/9 の出現回数（[1,3,7,9] の順）
    pub counts: [u64; 4],
    /// 遷移行列: transition[from][to] = count
    pub transition_matrix: [[u64; 4]; 4],

    /// consecutive_same[digit_idx][depth]
    ///
    /// - digit_idx: 0=末尾1, 1=末尾3, 2=末尾7, 3=末尾9
    /// - depth: 0=単独(1→?), 1=2連続(11→?), 2=3連続(111→?), 3=4連続(1111→?)
    pub consecutive_same: [[ConsecutiveStats; 4]; 4],

    /// cycle_1379[digit_idx]
    ///
    /// - digit_idx=0: 1379→?
    /// - digit_idx=1: 3791→?
    /// - digit_idx=2: 7913→?
    /// - digit_idx=3: 9137→?
    pub cycle_1379: [CycleStats; 4],

    /// 2周サイクル後（例: 13791379->?）の次の末尾統計
    pub cycle_2repeat: [CycleStats; 4],

    /// 各末尾の最大連続回数（[1,3,7,9] の順）
    pub max_run: [u64; 4],

    /// pattern_4gram[256] = 4桁パターン(4進数エンコード)の出現回数
    pub pattern_4gram: Vec<u64>, // 256要素 (4^4)
}

impl Default for LastDigitResult {
    fn default() -> Self {
        Self {
            counts: [0u64; 4],
            transition_matrix: [[0u64; 4]; 4],
            consecutive_same: [[ConsecutiveStats::default(); 4]; 4],
            cycle_1379: [CycleStats::default(); 4],
            cycle_2repeat: [CycleStats::default(); 4],
            max_run: [0u64; 4],
            pattern_4gram: vec![0u64; 256],
        }
    }
}

/// バイナリ primes ファイル（`.bin`）から末尾 1/3/7/9 の出現回数を集計する。
///
/// - ファイル形式: little-endian `u64` の連続（8バイト/レコード）
/// - 集計対象: `p == 2` と `p == 5` は除外し、それ以外の素数の末尾（`p % 10`）をカウント
/// - `counts` は `[1, 3, 7, 9]` の順で返す
///
/// 戻り値: `(LastDigitResult, total_considered, processed_records)`
pub fn analyze_last_digits_binary_file(
    path: &Path,
    stop_flag: &AtomicBool,
    sender: &mpsc::Sender<WorkerMessage>,
    shared_result: &Arc<Mutex<LastDigitResult>>,
    shared_processed: &Arc<Mutex<u64>>,
) -> PrimeResult<(LastDigitResult, u64, u64)> {
    let (mut reader, total_records) = open_binary_primes_file(path)?;
    let mut buf = [0u8; 8];

    let mut result = LastDigitResult::default();
    let mut idx: u64 = 0;
    let mut skipped_2_5: u64 = 0;
    let mut unexpected_last_digit: u64 = 0;
    let mut prev_idx: Option<usize> = None;
    let mut history: [usize; 4] = [0usize; 4];
    let mut history_len: usize = 0;
    let mut history8: [usize; 8] = [0usize; 8];
    let mut history8_len: usize = 0;
    let mut run_digit: Option<usize> = None;
    let mut run_len: u64 = 0;

    while idx < total_records {
        if stop_flag.load(Ordering::SeqCst) {
            break;
        }

        let p = read_next_u64(&mut reader, &mut buf, idx)?;
        idx += 1;

        // 2 と 5 は例外なので、分布計算から除外する
        if p == 2 || p == 5 {
            skipped_2_5 += 1;
        } else {
            let cur = match p % 10 {
                1 => Some(0usize),
                3 => Some(1usize),
                7 => Some(2usize),
                9 => Some(3usize),
                _ => None,
            };

            if let Some(cur_i) = cur {
                // 連続同一パターン（1→?, 11→?, 111→?, 1111→?）
                if history_len > 0 {
                    let prev_last = history[history_len - 1];
                    let mut r = 1usize;
                    while r < history_len && history[history_len - 1 - r] == prev_last {
                        r += 1;
                    }
                    let r = r.min(4);
                    for k in 1..=r {
                        let s = &mut result.consecutive_same[prev_last][k - 1];
                        s.occurrences = s.occurrences.saturating_add(1);
                        s.next_counts[cur_i] = s.next_counts[cur_i].saturating_add(1);
                    }
                }

                // サイクルパターン（1379→?, 3791→?, 7913→?, 9137→?）
                if history_len == 4 {
                    let cycle_idx = match history {
                        [0, 1, 2, 3] => Some(0usize), // 1379→?
                        [1, 2, 3, 0] => Some(1usize), // 3791→?
                        [2, 3, 0, 1] => Some(2usize), // 7913→?
                        [3, 0, 1, 2] => Some(3usize), // 9137→?
                        _ => None,
                    };
                    if let Some(ci) = cycle_idx {
                        let s: &mut CycleStats = &mut result.cycle_1379[ci];
                        s.occurrences = s.occurrences.saturating_add(1);
                        s.next_counts[cur_i] = s.next_counts[cur_i].saturating_add(1);
                    }
                }

                // 遷移行列
                if let Some(prev_i) = prev_idx {
                    result.transition_matrix[prev_i][cur_i] =
                        result.transition_matrix[prev_i][cur_i].saturating_add(1);
                }
                prev_idx = Some(cur_i);

                // 出現回数
                result.counts[cur_i] = result.counts[cur_i].saturating_add(1);

                // 最大連続長
                if run_digit == Some(cur_i) {
                    run_len = run_len.saturating_add(1);
                } else {
                    run_digit = Some(cur_i);
                    run_len = 1;
                }
                if run_len > result.max_run[cur_i] {
                    result.max_run[cur_i] = run_len;
                }

                // 履歴更新（直前4つ）
                if history_len < 4 {
                    history[history_len] = cur_i;
                    history_len += 1;
                } else {
                    history[0] = history[1];
                    history[1] = history[2];
                    history[2] = history[3];
                    history[3] = cur_i;
                }

                // 2周サイクルパターン（13791379->?, 37913791->?, 79137913->?, 91379137->?）
                // 注: history8 更新の前にチェックすることで、cur_i が「次の数字」となる
                if history8_len == 8 {
                    let cycle_idx = match history8 {
                        [0, 1, 2, 3, 0, 1, 2, 3] => Some(0usize), // 13791379->?
                        [1, 2, 3, 0, 1, 2, 3, 0] => Some(1usize), // 37913791->?
                        [2, 3, 0, 1, 2, 3, 0, 1] => Some(2usize), // 79137913->?
                        [3, 0, 1, 2, 3, 0, 1, 2] => Some(3usize), // 91379137->?
                        _ => None,
                    };
                    if let Some(ci) = cycle_idx {
                        let s: &mut CycleStats = &mut result.cycle_2repeat[ci];
                        s.occurrences = s.occurrences.saturating_add(1);
                        s.next_counts[cur_i] = s.next_counts[cur_i].saturating_add(1);
                    }
                }

                // 履歴更新（直前8つ: cycle継続判定に使用）
                if history8_len < 8 {
                    history8[history8_len] = cur_i;
                    history8_len += 1;
                } else {
                    history8[0] = history8[1];
                    history8[1] = history8[2];
                    history8[2] = history8[3];
                    history8[3] = history8[4];
                    history8[4] = history8[5];
                    history8[5] = history8[6];
                    history8[6] = history8[7];
                    history8[7] = cur_i;
                }

                // 4gram（連続4つの末尾パターン）
                if history_len == 4 {
                    let k = encode_4gram(history[0], history[1], history[2], history[3]);
                    if let Some(slot) = result.pattern_4gram.get_mut(k) {
                        *slot = slot.saturating_add(1);
                    }
                }
            } else {
                // 異常系: 状態列が壊れるので履歴をリセット
                unexpected_last_digit += 1;
                prev_idx = None;
                history_len = 0;
                history8_len = 0;
                run_digit = None;
                run_len = 0;
            }
        }

        if idx % LOG_INTERVAL == 0 || idx == total_records {
            let total_considered = result.counts.iter().sum::<u64>();
            publish_realtime(
                idx,
                total_records,
                sender,
                shared_result,
                shared_processed,
                &result,
                total_considered,
            );
        }
    }

    // 最終 publish（停止でも完了でも一度送る）
    let total_considered = result.counts.iter().sum::<u64>();
    publish_realtime(
        idx,
        total_records,
        sender,
        shared_result,
        shared_processed,
        &result,
        total_considered,
    );

    if skipped_2_5 > 0 {
        sender
            .send(WorkerMessage::Log(format!(
                "Analyze: skipped {skipped_2_5} record(s) for p=2 or p=5 (excluded from distribution)."
            )))
            .ok();
    }
    if unexpected_last_digit > 0 {
        sender
            .send(WorkerMessage::Log(format!(
                "Analyze: found {unexpected_last_digit} record(s) with unexpected last digit (not 1/3/7/9)."
            )))
            .ok();
    }

    Ok((result, total_considered, idx))
}

fn label_primary(ui: &mut egui::Ui, text: impl ToString) {
    ui.label(
        egui::RichText::new(text.to_string())
            .size(font_sizes::BODY)
            .color(colors::TEXT_PRIMARY),
    );
}

fn label_secondary(ui: &mut egui::Ui, text: impl ToString) {
    ui.label(
        egui::RichText::new(text.to_string())
            .size(font_sizes::BODY)
            .color(colors::TEXT_SECONDARY),
    );
}

fn label_dash(ui: &mut egui::Ui) {
    label_secondary(ui, "—");
}

fn label_percent_primary(ui: &mut egui::Ui, pct: f64) {
    label_primary(ui, format!("{pct:.6}%"));
}

/// LastDigit タブの Markdown レポート（Copy/Save 用）。
pub fn format_last_digit_as_markdown(
    result: &LastDigitResult,
    total: u64,
    file_path: &str,
) -> String {
    let mut md = String::new();

    md.push_str("# Prime Last Digit Analysis\n\n");
    md.push_str(&format!("**File**: {}\n", file_path.trim()));
    md.push_str(&format!("**Total primes (excl. 2,5)**: {total}\n\n"));
    md.push_str("Note: This analysis excludes p=2 and p=5.\n\n");

    // --- Last Digit Distribution ---
    md.push_str("## Last Digit Distribution\n");
    md.push_str("| Digit | Count | % |\n");
    md.push_str("|-------|-------|---|\n");
    for (i, d) in [1u64, 3u64, 7u64, 9u64].iter().enumerate() {
        let c = result.counts[i];
        let pct = if total > 0 {
            (c as f64 / total as f64) * 100.0
        } else {
            0.0
        };
        md.push_str(&format!("| {d} | {c} | {pct:.6}% |\n"));
    }
    md.push('\n');

    // --- Transition Probability ---
    md.push_str("## Transition Probability (%)\n");
    md.push_str("| From\\To | 1 | 3 | 7 | 9 |\n");
    md.push_str("|---------|---|---|---|---|\n");
    let digits = [1u64, 3u64, 7u64, 9u64];
    let m = &result.transition_matrix;
    let row_sum: [u64; 4] = [
        m[0].iter().sum(),
        m[1].iter().sum(),
        m[2].iter().sum(),
        m[3].iter().sum(),
    ];
    for from in 0..4usize {
        md.push_str(&format!("| {} |", digits[from]));
        for to in 0..4usize {
            if row_sum[from] > 0 {
                let pct = (m[from][to] as f64 / row_sum[from] as f64) * 100.0;
                md.push_str(&format!(" {pct:.6}% |"));
            } else {
                md.push_str(" — |");
            }
        }
        md.push('\n');
    }
    md.push('\n');

    // --- Consecutive Same Digit ---
    md.push_str("## Consecutive Same Digit Probability (%)\n");
    md.push_str("| Pattern | ->1 | ->3 | ->7 | ->9 | N |\n");
    md.push_str("|---------|-----|-----|-----|-----|---|\n");
    for digit_idx in 0..4usize {
        let d = digits[digit_idx];
        for depth in 0..4usize {
            let pat = d.to_string().repeat(depth + 1);
            let s = result.consecutive_same[digit_idx][depth];
            md.push_str(&format!("| {pat}-> |"));
            for to in 0..4usize {
                if s.occurrences > 0 {
                    let pct = (s.next_counts[to] as f64 / s.occurrences as f64) * 100.0;
                    md.push_str(&format!(" {pct:.6}% |"));
                } else {
                    md.push_str(" — |");
                }
            }
            md.push_str(&format!(" {} |\n", s.occurrences));
        }
    }
    md.push('\n');

    // --- Cycle Pattern Probability ---
    md.push_str("## Cycle Pattern Probability (%)\n");
    md.push_str("| Pattern | Expected | Actual | Δ vs baseline | N |\n");
    md.push_str("|---------|----------|--------|---------------|---|\n");
    for i in 0..4usize {
        let pat = format!(
            "{}{}{}{}->",
            digits[i],
            digits[(i + 1) % 4],
            digits[(i + 2) % 4],
            digits[(i + 3) % 4]
        );
        let s = result.cycle_1379[i];
        let last_idx = (i + 3) % 4;
        let m = &result.transition_matrix;
        let row_sum = m[last_idx].iter().sum::<u64>();
        let baseline = if row_sum > 0 {
            (m[last_idx][i] as f64 / row_sum as f64) * 100.0
        } else {
            25.0
        };
        let expected = format!(
            "{} ({}->{}: {baseline:.6}%)",
            digits[i], digits[last_idx], digits[i]
        );
        if s.occurrences > 0 {
            let actual = (s.next_counts[i] as f64 / s.occurrences as f64) * 100.0;
            let delta = actual - baseline;
            md.push_str(&format!(
                "| {pat} | {expected} | {actual:.6}% | {delta:+.6}% | {} |\n",
                s.occurrences
            ));
        } else {
            md.push_str(&format!(
                "| {pat} | {expected} | — | — | {} |\n",
                s.occurrences
            ));
        }
    }
    md.push('\n');

    // --- Cycle Repeat Comparison ---
    md.push_str("## Cycle Repeat Comparison (1x vs 2x)\n");
    md.push_str("| Pattern | ->1 | ->3 | ->7 | ->9 | N |\n");
    md.push_str("|---------|-----|-----|-----|-----|---|\n");
    for ci in 0..4usize {
        let pat = format!(
            "{}{}{}{}->",
            digits[ci],
            digits[(ci + 1) % 4],
            digits[(ci + 2) % 4],
            digits[(ci + 3) % 4]
        );
        let s = result.cycle_1379[ci];
        md.push_str(&format!("| {pat} |"));
        for to in 0..4usize {
            if s.occurrences > 0 {
                let pct = (s.next_counts[to] as f64 / s.occurrences as f64) * 100.0;
                md.push_str(&format!(" {pct:.6}% |"));
            } else {
                md.push_str(" — |");
            }
        }
        md.push_str(&format!(" {} |\n", s.occurrences));
    }
    for ci in 0..4usize {
        let pat = format!(
            "{}{}{}{}{}{}{}{}->",
            digits[ci],
            digits[(ci + 1) % 4],
            digits[(ci + 2) % 4],
            digits[(ci + 3) % 4],
            digits[ci],
            digits[(ci + 1) % 4],
            digits[(ci + 2) % 4],
            digits[(ci + 3) % 4]
        );
        let s = result.cycle_2repeat[ci];
        md.push_str(&format!("| {pat} |"));
        for to in 0..4usize {
            if s.occurrences > 0 {
                let pct = (s.next_counts[to] as f64 / s.occurrences as f64) * 100.0;
                md.push_str(&format!(" {pct:.6}% |"));
            } else {
                md.push_str(" — |");
            }
        }
        md.push_str(&format!(" {} |\n", s.occurrences));
    }
    md.push('\n');

    // --- Maximum Run Length ---
    md.push_str("## Maximum Run Length\n");
    md.push_str("| Digit | Max Run |\n");
    md.push_str("|-------|---------|\n");
    for i in 0..4usize {
        md.push_str(&format!("| {} | {} |\n", digits[i], result.max_run[i]));
    }
    md.push('\n');

    // --- 4-Gram Pattern Ranking ---
    md.push_str("## 4-Gram Pattern Ranking\n");
    let mut entries: Vec<(usize, u64)> = result
        .pattern_4gram
        .iter()
        .enumerate()
        .filter_map(|(i, &c)| (c > 0).then_some((i, c)))
        .collect();
    entries.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let decode = |idx: usize| -> String {
        let a = (idx / 64) % 4;
        let b = (idx / 16) % 4;
        let c = (idx / 4) % 4;
        let d = idx % 4;
        format!("{}{}{}{}", digits[a], digits[b], digits[c], digits[d])
    };

    md.push_str("| Top 5 | Count | Bottom 5 | Count |\n");
    md.push_str("|-------|-------|----------|-------|\n");
    for i in 0..5usize {
        let top = entries.get(i).copied();
        let bottom = entries.get(entries.len().saturating_sub(1 + i)).copied();

        match top {
            Some((k, c)) => md.push_str(&format!("| {} | {} |", decode(k), c)),
            None => md.push_str("| — | — |"),
        }
        match bottom {
            Some((k, c)) => md.push_str(&format!(" {} | {} |\n", decode(k), c)),
            None => md.push_str(" — | — |\n"),
        }
    }

    md
}

/// LastDigit タブの UI 描画（results 部分）。
pub fn render_last_digit_results_ui(ui: &mut egui::Ui, result: &LastDigitResult, total: u64) {
    render_last_digit_section(ui, result, total);
    ui.add_space(20.0);
    render_transition_section(ui, result);
    ui.add_space(20.0);
    render_consecutive_section(ui, result);
    ui.add_space(20.0);
    render_cycle_section(ui, result);
    ui.add_space(20.0);
    render_max_run_section(ui, result);
    ui.add_space(20.0);
    render_pattern_ranking_section(ui, result);
}

fn render_last_digit_section(ui: &mut egui::Ui, result: &LastDigitResult, total: u64) {
    ui.label(section_title("Last Digit Distribution"));
    ui.add_space(12.0);

    let digits = [1u64, 3u64, 7u64, 9u64];

    egui::Grid::new("analyze_last_digit_grid")
        .striped(true)
        .spacing([16.0, 8.0])
        .show(ui, |ui| {
            ui.label(field_label("Digit"));
            ui.label(field_label("Count"));
            ui.label(field_label("%"));
            ui.end_row();

            for (i, d) in digits.iter().enumerate() {
                let c = result.counts[i];
                let pct = if total > 0 {
                    (c as f64 / total as f64) * 100.0
                } else {
                    0.0
                };
                label_primary(ui, d);
                label_primary(ui, c);
                label_percent_primary(ui, pct);
                ui.end_row();
            }

            label_secondary(ui, "Total");
            label_secondary(ui, total);
            if total > 0 {
                label_secondary(ui, "100.000000%");
            } else {
                label_dash(ui);
            }
            ui.end_row();
        });

    ui.add_space(12.0);
    ui.label(
        egui::RichText::new("Note: This distribution excludes p=2 and p=5.")
            .size(font_sizes::LABEL)
            .color(colors::TEXT_SECONDARY),
    );
}

fn render_transition_section(ui: &mut egui::Ui, result: &LastDigitResult) {
    ui.label(section_title("Transition Probability (%)"));
    ui.add_space(12.0);

    let digits = [1u64, 3u64, 7u64, 9u64];
    let m = &result.transition_matrix;
    let row_sum: [u64; 4] = [
        m[0].iter().sum(),
        m[1].iter().sum(),
        m[2].iter().sum(),
        m[3].iter().sum(),
    ];

    egui::Grid::new("analyze_transition_grid")
        .striped(true)
        .spacing([16.0, 8.0])
        .show(ui, |ui| {
            ui.label(field_label("From\\To"));
            for d in digits {
                ui.label(field_label(&format!("{d}")));
            }
            ui.end_row();

            for from in 0..4usize {
                label_primary(ui, digits[from]);

                for to in 0..4usize {
                    let val = m[from][to];
                    if row_sum[from] > 0 {
                        let pct = (val as f64 / row_sum[from] as f64) * 100.0;
                        label_percent_primary(ui, pct);
                    } else {
                        label_dash(ui);
                    }
                }
                ui.end_row();
            }
        });
}

fn render_consecutive_section(ui: &mut egui::Ui, result: &LastDigitResult) {
    ui.label(section_title("Consecutive Same Digit Probability (%)"));
    ui.add_space(12.0);

    let digits = [1u64, 3u64, 7u64, 9u64];

    egui::Grid::new("analyze_consecutive_grid")
        .striped(true)
        .spacing([16.0, 8.0])
        .show(ui, |ui| {
            ui.label(field_label("Pattern"));
            for d in digits {
                ui.label(field_label(&format!("->{d}")));
            }
            ui.label(field_label("N"));
            ui.end_row();

            for digit_idx in 0..4usize {
                let d = digits[digit_idx];
                for depth in 0..4usize {
                    let pat = d.to_string().repeat(depth + 1);
                    let s = result.consecutive_same[digit_idx][depth];
                    ui.label(
                        egui::RichText::new(format!("{pat}->"))
                            .size(font_sizes::BODY)
                            .color(colors::TEXT_PRIMARY),
                    );
                    for to in 0..4usize {
                        if s.occurrences > 0 {
                            let pct = (s.next_counts[to] as f64 / s.occurrences as f64) * 100.0;
                            ui.label(
                                egui::RichText::new(format!("{pct:.6}%"))
                                    .size(font_sizes::BODY)
                                    .color(colors::TEXT_PRIMARY),
                            );
                        } else {
                            ui.label(
                                egui::RichText::new("—")
                                    .size(font_sizes::BODY)
                                    .color(colors::TEXT_SECONDARY),
                            );
                        }
                    }
                    ui.label(
                        egui::RichText::new(format!("{}", s.occurrences))
                            .size(font_sizes::BODY)
                            .color(colors::TEXT_SECONDARY),
                    );
                    ui.end_row();
                }
            }
        });
}

fn render_cycle_section(ui: &mut egui::Ui, result: &LastDigitResult) {
    ui.label(section_title("Cycle Pattern Probability (%)"));
    ui.add_space(12.0);

    let digits = [1u64, 3u64, 7u64, 9u64];
    let m = &result.transition_matrix;
    let row_sum: [u64; 4] = [
        m[0].iter().sum(),
        m[1].iter().sum(),
        m[2].iter().sum(),
        m[3].iter().sum(),
    ];

    egui::Grid::new("analyze_cycle_grid")
        .striped(true)
        .spacing([16.0, 8.0])
        .show(ui, |ui| {
            ui.label(field_label("Pattern"));
            ui.label(field_label("Expected"));
            ui.label(field_label("Actual"));
            ui.label(field_label("Δ vs baseline"));
            ui.label(field_label("N"));
            ui.end_row();

            for i in 0..4usize {
                let pat = format!(
                    "{}{}{}{}->",
                    digits[i],
                    digits[(i + 1) % 4],
                    digits[(i + 2) % 4],
                    digits[(i + 3) % 4],
                );
                let s = result.cycle_1379[i];
                let last_idx = (i + 3) % 4;
                let baseline = if row_sum[last_idx] > 0 {
                    (m[last_idx][i] as f64 / row_sum[last_idx] as f64) * 100.0
                } else {
                    25.0
                };
                ui.label(
                    egui::RichText::new(pat)
                        .size(font_sizes::BODY)
                        .color(colors::TEXT_PRIMARY),
                );
                ui.label(
                    egui::RichText::new(format!(
                        "{} ({}->{}: {baseline:.6}%)",
                        digits[i], digits[last_idx], digits[i]
                    ))
                    .size(font_sizes::BODY)
                    .color(colors::TEXT_SECONDARY),
                );

                if s.occurrences > 0 {
                    let actual = (s.next_counts[i] as f64 / s.occurrences as f64) * 100.0;
                    let delta = actual - baseline;
                    ui.label(
                        egui::RichText::new(format!("{actual:.6}%"))
                            .size(font_sizes::BODY)
                            .color(colors::TEXT_PRIMARY),
                    );
                    ui.label(
                        egui::RichText::new(format!("{delta:+.6}%"))
                            .size(font_sizes::BODY)
                            .color(colors::TEXT_PRIMARY),
                    );
                } else {
                    ui.label(
                        egui::RichText::new("—")
                            .size(font_sizes::BODY)
                            .color(colors::TEXT_SECONDARY),
                    );
                    ui.label(
                        egui::RichText::new("—")
                            .size(font_sizes::BODY)
                            .color(colors::TEXT_SECONDARY),
                    );
                }

                ui.label(
                    egui::RichText::new(format!("{}", s.occurrences))
                        .size(font_sizes::BODY)
                        .color(colors::TEXT_SECONDARY),
                );
                ui.end_row();
            }
        });

    // 2周サイクルとの比較
    ui.add_space(16.0);
    ui.label(section_title("Cycle Repeat Comparison (1x vs 2x)"));
    ui.add_space(12.0);

    egui::Grid::new("analyze_cycle_repeat_grid")
        .striped(true)
        .spacing([16.0, 8.0])
        .show(ui, |ui| {
            ui.label(field_label("Pattern"));
            for d in digits {
                ui.label(field_label(&format!("->{d}")));
            }
            ui.label(field_label("N"));
            ui.end_row();

            // 1周サイクル
            for ci in 0..4usize {
                let pat = format!(
                    "{}{}{}{}->",
                    digits[ci],
                    digits[(ci + 1) % 4],
                    digits[(ci + 2) % 4],
                    digits[(ci + 3) % 4],
                );
                let s = result.cycle_1379[ci];
                ui.label(
                    egui::RichText::new(pat)
                        .size(font_sizes::BODY)
                        .color(colors::TEXT_PRIMARY),
                );
                for to in 0..4usize {
                    if s.occurrences > 0 {
                        let pct = (s.next_counts[to] as f64 / s.occurrences as f64) * 100.0;
                        ui.label(
                            egui::RichText::new(format!("{pct:.6}%"))
                                .size(font_sizes::BODY)
                                .color(colors::TEXT_PRIMARY),
                        );
                    } else {
                        ui.label(
                            egui::RichText::new("—")
                                .size(font_sizes::BODY)
                                .color(colors::TEXT_SECONDARY),
                        );
                    }
                }
                ui.label(
                    egui::RichText::new(format!("{}", s.occurrences))
                        .size(font_sizes::BODY)
                        .color(colors::TEXT_SECONDARY),
                );
                ui.end_row();
            }

            // 区切り行（空行）
            ui.label(egui::RichText::new("").size(4.0));
            ui.end_row();

            // 2周サイクル
            for ci in 0..4usize {
                let pat = format!(
                    "{}{}{}{}{}{}{}{}->",
                    digits[ci],
                    digits[(ci + 1) % 4],
                    digits[(ci + 2) % 4],
                    digits[(ci + 3) % 4],
                    digits[ci],
                    digits[(ci + 1) % 4],
                    digits[(ci + 2) % 4],
                    digits[(ci + 3) % 4],
                );
                let s = result.cycle_2repeat[ci];
                ui.label(
                    egui::RichText::new(pat)
                        .size(font_sizes::BODY)
                        .color(colors::TEXT_PRIMARY),
                );
                for to in 0..4usize {
                    if s.occurrences > 0 {
                        let pct = (s.next_counts[to] as f64 / s.occurrences as f64) * 100.0;
                        ui.label(
                            egui::RichText::new(format!("{pct:.6}%"))
                                .size(font_sizes::BODY)
                                .color(colors::TEXT_PRIMARY),
                        );
                    } else {
                        ui.label(
                            egui::RichText::new("—")
                                .size(font_sizes::BODY)
                                .color(colors::TEXT_SECONDARY),
                        );
                    }
                }
                ui.label(
                    egui::RichText::new(format!("{}", s.occurrences))
                        .size(font_sizes::BODY)
                        .color(colors::TEXT_SECONDARY),
                );
                ui.end_row();
            }
        });
}

fn render_max_run_section(ui: &mut egui::Ui, result: &LastDigitResult) {
    ui.label(section_title("Maximum Run Length"));
    ui.add_space(12.0);

    let digits = [1u64, 3u64, 7u64, 9u64];
    egui::Grid::new("analyze_max_run_grid")
        .striped(true)
        .spacing([16.0, 8.0])
        .show(ui, |ui| {
            ui.label(field_label("Digit"));
            ui.label(field_label("Max Run"));
            ui.end_row();

            for i in 0..4usize {
                ui.label(
                    egui::RichText::new(format!("{}", digits[i]))
                        .size(font_sizes::BODY)
                        .color(colors::TEXT_PRIMARY),
                );
                ui.label(
                    egui::RichText::new(format!("{}", result.max_run[i]))
                        .size(font_sizes::BODY)
                        .color(colors::TEXT_PRIMARY),
                );
                ui.end_row();
            }
        });
}

fn render_pattern_ranking_section(ui: &mut egui::Ui, result: &LastDigitResult) {
    ui.label(section_title("4-Gram Pattern Ranking"));
    ui.add_space(12.0);

    let digits = [1u64, 3u64, 7u64, 9u64];

    let mut entries: Vec<(usize, u64)> = result
        .pattern_4gram
        .iter()
        .enumerate()
        .filter_map(|(i, &c)| (c > 0).then_some((i, c)))
        .collect();

    entries.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let top: Vec<(usize, u64)> = entries.iter().take(5).copied().collect();
    let bottom: Vec<(usize, u64)> = entries.iter().rev().take(5).copied().collect();

    let decode = |idx: usize| -> String {
        let a = (idx / 64) % 4;
        let b = (idx / 16) % 4;
        let c = (idx / 4) % 4;
        let d = idx % 4;
        format!("{}{}{}{}", digits[a], digits[b], digits[c], digits[d])
    };

    egui::Grid::new("analyze_4gram_ranking_grid")
        .striped(true)
        .spacing([16.0, 8.0])
        .show(ui, |ui| {
            ui.label(field_label("Top 5"));
            ui.label(field_label("Count"));
            ui.label(field_label("Bottom 5"));
            ui.label(field_label("Count"));
            ui.end_row();

            for i in 0..5usize {
                if let Some((k, c)) = top.get(i) {
                    ui.label(
                        egui::RichText::new(decode(*k))
                            .size(font_sizes::BODY)
                            .color(colors::TEXT_PRIMARY),
                    );
                    ui.label(
                        egui::RichText::new(format!("{c}"))
                            .size(font_sizes::BODY)
                            .color(colors::TEXT_PRIMARY),
                    );
                } else {
                    ui.label(
                        egui::RichText::new("—")
                            .size(font_sizes::BODY)
                            .color(colors::TEXT_SECONDARY),
                    );
                    ui.label(
                        egui::RichText::new("—")
                            .size(font_sizes::BODY)
                            .color(colors::TEXT_SECONDARY),
                    );
                }

                if let Some((k, c)) = bottom.get(i) {
                    ui.label(
                        egui::RichText::new(decode(*k))
                            .size(font_sizes::BODY)
                            .color(colors::TEXT_PRIMARY),
                    );
                    ui.label(
                        egui::RichText::new(format!("{c}"))
                            .size(font_sizes::BODY)
                            .color(colors::TEXT_PRIMARY),
                    );
                } else {
                    ui.label(
                        egui::RichText::new("—")
                            .size(font_sizes::BODY)
                            .color(colors::TEXT_SECONDARY),
                    );
                    ui.label(
                        egui::RichText::new("—")
                            .size(font_sizes::BODY)
                            .color(colors::TEXT_SECONDARY),
                    );
                }

                ui.end_row();
            }
        });
}
