//! Analyze Tab: Mod30 (distribution + transitions)
//!
//! - types + engine + markdown + UI をこのファイルに集約する。

#![allow(clippy::needless_range_loop)]

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use eframe::egui;

use crate::analyze::MOD30_RESIDUES;
use crate::engine_types::PrimeResult;
use crate::ui_components::{field_label, section_title};
use crate::ui_theme::{colors, font_sizes};
use crate::worker_message::WorkerMessage;

const READER_CAPACITY_BYTES: usize = 8 * 1024 * 1024;
const LOG_INTERVAL: u64 = 1_000_000;

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
    shared_result: &Arc<Mutex<Mod30Result>>,
    shared_processed: &Arc<Mutex<u64>>,
    result: &Mod30Result,
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

/// mod 30 用: 連続同一パターン統計（8状態版）。
#[derive(Debug, Default, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct ConsecutiveStats8 {
    pub occurrences: u64,
    pub next_counts: [u64; 8],
}

/// mod 30 用: サイクルパターン統計（8状態版）。
#[derive(Debug, Default, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct CycleStats8 {
    pub occurrences: u64,
    pub next_counts: [u64; 8],
}

/// mod 30（{1,7,11,13,17,19,23,29}）の分析結果。
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct Mod30Result {
    /// 出現回数（[MOD30_RESIDUES] の順）
    pub counts: [u64; 8],
    /// 遷移行列: transition[from][to] = count
    pub transition_matrix: [[u64; 8]; 8],
    /// consecutive_same[residue_idx][depth]
    ///
    /// - depth: 0=単独(x1→?), 1=2連続(x2→?), 2=3連続(x3→?), 3=4連続(x4→?)
    pub consecutive_same: [[ConsecutiveStats8; 4]; 8],
    /// サイクル（8種類の回転）: cycle_8[i] は
    /// `MOD30_RESIDUES[i..] + MOD30_RESIDUES[..i]` が出たときの次の分布
    pub cycle_8: [CycleStats8; 8],
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

/// バイナリ primes ファイル（`.bin`）から mod 30（{1,7,11,13,17,19,23,29}）の統計を集計する。
///
/// - 集計対象: `p == 2` と `p == 3` と `p == 5` は除外（剰余類 8 種類に入らないため）
///
/// 戻り値: `(Mod30Result, total_considered, processed_records)`
pub fn analyze_mod30_binary_file(
    path: &Path,
    stop_flag: &AtomicBool,
    sender: &mpsc::Sender<WorkerMessage>,
    shared_result: &Arc<Mutex<Mod30Result>>,
    shared_processed: &Arc<Mutex<u64>>,
) -> PrimeResult<(Mod30Result, u64, u64)> {
    let (mut reader, total_records) = open_binary_primes_file(path)?;
    let mut buf = [0u8; 8];

    let mut result = Mod30Result::default();
    let mut idx: u64 = 0;
    let mut skipped_2_3_5: u64 = 0;
    let mut unexpected_residue: u64 = 0;
    let mut prev_idx: Option<usize> = None;
    let mut history: [usize; 4] = [0usize; 4];
    let mut history_len: usize = 0;
    let mut history8: [usize; 8] = [0usize; 8];
    let mut history8_len: usize = 0;

    while idx < total_records {
        if stop_flag.load(Ordering::SeqCst) {
            break;
        }

        let p = read_next_u64(&mut reader, &mut buf, idx)?;
        idx += 1;

        // 2, 3, 5 は剰余類 8 種類から外れるので除外する
        if p == 2 || p == 3 || p == 5 {
            skipped_2_3_5 += 1;
        } else {
            let cur = mod30_residue_to_index(p % 30);
            if let Some(cur_i) = cur {
                // 連続同一パターン（x1→?, x2→?, x3→?, x4→?）
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

                // サイクルパターン（8回転）
                // 注: history8 更新の前にチェックすることで、cur_i が「次の剰余」となる
                if history8_len == 8 {
                    let cycle_idx = match history8 {
                        [0, 1, 2, 3, 4, 5, 6, 7] => Some(0usize),
                        [1, 2, 3, 4, 5, 6, 7, 0] => Some(1usize),
                        [2, 3, 4, 5, 6, 7, 0, 1] => Some(2usize),
                        [3, 4, 5, 6, 7, 0, 1, 2] => Some(3usize),
                        [4, 5, 6, 7, 0, 1, 2, 3] => Some(4usize),
                        [5, 6, 7, 0, 1, 2, 3, 4] => Some(5usize),
                        [6, 7, 0, 1, 2, 3, 4, 5] => Some(6usize),
                        [7, 0, 1, 2, 3, 4, 5, 6] => Some(7usize),
                        _ => None,
                    };
                    if let Some(ci) = cycle_idx {
                        let s: &mut CycleStats8 = &mut result.cycle_8[ci];
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

                // 履歴更新（直前8つ）
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
            } else {
                // 異常系: 状態列が壊れるので履歴をリセット
                unexpected_residue += 1;
                prev_idx = None;
                history_len = 0;
                history8_len = 0;
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

    // 最終 publish
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

    if skipped_2_3_5 > 0 {
        sender
            .send(WorkerMessage::Log(format!(
                "Analyze(Mod30): skipped {skipped_2_3_5} record(s) for p=2,3,5 (excluded from mod30 distribution)."
            )))
            .ok();
    }
    if unexpected_residue > 0 {
        sender
            .send(WorkerMessage::Log(format!(
                "Analyze(Mod30): found {unexpected_residue} record(s) with unexpected p%30 (not in residues set)."
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

/// Mod30 タブの Markdown レポート（Copy/Save 用）。
pub fn format_mod30_as_markdown(result: &Mod30Result, total: u64, file_path: &str) -> String {
    let mut md = String::new();

    md.push_str("# Prime Mod30 Analysis\n\n");
    md.push_str(&format!("**File**: {}\n", file_path.trim()));
    md.push_str(&format!("**Total primes (excl. 2,3,5)**: {total}\n\n"));
    md.push_str("Note: This analysis excludes p=2, p=3, and p=5.\n\n");

    // --- Distribution ---
    md.push_str("## Mod30 Residue Distribution\n");
    md.push_str("| Residue (mod 30) | Count | % |\n");
    md.push_str("|------------------|-------|---|\n");
    for (i, r) in MOD30_RESIDUES.iter().enumerate() {
        let c = result.counts[i];
        let pct = if total > 0 {
            (c as f64 / total as f64) * 100.0
        } else {
            0.0
        };
        md.push_str(&format!("| {r} | {c} | {pct:.6}% |\n"));
    }
    md.push('\n');

    // --- Transition ---
    md.push_str("## Transition Probability (%)\n");
    md.push_str("| From\\To | 1 | 7 | 11 | 13 | 17 | 19 | 23 | 29 |\n");
    md.push_str("|---------|---|---|----|----|----|----|----|----|\n");
    let m = &result.transition_matrix;
    let mut row_sum = [0u64; 8];
    for from in 0..8usize {
        row_sum[from] = m[from].iter().sum();
    }
    for from in 0..8usize {
        md.push_str(&format!("| {} |", MOD30_RESIDUES[from]));
        for to in 0..8usize {
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

    // --- Consecutive Same ---
    md.push_str("## Consecutive Same Residue Probability (%)\n");
    md.push_str("| Pattern | ->1 | ->7 | ->11 | ->13 | ->17 | ->19 | ->23 | ->29 | N |\n");
    md.push_str("|---------|-----|-----|------|------|------|------|------|------|---|\n");
    for residue_idx in 0..8usize {
        let r = MOD30_RESIDUES[residue_idx];
        for depth in 0..4usize {
            let pat = format!("{r}x{}->", depth + 1);
            let s = result.consecutive_same[residue_idx][depth];
            md.push_str(&format!("| {pat} |"));
            for to in 0..8usize {
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

    // --- Cycle Pattern ---
    md.push_str("## Cycle Pattern Probability (%)\n");
    md.push_str("| Pattern | Expected | Actual | Δ vs baseline | N |\n");
    md.push_str("|---------|----------|--------|---------------|---|\n");
    for i in 0..8usize {
        let mut pat = String::new();
        for j in 0..8usize {
            if j > 0 {
                pat.push('-');
            }
            pat.push_str(&format!("{}", MOD30_RESIDUES[(i + j) % 8]));
        }
        pat.push_str("->");
        let s = result.cycle_8[i];
        let last_idx = (i + 7) % 8;
        let m = &result.transition_matrix;
        let row_sum = m[last_idx].iter().sum::<u64>();
        let baseline = if row_sum > 0 {
            (m[last_idx][i] as f64 / row_sum as f64) * 100.0
        } else {
            12.5
        };
        let expected = format!(
            "{} ({}->{}: {baseline:.6}%)",
            MOD30_RESIDUES[i], MOD30_RESIDUES[last_idx], MOD30_RESIDUES[i]
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

    md
}

/// Mod30 タブの UI 描画（results 部分）。
pub fn render_mod30_results_ui(ui: &mut egui::Ui, result: &Mod30Result, total: u64) {
    render_mod30_distribution_section(ui, result, total);
    ui.add_space(20.0);
    render_mod30_transition_section(ui, result);
    ui.add_space(20.0);
    render_mod30_consecutive_section(ui, result);
    ui.add_space(20.0);
    render_mod30_cycle_section(ui, result);
}

fn render_mod30_distribution_section(ui: &mut egui::Ui, result: &Mod30Result, total: u64) {
    ui.label(section_title("Mod30 Residue Distribution"));
    ui.add_space(12.0);

    egui::Grid::new("analyze_mod30_dist_grid")
        .striped(true)
        .spacing([16.0, 8.0])
        .show(ui, |ui| {
            ui.label(field_label("Residue"));
            ui.label(field_label("Count"));
            ui.label(field_label("%"));
            ui.end_row();

            for (i, r) in MOD30_RESIDUES.iter().enumerate() {
                let c = result.counts[i];
                let pct = if total > 0 {
                    (c as f64 / total as f64) * 100.0
                } else {
                    0.0
                };
                label_primary(ui, r);
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
}

fn render_mod30_transition_section(ui: &mut egui::Ui, result: &Mod30Result) {
    ui.label(section_title("Transition Probability (%)"));
    ui.add_space(12.0);

    let m = &result.transition_matrix;
    let mut row_sum = [0u64; 8];
    for from in 0..8usize {
        row_sum[from] = m[from].iter().sum();
    }

    egui::ScrollArea::horizontal()
        .id_salt("analyze_mod30_transition_scroll_x")
        .show(ui, |ui| {
            egui::Grid::new("analyze_mod30_transition_grid")
                .striped(true)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    ui.label(field_label("From\\To"));
                    for r in MOD30_RESIDUES {
                        ui.label(field_label(&format!("{r}")));
                    }
                    ui.end_row();

                    for from in 0..8usize {
                        label_primary(ui, MOD30_RESIDUES[from]);
                        for to in 0..8usize {
                            if row_sum[from] > 0 {
                                let pct = (m[from][to] as f64 / row_sum[from] as f64) * 100.0;
                                label_percent_primary(ui, pct);
                            } else {
                                label_dash(ui);
                            }
                        }
                        ui.end_row();
                    }
                });
        });
}

fn render_mod30_consecutive_section(ui: &mut egui::Ui, result: &Mod30Result) {
    ui.label(section_title("Consecutive Same Residue Probability (%)"));
    ui.add_space(12.0);

    egui::ScrollArea::horizontal()
        .id_salt("analyze_mod30_consecutive_scroll_x")
        .show(ui, |ui| {
            egui::Grid::new("analyze_mod30_consecutive_grid")
                .striped(true)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    ui.label(field_label("Pattern"));
                    for r in MOD30_RESIDUES {
                        ui.label(field_label(&format!("->{r}")));
                    }
                    ui.label(field_label("N"));
                    ui.end_row();

                    for residue_idx in 0..8usize {
                        let r = MOD30_RESIDUES[residue_idx];
                        for depth in 0..4usize {
                            let pat = format!("{r}x{}->", depth + 1);
                            let s = result.consecutive_same[residue_idx][depth];
                            ui.label(
                                egui::RichText::new(pat)
                                    .size(font_sizes::BODY)
                                    .color(colors::TEXT_PRIMARY),
                            );
                            for to in 0..8usize {
                                if s.occurrences > 0 {
                                    let pct =
                                        (s.next_counts[to] as f64 / s.occurrences as f64) * 100.0;
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
        });
}

fn render_mod30_cycle_section(ui: &mut egui::Ui, result: &Mod30Result) {
    ui.label(section_title("Cycle Pattern Probability (%)"));
    ui.add_space(12.0);

    let m = &result.transition_matrix;
    let mut row_sum = [0u64; 8];
    for from in 0..8usize {
        row_sum[from] = m[from].iter().sum();
    }

    egui::Grid::new("analyze_mod30_cycle_grid")
        .striped(true)
        .spacing([16.0, 8.0])
        .show(ui, |ui| {
            ui.label(field_label("Pattern"));
            ui.label(field_label("Expected"));
            ui.label(field_label("Actual"));
            ui.label(field_label("Δ vs baseline"));
            ui.label(field_label("N"));
            ui.end_row();

            for i in 0..8usize {
                let mut pat = String::new();
                for j in 0..8usize {
                    if j > 0 {
                        pat.push('-');
                    }
                    pat.push_str(&format!("{}", MOD30_RESIDUES[(i + j) % 8]));
                }
                pat.push_str("->");
                let s = result.cycle_8[i];
                let last_idx = (i + 7) % 8;
                let baseline = if row_sum[last_idx] > 0 {
                    (m[last_idx][i] as f64 / row_sum[last_idx] as f64) * 100.0
                } else {
                    12.5
                };
                ui.label(
                    egui::RichText::new(pat)
                        .size(font_sizes::BODY)
                        .color(colors::TEXT_PRIMARY),
                );
                ui.label(
                    egui::RichText::new(format!(
                        "{} ({}->{}: {baseline:.6}%)",
                        MOD30_RESIDUES[i], MOD30_RESIDUES[last_idx], MOD30_RESIDUES[i]
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
}
