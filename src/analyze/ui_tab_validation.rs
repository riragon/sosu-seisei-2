use eframe::egui;
use serde_json::json;
use std::cmp::Ordering;

use crate::analyze::tab_validation::{ValidationBundle, ValidationResult};
use crate::analyze::ui_analyze::{label_primary, label_secondary, try_read_shared};
use crate::app_state::AnalyzeState;
use crate::ui_components::section_title;
use crate::ui_theme::{colors, font_sizes};

pub(crate) fn format_validation_bundle_as_markdown(
    bundle: &ValidationBundle,
    file_path: &str,
) -> String {
    let mut md = String::new();
    md.push_str("# Validation Report\n\n");
    md.push_str(&format!("**File**: {}\n", file_path.trim()));
    md.push_str("**Moduli**: mod 30 + mod 210\n\n");

    md.push_str("## mod 30\n\n");
    format_validation_subreport(&mut md, &bundle.mod30, bundle.mod30_triplets);

    md.push_str("\n\n## mod 210\n\n");
    format_validation_subreport(&mut md, &bundle.mod210, bundle.mod210_triplets);

    md.push_str("\n\n## E. Theory (Lemke Oliver–Soundararajan, 2016)\n");
    md.push_str("*Reference: arXiv:1603.03720 \"Unexpected biases in the distribution of consecutive primes\"*\n");
    let lo = bundle
        .mod30
        .lemke_oliver
        .as_ref()
        .or(bundle.mod210.lemke_oliver.as_ref());
    if lo.is_none() {
        md.push_str("_No theory comparison computed._\n");
    } else if let Some(list) = lo {
        for comp in list {
            md.push_str(&format!(
                "\n### mod {} (prime_max={})\n\n",
                comp.modulus, comp.prime_max
            ));
            md.push_str("#### Summary\n");
            md.push_str(&format!("- Verdict: {}\n", comp.verdict));
            md.push_str(&format!(
                "- χ² = {:.6}, df = {}, p = {:.6}\n",
                comp.chi_squared, comp.df, comp.p_value
            ));

            let mut ok = 0u64;
            let mut tot = 0u64;
            for r in &comp.pairwise {
                if r.expected_c_sign == 0 {
                    continue;
                }
                tot += 1;
                let s = if r.z_score > 0.0 {
                    1
                } else if r.z_score < 0.0 {
                    -1
                } else {
                    0
                };
                if s == r.expected_c_sign {
                    ok += 1;
                }
            }
            if tot > 0 {
                let pct = (ok as f64) * 100.0 / (tot as f64);
                md.push_str(&format!("- sign_match_ratio = {ok}/{tot} ({pct:.1}%)\n"));
            }

            match comp.verdict.as_str() {
                "Consistent" => {
                    md.push_str("- Interpretation: Consistent with the LOS first-order model (within expected statistical noise at this range).\n");
                }
                "QualitativeMatch" => {
                    md.push_str("- Interpretation: Qualitative sign pattern matches LOS (≥60%), but quantitative deviation remains (expected: higher-order terms and huge N make chi-squared very large).\n");
                }
                "PartialMatch" => {
                    md.push_str("- Interpretation: Partial sign pattern agreement (40-60%). Some LOS predictions hold, but significant deviations exist.\n");
                }
                "TheoryInconsistent" => {
                    md.push_str("- Interpretation: Sign pattern contradicts LOS predictions (<40% agreement).\n");
                }
                _ => {}
            }

            // Top deviations
            let mut top = comp.pairwise.clone();
            top.sort_by(|a, b| {
                b.z_score
                    .abs()
                    .partial_cmp(&a.z_score.abs())
                    .unwrap_or(Ordering::Equal)
            });
            if top.len() > 12 {
                top.truncate(12);
            }
            md.push_str("\n#### Top deviations (|z|)\n");
            md.push_str("| (a→b) | z-score | P_obs | P_theory | c_est | c_sign |\n");
            md.push_str("|-------|--------:|------:|---------:|------:|-------:|\n");
            for r in &top {
                md.push_str(&format!(
                    "| {}→{} | {:+.3} | {:.6} | {:.6} | {:+.3} | {} |\n",
                    r.from_residue,
                    r.to_residue,
                    r.z_score,
                    r.p_observed,
                    r.p_theory,
                    r.estimated_c,
                    r.expected_c_sign
                ));
            }

            // Residual matrix for mod 10
            if comp.modulus == 10 {
                md.push_str("\n#### Residual matrix (z-scores)\n");
                md.push_str("| From\\To | 1 | 3 | 7 | 9 |\n");
                md.push_str("|---------|---:|---:|---:|---:|\n");
                let idx10 = |v: u64| -> Option<usize> {
                    match v {
                        1 => Some(0),
                        3 => Some(1),
                        7 => Some(2),
                        9 => Some(3),
                        _ => None,
                    }
                };
                let mut z = [[0.0f64; 4]; 4];
                for r in &comp.pairwise {
                    if let (Some(i), Some(j)) = (idx10(r.from_residue), idx10(r.to_residue)) {
                        z[i][j] = r.z_score;
                    }
                }
                for (ri, &from) in [1u64, 3, 7, 9].iter().enumerate() {
                    md.push_str(&format!(
                        "| {} | {:+.2} | {:+.2} | {:+.2} | {:+.2} |\n",
                        from, z[ri][0], z[ri][1], z[ri][2], z[ri][3]
                    ));
                }
            }

            // Scaling analysis
            if let Some(sc) = &comp.scaling {
                md.push_str("\n#### Scaling analysis (prefix by max p)\n");
                md.push_str(&format!(
                    "- Convergence: {}\n",
                    if sc.convergence_ok { "YES" } else { "NO" }
                ));
                md.push_str("\n| max_p | log10(max_p) | sign_match |\n");
                md.push_str("|-------|--------------|------------|\n");
                for b in &sc.bin_data {
                    md.push_str(&format!(
                        "| {} | {:.3} | {} |\n",
                        b.max_p,
                        b.log10_mid,
                        if b.sign_match { "YES" } else { "NO" }
                    ));
                }
            }
        }
    }

    let data = json!({
        "file": file_path.trim(),
        "mod30": {
            "total_triplets": bundle.mod30_triplets,
            "result": &bundle.mod30,
        },
        "mod210": {
            "total_triplets": bundle.mod210_triplets,
            "result": &bundle.mod210,
        },
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

fn format_validation_subreport(md: &mut String, result: &ValidationResult, total: u64) {
    md.push_str(&format!("**Total triplets**: {total}\n\n"));

    // --- Summary (human-friendly) ---
    let integrity_ok = result.integrity.c1_c2_consistent
        && result.integrity.row_sums_ok
        && (result.integrity.expected_triplets == 0
            || result.integrity.c2_sum == result.integrity.expected_triplets);
    let wheel_pick = result
        .wheel
        .iter()
        .find(|w| w.baseline.to_ascii_lowercase().contains("thinned"))
        .or_else(|| result.wheel.first());
    let verdict = if !integrity_ok {
        "FAIL"
    } else if let Some(w) = wheel_pick {
        if w.delta_cmi > 0.0 && w.delta_delta_ll > 0.0 {
            "PASS"
        } else {
            "WARN"
        }
    } else {
        "WARN"
    };
    let key_line = if let Some(w) = wheel_pick {
        format!(
            "- Key finding: baseline={}, ΔCMI={:+.9} nats, ΔΔLL={:+.9} bits\n",
            w.baseline, w.delta_cmi, w.delta_delta_ll
        )
    } else {
        "- Key finding: wheel comparison not available\n".to_string()
    };
    let interpretation = match verdict {
        "FAIL" => "Integrity check failed. Suspect implementation bugs or input inconsistencies first.",
        "PASS" => "Residual structure beyond wheel baseline detected. This is consistent with prime-specific patterns (Lemke Oliver-Soundararajan, 2016).",
        _ => "Difference is comparable to wheel baseline. The observed effect is likely sieve-derived (wheel structure only).",
    };
    md.push_str("### Summary\n");
    md.push_str(&format!("- Verdict: {verdict}\n"));
    md.push_str(&key_line);
    md.push_str(&format!("- Interpretation: {interpretation}\n\n"));

    md.push_str(&format!(
        "**Overall conclusion**: {}\n\n",
        result.overall_conclusion
    ));

    md.push_str("### Methodology\n");
    md.push_str("*This report is a quick sanity check to distinguish computation errors from genuine prime-specific vs sieve-derived patterns.*\n");
    md.push_str("- Holdout: in-sequence split (every 5th triplet is test)\n");
    md.push_str("- Smoothing: Laplace (alpha=1e-3) for log-loss evaluation\n");
    md.push_str("- CMI: computed from full (unsmoothed) counts (nats)\n\n");

    md.push_str("### A. Integrity Check\n");
    md.push_str("*Internal consistency check. FAIL strongly indicates implementation bugs or input inconsistencies.*\n");
    md.push_str("| Item | Value |\n|------|-------|\n");
    md.push_str(&format!("| C2 sum | {} |\n", result.integrity.c2_sum));
    md.push_str(&format!(
        "| Expected triplets | {} |\n",
        result.integrity.expected_triplets
    ));
    md.push_str(&format!(
        "| C1/C2 consistent | {} |\n",
        if result.integrity.c1_c2_consistent {
            "PASS"
        } else {
            "FAIL"
        }
    ));
    md.push_str(&format!(
        "| Row sums OK | {} |\n",
        if result.integrity.row_sums_ok {
            "PASS"
        } else {
            "FAIL"
        }
    ));
    md.push('\n');

    md.push_str("### B. Theory Comparison (P(k|j) vs uniform)\n");
    md.push_str("*Compares P(k|j) against uniform (1/phi). Small p-value indicates non-uniformity, but large samples yield tiny p-values even for minor deviations.*\n");
    md.push_str("| Metric | Value |\n|--------|-------|\n");
    md.push_str(&format!(
        "| expected_uniform | {:.12} |\n",
        result.theory.expected_uniform
    ));
    md.push_str(&format!(
        "| max_deviation | {:.12} |\n",
        result.theory.max_deviation
    ));
    md.push_str(&format!(
        "| chi_squared | {:.6} |\n",
        result.theory.chi_squared
    ));
    md.push_str(&format!("| df | {} |\n", result.theory.df));
    md.push_str(&format!("| p_value | {:.6} |\n", result.theory.p_value));
    md.push('\n');

    md.push_str("### C. Wheel Random Comparison (Critical)\n");
    md.push_str("*Compares primes with random sequences under the same coprimality constraint (wheel). Residual difference suggests prime-specific structure; no difference suggests sieve-derived.*\n");
    md.push_str("| Baseline | q | Prime CMI | Wheel CMI | Δ CMI | Prime ΔLL | Wheel ΔLL | Δ ΔLL | Conclusion |\n");
    md.push_str("|----------|---|----------:|----------:|------:|----------:|----------:|------:|------------|\n");
    for w in &result.wheel {
        let q = if w.accept_prob_q.is_finite() {
            format!("{:.6}", w.accept_prob_q)
        } else {
            "—".to_string()
        };
        md.push_str(&format!(
            "| {} | {} | {:.9} | {:.9} | {:+.9} | {:.9} | {:.9} | {:+.9} | {} |\n",
            w.baseline,
            q,
            w.prime_cmi,
            w.wheel_cmi,
            w.delta_cmi,
            w.prime_delta_ll,
            w.wheel_delta_ll,
            w.delta_delta_ll,
            w.conclusion
        ));
    }
    md.push('\n');

    md.push_str("### D. Range Dependency (prefix by max p)\n");
    md.push_str(
        "*Checks whether statistics stabilize (or fluctuate in sign) as max_p increases.*\n",
    );
    if result.range_dep.ranges.is_empty() {
        md.push_str("_No ranges computed._\n");
    } else {
        md.push_str("| max_p | triplets | CMI (nats) | ΔLL (bits) |\n");
        md.push_str("|-------|----------|-----------|------------|\n");
        for r in &result.range_dep.ranges {
            md.push_str(&format!(
                "| {} | {} | {:.9} | {:.9} |\n",
                r.max_p, r.triplets, r.cmi, r.delta_ll
            ));
        }
    }
}

#[allow(dead_code)]
fn format_validation_as_markdown(result: &ValidationResult, total: u64, file_path: &str) -> String {
    let mut md = String::new();
    md.push_str("# Validation Report\n\n");
    md.push_str(&format!("**File**: {}\n", file_path.trim()));
    md.push_str(&format!("**Modulus**: mod {}\n", result.modulus));
    md.push_str(&format!("**Total triplets**: {total}\n\n"));

    // --- Summary (human-friendly) ---
    let integrity_ok = result.integrity.c1_c2_consistent
        && result.integrity.row_sums_ok
        && (result.integrity.expected_triplets == 0
            || result.integrity.c2_sum == result.integrity.expected_triplets);
    let wheel_pick = result
        .wheel
        .iter()
        .find(|w| w.baseline.to_ascii_lowercase().contains("thinned"))
        .or_else(|| result.wheel.first());
    let verdict = if !integrity_ok {
        "FAIL"
    } else if let Some(w) = wheel_pick {
        if w.delta_cmi > 0.0 && w.delta_delta_ll > 0.0 {
            "PASS"
        } else {
            "WARN"
        }
    } else {
        "WARN"
    };
    let key_line = if let Some(w) = wheel_pick {
        format!(
            "- Key finding: baseline={}, ΔCMI={:+.9} nats, ΔΔLL={:+.9} bits\n",
            w.baseline, w.delta_cmi, w.delta_delta_ll
        )
    } else {
        "- Key finding: wheel comparison not available\n".to_string()
    };
    let interpretation = match verdict {
        "FAIL" => "Integrity check failed. Suspect implementation bugs or input inconsistencies first.",
        "PASS" => "Residual structure beyond wheel baseline detected. This is consistent with prime-specific patterns (Lemke Oliver-Soundararajan, 2016).",
        _ => "Difference is comparable to wheel baseline. The observed effect is likely sieve-derived (wheel structure only).",
    };
    md.push_str("## Summary\n");
    md.push_str(&format!("- Verdict: {verdict}\n"));
    md.push_str(&key_line);
    md.push_str(&format!("- Interpretation: {interpretation}\n\n"));

    md.push_str(&format!(
        "**Overall conclusion**: {}\n\n",
        result.overall_conclusion
    ));

    md.push_str("## Methodology\n");
    md.push_str("*This report is a quick sanity check to distinguish computation errors from genuine prime-specific vs sieve-derived patterns.*\n");
    md.push_str("- Holdout: in-sequence split (every 5th triplet is test)\n");
    md.push_str("- Smoothing: Laplace (alpha=1e-3) for log-loss evaluation\n");
    md.push_str("- CMI: computed from full (unsmoothed) counts (nats)\n\n");

    md.push_str("## A. Integrity Check\n");
    md.push_str("*Internal consistency check. FAIL strongly indicates implementation bugs or input inconsistencies.*\n");
    md.push_str("| Item | Value |\n|------|-------|\n");
    md.push_str(&format!("| C2 sum | {} |\n", result.integrity.c2_sum));
    md.push_str(&format!(
        "| Expected triplets | {} |\n",
        result.integrity.expected_triplets
    ));
    md.push_str(&format!(
        "| C1/C2 consistent | {} |\n",
        if result.integrity.c1_c2_consistent {
            "PASS"
        } else {
            "FAIL"
        }
    ));
    md.push_str(&format!(
        "| Row sums OK | {} |\n",
        if result.integrity.row_sums_ok {
            "PASS"
        } else {
            "FAIL"
        }
    ));
    md.push('\n');

    md.push_str("## B. Theory Comparison (P(k|j) vs uniform)\n");
    md.push_str("*Compares P(k|j) against uniform (1/phi). Small p-value indicates non-uniformity, but large samples yield tiny p-values even for minor deviations.*\n");
    md.push_str("| Metric | Value |\n|--------|-------|\n");
    md.push_str(&format!(
        "| expected_uniform | {:.12} |\n",
        result.theory.expected_uniform
    ));
    md.push_str(&format!(
        "| max_deviation | {:.12} |\n",
        result.theory.max_deviation
    ));
    md.push_str(&format!(
        "| chi_squared | {:.6} |\n",
        result.theory.chi_squared
    ));
    md.push_str(&format!("| df | {} |\n", result.theory.df));
    md.push_str(&format!("| p_value | {:.6} |\n", result.theory.p_value));
    md.push('\n');

    md.push_str("## C. Wheel Random Comparison (Critical)\n");
    md.push_str("*Compares primes with random sequences under the same coprimality constraint (wheel). Residual difference suggests prime-specific structure; no difference suggests sieve-derived.*\n");
    md.push_str("| Baseline | q | Prime CMI | Wheel CMI | Δ CMI | Prime ΔLL | Wheel ΔLL | Δ ΔLL | Conclusion |\n");
    md.push_str("|----------|---|----------:|----------:|------:|----------:|----------:|------:|------------|\n");
    for w in &result.wheel {
        let q = if w.accept_prob_q.is_finite() {
            format!("{:.6}", w.accept_prob_q)
        } else {
            "—".to_string()
        };
        md.push_str(&format!(
            "| {} | {} | {:.9} | {:.9} | {:+.9} | {:.9} | {:.9} | {:+.9} | {} |\n",
            w.baseline,
            q,
            w.prime_cmi,
            w.wheel_cmi,
            w.delta_cmi,
            w.prime_delta_ll,
            w.wheel_delta_ll,
            w.delta_delta_ll,
            w.conclusion
        ));
    }
    md.push('\n');

    md.push_str("## D. Range Dependency (prefix by max p)\n");
    md.push_str(
        "*Checks whether statistics stabilize (or fluctuate in sign) as max_p increases.*\n",
    );
    if result.range_dep.ranges.is_empty() {
        md.push_str("_No ranges computed._\n");
    } else {
        md.push_str("| max_p | triplets | CMI (nats) | ΔLL (bits) |\n");
        md.push_str("|-------|----------|-----------|------------|\n");
        for r in &result.range_dep.ranges {
            md.push_str(&format!(
                "| {} | {} | {:.9} | {:.9} |\n",
                r.max_p, r.triplets, r.cmi, r.delta_ll
            ));
        }
    }

    md.push_str("\n\n## E. Theory (Lemke Oliver–Soundararajan, 2016)\n");
    md.push_str("*Reference: arXiv:1603.03720 \"Unexpected biases in the distribution of consecutive primes\"*\n");
    if result.lemke_oliver.is_none() {
        md.push_str("_No theory comparison computed._\n");
    } else if let Some(list) = &result.lemke_oliver {
        for comp in list {
            md.push_str(&format!(
                "\n### mod {} (prime_max={})\n\n",
                comp.modulus, comp.prime_max
            ));
            md.push_str("#### Summary\n");
            md.push_str(&format!("- Verdict: {}\n", comp.verdict));
            md.push_str(&format!(
                "- χ² = {:.6}, df = {}, p = {:.6}\n",
                comp.chi_squared, comp.df, comp.p_value
            ));
            // Qualitative sign agreement ratio (based on z-score sign vs expected c-sign)
            let mut ok = 0u64;
            let mut tot = 0u64;
            for r in &comp.pairwise {
                if r.expected_c_sign == 0 {
                    continue;
                }
                tot += 1;
                let s = if r.z_score > 0.0 {
                    1
                } else if r.z_score < 0.0 {
                    -1
                } else {
                    0
                };
                if s == r.expected_c_sign {
                    ok += 1;
                }
            }
            if tot > 0 {
                let pct = (ok as f64) * 100.0 / (tot as f64);
                md.push_str(&format!("- sign_match_ratio = {ok}/{tot} ({pct:.1}%)\n"));
            }

            match comp.verdict.as_str() {
                "Consistent" => {
                    md.push_str("- Interpretation: Consistent with the LOS first-order model (within expected statistical noise at this range).\n");
                }
                "QualitativeMatch" => {
                    md.push_str("- Interpretation: Qualitative sign pattern matches LOS (≥60%), but quantitative deviation remains (expected: higher-order terms and huge N make χ² very large).\n");
                }
                "PartialMatch" => {
                    md.push_str("- Interpretation: Partial sign pattern agreement (40-60%). Some LOS predictions hold, but significant deviations exist.\n");
                }
                "TheoryInconsistent" => {
                    md.push_str("- Interpretation: Sign pattern contradicts LOS predictions (<40% agreement).\n");
                }
                _ => {}
            }
            if comp.modulus == 30 {
                md.push_str("- Note: mod 30 correction coefficients are approximate/heuristic (for sign-pattern verification only).\n");
            }

            // Top deviations
            let mut top = comp.pairwise.clone();
            top.sort_by(|a, b| {
                b.z_score
                    .abs()
                    .partial_cmp(&a.z_score.abs())
                    .unwrap_or(Ordering::Equal)
            });
            if top.len() > 12 {
                top.truncate(12);
            }
            md.push_str("\n#### Top deviations (|z|)\n");
            md.push_str("| (a→b) | z-score | P_obs | P_theory | c_est | c_sign |\n");
            md.push_str("|-------|--------:|------:|---------:|------:|-------:|\n");
            for r in &top {
                md.push_str(&format!(
                    "| {}→{} | {:+.3} | {:.6} | {:.6} | {:+.3} | {} |\n",
                    r.from_residue,
                    r.to_residue,
                    r.z_score,
                    r.p_observed,
                    r.p_theory,
                    r.estimated_c,
                    r.expected_c_sign
                ));
            }

            // mod10 is small: print full matrix
            if comp.modulus == 10 {
                md.push_str("\n#### Residual matrix (z-scores)\n");
                md.push_str("| From\\To | 1 | 3 | 7 | 9 |\n");
                md.push_str("|---------|---:|---:|---:|---:|\n");
                let idx10 = |v: u64| -> Option<usize> {
                    match v {
                        1 => Some(0),
                        3 => Some(1),
                        7 => Some(2),
                        9 => Some(3),
                        _ => None,
                    }
                };
                let mut z = [[0.0f64; 4]; 4];
                for r in &comp.pairwise {
                    if let (Some(i), Some(j)) = (idx10(r.from_residue), idx10(r.to_residue)) {
                        z[i][j] = r.z_score;
                    }
                }
                for (ri, &from) in [1u64, 3, 7, 9].iter().enumerate() {
                    md.push_str(&format!(
                        "| {} | {:+.2} | {:+.2} | {:+.2} | {:+.2} |\n",
                        from, z[ri][0], z[ri][1], z[ri][2], z[ri][3]
                    ));
                }
            }

            if let Some(sc) = &comp.scaling {
                md.push_str("\n#### Scaling analysis (prefix by max p)\n");
                md.push_str(&format!(
                    "- Convergence: {}\n",
                    if sc.convergence_ok { "YES" } else { "NO" }
                ));
                md.push_str("| max_p | log10(max_p) | sign_match |\n");
                md.push_str("|-------|--------------:|-----------|\n");
                for b in &sc.bin_data {
                    md.push_str(&format!(
                        "| {} | {:.3} | {} |\n",
                        b.max_p,
                        b.log10_mid,
                        if b.sign_match { "YES" } else { "NO" }
                    ));
                }
            }
        }
    }

    let data = json!({
        "file": file_path.trim(),
        "modulus": result.modulus,
        "total_triplets": total,
        "overall_conclusion": result.overall_conclusion,
        "integrity": result.integrity,
        "theory": result.theory,
        "lemke_oliver": result.lemke_oliver,
        "wheel": result.wheel,
        "range_dependency": result.range_dep,
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

pub(crate) fn render_validation_results(ui: &mut egui::Ui, state: &AnalyzeState) {
    let bundle = if state.running {
        try_read_shared(&state.shared_validation, &state.shared_processed)
            .map(|(b, _)| b)
            .unwrap_or_default()
    } else {
        state.validation.clone()
    };

    ui.label(section_title("Validation (dual: mod 30 + mod 210)"));
    ui.add_space(8.0);

    egui::CollapsingHeader::new(format!("mod 30 (triplets={})", bundle.mod30_triplets))
        .default_open(true)
        .show(ui, |ui| {
            render_validation_modulus(ui, "mod30", &bundle.mod30, bundle.mod30_triplets);
        });

    ui.add_space(10.0);
    egui::CollapsingHeader::new(format!("mod 210 (triplets={})", bundle.mod210_triplets))
        .default_open(false)
        .show(ui, |ui| {
            render_validation_modulus(ui, "mod210", &bundle.mod210, bundle.mod210_triplets);
        });

    ui.add_space(16.0);
    ui.label(section_title(
        "E. Theory (Lemke Oliver–Soundararajan, 2016)",
    ));
    ui.add_space(8.0);
    let lo = bundle
        .mod30
        .lemke_oliver
        .as_ref()
        .or(bundle.mod210.lemke_oliver.as_ref());
    if let Some(list) = lo {
        render_los_ui(ui, list);
    } else {
        label_secondary(ui, "No theory comparison computed.");
    }
}

fn render_validation_modulus(
    ui: &mut egui::Ui,
    id_prefix: &str,
    view_result: &ValidationResult,
    triplets_total: u64,
) {
    ui.add_space(4.0);
    label_secondary(
        ui,
        format!("Overall conclusion: {}", view_result.overall_conclusion),
    );
    ui.add_space(12.0);

    ui.label(section_title("A. Integrity Check"));
    ui.add_space(8.0);
    egui::Grid::new(format!("validation_integrity_grid_{id_prefix}"))
        .striped(true)
        .show(ui, |ui| {
            label_secondary(ui, "C2 sum");
            label_primary(ui, view_result.integrity.c2_sum);
            ui.end_row();
            label_secondary(ui, "Expected triplets");
            label_primary(ui, view_result.integrity.expected_triplets);
            ui.end_row();
            label_secondary(ui, "C1/C2 consistent");
            label_primary(
                ui,
                if view_result.integrity.c1_c2_consistent {
                    "PASS"
                } else {
                    "FAIL"
                },
            );
            ui.end_row();
            label_secondary(ui, "Row sums OK");
            label_primary(
                ui,
                if view_result.integrity.row_sums_ok {
                    "PASS"
                } else {
                    "FAIL"
                },
            );
            ui.end_row();
        });
    ui.add_space(16.0);

    ui.label(section_title("B. Theory Comparison"));
    ui.add_space(8.0);
    egui::Grid::new(format!("validation_theory_grid_{id_prefix}"))
        .striped(true)
        .show(ui, |ui| {
            label_secondary(ui, "Expected uniform");
            label_primary(ui, format!("{:.12}", view_result.theory.expected_uniform));
            ui.end_row();
            label_secondary(ui, "Max deviation");
            label_primary(ui, format!("{:.12}", view_result.theory.max_deviation));
            ui.end_row();
            label_secondary(ui, "Chi-squared");
            label_primary(ui, format!("{:.6}", view_result.theory.chi_squared));
            ui.end_row();
            label_secondary(ui, "df");
            label_primary(ui, view_result.theory.df);
            ui.end_row();
            label_secondary(ui, "p-value");
            label_primary(ui, format!("{:.6}", view_result.theory.p_value));
            ui.end_row();
        });
    ui.add_space(16.0);

    ui.label(section_title("C. Wheel Random Comparison (Critical)"));
    ui.add_space(8.0);
    egui::Grid::new(format!("validation_wheel_grid_{id_prefix}"))
        .striped(true)
        .show(ui, |ui| {
            label_secondary(ui, "baseline");
            label_secondary(ui, "q");
            label_secondary(ui, "wheel_triplets");
            label_secondary(ui, "ΔCMI");
            label_secondary(ui, "ΔΔLL");
            label_secondary(ui, "conclusion");
            ui.end_row();
            for w in &view_result.wheel {
                label_primary(ui, &w.baseline);
                label_primary(
                    ui,
                    if w.accept_prob_q.is_finite() {
                        format!("{:.6}", w.accept_prob_q)
                    } else {
                        "—".to_string()
                    },
                );
                label_primary(ui, w.wheel_sample_size);
                label_primary(ui, format!("{:+.9}", w.delta_cmi));
                label_primary(ui, format!("{:+.9}", w.delta_delta_ll));
                label_primary(ui, &w.conclusion);
                ui.end_row();
            }
        });
    ui.add_space(16.0);

    ui.label(section_title("D. Range Dependency (prefix by max p)"));
    ui.add_space(8.0);
    if view_result.range_dep.ranges.is_empty() {
        label_secondary(ui, "No ranges computed.");
    } else {
        egui::Grid::new(format!("validation_ranges_grid_{id_prefix}"))
            .striped(true)
            .show(ui, |ui| {
                label_secondary(ui, "max_p");
                label_secondary(ui, "triplets");
                label_secondary(ui, "CMI (nats)");
                label_secondary(ui, "ΔLL (bits)");
                ui.end_row();
                for r in &view_result.range_dep.ranges {
                    label_primary(ui, r.max_p);
                    label_primary(ui, r.triplets);
                    label_primary(ui, format!("{:.9}", r.cmi));
                    label_primary(ui, format!("{:.9}", r.delta_ll));
                    ui.end_row();
                }
            });
    }

    ui.add_space(12.0);
    label_secondary(ui, format!("Triplets (this modulus): {triplets_total}"));
}

fn render_los_ui(
    ui: &mut egui::Ui,
    list: &[crate::analyze::tab_validation::LemkeOliverComparison],
) {
    for (idx, comp) in list.iter().enumerate() {
        let mut ok = 0u64;
        let mut tot = 0u64;
        for r in &comp.pairwise {
            if r.expected_c_sign == 0 {
                continue;
            }
            tot += 1;
            let s = if r.z_score > 0.0 {
                1
            } else if r.z_score < 0.0 {
                -1
            } else {
                0
            };
            if s == r.expected_c_sign {
                ok += 1;
            }
        }
        let header = format!(
            "mod {} (prime_max={}, verdict={}{}{})",
            comp.modulus,
            comp.prime_max,
            comp.verdict,
            if tot > 0 { ", sign_match=" } else { "" },
            if tot > 0 {
                format!(
                    "{}/{} ({:.1}%)",
                    ok,
                    tot,
                    (ok as f64) * 100.0 / (tot as f64)
                )
            } else {
                "".to_string()
            }
        );
        ui.label(section_title(&header));
        ui.add_space(6.0);
        egui::Grid::new(format!("validation_lo_grid_{idx}"))
            .striped(true)
            .show(ui, |ui| {
                label_secondary(ui, "chi-squared");
                label_primary(ui, format!("{:.6}", comp.chi_squared));
                ui.end_row();
                label_secondary(ui, "df");
                label_primary(ui, comp.df);
                ui.end_row();
                label_secondary(ui, "p-value");
                label_primary(ui, format!("{:.6}", comp.p_value));
                ui.end_row();
            });
        ui.add_space(8.0);

        let mut top = comp.pairwise.clone();
        top.sort_by(|a, b| {
            b.z_score
                .abs()
                .partial_cmp(&a.z_score.abs())
                .unwrap_or(Ordering::Equal)
        });
        top.truncate(10);
        ui.label(section_title("Top deviations (|z|)"));
        ui.add_space(6.0);
        egui::Grid::new(format!("validation_lo_top_grid_{idx}"))
            .striped(true)
            .show(ui, |ui| {
                label_secondary(ui, "a→b");
                label_secondary(ui, "z");
                label_secondary(ui, "P_obs");
                label_secondary(ui, "P_theory");
                ui.end_row();
                for r in top {
                    label_primary(ui, format!("{}→{}", r.from_residue, r.to_residue));
                    label_primary(ui, format!("{:+.3}", r.z_score));
                    label_primary(ui, format!("{:.6}", r.p_observed));
                    label_primary(ui, format!("{:.6}", r.p_theory));
                    ui.end_row();
                }
            });
        ui.add_space(16.0);
    }
}

#[allow(dead_code)]
fn render_validation_results_single(ui: &mut egui::Ui, state: &AnalyzeState) {
    // Legacy single-view renderer (kept for reference); shows mod 30 side.
    let (bundle, _progress_total) = if state.running {
        try_read_shared(&state.shared_validation, &state.shared_processed)
            .unwrap_or_else(|| (ValidationBundle::default(), 0))
    } else {
        (state.validation.clone(), state.total_primes)
    };
    let view_total = bundle.mod30_triplets;
    let view_result = bundle.mod30;

    let title = format!("Validation (mod {})", view_result.modulus);
    ui.label(section_title(&title));
    ui.add_space(8.0);

    ui.label(
        egui::RichText::new(format!(
            "Overall conclusion: {}",
            view_result.overall_conclusion
        ))
        .size(font_sizes::BODY)
        .color(colors::TEXT_PRIMARY),
    );
    ui.add_space(16.0);

    ui.label(section_title("A. Integrity Check"));
    ui.add_space(8.0);
    egui::Grid::new("validation_integrity_grid")
        .striped(true)
        .show(ui, |ui| {
            label_secondary(ui, "C2 sum");
            label_primary(ui, view_result.integrity.c2_sum);
            ui.end_row();
            label_secondary(ui, "Expected triplets");
            label_primary(ui, view_result.integrity.expected_triplets);
            ui.end_row();
            label_secondary(ui, "C1/C2 consistent");
            label_primary(
                ui,
                if view_result.integrity.c1_c2_consistent {
                    "PASS"
                } else {
                    "FAIL"
                },
            );
            ui.end_row();
            label_secondary(ui, "Row sums OK");
            label_primary(
                ui,
                if view_result.integrity.row_sums_ok {
                    "PASS"
                } else {
                    "FAIL"
                },
            );
            ui.end_row();
        });
    ui.add_space(16.0);

    ui.label(section_title("B. Theory Comparison"));
    ui.add_space(8.0);
    egui::Grid::new("validation_theory_grid")
        .striped(true)
        .show(ui, |ui| {
            label_secondary(ui, "Expected uniform");
            label_primary(ui, format!("{:.12}", view_result.theory.expected_uniform));
            ui.end_row();
            label_secondary(ui, "Max deviation");
            label_primary(ui, format!("{:.12}", view_result.theory.max_deviation));
            ui.end_row();
            label_secondary(ui, "Chi-squared");
            label_primary(ui, format!("{:.6}", view_result.theory.chi_squared));
            ui.end_row();
            label_secondary(ui, "df");
            label_primary(ui, view_result.theory.df);
            ui.end_row();
            label_secondary(ui, "p-value");
            label_primary(ui, format!("{:.6}", view_result.theory.p_value));
            ui.end_row();
        });
    ui.add_space(16.0);

    ui.label(section_title("C. Wheel Random Comparison (Critical)"));
    ui.add_space(8.0);
    egui::Grid::new("validation_wheel_grid")
        .striped(true)
        .show(ui, |ui| {
            label_secondary(ui, "baseline");
            label_secondary(ui, "q");
            label_secondary(ui, "wheel_triplets");
            label_secondary(ui, "ΔCMI");
            label_secondary(ui, "ΔΔLL");
            label_secondary(ui, "conclusion");
            ui.end_row();
            for w in &view_result.wheel {
                label_primary(ui, &w.baseline);
                label_primary(
                    ui,
                    if w.accept_prob_q.is_finite() {
                        format!("{:.6}", w.accept_prob_q)
                    } else {
                        "—".to_string()
                    },
                );
                label_primary(ui, w.wheel_sample_size);
                label_primary(ui, format!("{:+.9}", w.delta_cmi));
                label_primary(ui, format!("{:+.9}", w.delta_delta_ll));
                label_primary(ui, &w.conclusion);
                ui.end_row();
            }
        });
    ui.add_space(16.0);

    ui.label(section_title("D. Range Dependency (prefix by max p)"));
    ui.add_space(8.0);
    if view_result.range_dep.ranges.is_empty() {
        label_secondary(ui, "No ranges computed.");
    } else {
        egui::Grid::new("validation_ranges_grid")
            .striped(true)
            .show(ui, |ui| {
                label_secondary(ui, "max_p");
                label_secondary(ui, "triplets");
                label_secondary(ui, "CMI (nats)");
                label_secondary(ui, "ΔLL (bits)");
                ui.end_row();
                for r in &view_result.range_dep.ranges {
                    label_primary(ui, r.max_p);
                    label_primary(ui, r.triplets);
                    label_primary(ui, format!("{:.9}", r.cmi));
                    label_primary(ui, format!("{:.9}", r.delta_ll));
                    ui.end_row();
                }
            });
    }

    ui.add_space(16.0);
    ui.label(section_title(
        "E. Theory (Lemke Oliver–Soundararajan, 2016)",
    ));
    ui.add_space(8.0);
    if view_result.lemke_oliver.is_none() {
        label_secondary(ui, "No theory comparison computed.");
    } else if let Some(list) = &view_result.lemke_oliver {
        for (idx, comp) in list.iter().enumerate() {
            // Qualitative sign agreement ratio (based on z-score sign vs expected c-sign)
            let mut ok = 0u64;
            let mut tot = 0u64;
            for r in &comp.pairwise {
                if r.expected_c_sign == 0 {
                    continue;
                }
                tot += 1;
                let s = if r.z_score > 0.0 {
                    1
                } else if r.z_score < 0.0 {
                    -1
                } else {
                    0
                };
                if s == r.expected_c_sign {
                    ok += 1;
                }
            }
            let header = format!(
                "mod {} (prime_max={}, verdict={}{}{})",
                comp.modulus,
                comp.prime_max,
                comp.verdict,
                if tot > 0 { ", sign_match=" } else { "" },
                if tot > 0 {
                    format!(
                        "{}/{} ({:.1}%)",
                        ok,
                        tot,
                        (ok as f64) * 100.0 / (tot as f64)
                    )
                } else {
                    "".to_string()
                }
            );
            ui.label(section_title(&header));
            ui.add_space(6.0);
            if comp.modulus == 30 {
                label_secondary(
                    ui,
                    "Note: mod 30 coefficients are approximate/heuristic (for sign-pattern verification only).",
                );
                ui.add_space(6.0);
            }
            egui::Grid::new(format!("validation_lo_grid_{idx}"))
                .striped(true)
                .show(ui, |ui| {
                    label_secondary(ui, "chi-squared");
                    label_primary(ui, format!("{:.6}", comp.chi_squared));
                    ui.end_row();
                    label_secondary(ui, "df");
                    label_primary(ui, comp.df);
                    ui.end_row();
                    label_secondary(ui, "p-value");
                    label_primary(ui, format!("{:.6}", comp.p_value));
                    ui.end_row();
                });
            ui.add_space(8.0);

            // Top deviations by |z|
            let mut top = comp.pairwise.clone();
            top.sort_by(|a, b| {
                b.z_score
                    .abs()
                    .partial_cmp(&a.z_score.abs())
                    .unwrap_or(Ordering::Equal)
            });
            top.truncate(10);
            ui.label(section_title("Top deviations (|z|)"));
            ui.add_space(6.0);
            egui::Grid::new(format!("validation_lo_top_grid_{idx}"))
                .striped(true)
                .show(ui, |ui| {
                    label_secondary(ui, "a→b");
                    label_secondary(ui, "z");
                    label_secondary(ui, "P_obs");
                    label_secondary(ui, "P_theory");
                    ui.end_row();
                    for r in top {
                        label_primary(ui, format!("{}→{}", r.from_residue, r.to_residue));
                        label_primary(ui, format!("{:+.3}", r.z_score));
                        label_primary(ui, format!("{:.6}", r.p_observed));
                        label_primary(ui, format!("{:.6}", r.p_theory));
                        ui.end_row();
                    }
                });

            // mod10 is small: show z-score matrix too
            if comp.modulus == 10 {
                ui.add_space(8.0);
                ui.label(section_title("Residual matrix (z-scores)"));
                ui.add_space(6.0);
                let idx10 = |v: u64| -> Option<usize> {
                    match v {
                        1 => Some(0),
                        3 => Some(1),
                        7 => Some(2),
                        9 => Some(3),
                        _ => None,
                    }
                };
                let mut z = [[0.0f64; 4]; 4];
                for r in &comp.pairwise {
                    if let (Some(i), Some(j)) = (idx10(r.from_residue), idx10(r.to_residue)) {
                        z[i][j] = r.z_score;
                    }
                }
                egui::Grid::new(format!("validation_lo_zmat_{idx}"))
                    .striped(true)
                    .show(ui, |ui| {
                        label_secondary(ui, "From\\To");
                        for &h in &[1u64, 3, 7, 9] {
                            label_secondary(ui, h);
                        }
                        ui.end_row();
                        for (ri, &from) in [1u64, 3, 7, 9].iter().enumerate() {
                            label_secondary(ui, from);
                            for cj in 0..4 {
                                label_primary(ui, format!("{:+.2}", z[ri][cj]));
                            }
                            ui.end_row();
                        }
                    });
            }

            ui.add_space(16.0);
        }
    }

    ui.add_space(12.0);
    label_secondary(ui, format!("Displayed total triplets: {view_total}"));
}
