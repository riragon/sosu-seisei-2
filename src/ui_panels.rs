//! メイン画面のパネル描画ロジック。
//!
//! - もともと `app.rs` の `impl MyApp` 内にあった描画メソッドを、
//!   LLM が読みやすいようにこのファイルに集約しています。
//! - すべて `&mut MyApp` を引数に取り、状態は `MyApp` にだけ持たせます。

use eframe::egui;

use crate::app::{AppTab, MyApp};
use crate::config::{OutputFormat, WheelType};
use crate::ui_components::{field_label, section_title, styled_text_edit};
use crate::ui_panel_density::render_density_panel;
use crate::ui_panel_explore::render_explore_panel;
use crate::ui_panel_gap::render_gap_panel;
use crate::ui_panel_generator::render_generator_panel;
use crate::ui_panel_spiral::render_spiral_panel;
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
                // タイトル
                ui.label(
                    egui::RichText::new("Sosu-Seisei")
                        .size(font_sizes::TITLE)
                        .color(colors::TEXT_PRIMARY),
                );

                ui.add_space(16.0);

                // タブボタン: Generator / Explore / Gap / Density / Spiral
                render_tab_buttons(app, ui);

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

    ui.add_space(8.0);

    // Options ボタン（Generator モードのみ表示）
    if app.current_tab == AppTab::Generator {
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
        if ui
            .add(
                egui::Button::new(egui::RichText::new("Run").color(egui::Color32::WHITE))
                    .fill(colors::ACCENT)
                    .min_size(run_button_size),
            )
            .clicked()
        {
            // タブに応じて異なる処理を実行
            match app.current_tab {
                AppTab::Generator => app.start_worker(),
                AppTab::Explore => app.start_explore(),
                AppTab::Gap => app.start_gap(),
                AppTab::Density => app.start_density(),
                AppTab::Spiral => app.start_spiral(),
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
}

/// Advanced Options ウィンドウを描画
pub fn render_advanced_options_window(app: &mut MyApp, ctx: &egui::Context) {
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
    match app.current_tab {
        AppTab::Generator => render_generator_panel(app, ctx),
        AppTab::Explore => render_explore_panel(app, ctx),
        AppTab::Gap => render_gap_panel(app, ctx),
        AppTab::Density => render_density_panel(app, ctx),
        AppTab::Spiral => render_spiral_panel(app, ctx),
    }
}


