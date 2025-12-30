#![allow(clippy::needless_range_loop)]
#![allow(clippy::too_many_arguments)]

use eframe::egui;
use serde_json::json;

use crate::analyze::tab_prhs::{PRHSBinStats, PRHSResult};
use crate::analyze::ui_analyze::{label_primary, try_read_shared};
use crate::analyze::{PRHSBinMode, MOD30_RESIDUES};
use crate::app_state::AnalyzeState;
use crate::ui_components::{field_label, section_title, ZoomPanState};
use crate::ui_graph_utils::{
    compute_graph_rect, draw_axes, draw_legend, draw_polyline, AxisLabels, GraphMargins, LegendItem,
};
use crate::ui_theme::{colors, font_sizes};

pub(crate) fn format_prhs_as_markdown(
    result: &PRHSResult,
    total_triplets: u64,
    file_path: &str,
    view_mode: PRHSBinMode,
    prhs_log10_bin_width: f64,
    prhs_equal_bin_primes: u64,
    prhs_train_ratio: f64,
    exclude_diagonal: bool,
    min_nij: u64,
) -> String {
    let mut md = String::new();

    md.push_str("# mod 30 PRHS\n");
    md.push_str("Prime Residue History Study\n\n");
    md.push_str(&format!("**File**: {}\n", file_path.trim()));
    md.push_str(&format!(
        "**Total samples (triplets)**: {total_triplets}\n\n"
    ));

    // --- Summary (human-friendly) ---
    let verdict = if result.global.delta_ll > 0.0 {
        "PASS"
    } else {
        "WARN"
    };
    let interpretation = if result.global.delta_ll > 0.0 {
        "DeltaLL>0: history-aware model (M2) is more predictive. Detects prime history dependence (known phenomenon per Lemke Oliver-Soundararajan, 2016)."
    } else {
        "DeltaLL<=0: history effect is weak or unclear (sign may fluctuate depending on range/sample size)."
    };
    md.push_str("## Summary\n");
    md.push_str(&format!("- Verdict: {verdict}\n"));
    md.push_str(&format!(
        "- Key finding: ΔLL={:+.6} bits, CMI={:.6} nats (holdout)\n",
        result.global.delta_ll, result.global.cmi
    ));
    md.push_str(&format!("- Interpretation: {interpretation}\n\n"));

    md.push_str("## Methodology\n");
    md.push_str("- **Dataset**: primes p in the file, excluding p=2,3,5; states are residues mod 30 mapped to 8 indices.\n");
    md.push_str(&format!(
        "- **State mapping**: S = {{{}}} (mod 30), idx=0..7 in that order.\n",
        MOD30_RESIDUES
            .iter()
            .map(|r| r.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ));
    md.push_str("- **Sample definition**: each sample is a triplet (X_{n-1}, X_n, X_{n+1}) over the filtered prime sequence.\n");
    md.push_str(&format!(
        "- **Holdout (global)**: by file position (1-pass). Train = first {prhs_train_ratio:.3}, Test = last {:.3}.\n",
        (1.0 - prhs_train_ratio).max(0.0)
    ));
    md.push_str("- **Holdout (bins)**: in-bin split by bin-local sample counter (every 5th triplet is test, ~20%).\n");
    md.push_str(&format!(
        "- **Binning params**: log10 bin width={prhs_log10_bin_width:.6}, equal-count bin primes={prhs_equal_bin_primes}.\n"
    ));
    md.push_str("- **Smoothing (log-loss)**: Laplace α=0.001 on training counts.\n");
    md.push_str("- **Smoothing (P/KL display)**: α=1e-9 for normalization.\n\n");
    md.push_str(&format!(
        "- **Top Contexts filter**: exclude_diagonal(i==j) = {exclude_diagonal}\n\n"
    ));
    md.push_str(&format!(
        "- **Top Contexts filter**: min_Nij = {min_nij}\n\n"
    ));

    md.push_str("## Global Statistics (holdout)\n");
    md.push_str("*DeltaLL>0 means the history model (M2) predicts better. CMI measures history dependence strength (nats).*\n");
    md.push_str("| Metric | Value |\n");
    md.push_str("|--------|-------|\n");
    md.push_str(&format!(
        "| log-loss M1 (bits) | {:.6} |\n",
        result.global.log_loss_m1
    ));
    md.push_str(&format!(
        "| log-loss M2 (bits) | {:.6} |\n",
        result.global.log_loss_m2
    ));
    md.push_str(&format!(
        "| ΔLL = M1 - M2 (bits) | {:+.6} |\n",
        result.global.delta_ll
    ));
    md.push_str(&format!("| CMI (nats) | {:.6} |\n", result.global.cmi));
    md.push_str(&format!(
        "| AIC diff (M1 - M2) | {:+.3} |\n",
        result.global.aic_m1 - result.global.aic_m2
    ));
    md.push_str(&format!(
        "| BIC diff (M1 - M2) | {:+.3} |\n\n",
        result.global.bic_m1 - result.global.bic_m2
    ));

    // P1 derived from c2
    let mut c1 = [[0u64; 8]; 8];
    for i in 0..8usize {
        for j in 0..8usize {
            for k in 0..8usize {
                c1[j][k] = c1[j][k].saturating_add(result.c2[i][j][k]);
            }
        }
    }

    md.push_str("## P1 Transition Probability (%)\n");
    md.push_str(
        "*P1 is the first-order model transition probability predicting X_{n+1} from X_n only.*\n",
    );
    md.push_str("| From\\\\To | 1 | 7 | 11 | 13 | 17 | 19 | 23 | 29 |\n");
    md.push_str("|---------|---|---|----|----|----|----|----|----|\n");
    for j in 0..8usize {
        let row_sum: u64 = c1[j].iter().sum();
        md.push_str(&format!("| {} |", MOD30_RESIDUES[j]));
        for k in 0..8usize {
            if row_sum > 0 {
                let p = (c1[j][k] as f64 / row_sum as f64) * 100.0;
                md.push_str(&format!(" {p:.3}% |"));
            } else {
                md.push_str(" — |");
            }
        }
        md.push('\n');
    }
    md.push('\n');

    // KL matrix
    md.push_str("## KL(i,j) Matrix (nats)\n");
    md.push_str("*KL(i,j) measures how much the next-step distribution P2(k|i,j) diverges from P1(k|j) (nats).*\n");
    md.push_str("| i\\\\j | 1 | 7 | 11 | 13 | 17 | 19 | 23 | 29 |\n");
    md.push_str("|------|---|---|----|----|----|----|----|----|\n");
    for i in 0..8usize {
        md.push_str(&format!("| {} |", MOD30_RESIDUES[i]));
        for j in 0..8usize {
            md.push_str(&format!(" {:.6} |", result.global.kl_matrix[i][j]));
        }
        md.push('\n');
    }
    md.push('\n');

    // Top contexts
    let mut entries: Vec<(f64, usize, usize)> = Vec::new();
    for i in 0..8usize {
        for j in 0..8usize {
            if exclude_diagonal && i == j {
                continue;
            }
            entries.push((result.global.kl_matrix[i][j], i, j));
        }
    }
    entries.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    md.push_str("## Top Contexts by KL (nats)\n");
    md.push_str("*Higher KL means history changes the next-step distribution more. Small N_ij can be noisy, so filter by min_Nij.*\n");
    md.push_str("| Rank | (X_{n-1}, X_n) | KL(i,j) | N_ij | N_j | support% |\n");
    md.push_str("|------|-----------------|--------|------|-----|----------|\n");
    let mut shown = 0usize;
    for (kl, i, j) in entries.iter().map(|e| (e.0, e.1, e.2)) {
        if shown >= 10 {
            break;
        }
        let nij: u64 = result.c2[i][j].iter().sum();
        if nij < min_nij {
            continue;
        }
        let mut nj: u64 = 0;
        for ii in 0..8usize {
            nj = nj.saturating_add(result.c2[ii][j].iter().sum::<u64>());
        }
        let support_pct = if total_triplets > 0 {
            (nij as f64 / total_triplets as f64) * 100.0
        } else {
            0.0
        };
        md.push_str(&format!(
            "| {} | ({}, {}) | {:.6} | {} | {} | {:.6}% |\n",
            shown + 1,
            MOD30_RESIDUES[i],
            MOD30_RESIDUES[j],
            kl,
            nij,
            nj,
            support_pct
        ));
        shown += 1;
    }
    md.push('\n');

    // Top-K detail
    md.push_str("## Top-K Context Detail (P2 vs P1)\n");
    md.push_str(
        "*For each top context, lists the k values where P2(k|i,j) differs most from P1(k|j).*\n",
    );
    let top_k = 5usize;
    let alpha = 1e-9_f64;

    let mut shown_detail = 0usize;
    for (kl, i, j) in entries.iter().map(|e| (e.0, e.1, e.2)) {
        if shown_detail >= top_k {
            break;
        }
        let nij: u64 = result.c2[i][j].iter().sum();
        if nij < min_nij {
            continue;
        }
        let support_pct = if total_triplets > 0 {
            (nij as f64 / total_triplets as f64) * 100.0
        } else {
            0.0
        };
        md.push_str(&format!(
            "### #{}: context (X_{{n-1}}={}, X_n={})  KL={:.6} nats  N_ij={} ({:.6}%)\n\n",
            shown_detail + 1,
            MOD30_RESIDUES[i],
            MOD30_RESIDUES[j],
            kl,
            nij,
            support_pct
        ));
        let p2 = normalize_row_probs(&result.c2[i][j], alpha);
        let p1 = normalize_row_probs(&c1[j], alpha);

        let mut ks: Vec<(f64, usize)> = (0..8usize).map(|k| ((p2[k] - p1[k]).abs(), k)).collect();
        ks.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        md.push_str("| k (residue) | P2(k|i,j) | P1(k|j) | Δ |\n");
        md.push_str("|------------|-----------|---------|---|\n");
        for (_d, k) in ks {
            let d = p2[k] - p1[k];
            md.push_str(&format!(
                "| {} ({}) | {:.4}% | {:.4}% | {:+.4}% |\n",
                k,
                MOD30_RESIDUES[k],
                p2[k] * 100.0,
                p1[k] * 100.0,
                d * 100.0
            ));
        }
        md.push('\n');
        shown_detail += 1;
    }

    md.push_str("## Bin Statistics\n");
    md.push_str("*Bins check stability across ranges. Positive DeltaLL in a bin indicates history effect in that range.*\n");
    md.push_str(&format!("**Mode**: {view_mode:?}\n\n"));
    md.push_str("| Bin | Label | N | N(test) | ΔLL (bits) | CMI (nats) |\n");
    md.push_str("|-----|-------|---|---------|------------|-----------|\n");
    for b in result.bins.iter().filter(|b| b.mode == view_mode) {
        md.push_str(&format!(
            "| {} | {} | {} | {} | {:+.6} | {:.6} |\n",
            b.bin_index,
            b.label,
            b.stats.sample_count,
            b.stats.test_count,
            b.stats.delta_ll,
            b.stats.cmi
        ));
    }

    // --- Machine-readable block (JSON) ---
    let mut p1_prob: Vec<Vec<f64>> = Vec::with_capacity(8);
    for j in 0..8usize {
        let row_sum: u64 = c1[j].iter().sum();
        let mut row: Vec<f64> = Vec::with_capacity(8);
        for k in 0..8usize {
            let v = if row_sum > 0 {
                (c1[j][k] as f64) / (row_sum as f64)
            } else {
                0.0
            };
            row.push(if v.is_finite() { v } else { 0.0 });
        }
        p1_prob.push(row);
    }

    let mut kl: Vec<Vec<f64>> = Vec::with_capacity(8);
    for i in 0..8usize {
        let mut row: Vec<f64> = Vec::with_capacity(8);
        for j in 0..8usize {
            let v = result.global.kl_matrix[i][j];
            row.push(if v.is_finite() { v } else { 0.0 });
        }
        kl.push(row);
    }

    let mut top_contexts_json = Vec::new();
    for (rank, (klv, i, j)) in entries.iter().take(10).enumerate() {
        let nij: u64 = result.c2[*i][*j].iter().sum();
        let support_pct = if total_triplets > 0 {
            (nij as f64 / total_triplets as f64) * 100.0
        } else {
            0.0
        };
        top_contexts_json.push(json!({
            "rank": rank + 1,
            "i": *i,
            "j": *j,
            "residue_i": MOD30_RESIDUES[*i],
            "residue_j": MOD30_RESIDUES[*j],
            "kl_nats": if klv.is_finite() { *klv } else { 0.0 },
            "n_ij": nij,
            "support_pct": support_pct
        }));
    }

    let mut bins_json = Vec::new();
    for b in &result.bins {
        bins_json.push(json!({
            "mode": format!("{:?}", b.mode),
            "bin_index": b.bin_index,
            "label": b.label,
            "n_total": b.stats.sample_count,
            "n_test": b.stats.test_count,
            "delta_ll_bits": b.stats.delta_ll,
            "cmi_nats": b.stats.cmi
        }));
    }

    let data = json!({
        "dataset": {
            "file": file_path.trim(),
            "excluded_primes": [2, 3, 5],
            "total_triplets": total_triplets,
            "min_p": null,
            "max_p": null
        },
        "state_mapping": {
            "residues": MOD30_RESIDUES,
            "idx_order": [0,1,2,3,4,5,6,7]
        },
        "display_options": {
            "exclude_diagonal": exclude_diagonal,
            "min_nij": min_nij
        },
        "holdout": {
            "global": {
                "method": "file_position_split",
                "train_ratio": prhs_train_ratio,
                "test_ratio": (1.0 - prhs_train_ratio).max(0.0)
            },
            "bins": {
                "method": "in_bin_modulo",
                "description": "every 5th triplet in each bin is test (bin-local counter)",
                "test_ratio_approx": 0.2
            }
        },
        "smoothing": {
            "log_loss": {"method": "laplace", "alpha": 0.001},
            "p_display": {"method": "laplace", "alpha": 1e-9}
        },
        "global": {
            "sample_count": result.global.sample_count,
            "test_count": result.global.test_count,
            "log_loss_m1_bits": result.global.log_loss_m1,
            "log_loss_m2_bits": result.global.log_loss_m2,
            "delta_ll_bits": result.global.delta_ll,
            "cmi_nats": result.global.cmi,
            "aic_m1": result.global.aic_m1,
            "aic_m2": result.global.aic_m2,
            "bic_m1": result.global.bic_m1,
            "bic_m2": result.global.bic_m2,
            "aic_diff_m1_minus_m2": result.global.aic_m1 - result.global.aic_m2,
            "bic_diff_m1_minus_m2": result.global.bic_m1 - result.global.bic_m2,
            "k1": 56,
            "k2": 448
        },
        "binning_params": {
            "log10_bin_width": prhs_log10_bin_width,
            "equal_bin_primes": prhs_equal_bin_primes
        },
        "P1": p1_prob,
        "KL": kl,
        "top_contexts": top_contexts_json,
        "bins": bins_json
    });

    md.push_str("\n\n## DATA (machine-readable)\n");
    md.push_str("```json\n");
    match serde_json::to_string_pretty(&data) {
        Ok(s) => md.push_str(&s),
        Err(_) => md.push_str("{\"error\":\"failed to serialize\"}"),
    }
    md.push_str("\n```\n");

    md
}

pub(crate) fn render_prhs_results(ui: &mut egui::Ui, state: &AnalyzeState) {
    let (view_result, view_total) = if state.running {
        try_read_shared(&state.shared_prhs, &state.shared_processed)
            .unwrap_or_else(|| (PRHSResult::default(), 0))
    } else {
        (state.prhs.clone(), state.total_primes)
    };

    let min_nij = state
        .prhs_min_context_nij_input
        .trim()
        .parse::<u64>()
        .ok()
        .unwrap_or(0);

    render_prhs_global_section(ui, &view_result, view_total);
    ui.add_space(20.0);
    render_prhs_p1_heatmap_section(ui, &view_result);
    ui.add_space(20.0);
    render_prhs_kl_section(ui, &view_result, state.prhs_exclude_diagonal, min_nij);
    ui.add_space(20.0);
    render_prhs_topk_detail_section(ui, &view_result, state.prhs_exclude_diagonal, min_nij);
    ui.add_space(20.0);
    render_prhs_bin_trend_section(ui, &view_result, state.prhs_view_bin_mode);
    ui.add_space(20.0);
    render_prhs_bins_section(ui, &view_result, state.prhs_view_bin_mode);
}

fn render_prhs_global_section(ui: &mut egui::Ui, result: &PRHSResult, total_triplets: u64) {
    ui.label(section_title("mod 30 PRHS — Global Stats"));
    ui.add_space(12.0);

    let s = &result.global;
    egui::Grid::new("analyze_prhs_global_grid")
        .striped(true)
        .spacing([16.0, 8.0])
        .show(ui, |ui| {
            ui.label(field_label("Samples (triplets)"));
            label_primary(ui, total_triplets);
            ui.end_row();

            ui.label(field_label("Holdout samples"));
            label_primary(ui, s.test_count);
            ui.end_row();

            ui.label(field_label("log-loss M1 (bits/sample)"));
            label_primary(ui, format!("{:.6}", s.log_loss_m1));
            ui.end_row();

            ui.label(field_label("log-loss M2 (bits/sample)"));
            label_primary(ui, format!("{:.6}", s.log_loss_m2));
            ui.end_row();

            ui.label(field_label("ΔLL = M1 - M2 (bits/sample)"));
            label_primary(ui, format!("{:+.6}", s.delta_ll));
            ui.end_row();

            ui.label(field_label("CMI (nats)"));
            label_primary(ui, format!("{:.6}", s.cmi));
            ui.end_row();

            ui.label(field_label("AIC diff (M1 - M2)"));
            label_primary(ui, format!("{:+.3}", s.aic_m1 - s.aic_m2));
            ui.end_row();

            ui.label(field_label("BIC diff (M1 - M2)"));
            label_primary(ui, format!("{:+.3}", s.bic_m1 - s.bic_m2));
            ui.end_row();
        });
}

fn render_prhs_p1_heatmap_section(ui: &mut egui::Ui, result: &PRHSResult) {
    ui.label(section_title("P1 Heatmap (P(X_{n+1} | X_n), %)"));
    ui.add_space(12.0);

    // c1 derived from c2
    let mut c1 = [[0u64; 8]; 8];
    for i in 0..8usize {
        for j in 0..8usize {
            for k in 0..8usize {
                c1[j][k] = c1[j][k].saturating_add(result.c2[i][j][k]);
            }
        }
    }

    // normalize by max probability
    let mut max_p = 0.0f64;
    let mut pmat = [[0.0f64; 8]; 8];
    for j in 0..8usize {
        let row_sum: u64 = c1[j].iter().sum();
        if row_sum == 0 {
            continue;
        }
        for k in 0..8usize {
            let p = (c1[j][k] as f64) / (row_sum as f64);
            pmat[j][k] = p;
            if p > max_p {
                max_p = p;
            }
        }
    }

    let accent = colors::ACCENT;
    let (r, g, b) = (accent.r(), accent.g(), accent.b());
    let cell_w = 56.0;
    let cell_h = 28.0;

    egui::ScrollArea::horizontal()
        .id_salt("analyze_prhs_heatmap_scroll_x")
        .show(ui, |ui| {
            egui::Grid::new("analyze_prhs_heatmap_grid")
                .spacing([6.0, 6.0])
                .show(ui, |ui| {
                    ui.add_sized([cell_w, cell_h], egui::Label::new(field_label("From\\To")));
                    for to in 0..8usize {
                        ui.add_sized(
                            [cell_w, cell_h],
                            egui::Label::new(field_label(&format!("{}", MOD30_RESIDUES[to]))),
                        );
                    }
                    ui.end_row();

                    for from in 0..8usize {
                        ui.add_sized(
                            [cell_w, cell_h],
                            egui::Label::new(
                                egui::RichText::new(format!("{}", MOD30_RESIDUES[from]))
                                    .size(font_sizes::BODY)
                                    .color(colors::TEXT_PRIMARY),
                            ),
                        );

                        for to in 0..8usize {
                            let p = pmat[from][to];
                            let t = if max_p > 0.0 {
                                (p / max_p).clamp(0.0, 1.0)
                            } else {
                                0.0
                            };
                            let alpha = (24.0 + t * 200.0) as u8;
                            let fill = egui::Color32::from_rgba_unmultiplied(r, g, b, alpha);

                            let pct = p * 100.0;
                            let text = if pct > 0.0 {
                                format!("{pct:.1}%")
                            } else {
                                "—".to_string()
                            };
                            let count = c1[from][to];

                            ui.add_sized(
                                [cell_w, cell_h],
                                egui::Button::new(
                                    egui::RichText::new(text)
                                        .size(font_sizes::LABEL)
                                        .color(egui::Color32::WHITE),
                                )
                                .fill(fill)
                                .sense(egui::Sense::hover()),
                            )
                            .on_hover_text(format!(
                                "from {} to {}: count={count}",
                                MOD30_RESIDUES[from], MOD30_RESIDUES[to]
                            ));
                        }
                        ui.end_row();
                    }
                });
        });
}

fn render_prhs_kl_section(
    ui: &mut egui::Ui,
    result: &PRHSResult,
    exclude_diagonal: bool,
    min_nij: u64,
) {
    ui.label(section_title("KL(i,j) (P2(·|i,j) vs P1(·|j), nats)"));
    ui.add_space(12.0);

    // Top-K
    let mut entries: Vec<(f64, usize, usize)> = Vec::new();
    for i in 0..8usize {
        for j in 0..8usize {
            if exclude_diagonal && i == j {
                continue;
            }
            entries.push((result.global.kl_matrix[i][j], i, j));
        }
    }
    entries.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    // scale
    let mut max_kl = 0.0f64;
    for i in 0..8usize {
        for j in 0..8usize {
            let v = result.global.kl_matrix[i][j];
            if v.is_finite() && v > max_kl {
                max_kl = v;
            }
        }
    }

    // Top-K + Heatmap columns
    ui.columns(2, |cols| {
        cols[0].vertical(|ui| {
            ui.label(section_title("Top contexts"));
            ui.add_space(8.0);

            egui::Grid::new("analyze_prhs_kl_top_grid")
                .striped(true)
                .spacing([16.0, 8.0])
                .show(ui, |ui| {
                    ui.label(field_label("Rank"));
                    ui.label(field_label("(X_{n-1}, X_n)"));
                    ui.label(field_label("KL (nats)"));
                    ui.end_row();

                    let mut shown = 0usize;
                    for (kl, i, j) in entries.iter().map(|e| (e.0, e.1, e.2)) {
                        if shown >= 10 {
                            break;
                        }
                        let nij: u64 = result.c2[i][j].iter().sum();
                        if nij < min_nij.max(1) {
                            continue;
                        }
                        label_primary(ui, shown + 1);
                        label_primary(
                            ui,
                            format!("({}, {})", MOD30_RESIDUES[i], MOD30_RESIDUES[j]),
                        );
                        label_primary(ui, format!("{kl:.6}"));
                        ui.end_row();
                        shown += 1;
                    }
                });
        });

        cols[1].vertical(|ui| {
            ui.label(section_title("KL heatmap"));
            ui.add_space(8.0);

            let cell_w = 46.0;
            let cell_h = 24.0;
            let (r, g, b) = (colors::DANGER.r(), colors::DANGER.g(), colors::DANGER.b());

            egui::ScrollArea::horizontal()
                .id_salt("analyze_prhs_kl_heatmap_scroll_x")
                .show(ui, |ui| {
                    egui::Grid::new("analyze_prhs_kl_heatmap_grid")
                        .spacing([6.0, 6.0])
                        .show(ui, |ui| {
                            ui.add_sized([cell_w, cell_h], egui::Label::new(field_label("i\\j")));
                            for j in 0..8usize {
                                ui.add_sized(
                                    [cell_w, cell_h],
                                    egui::Label::new(field_label(&format!(
                                        "{}",
                                        MOD30_RESIDUES[j]
                                    ))),
                                );
                            }
                            ui.end_row();

                            for i in 0..8usize {
                                ui.add_sized(
                                    [cell_w, cell_h],
                                    egui::Label::new(
                                        egui::RichText::new(format!("{}", MOD30_RESIDUES[i]))
                                            .size(font_sizes::BODY)
                                            .color(colors::TEXT_PRIMARY),
                                    ),
                                );

                                for j in 0..8usize {
                                    let v = result.global.kl_matrix[i][j].max(0.0);
                                    let t = if max_kl > 0.0 {
                                        (v / max_kl).clamp(0.0, 1.0)
                                    } else {
                                        0.0
                                    };
                                    let alpha = (20.0 + t * 215.0) as u8;
                                    let fill =
                                        egui::Color32::from_rgba_unmultiplied(r, g, b, alpha);

                                    let text = if v > 0.0 {
                                        format!("{v:.3}")
                                    } else {
                                        "—".to_string()
                                    };
                                    ui.add_sized(
                                        [cell_w, cell_h],
                                        egui::Button::new(
                                            egui::RichText::new(text)
                                                .size(font_sizes::LABEL)
                                                .color(egui::Color32::WHITE),
                                        )
                                        .fill(fill)
                                        .sense(egui::Sense::hover()),
                                    )
                                    .on_hover_text(format!(
                                        "KL(i={}, j={}) = {:.6} nats",
                                        MOD30_RESIDUES[i],
                                        MOD30_RESIDUES[j],
                                        result.global.kl_matrix[i][j]
                                    ));
                                }
                                ui.end_row();
                            }
                        });
                });
        });
    });

    ui.add_space(12.0);

    // full table
    ui.label(section_title("KL matrix (table)"));
    ui.add_space(8.0);
    egui::ScrollArea::horizontal()
        .id_salt("analyze_prhs_kl_matrix_scroll_x")
        .show(ui, |ui| {
            egui::Grid::new("analyze_prhs_kl_matrix_grid")
                .striped(true)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    ui.label(field_label("i\\j"));
                    for j in 0..8usize {
                        ui.label(field_label(&format!("{}", MOD30_RESIDUES[j])));
                    }
                    ui.end_row();

                    for i in 0..8usize {
                        label_primary(ui, MOD30_RESIDUES[i]);
                        for j in 0..8usize {
                            label_primary(ui, format!("{:.4}", result.global.kl_matrix[i][j]));
                        }
                        ui.end_row();
                    }
                });
        });
}

fn render_prhs_bins_section(ui: &mut egui::Ui, result: &PRHSResult, mode: PRHSBinMode) {
    ui.label(section_title("Bin Statistics"));
    ui.add_space(12.0);

    let bins: Vec<&PRHSBinStats> = result.bins.iter().filter(|b| b.mode == mode).collect();

    ui.label(
        egui::RichText::new(format!("Mode: {:?}  (bins: {})", mode, bins.len()))
            .size(font_sizes::LABEL)
            .color(colors::TEXT_SECONDARY),
    );
    ui.add_space(8.0);

    egui::ScrollArea::vertical()
        .id_salt("analyze_prhs_bins_scroll")
        .max_height(260.0)
        .show(ui, |ui| {
            egui::Grid::new("analyze_prhs_bins_grid")
                .striped(true)
                .spacing([16.0, 8.0])
                .show(ui, |ui| {
                    ui.label(field_label("Bin"));
                    ui.label(field_label("Label"));
                    ui.label(field_label("N"));
                    ui.label(field_label("N(test)"));
                    ui.label(field_label("ΔLL (bits)"));
                    ui.label(field_label("CMI (nats)"));
                    ui.end_row();

                    for b in bins.iter().take(200) {
                        label_primary(ui, b.bin_index);
                        label_primary(ui, &b.label);
                        label_primary(ui, b.stats.sample_count);
                        label_primary(ui, b.stats.test_count);
                        label_primary(ui, format!("{:+.6}", b.stats.delta_ll));
                        label_primary(ui, format!("{:.6}", b.stats.cmi));
                        ui.end_row();
                    }
                });
        });
}

fn normalize_row_probs(counts: &[u64; 8], alpha: f64) -> [f64; 8] {
    let sum: u64 = counts.iter().sum();
    let denom = sum as f64 + 8.0 * alpha;
    let mut out = [0.0f64; 8];
    for k in 0..8usize {
        out[k] = (counts[k] as f64 + alpha) / denom;
    }
    out
}

fn render_prob_bar_row(ui: &mut egui::Ui, label: &str, p2: f64, p1: f64, delta: f64, max_p: f64) {
    let bar_w: f32 = 160.0;
    let bar_h: f32 = 14.0;
    let gap = 6.0;

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(label)
                .size(font_sizes::LABEL)
                .color(colors::TEXT_SECONDARY),
        );

        let (rect2, _resp2) =
            ui.allocate_exact_size(egui::vec2(bar_w, bar_h), egui::Sense::hover());
        let painter2 = ui.painter_at(rect2);
        painter2.rect_filled(rect2, 2.0, colors::CARD_BG);
        let w2: f32 = if max_p > 0.0 {
            bar_w * ((p2 / max_p).clamp(0.0, 1.0) as f32)
        } else {
            0.0
        };
        let fill2 = egui::Rect::from_min_size(rect2.min, egui::vec2(w2, bar_h));
        painter2.rect_filled(fill2, 2.0, colors::ACCENT);

        ui.add_space(gap);

        let (rect1, _resp1) =
            ui.allocate_exact_size(egui::vec2(bar_w, bar_h), egui::Sense::hover());
        let painter1 = ui.painter_at(rect1);
        painter1.rect_filled(rect1, 2.0, colors::CARD_BG);
        let w1: f32 = if max_p > 0.0 {
            bar_w * ((p1 / max_p).clamp(0.0, 1.0) as f32)
        } else {
            0.0
        };
        let fill1 = egui::Rect::from_min_size(rect1.min, egui::vec2(w1, bar_h));
        painter1.rect_filled(fill1, 2.0, colors::TEXT_SECONDARY);

        ui.add_space(10.0);

        let sign = if delta >= 0.0 { "+" } else { "" };
        ui.label(
            egui::RichText::new(format!(
                "P2 {:.2}%  P1 {:.2}%  Δ {sign}{:.2}%",
                p2 * 100.0,
                p1 * 100.0,
                delta * 100.0
            ))
            .size(font_sizes::LABEL)
            .color(colors::TEXT_PRIMARY),
        );
    });
}

fn render_prhs_topk_detail_section(
    ui: &mut egui::Ui,
    result: &PRHSResult,
    exclude_diagonal: bool,
    min_nij: u64,
) {
    ui.label(section_title("Top-K Context Detail (P2 vs P1)"));
    ui.add_space(12.0);

    let mut entries: Vec<(f64, usize, usize)> = Vec::new();
    for i in 0..8usize {
        for j in 0..8usize {
            if exclude_diagonal && i == j {
                continue;
            }
            entries.push((result.global.kl_matrix[i][j], i, j));
        }
    }
    entries.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut c1 = [[0u64; 8]; 8];
    for i in 0..8usize {
        for j in 0..8usize {
            for k in 0..8usize {
                c1[j][k] = c1[j][k].saturating_add(result.c2[i][j][k]);
            }
        }
    }

    let alpha = 1e-9_f64;
    let top_k = 5usize;

    let mut shown = 0usize;
    for (kl, i, j) in entries.iter().map(|e| (e.0, e.1, e.2)) {
        if shown >= top_k {
            break;
        }
        let nij: u64 = result.c2[i][j].iter().sum();
        if nij < min_nij.max(1) {
            continue;
        }
        let header = format!(
            "#{shown}: context (Xₙ₋₁={}, Xₙ={})  KL={:.6} nats",
            MOD30_RESIDUES[i], MOD30_RESIDUES[j], kl
        );

        egui::CollapsingHeader::new(header)
            .default_open(shown == 0)
            .show(ui, |ui| {
                let counts_p2 = &result.c2[i][j];
                let p2 = normalize_row_probs(counts_p2, alpha);
                let p1 = normalize_row_probs(&c1[j], alpha);

                let mut max_p = 0.0f64;
                for k in 0..8usize {
                    max_p = max_p.max(p2[k]).max(p1[k]);
                }

                let mut ks: Vec<(f64, usize)> =
                    (0..8usize).map(|k| ((p2[k] - p1[k]).abs(), k)).collect();
                ks.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

                for (_dabs, k) in ks {
                    let delta = p2[k] - p1[k];
                    let label = format!("k={} ({})", k, MOD30_RESIDUES[k]);
                    render_prob_bar_row(ui, &label, p2[k], p1[k], delta, max_p);
                }

                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(
                        "Bars: P2=blue (order-2), P1=gray (order-1). Sorted by |Δ|.",
                    )
                    .size(font_sizes::LABEL)
                    .color(colors::TEXT_SECONDARY),
                );
            });

        ui.add_space(8.0);
        shown += 1;
    }
}

fn render_prhs_bin_trend_section(ui: &mut egui::Ui, result: &PRHSResult, mode: PRHSBinMode) {
    ui.label(section_title("Bin Trend (ΔLL, CMI)"));
    ui.add_space(12.0);

    let mut bins: Vec<&PRHSBinStats> = result.bins.iter().filter(|b| b.mode == mode).collect();
    bins.sort_by_key(|b| b.bin_index);

    if bins.len() < 2 {
        ui.label(
            egui::RichText::new("Not enough bins to draw a trend yet.")
                .size(font_sizes::LABEL)
                .color(colors::TEXT_SECONDARY),
        );
        return;
    }

    let x_min = bins.first().map(|b| b.bin_index as f64).unwrap_or(0.0);
    let x_max = bins.last().map(|b| b.bin_index as f64).unwrap_or(1.0);

    let mut y_min = f64::INFINITY;
    let mut y_max = -f64::INFINITY;
    for b in &bins {
        y_min = y_min.min(b.stats.delta_ll).min(b.stats.cmi);
        y_max = y_max.max(b.stats.delta_ll).max(b.stats.cmi);
    }
    if !y_min.is_finite() || !y_max.is_finite() || (y_max - y_min).abs() < 1e-12 {
        y_min = 0.0;
        y_max = 1.0;
    } else {
        let pad = (y_max - y_min) * 0.1;
        y_min -= pad;
        y_max += pad;
    }

    let rect = ui.allocate_space(egui::vec2(ui.available_width(), 220.0)).1;
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, colors::CARD_BG);

    let margins = GraphMargins::default();
    let graph_rect = compute_graph_rect(rect, &margins);
    let view = ZoomPanState::default();

    let axis_labels = AxisLabels {
        y_max: format!("{y_max:.4}"),
        y_min: format!("{y_min:.4}"),
        x_min: format!("{}", x_min as i64),
        x_max: format!("{}", x_max as i64),
    };
    draw_axes(
        &painter,
        graph_rect,
        &view,
        &axis_labels,
        colors::TEXT_SECONDARY,
    );

    let data_range = (x_min, x_max, y_min, y_max);
    let mut pts_delta: Vec<egui::Pos2> = Vec::with_capacity(bins.len());
    let mut pts_cmi: Vec<egui::Pos2> = Vec::with_capacity(bins.len());
    for b in &bins {
        let x = b.bin_index as f64;
        let p_delta =
            crate::ui_graph_utils::data_to_screen(x, b.stats.delta_ll, data_range, graph_rect);
        let p_cmi = crate::ui_graph_utils::data_to_screen(x, b.stats.cmi, data_range, graph_rect);
        pts_delta.push(p_delta);
        pts_cmi.push(p_cmi);
    }

    draw_polyline(
        &painter,
        graph_rect,
        &view,
        &pts_delta,
        egui::Stroke::new(2.0, colors::ACCENT),
    );
    draw_polyline(
        &painter,
        graph_rect,
        &view,
        &pts_cmi,
        egui::Stroke::new(2.0, colors::DANGER),
    );

    let items = [
        LegendItem {
            label: "ΔLL (bits)",
            color: colors::ACCENT,
        },
        LegendItem {
            label: "CMI (nats)",
            color: colors::DANGER,
        },
    ];
    draw_legend(&painter, graph_rect, &items);
}
