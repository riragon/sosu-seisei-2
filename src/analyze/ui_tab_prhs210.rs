#![allow(clippy::too_many_arguments)]

use eframe::egui;
use serde_json::json;

use crate::analyze::tab_prhs::{PRHS210BinStats, PRHS210Result};
use crate::analyze::ui_analyze::{label_primary, try_read_shared};
use crate::analyze::{PRHSBinMode, MOD210_RESIDUES};
use crate::app_state::AnalyzeState;
use crate::ui_components::{field_label, section_title, ZoomPanState};
use crate::ui_graph_utils::{
    compute_graph_rect, draw_axes, draw_legend, draw_polyline, AxisLabels, GraphMargins, LegendItem,
};
use crate::ui_theme::{colors, font_sizes};

pub(crate) fn format_prhs210_as_markdown(
    result: &PRHS210Result,
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

    md.push_str("# mod 210 PRHS\n");
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
    md.push_str("- **Dataset**: primes p in the file, excluding p=2,3,5,7; states are residues mod 210 mapped to 48 indices.\n");
    md.push_str(&format!(
        "- **State mapping**: S (mod 210, φ(210)=48) = [{}] (idx=0..47 in that order).\n",
        MOD210_RESIDUES
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

    md.push_str("## Top Contexts by KL (nats)\n");
    md.push_str("*Higher KL means history changes the next-step distribution more. Small N_ij can be noisy, so filter by min_Nij.*\n");
    md.push_str("| Rank | (X_{n-1}, X_n) | KL(i,j) | N_ij | support% |\n");
    md.push_str("|------|-----------------|--------|------|----------|\n");
    let mut shown_rank = 0usize;
    for c in result.top_contexts.iter() {
        if exclude_diagonal && c.i == c.j {
            continue;
        }
        if c.n_ij < min_nij.max(1) {
            continue;
        }
        shown_rank += 1;
        if shown_rank > 20 {
            break;
        }
        md.push_str(&format!(
            "| {} | ({}, {}) | {:.6} | {} | {:.6}% |\n",
            shown_rank, c.residue_i, c.residue_j, c.kl_nats, c.n_ij, c.support_pct
        ));
    }
    md.push('\n');

    md.push_str("## Top-K Context Detail (Δ top-3)\n");
    md.push_str("*For each top context, lists k values where P2 differs most from P1.*\n");
    let mut shown = 0usize;
    for c in result.top_contexts.iter() {
        if exclude_diagonal && c.i == c.j {
            continue;
        }
        if c.n_ij < min_nij.max(1) {
            continue;
        }
        shown += 1;
        if shown > 10 {
            break;
        }
        md.push_str(&format!(
            "### #{}: context (X_{{n-1}}={}, X_n={})  KL={:.6} nats  N_ij={} ({:.6}%)\n\n",
            shown, c.residue_i, c.residue_j, c.kl_nats, c.n_ij, c.support_pct
        ));
        md.push_str("| k (residue) | P2 | P1 | Δ |\n");
        md.push_str("|------------|----|----|---|\n");
        for d in &c.delta_top {
            md.push_str(&format!(
                "| {} ({}) | {:.4}% | {:.4}% | {:+.4}% |\n",
                d.k,
                d.residue_k,
                d.p2 * 100.0,
                d.p1 * 100.0,
                d.delta * 100.0
            ));
        }
        md.push('\n');
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

    // Machine-readable
    let bins_json: Vec<_> = result
        .bins
        .iter()
        .map(|b| {
            json!({
                "mode": format!("{:?}", b.mode),
                "bin_index": b.bin_index,
                "label": b.label,
                "n_total": b.stats.sample_count,
                "n_test": b.stats.test_count,
                "delta_ll_bits": b.stats.delta_ll,
                "cmi_nats": b.stats.cmi
            })
        })
        .collect();

    let data = json!({
        "dataset": {
            "file": file_path.trim(),
            "excluded_primes": [2, 3, 5, 7],
            "total_triplets": total_triplets,
            "min_p": null,
            "max_p": null
        },
        "state_mapping": {
            "modulus": 210,
            "phi": 48,
            "residues": MOD210_RESIDUES.to_vec(),
            "idx_order": (0..48).collect::<Vec<_>>()
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
            "k1": 2256,
            "k2": 108288
        },
        "binning_params": {
            "log10_bin_width": prhs_log10_bin_width,
            "equal_bin_primes": prhs_equal_bin_primes
        },
        "P1": result.p1,
        "KL": result.global.kl_matrix,
        "top_contexts": result.top_contexts,
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

pub(crate) fn render_prhs210_results(ui: &mut egui::Ui, state: &AnalyzeState) {
    let (view_result, view_total) = if state.running {
        try_read_shared(&state.shared_prhs210, &state.shared_processed)
            .unwrap_or_else(|| (PRHS210Result::default(), 0))
    } else {
        (state.prhs210.clone(), state.total_primes)
    };

    let min_nij = state
        .prhs_min_context_nij_input
        .trim()
        .parse::<u64>()
        .ok()
        .unwrap_or(0);

    render_prhs210_global_section(ui, &view_result, view_total);
    ui.add_space(20.0);
    render_prhs210_top_contexts_section(ui, &view_result, state.prhs_exclude_diagonal, min_nij);
    ui.add_space(20.0);
    render_prhs210_bin_trend_section(ui, &view_result, state.prhs_view_bin_mode);
    ui.add_space(20.0);
    render_prhs210_bins_section(ui, &view_result, state.prhs_view_bin_mode);
}

fn render_prhs210_global_section(ui: &mut egui::Ui, result: &PRHS210Result, total_triplets: u64) {
    ui.label(section_title("mod 210 PRHS — Global Stats"));
    ui.add_space(12.0);

    let s = &result.global;
    egui::Grid::new("analyze_prhs210_global_grid")
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

            ui.label(field_label("Params (k1/k2)"));
            label_primary(ui, "2256 / 108288");
            ui.end_row();
        });
}

fn render_prhs210_top_contexts_section(
    ui: &mut egui::Ui,
    result: &PRHS210Result,
    exclude_diagonal: bool,
    min_nij: u64,
) {
    ui.label(section_title("Top contexts (by KL) — with support"));
    ui.add_space(12.0);

    egui::Grid::new("analyze_prhs210_top_contexts_grid")
        .striped(true)
        .spacing([16.0, 8.0])
        .show(ui, |ui| {
            ui.label(field_label("Rank"));
            ui.label(field_label("(X_{n-1}, X_n)"));
            ui.label(field_label("KL (nats)"));
            ui.label(field_label("N_ij"));
            ui.label(field_label("support%"));
            ui.label(field_label("Δ top-3 (k: Δ%)"));
            ui.end_row();

            let mut shown_rank = 0usize;
            for c in result.top_contexts.iter() {
                if exclude_diagonal && c.i == c.j {
                    continue;
                }
                if c.n_ij < min_nij.max(1) {
                    continue;
                }
                shown_rank += 1;
                if shown_rank > 20 {
                    break;
                }
                label_primary(ui, shown_rank);
                label_primary(ui, format!("({}, {})", c.residue_i, c.residue_j));
                label_primary(ui, format!("{:.6}", c.kl_nats));
                label_primary(ui, c.n_ij);
                label_primary(ui, format!("{:.6}%", c.support_pct));
                let summary = c
                    .delta_top
                    .iter()
                    .map(|d| format!("{}:{:+.2}", d.residue_k, d.delta * 100.0))
                    .collect::<Vec<_>>()
                    .join(", ");
                label_primary(ui, summary);
                ui.end_row();
            }
        });

    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(
            "Note: Δ top-3 is computed from smoothed P2(·|i,j) vs smoothed P1(·|j) (α=1e-9).",
        )
        .size(font_sizes::LABEL)
        .color(colors::TEXT_SECONDARY),
    );
}

fn render_prhs210_bins_section(ui: &mut egui::Ui, result: &PRHS210Result, mode: PRHSBinMode) {
    ui.label(section_title("Bin Statistics"));
    ui.add_space(12.0);

    let bins: Vec<&PRHS210BinStats> = result.bins.iter().filter(|b| b.mode == mode).collect();

    ui.label(
        egui::RichText::new(format!("Mode: {:?}  (bins: {})", mode, bins.len()))
            .size(font_sizes::LABEL)
            .color(colors::TEXT_SECONDARY),
    );
    ui.add_space(8.0);

    egui::ScrollArea::vertical()
        .id_salt("analyze_prhs210_bins_scroll")
        .max_height(260.0)
        .show(ui, |ui| {
            egui::Grid::new("analyze_prhs210_bins_grid")
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

fn render_prhs210_bin_trend_section(ui: &mut egui::Ui, result: &PRHS210Result, mode: PRHSBinMode) {
    ui.label(section_title("Bin Trend (ΔLL, CMI)"));
    ui.add_space(12.0);

    let mut bins: Vec<&PRHS210BinStats> = result.bins.iter().filter(|b| b.mode == mode).collect();
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
