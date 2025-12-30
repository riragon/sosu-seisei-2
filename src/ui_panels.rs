//! メイン画面のパネル描画ロジック。
//!
//! - もともと `app.rs` の `impl MyApp` 内にあった描画メソッドを、
//!   LLM が読みやすいようにこのファイルに集約しています。
//! - すべて `&mut MyApp` を引数に取り、状態は `MyApp` にだけ持たせます。

use eframe::egui;

use crate::analyze::ui_analyze::render_analyze_panel;
use crate::analyze::AnalyzeTab;
use crate::app::{AppMode, AppTab, MyApp};
use crate::config::{OutputFormat, WheelType};
use crate::seisei::ui_density::render_density_panel;
use crate::seisei::ui_explore::render_explore_panel;
use crate::seisei::ui_gap::render_gap_panel;
use crate::seisei::ui_generator::render_generator_panel;
use crate::seisei::ui_spiral::render_spiral_panel;
use crate::ui_components::{field_label, section_title, styled_text_edit};
use crate::ui_theme::{colors, font_sizes, layout};

/// ヘッダーパネルを描画
pub fn render_header(app: &mut MyApp, ctx: &egui::Context) {
    egui::TopBottomPanel::top("header")
        .frame(
            egui::Frame::none()
                .fill(colors::SURFACE_BG)
                .inner_margin(egui::Margin::symmetric(24.0, 16.0)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                // タイトル（クリックでモード切り替え）
                let title_text = match app.analyze_mode {
                    AppMode::Seisei => "Sosu-Seisei",
                    AppMode::Analyze => "Sosu-Analyze",
                };
                let title_clicked = ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(title_text)
                                .size(font_sizes::TITLE)
                                .color(colors::TEXT_PRIMARY),
                        )
                        .frame(false),
                    )
                    .clicked();
                if title_clicked {
                    if app.is_running {
                        app.log
                            .push_str("Cannot switch app mode while a computation is running.\n");
                    } else {
                        app.analyze_mode = match app.analyze_mode {
                            AppMode::Seisei => AppMode::Analyze,
                            AppMode::Analyze => AppMode::Seisei,
                        };
                        // モード切替時は Generator のオプションウィンドウを閉じる
                        app.show_advanced_options = false;
                    }
                }

                ui.add_space(16.0);

                // タブボタン（Seisei のみ）
                if app.analyze_mode == AppMode::Seisei {
                    render_tab_buttons(app, ui);
                } else {
                    let label = match app.analyze.current_tab {
                        AnalyzeTab::LastDigit => "LastDigit",
                        AnalyzeTab::Mod30 => "Mod30 Trans",
                        AnalyzeTab::Mod30PRHS => "mod 30 PRHS",
                        AnalyzeTab::Mod210PRHS => "mod 210 PRHS",
                        AnalyzeTab::Validation => "Validation",
                    };
                    ui.label(
                        egui::RichText::new(label)
                            .size(font_sizes::BODY)
                            .color(colors::TEXT_SECONDARY),
                    );
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    render_header_buttons(app, ui);
                });
            });
        });
}

/// タブ切り替えボタン（Generator / π(x) / Gap / Density / Spiral）
fn render_tab_buttons(app: &mut MyApp, ui: &mut egui::Ui) {
    let tabs = [
        ("Generator", AppTab::Generator),
        ("π(x)", AppTab::Explore),
        ("Gap", AppTab::Gap),
        ("Density", AppTab::Density),
        ("Spiral", AppTab::Spiral),
    ];

    for (i, (label, tab)) in tabs.iter().enumerate() {
        if i > 0 {
            ui.add_space(4.0);
        }
        if tab_button(ui, label, app.current_tab == *tab) {
            app.current_tab = *tab;
        }
    }
}

/// 単一タブボタンを描画し、クリックされたかどうかを返す
fn tab_button(ui: &mut egui::Ui, label: &str, selected: bool) -> bool {
    let tab_size = egui::vec2(90.0, 28.0);
    let fill = if selected {
        colors::ACCENT
    } else {
        egui::Color32::TRANSPARENT
    };
    let text_color = if selected {
        egui::Color32::WHITE
    } else {
        colors::TEXT_SECONDARY
    };

    ui.add(
        egui::Button::new(egui::RichText::new(label).color(text_color))
            .fill(fill)
            .min_size(tab_size),
    )
    .clicked()
}

/// ヘッダー内のボタン群を描画
fn render_header_buttons(app: &mut MyApp, ui: &mut egui::Ui) {
    let button_size = egui::vec2(90.0, layout::BUTTON_HEIGHT);
    let run_button_size = egui::vec2(100.0, layout::BUTTON_HEIGHT);
    let all_run_button_size = egui::vec2(110.0, layout::BUTTON_HEIGHT);

    ui.add_space(8.0);

    // Options ボタン（Generator モードのみ表示）
    if app.analyze_mode == AppMode::Seisei && app.current_tab == AppTab::Generator {
        if ui
            .add(egui::Button::new("Options").min_size(button_size))
            .clicked()
        {
            app.show_advanced_options = !app.show_advanced_options;
        }
        ui.add_space(8.0);
    }

    // Run / Stop ボタン
    if !app.is_running {
        // Analyze モードのみ: All Run（全タブ実行 + 自動保存）
        if app.analyze_mode == AppMode::Analyze {
            if ui
                .add(
                    egui::Button::new(egui::RichText::new("All Run").color(egui::Color32::WHITE))
                        .fill(colors::ACCENT)
                        .min_size(all_run_button_size),
                )
                .clicked()
            {
                app.start_all_run();
            }
            ui.add_space(8.0);
        }

        if ui
            .add(
                egui::Button::new(egui::RichText::new("Run").color(egui::Color32::WHITE))
                    .fill(colors::ACCENT)
                    .min_size(run_button_size),
            )
            .clicked()
        {
            // モード／タブに応じて異なる処理を実行
            match app.analyze_mode {
                AppMode::Analyze => app.start_analyze(),
                AppMode::Seisei => match app.current_tab {
                    AppTab::Generator => app.start_worker(),
                    AppTab::Explore => app.start_explore(),
                    AppTab::Gap => app.start_gap(),
                    AppTab::Density => app.start_density(),
                    AppTab::Spiral => app.start_spiral(),
                },
            }
        }
    } else if ui
        .add(
            egui::Button::new(egui::RichText::new("Stop").color(egui::Color32::WHITE))
                .fill(colors::DANGER)
                .min_size(run_button_size),
        )
        .clicked()
    {
        app.stop_flag
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    // All Run 進捗表示（Analyze モードのみ）
    if app.analyze_mode == AppMode::Analyze && app.analyze.all_run_mode {
        ui.add_space(12.0);
        let total = app.analyze.all_run_completed.len()
            + app.analyze.all_run_pending.len()
            + if app.analyze.running { 1 } else { 0 };
        let total = total.max(5); // 期待値（固定5タブ）
        let done = app.analyze.all_run_completed.len();
        let tab = match app.analyze.current_tab {
            AnalyzeTab::LastDigit => "LastDigit",
            AnalyzeTab::Mod30 => "Mod30",
            AnalyzeTab::Mod30PRHS => "Mod30PRHS",
            AnalyzeTab::Mod210PRHS => "Mod210PRHS",
            AnalyzeTab::Validation => "Validation",
        };
        let status = if app.analyze.running {
            format!("All Run: {done}/{total} (running {tab})")
        } else {
            format!("All Run: {done}/{total}")
        };
        ui.label(
            egui::RichText::new(status)
                .size(font_sizes::LABEL)
                .color(colors::TEXT_SECONDARY),
        );
    }
}

/// Advanced Options ウィンドウを描画
pub fn render_advanced_options_window(app: &mut MyApp, ctx: &egui::Context) {
    if app.analyze_mode != AppMode::Seisei {
        return;
    }
    if !app.show_advanced_options {
        return;
    }

    egui::Window::new("Advanced Options")
        .title_bar(false)
        .collapsible(false)
        .resizable(true)
        .default_size([360.0, 450.0])
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .frame(
            egui::Frame::none()
                .fill(colors::CARD_BG)
                .rounding(egui::Rounding::same(layout::CARD_ROUNDING))
                .inner_margin(egui::Margin::same(20.0))
                .shadow(egui::epaint::Shadow {
                    offset: egui::vec2(0.0, 4.0),
                    blur: 20.0,
                    spread: 0.0,
                    color: egui::Color32::from_black_alpha(100),
                }),
        )
        .show(ctx, |ui| {
            ui.set_min_width(300.0);

            // タイトルと Done ボタンを同じ行に
            ui.horizontal(|ui| {
                ui.label(section_title("Advanced Options"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("Done").color(egui::Color32::WHITE),
                            )
                            .fill(colors::ACCENT)
                            .min_size(egui::vec2(70.0, 28.0)),
                        )
                        .clicked()
                    {
                        app.show_advanced_options = false;
                    }
                });
            });

            ui.add_space(12.0);

            // スクロール可能なエリア
            egui::ScrollArea::vertical()
                .max_height(380.0)
                .show(ui, |ui| {
                    render_advanced_options_fields(app, ui);
                });
        });
}

/// Advanced Options のフィールド群を描画
fn render_advanced_options_fields(app: &mut MyApp, ui: &mut egui::Ui) {
    let input_height = 32.0;

    // Split Count
    ui.label(field_label("Split Count"));
    ui.add_space(4.0);
    ui.add_sized(
        [ui.available_width(), input_height],
        styled_text_edit(&mut app.split_count_input),
    );
    ui.add_space(12.0);

    // Segment Size
    ui.label(field_label("Segment Size"));
    ui.add_space(4.0);
    ui.add_sized(
        [ui.available_width(), input_height],
        styled_text_edit(&mut app.segment_size_input),
    );
    ui.add_space(12.0);

    // Buffer Size
    ui.label(field_label("Buffer Size"));
    ui.add_space(4.0);
    ui.add_sized(
        [ui.available_width(), input_height],
        styled_text_edit(&mut app.writer_buffer_size_input),
    );
    ui.add_space(12.0);

    // Format
    ui.label(field_label("Format"));
    ui.add_space(4.0);
    egui::ComboBox::new("output_format", "")
        .selected_text(format!("{:?}", app.selected_format))
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut app.selected_format, OutputFormat::Text, "Text");
            ui.selectable_value(&mut app.selected_format, OutputFormat::CSV, "CSV");
            ui.selectable_value(&mut app.selected_format, OutputFormat::JSON, "JSON");
            ui.selectable_value(&mut app.selected_format, OutputFormat::Binary, "Binary");
        });
    ui.add_space(12.0);

    // Wheel Algorithm
    ui.label(field_label("Wheel Algorithm"));
    ui.add_space(4.0);
    egui::ComboBox::new("wheel_type", "")
        .selected_text(format!("{:?}", app.selected_wheel_type))
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut app.selected_wheel_type, WheelType::Odd, "Odd");
            ui.selectable_value(&mut app.selected_wheel_type, WheelType::Mod6, "Mod6");
            ui.selectable_value(
                &mut app.selected_wheel_type,
                WheelType::Mod30,
                "Mod30 (Recommended)",
            );
        });
    ui.add_space(12.0);

    // Timestamp prefix option
    ui.horizontal(|ui| {
        ui.checkbox(&mut app.use_timestamp_prefix, "");
        ui.label(
            egui::RichText::new("Add timestamp prefix to filenames")
                .size(font_sizes::BODY)
                .color(colors::TEXT_PRIMARY),
        );
    });
}

/// メインパネル（タブに応じて Generator / Explore / Gap / Density / Spiral を描画）
pub fn render_main_panel(app: &mut MyApp, ctx: &egui::Context) {
    match app.analyze_mode {
        AppMode::Analyze => render_analyze_panel(app, ctx),
        AppMode::Seisei => match app.current_tab {
            AppTab::Generator => render_generator_panel(app, ctx),
            AppTab::Explore => render_explore_panel(app, ctx),
            AppTab::Gap => render_gap_panel(app, ctx),
            AppTab::Density => render_density_panel(app, ctx),
            AppTab::Spiral => render_spiral_panel(app, ctx),
        },
    }
}
