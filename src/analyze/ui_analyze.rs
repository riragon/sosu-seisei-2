use eframe::egui;
use rfd::FileDialog;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::analyze::tab_last_digit::LastDigitResult;
use crate::analyze::tab_mod30::Mod30Result;
use crate::analyze::tab_prhs::{PRHS210Result, PRHSResult};
use crate::analyze::tab_validation::ValidationBundle;
use crate::analyze::{AnalyzeTab, PRHSBinMode};
use crate::app::MyApp;
use crate::app_state::AnalyzeState;
use crate::ui_components::{
    card_frame, field_label, render_progress_header, section_title, styled_text_edit,
};
use crate::ui_theme::{colors, font_sizes, layout};

use crate::analyze::ui_tab_prhs::format_prhs_as_markdown;
use crate::analyze::ui_tab_prhs::render_prhs_results;
use crate::analyze::ui_tab_prhs210::format_prhs210_as_markdown;
use crate::analyze::ui_tab_prhs210::render_prhs210_results;
use crate::analyze::ui_tab_validation::format_validation_bundle_as_markdown;
use crate::analyze::ui_tab_validation::render_validation_results;

/// Sosu-Analyze モードのパネル（初期: 末尾 1/3/7/9 の出現率テーブル）
pub fn render_analyze_panel(app: &mut MyApp, ctx: &egui::Context) {
    egui::CentralPanel::default()
        .frame(
            egui::Frame::none()
                .fill(colors::SURFACE_BG)
                .inner_margin(egui::Margin::same(layout::PANEL_MARGIN)),
        )
        .show(ctx, |ui| {
            // columns クロージャ内で outer `ui` を参照すると借用衝突するため、先に高さを確定する
            let height = ui.available_height();
            ui.columns(2, |columns| {
                let col_width_l = columns[0].available_width();
                let col_width_r = columns[1].available_width();

                render_common_input_card(&mut columns[0], app, col_width_l, height);
                render_results_card(&mut columns[1], &app.analyze, col_width_r, height);
            });
        });
}

pub(crate) fn try_read_shared<T: Clone>(
    shared: &Arc<Mutex<T>>,
    shared_processed: &Arc<Mutex<u64>>,
) -> Option<(T, u64)> {
    let result = shared.try_lock().ok()?.clone();
    let total = *shared_processed.try_lock().ok()?;
    Some((result, total))
}

pub(crate) fn label_primary(ui: &mut egui::Ui, text: impl ToString) {
    ui.label(
        egui::RichText::new(text.to_string())
            .size(font_sizes::BODY)
            .color(colors::TEXT_PRIMARY),
    );
}

pub(crate) fn label_secondary(ui: &mut egui::Ui, text: impl ToString) {
    ui.label(
        egui::RichText::new(text.to_string())
            .size(font_sizes::BODY)
            .color(colors::TEXT_SECONDARY),
    );
}

fn render_common_input_card(ui: &mut egui::Ui, app: &mut MyApp, _width: f32, height: f32) {
    card_frame().show(ui, |ui| {
        ui.set_min_height(height - layout::CARD_HEIGHT_OFFSET);

        ui.label(section_title("Analyze Tabs"));
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.selectable_value(&mut app.analyze.current_tab, AnalyzeTab::LastDigit, "LastDigit");
            ui.selectable_value(&mut app.analyze.current_tab, AnalyzeTab::Mod30, "Mod30 Trans");
            ui.selectable_value(&mut app.analyze.current_tab, AnalyzeTab::Mod30PRHS, "mod 30 PRHS");
            ui.selectable_value(&mut app.analyze.current_tab, AnalyzeTab::Mod210PRHS, "mod 210 PRHS");
            ui.selectable_value(&mut app.analyze.current_tab, AnalyzeTab::Validation, "Validation");
        });

        ui.add_space(16.0);
        ui.label(section_title("Input"));
        ui.add_space(12.0);

        ui.label(field_label("Binary primes file (.bin)"));
        ui.add_space(4.0);

        let state = &mut app.analyze;
        ui.add_sized(
            [ui.available_width(), layout::INPUT_HEIGHT],
            styled_text_edit(&mut state.file_path),
        );
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            if ui
                .add(egui::Button::new("Browse").min_size(egui::vec2(80.0, layout::BUTTON_HEIGHT)))
                .clicked()
            {
                let mut dialog = FileDialog::new().add_filter("Binary primes", &["bin"]);
                let current = state.file_path.trim();
                if !current.is_empty() {
                    // 既存パスがあれば、そのディレクトリを初期位置にする
                    if let Some(parent) = std::path::Path::new(current).parent() {
                        dialog = dialog.set_directory(parent);
                    }
                }
                if let Some(path) = dialog.pick_file() {
                    state.file_path = path.to_string_lossy().to_string();
                    state.file_loaded = true;
                }
            }

            ui.add_space(8.0);

            // 進捗テキスト（簡易）
            let percent = state.progress.clamp(0.0, 1.0) * 100.0;
            let label = if app.is_running && state.running {
                format!("Reading... {percent:.1}%")
            } else if state.file_loaded && state.total_primes > 0 {
                "Ready".to_string()
            } else {
                "Select a file".to_string()
            };
            ui.label(
                egui::RichText::new(label)
                    .size(font_sizes::LABEL)
                    .color(colors::TEXT_SECONDARY),
            );
        });

        ui.add_space(16.0);

        // 進捗バー（Analyze 実行中のみ）
        if app.is_running && state.running {
            let progress = state.progress.clamp(0.0, 1.0);
            let percent = progress * 100.0;
            render_progress_header(ui, percent, progress);
        }

        ui.add_space(16.0);

        let total_label = match state.current_tab {
            AnalyzeTab::LastDigit => "Total primes (excluding 2 and 5)",
            AnalyzeTab::Mod30 => "Total primes (excluding 2,3,5)",
            AnalyzeTab::Mod30PRHS => "Total samples (triplets)",
            AnalyzeTab::Mod210PRHS => "Total samples (triplets)",
            AnalyzeTab::Validation => "Total samples (triplets)",
        };
        ui.label(field_label(total_label));
        let total_primes = if app.is_running && state.running {
            state
                .shared_processed
                .try_lock()
                .map(|g| *g)
                .unwrap_or(state.total_primes)
        } else {
            state.total_primes
        };
        ui.label(
            egui::RichText::new(format!("{total_primes}"))
                .size(font_sizes::BODY)
                .color(colors::TEXT_PRIMARY),
        );

        // PRHS の設定（左カード側で編集）
        if state.current_tab == AnalyzeTab::Mod30PRHS || state.current_tab == AnalyzeTab::Mod210PRHS {
            ui.add_space(16.0);
            ui.label(section_title("PRHS Settings"));
            ui.add_space(12.0);

            // 初期値を空のときだけ補う（ユーザーが明示的に変更可能）
            if state.prhs_log10_bin_width_input.trim().is_empty() {
                state.prhs_log10_bin_width_input = "0.2".to_string();
            }
            if state.prhs_equal_bin_primes_input.trim().is_empty() {
                state.prhs_equal_bin_primes_input = "5000000".to_string();
            }
            if state.prhs_train_ratio_input.trim().is_empty() {
                state.prhs_train_ratio_input = "0.7".to_string();
            }
            if state.prhs_min_context_nij_input.trim().is_empty() {
                state.prhs_min_context_nij_input = "0".to_string();
            }

            ui.label(field_label("Bin view mode"));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.selectable_value(&mut state.prhs_view_bin_mode, PRHSBinMode::Log10, "Log10");
                ui.selectable_value(
                    &mut state.prhs_view_bin_mode,
                    PRHSBinMode::EqualCount,
                    "EqualCount",
                );
            });
            ui.add_space(12.0);

            ui.label(field_label("Log10 bin width (e.g. 0.2)"));
            ui.add_space(4.0);
            ui.add_sized(
                [ui.available_width(), layout::INPUT_HEIGHT],
                styled_text_edit(&mut state.prhs_log10_bin_width_input),
            );
            ui.add_space(12.0);

            ui.label(field_label("Equal-count bin primes (e.g. 5000000)"));
            ui.add_space(4.0);
            ui.add_sized(
                [ui.available_width(), layout::INPUT_HEIGHT],
                styled_text_edit(&mut state.prhs_equal_bin_primes_input),
            );
            ui.add_space(12.0);

            ui.label(field_label("Train ratio (0.1..0.95, e.g. 0.7)"));
            ui.add_space(4.0);
            ui.add_sized(
                [ui.available_width(), layout::INPUT_HEIGHT],
                styled_text_edit(&mut state.prhs_train_ratio_input),
            );

            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(
                    "Note: global holdout uses file-position split; bin stats use an in-bin split (every 5th sample) for per-bin evaluation.",
                )
                .size(font_sizes::LABEL)
                .color(colors::TEXT_SECONDARY),
            );

            ui.add_space(8.0);
            ui.checkbox(
                &mut state.prhs_exclude_diagonal,
                "Exclude diagonal contexts (i==j) in Top Contexts",
            );

            ui.add_space(8.0);
            ui.label(field_label("Top Contexts min N_ij (e.g. 0, 50, 200)"));
            ui.add_space(4.0);
            ui.add_sized(
                [ui.available_width(), layout::INPUT_HEIGHT],
                styled_text_edit(&mut state.prhs_min_context_nij_input),
            );
        }

        // Validation の設定（左カード側で編集）
        if state.current_tab == AnalyzeTab::Validation {
            ui.add_space(16.0);
            ui.label(section_title("Validation Settings"));
            ui.add_space(12.0);

            if state.validation_seed_input.trim().is_empty() {
                state.validation_seed_input = "1".to_string();
            }
            if state.validation_ranges_input.trim().is_empty() {
                state.validation_ranges_input = "1e6,1e7,1e8".to_string();
            }

            ui.label(field_label("Modulus"));
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("This runs validation for both mod 30 and mod 210 in one pass.")
                    .size(font_sizes::LABEL)
                    .color(colors::TEXT_SECONDARY),
            );
            ui.add_space(12.0);

            ui.label(field_label(
                "Wheel samples (triplets). Empty = same as prime triplets",
            ));
            ui.add_space(4.0);
            ui.add_sized(
                [ui.available_width(), layout::INPUT_HEIGHT],
                styled_text_edit(&mut state.validation_wheel_samples_input),
            );
            ui.add_space(12.0);

            ui.label(field_label("Random seed (u64)"));
            ui.add_space(4.0);
            ui.add_sized(
                [ui.available_width(), layout::INPUT_HEIGHT],
                styled_text_edit(&mut state.validation_seed_input),
            );
            ui.add_space(12.0);

            ui.label(field_label(
                "Range dependency (max p list, e.g. 1e6,1e7,1e8 or 10^6)",
            ));
            ui.add_space(4.0);
            ui.add_sized(
                [ui.available_width(), layout::INPUT_HEIGHT],
                styled_text_edit(&mut state.validation_ranges_input),
            );
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(
                    "Note: holdout uses an in-sequence split (every 5th triplet is test).",
                )
                .size(font_sizes::LABEL)
                .color(colors::TEXT_SECONDARY),
            );
        }
    });
}

/// Markdown 出力から JSON セクションを除去する（Copy ボタン用）
fn strip_json_section(md: &str) -> String {
    if let Some(pos) = md.find("\n\n## DATA (machine-readable)") {
        md[..pos].to_string()
    } else {
        md.to_string()
    }
}

/// Auto-save 用: 指定タブの Markdown レポート（JSON セクションなし）を生成する。
pub(crate) fn build_analyze_report_markdown_nojson(
    state: &AnalyzeState,
    tab: AnalyzeTab,
) -> (String, &'static str) {
    let tab_suffix = match tab {
        AnalyzeTab::LastDigit => "lastdigit",
        AnalyzeTab::Mod30 => "mod30",
        AnalyzeTab::Mod30PRHS => "mod30prhs",
        AnalyzeTab::Mod210PRHS => "mod210prhs",
        AnalyzeTab::Validation => "validation",
    };

    let md = match tab {
        AnalyzeTab::LastDigit => crate::analyze::tab_last_digit::format_last_digit_as_markdown(
            &state.last_digit,
            state.total_primes,
            &state.file_path,
        ),
        AnalyzeTab::Mod30 => crate::analyze::tab_mod30::format_mod30_as_markdown(
            &state.mod30,
            state.total_primes,
            &state.file_path,
        ),
        AnalyzeTab::Mod30PRHS => {
            let prhs_log10_bin_width = state
                .prhs_log10_bin_width_input
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|v| *v > 0.0 && *v <= 2.0)
                .unwrap_or(0.2_f64);
            let prhs_equal_bin_primes = state
                .prhs_equal_bin_primes_input
                .trim()
                .parse::<u64>()
                .ok()
                .filter(|v| *v > 0)
                .unwrap_or(5_000_000_u64);
            let prhs_train_ratio = state
                .prhs_train_ratio_input
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|v| *v >= 0.1 && *v <= 0.95)
                .unwrap_or(0.7_f64);
            let prhs_min_nij = state
                .prhs_min_context_nij_input
                .trim()
                .parse::<u64>()
                .ok()
                .unwrap_or(0);

            format_prhs_as_markdown(
                &state.prhs,
                state.total_primes,
                &state.file_path,
                state.prhs_view_bin_mode,
                prhs_log10_bin_width,
                prhs_equal_bin_primes,
                prhs_train_ratio,
                state.prhs_exclude_diagonal,
                prhs_min_nij,
            )
        }
        AnalyzeTab::Mod210PRHS => {
            let prhs_log10_bin_width = state
                .prhs_log10_bin_width_input
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|v| *v > 0.0 && *v <= 2.0)
                .unwrap_or(0.2_f64);
            let prhs_equal_bin_primes = state
                .prhs_equal_bin_primes_input
                .trim()
                .parse::<u64>()
                .ok()
                .filter(|v| *v > 0)
                .unwrap_or(5_000_000_u64);
            let prhs_train_ratio = state
                .prhs_train_ratio_input
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|v| *v >= 0.1 && *v <= 0.95)
                .unwrap_or(0.7_f64);
            let prhs_min_nij = state
                .prhs_min_context_nij_input
                .trim()
                .parse::<u64>()
                .ok()
                .unwrap_or(0);

            format_prhs210_as_markdown(
                &state.prhs210,
                state.total_primes,
                &state.file_path,
                state.prhs_view_bin_mode,
                prhs_log10_bin_width,
                prhs_equal_bin_primes,
                prhs_train_ratio,
                state.prhs_exclude_diagonal,
                prhs_min_nij,
            )
        }
        AnalyzeTab::Validation => {
            format_validation_bundle_as_markdown(&state.validation, &state.file_path)
        }
    };

    (strip_json_section(&md), tab_suffix)
}

/// Markdown 出力から JSON 部分のみを抽出する（JSON ボタン用）
fn extract_json_section(md: &str) -> String {
    if let Some(start) = md.find("```json\n") {
        let json_start = start + 8; // "```json\n" の長さ
        if let Some(end) = md[json_start..].find("\n```") {
            return md[json_start..json_start + end].to_string();
        }
    }
    "{}".to_string()
}

fn render_results_card(ui: &mut egui::Ui, state: &AnalyzeState, _width: f32, height: f32) {
    card_frame().show(ui, |ui| {
        ui.set_min_height(height - layout::CARD_HEIGHT_OFFSET);

        let tab_suffix = match state.current_tab {
            AnalyzeTab::LastDigit => "lastdigit",
            AnalyzeTab::Mod30 => "mod30",
            AnalyzeTab::Mod30PRHS => "mod30prhs",
            AnalyzeTab::Mod210PRHS => "mod210prhs",
            AnalyzeTab::Validation => "validation",
        };
        let save_status_id = egui::Id::new(format!("analyze_save_status_{tab_suffix}"));

        ui.horizontal(|ui| {
            ui.label(section_title("Results"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let json_clicked = ui.button("JSON").clicked();
                let copy_clicked = ui.button("Copy").clicked();
                let save_clicked = ui.button("Save").clicked();

                if copy_clicked || json_clicked || save_clicked {
                    match state.current_tab {
                        AnalyzeTab::LastDigit => {
                            let (result, total) = if state.running {
                                try_read_shared(&state.shared_result, &state.shared_processed)
                                    .unwrap_or_else(|| (LastDigitResult::default(), 0))
                            } else {
                                (state.last_digit.clone(), state.total_primes)
                            };
                            let md = crate::analyze::tab_last_digit::format_last_digit_as_markdown(
                                &result,
                                total,
                                &state.file_path,
                            );
                            if save_clicked {
                                save_markdown_report(
                                    ui,
                                    save_status_id,
                                    &state.file_path,
                                    tab_suffix,
                                    strip_json_section(&md),
                                );
                            } else {
                                let out = if json_clicked {
                                    extract_json_section(&md)
                                } else {
                                    strip_json_section(&md)
                                };
                                ui.ctx().copy_text(out);
                            }
                        }
                        AnalyzeTab::Mod30 => {
                            let (result, total) = if state.running {
                                try_read_shared(&state.shared_mod30, &state.shared_processed)
                                    .unwrap_or_else(|| (Mod30Result::default(), 0))
                            } else {
                                (state.mod30.clone(), state.total_primes)
                            };
                            let md = crate::analyze::tab_mod30::format_mod30_as_markdown(
                                &result,
                                total,
                                &state.file_path,
                            );
                            if save_clicked {
                                save_markdown_report(
                                    ui,
                                    save_status_id,
                                    &state.file_path,
                                    tab_suffix,
                                    strip_json_section(&md),
                                );
                            } else {
                                let out = if json_clicked {
                                    extract_json_section(&md)
                                } else {
                                    strip_json_section(&md)
                                };
                                ui.ctx().copy_text(out);
                            }
                        }
                        AnalyzeTab::Mod30PRHS => {
                            let (result, total) = if state.running {
                                try_read_shared(&state.shared_prhs, &state.shared_processed)
                                    .unwrap_or_else(|| (PRHSResult::default(), 0))
                            } else {
                                (state.prhs.clone(), state.total_primes)
                            };
                            let prhs_log10_bin_width = state
                                .prhs_log10_bin_width_input
                                .trim()
                                .parse::<f64>()
                                .ok()
                                .filter(|v| *v > 0.0 && *v <= 2.0)
                                .unwrap_or(0.2_f64);
                            let prhs_equal_bin_primes = state
                                .prhs_equal_bin_primes_input
                                .trim()
                                .parse::<u64>()
                                .ok()
                                .filter(|v| *v > 0)
                                .unwrap_or(5_000_000_u64);
                            let prhs_train_ratio = state
                                .prhs_train_ratio_input
                                .trim()
                                .parse::<f64>()
                                .ok()
                                .filter(|v| *v >= 0.1 && *v <= 0.95)
                                .unwrap_or(0.7_f64);
                            let prhs_min_nij = state
                                .prhs_min_context_nij_input
                                .trim()
                                .parse::<u64>()
                                .ok()
                                .unwrap_or(0);
                            let md = format_prhs_as_markdown(
                                &result,
                                total,
                                &state.file_path,
                                state.prhs_view_bin_mode,
                                prhs_log10_bin_width,
                                prhs_equal_bin_primes,
                                prhs_train_ratio,
                                state.prhs_exclude_diagonal,
                                prhs_min_nij,
                            );
                            if save_clicked {
                                save_markdown_report(
                                    ui,
                                    save_status_id,
                                    &state.file_path,
                                    tab_suffix,
                                    strip_json_section(&md),
                                );
                            } else {
                                let out = if json_clicked {
                                    extract_json_section(&md)
                                } else {
                                    strip_json_section(&md)
                                };
                                ui.ctx().copy_text(out);
                            }
                        }
                        AnalyzeTab::Mod210PRHS => {
                            let (result, total) = if state.running {
                                try_read_shared(&state.shared_prhs210, &state.shared_processed)
                                    .unwrap_or_else(|| (PRHS210Result::default(), 0))
                            } else {
                                (state.prhs210.clone(), state.total_primes)
                            };
                            let prhs_log10_bin_width = state
                                .prhs_log10_bin_width_input
                                .trim()
                                .parse::<f64>()
                                .ok()
                                .filter(|v| *v > 0.0 && *v <= 2.0)
                                .unwrap_or(0.2_f64);
                            let prhs_equal_bin_primes = state
                                .prhs_equal_bin_primes_input
                                .trim()
                                .parse::<u64>()
                                .ok()
                                .filter(|v| *v > 0)
                                .unwrap_or(5_000_000_u64);
                            let prhs_train_ratio = state
                                .prhs_train_ratio_input
                                .trim()
                                .parse::<f64>()
                                .ok()
                                .filter(|v| *v >= 0.1 && *v <= 0.95)
                                .unwrap_or(0.7_f64);
                            let prhs_min_nij = state
                                .prhs_min_context_nij_input
                                .trim()
                                .parse::<u64>()
                                .ok()
                                .unwrap_or(0);
                            let md = format_prhs210_as_markdown(
                                &result,
                                total,
                                &state.file_path,
                                state.prhs_view_bin_mode,
                                prhs_log10_bin_width,
                                prhs_equal_bin_primes,
                                prhs_train_ratio,
                                state.prhs_exclude_diagonal,
                                prhs_min_nij,
                            );
                            if save_clicked {
                                save_markdown_report(
                                    ui,
                                    save_status_id,
                                    &state.file_path,
                                    tab_suffix,
                                    strip_json_section(&md),
                                );
                            } else {
                                let out = if json_clicked {
                                    extract_json_section(&md)
                                } else {
                                    strip_json_section(&md)
                                };
                                ui.ctx().copy_text(out);
                            }
                        }
                        AnalyzeTab::Validation => {
                            let bundle: ValidationBundle = if state.running {
                                try_read_shared(&state.shared_validation, &state.shared_processed)
                                    .map(|(b, _)| b)
                                    .unwrap_or_default()
                            } else {
                                state.validation.clone()
                            };
                            let md =
                                format_validation_bundle_as_markdown(&bundle, &state.file_path);
                            if save_clicked {
                                save_markdown_report(
                                    ui,
                                    save_status_id,
                                    &state.file_path,
                                    tab_suffix,
                                    strip_json_section(&md),
                                );
                            } else {
                                let out = if json_clicked {
                                    extract_json_section(&md)
                                } else {
                                    strip_json_section(&md)
                                };
                                ui.ctx().copy_text(out);
                            }
                        }
                    }
                }
            });
        });

        let save_msg: Option<String> = ui
            .ctx()
            .data_mut(|d| d.get_persisted::<String>(save_status_id));
        if let Some(msg) = save_msg {
            if !msg.trim().is_empty() {
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(msg)
                        .size(font_sizes::LABEL)
                        .color(colors::TEXT_SECONDARY),
                );
            }
        }
        ui.add_space(12.0);

        egui::ScrollArea::vertical()
            .id_salt("analyze_results_scroll")
            .show(ui, |ui| match state.current_tab {
                AnalyzeTab::LastDigit => render_last_digit_results(ui, state),
                AnalyzeTab::Mod30 => render_mod30_results(ui, state),
                AnalyzeTab::Mod30PRHS => render_prhs_results(ui, state),
                AnalyzeTab::Mod210PRHS => render_prhs210_results(ui, state),
                AnalyzeTab::Validation => render_validation_results(ui, state),
            });
    });
}

fn save_markdown_report(
    ui: &egui::Ui,
    save_status_id: egui::Id,
    input_file_path: &str,
    tab_suffix: &str,
    markdown: String,
) {
    let trimmed = input_file_path.trim();
    if trimmed.is_empty() {
        ui.ctx().data_mut(|d| {
            d.insert_persisted(
                save_status_id,
                "Save failed: input file path is empty (select a file first).".to_string(),
            );
        });
        return;
    }

    let base = PathBuf::from(trimmed);
    let out_path = base.with_extension(format!("{tab_suffix}.md"));

    match std::fs::write(&out_path, markdown) {
        Ok(_) => {
            ui.ctx().data_mut(|d| {
                d.insert_persisted(
                    save_status_id,
                    format!("Saved: {}", out_path.to_string_lossy()),
                );
            });
        }
        Err(e) => {
            ui.ctx().data_mut(|d| {
                d.insert_persisted(
                    save_status_id,
                    format!("Save failed: {} ({})", out_path.to_string_lossy(), e),
                );
            });
        }
    }
}

fn render_last_digit_results(ui: &mut egui::Ui, state: &AnalyzeState) {
    if state.running {
        let (view_result, view_total) =
            try_read_shared(&state.shared_result, &state.shared_processed)
                .unwrap_or_else(|| (LastDigitResult::default(), 0));
        crate::analyze::tab_last_digit::render_last_digit_results_ui(ui, &view_result, view_total);
    } else {
        crate::analyze::tab_last_digit::render_last_digit_results_ui(
            ui,
            &state.last_digit,
            state.total_primes,
        );
    }
}

fn render_mod30_results(ui: &mut egui::Ui, state: &AnalyzeState) {
    if state.running {
        let (view_result, view_total) =
            try_read_shared(&state.shared_mod30, &state.shared_processed)
                .unwrap_or_else(|| (Mod30Result::default(), 0));
        crate::analyze::tab_mod30::render_mod30_results_ui(ui, &view_result, view_total);
    } else {
        let view_total = state.total_primes;
        let view_result = &state.mod30;
        crate::analyze::tab_mod30::render_mod30_results_ui(ui, view_result, view_total);
    }
}
