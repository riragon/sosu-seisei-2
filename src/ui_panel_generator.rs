use eframe::egui;
use rfd::FileDialog;

use crate::app::MyApp;
use crate::ui_components::{
    card_frame, field_label, render_range_input_pair, section_title, styled_text_edit,
};
use crate::ui_theme::{colors, font_sizes, layout};

/// Generator モードのパネル（4カードレイアウト）
pub fn render_generator_panel(app: &mut MyApp, ctx: &egui::Context) {
    egui::CentralPanel::default()
        .frame(
            egui::Frame::none()
                .fill(colors::SURFACE_BG)
                .inner_margin(egui::Margin::same(layout::PANEL_MARGIN)),
        )
        .show(ctx, |ui| {
            // 利用可能な高さを計算して2分割
            let available_height = ui.available_height();
            let card_height = (available_height - layout::CARD_GAP) / 2.0;

            ui.columns(2, |columns| {
                let col_width_l = columns[0].available_width();
                let col_width_r = columns[1].available_width();

                // 左カラム: Range + Output
                render_range_card(&mut columns[0], app, col_width_l, card_height);
                columns[0].add_space(layout::CARD_GAP);
                render_output_card(&mut columns[0], app, col_width_l, card_height);

                // 右カラム: Progress + Log
                render_progress_card(&mut columns[1], app, col_width_r, card_height);
                columns[1].add_space(layout::CARD_GAP);
                render_log_card(&mut columns[1], app, col_width_r, card_height);
            });
        });
}

/// Range カードを描画
fn render_range_card(ui: &mut egui::Ui, app: &mut MyApp, _width: f32, height: f32) {
    card_frame().show(ui, |ui| {
        ui.set_min_height(height - layout::CARD_HEIGHT_OFFSET);

        ui.label(section_title("Range"));
        ui.add_space(12.0);

        // Minimum / Maximum を他タブと同じ 2 カラムレイアウト・固定幅で表示（各入力の直下に 10^k を表示）
        render_range_input_pair(
            ui,
            "Minimum",
            "Maximum",
            &mut app.prime_min_input,
            &mut app.prime_max_input,
            layout::INPUT_WIDTH_MEDIUM,
            layout::INPUT_WIDTH_MEDIUM,
        );
    });
}

/// Output カードを描画
fn render_output_card(ui: &mut egui::Ui, app: &mut MyApp, _width: f32, height: f32) {
    card_frame().show(ui, |ui| {
        ui.set_min_height(height - layout::CARD_HEIGHT_OFFSET);

        ui.label(section_title("Output"));
        ui.add_space(16.0);

        ui.horizontal(|ui| {
            ui.checkbox(&mut app.last_prime_only, "");
            ui.label(
                egui::RichText::new("Last prime only")
                    .size(font_sizes::BODY)
                    .color(colors::TEXT_PRIMARY),
            );
        });
        ui.label(
            egui::RichText::new("Skip file output, show only the final prime")
                .size(font_sizes::LABEL)
                .color(colors::TEXT_SECONDARY),
        );

        ui.add_space(16.0);

        ui.label(field_label("Directory"));
        ui.add_space(4.0);
        // 入力欄を1行使い、その下に Browse ボタンを配置して干渉を避ける
        ui.add_sized(
            [ui.available_width(), layout::INPUT_HEIGHT],
            styled_text_edit(&mut app.output_dir_input),
        );
        ui.add_space(8.0);
        // Browse ボタンは横幅を固定して左寄せにする
        ui.horizontal(|ui| {
            if ui
                .add(
                    egui::Button::new("Browse")
                        .min_size(egui::vec2(80.0, layout::BUTTON_HEIGHT)),
                )
                .clicked()
            {
                let current = app.output_dir_input.trim();
                let dialog = if current.is_empty() {
                    FileDialog::new()
                } else {
                    FileDialog::new().set_directory(current)
                };
                if let Some(path) = dialog.pick_folder() {
                    app.output_dir_input = path.to_string_lossy().to_string();
                }
            }
        });
    });
}

/// Progress カードを描画
fn render_progress_card(ui: &mut egui::Ui, app: &MyApp, _width: f32, height: f32) {
    card_frame().show(ui, |ui| {
        ui.set_min_height(height - layout::CARD_HEIGHT_OFFSET);

        // 大きな進捗表示
        let percent = if app.total_range > 0 {
            (app.current_processed as f32 / app.total_range as f32) * 100.0
        } else {
            0.0
        };

        ui.label(
            egui::RichText::new(format!("{:.1}%", percent))
                .size(font_sizes::HERO)
                .color(colors::TEXT_PRIMARY),
        );

        ui.add_space(12.0);

        // プログレスバー
        ui.add(
            egui::ProgressBar::new(app.progress)
                .fill(colors::ACCENT)
                .desired_height(8.0),
        );

        ui.add_space(16.0);

        // 詳細情報
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(field_label("Processed"));
                ui.label(
                    egui::RichText::new(if app.total_range > 0 {
                        format!("{} / {}", app.current_processed, app.total_range)
                    } else {
                        "—".to_string()
                    })
                    .size(font_sizes::BODY)
                    .color(colors::TEXT_PRIMARY),
                );
            });

            ui.add_space(32.0);

            ui.vertical(|ui| {
                ui.label(field_label("ETA"));
                ui.label(
                    egui::RichText::new(&app.eta)
                        .size(font_sizes::BODY)
                        .color(colors::TEXT_PRIMARY),
                );
            });
        });

        ui.add_space(12.0);

        ui.horizontal(|ui| {
            ui.label(field_label("Memory"));
            ui.label(
                egui::RichText::new(format!("{} / {} KB", app.mem_usage, app.total_mem))
                    .size(font_sizes::LABEL)
                    .color(colors::TEXT_SECONDARY),
            );
        });
    });
}

/// Log カードを描画
fn render_log_card(ui: &mut egui::Ui, app: &MyApp, _width: f32, height: f32) {
    card_frame().show(ui, |ui| {
        ui.set_min_height(height - layout::CARD_HEIGHT_OFFSET);

        ui.label(section_title("Log"));
        ui.add_space(12.0);

        egui::ScrollArea::vertical()
            .id_salt("log_scroll_right")
            .show(ui, |ui| {
                if app.log.is_empty() {
                    ui.label(
                        egui::RichText::new("No activity yet")
                            .size(font_sizes::LABEL)
                            .color(colors::TEXT_SECONDARY),
                    );
                } else {
                    for line in app.log.lines().rev() {
                        ui.label(
                            egui::RichText::new(line)
                                .size(font_sizes::LABEL)
                                .color(colors::TEXT_SECONDARY),
                        );
                    }
                }
            });
    });
}


