use eframe::egui;

use crate::app::MyApp;
use crate::ui_components::{
    calc_percent, card_frame, draw_graph_tooltip, field_label, handle_zoom_and_pan,
    render_progress_header, render_range_input_pair, render_speed_slider, section_title,
    GraphTooltipStyle, ZoomPanState,
};
use crate::ui_graph_utils::{
    compute_graph_rect, draw_axes, pick_hovered_bar, AxisLabels, BarInfo, GraphMargins,
    DEFAULT_ZOOM_CONFIG,
};
use crate::ui_theme::{colors, font_sizes, layout};

/// Gap モードのパネル（素数ギャップのヒストグラム）
pub fn render_gap_panel(app: &mut MyApp, ctx: &egui::Context) {
    egui::CentralPanel::default()
        .frame(
            egui::Frame::none()
                .fill(colors::SURFACE_BG)
                .inner_margin(egui::Margin::same(layout::PANEL_MARGIN)),
        )
        .show(ctx, |ui| {
            // 上部: Range と Progress を横並び
            let top_card_height = layout::TOP_CARD_HEIGHT;

            ui.columns(2, |columns| {
                // 左: Range カード
                render_gap_range_card(&mut columns[0], app, top_card_height);

                // 右: Progress カード
                render_gap_progress_card(&mut columns[1], app, top_card_height);
            });

            ui.add_space(layout::CARD_GAP);

            // 下部: ヒストグラム + 統計テキスト
            render_gap_histogram_and_stats(ui, app);
        });
}

/// Gap の Range カード
fn render_gap_range_card(ui: &mut egui::Ui, app: &mut MyApp, height: f32) {
    card_frame().show(ui, |ui| {
        ui.set_min_height(height);

        ui.label(section_title("Range"));
        ui.add_space(12.0);

        // Min/Max 入力と、それぞれの直下に 10^k を表示
        render_range_input_pair(
            ui,
            "Minimum",
            "Maximum",
            &mut app.gap.min_input,
            &mut app.gap.max_input,
            layout::INPUT_WIDTH_MEDIUM,
            layout::INPUT_WIDTH_MEDIUM,
        );
        ui.add_space(12.0);

        // Speed スライダー（共通コンポーネント）
        render_speed_slider(ui, "Speed:", &mut app.gap.speed);
    });
}

/// Gap の Progress カード
fn render_gap_progress_card(ui: &mut egui::Ui, app: &MyApp, height: f32) {
    card_frame().show(ui, |ui| {
        ui.set_min_height(height);

        let percent = calc_percent(app.gap.processed, app.gap.total);

        // 進捗ヘッダー（パーセント + プログレスバー）
        render_progress_header(ui, percent, app.gap.progress);

        ui.add_space(12.0);

        // 詳細情報
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(field_label("Gaps processed"));
                ui.label(
                    egui::RichText::new(if app.gap.total > 0 {
                        format!("{} / {}", app.gap.processed, app.gap.total)
                    } else {
                        "—".to_string()
                    })
                    .size(font_sizes::BODY)
                    .color(colors::TEXT_PRIMARY),
                );
            });

            ui.add_space(24.0);

            ui.vertical(|ui| {
                ui.label(field_label("Current x"));
                ui.label(
                    egui::RichText::new(if app.gap.running || app.gap.prime_count > 0 {
                        format!("{}", app.gap.current_x)
                    } else {
                        "—".to_string()
                    })
                    .size(font_sizes::BODY)
                    .color(colors::ACCENT),
                );
            });

            ui.add_space(24.0);

            ui.vertical(|ui| {
                ui.label(field_label("Primes found"));
                ui.label(
                    egui::RichText::new(format!("{}", app.gap.prime_count))
                        .size(font_sizes::BODY)
                        .color(colors::TEXT_SECONDARY),
                );
            });
        });
    });
}

/// Gap のヒストグラム + 統計テキスト行
fn render_gap_histogram_and_stats(ui: &mut egui::Ui, app: &mut MyApp) {
    ui.columns(2, |columns| {
        render_gap_histogram(&mut columns[0], app);
        render_gap_stats(&mut columns[1], app);
    });
}

/// Gap ヒストグラムを描画（ズーム・ツールチップ対応）
fn render_gap_histogram(ui: &mut egui::Ui, app: &mut MyApp) {
    card_frame().show(ui, |ui| {
        // 下段カードがウィンドウ下端まできれいに伸びるよう、残り高さいっぱいを使う
        ui.set_min_height(ui.available_height());

        // ヘッダー:
        // 1行目: タイトルのみ
        ui.horizontal(|ui| {
            ui.label(section_title("Gap Histogram"));
        });

        // 2行目: Reset / Zoom / Scale（右寄せ）
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // 右端: Reset View ボタン
                if ui
                    .add(egui::Button::new("Reset View").min_size(egui::vec2(80.0, 24.0)))
                    .clicked()
                {
                    app.gap.view = ZoomPanState::default();
                }

                // ズーム率表示
                ui.label(
                    egui::RichText::new(format!("{:.0}%", app.gap.view.zoom * 100.0))
                        .size(font_sizes::LABEL)
                        .color(colors::TEXT_SECONDARY),
                );

                ui.add_space(16.0);

                // Log/Linear スケール切り替えボタン（左側に表示）
                let scale_label = if app.gap.log_scale {
                    "Scale: Log"
                } else {
                    "Scale: Linear"
                };
                if ui
                    .add(egui::Button::new(scale_label).min_size(egui::vec2(110.0, 24.0)))
                    .on_hover_text("ギャップ出現頻度のスケール（Linear/Log）を切り替え")
                    .clicked()
                {
                    app.gap.log_scale = !app.gap.log_scale;
                }
            });
        });
        ui.add_space(8.0);

        let rect = ui.available_rect_before_wrap();
        let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
        let painter = ui.painter_at(rect);

        painter.rect_filled(rect, 0.0, colors::CARD_BG);

        if app.gap.data.is_empty() {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Press Run to start gap visualization\n\nMouse wheel: Zoom\nDrag: Pan",
                egui::FontId::proportional(16.0),
                colors::TEXT_SECONDARY,
            );
            return;
        }

        // 全ギャップ統計（ランキング用）
        let mut all_freq: Vec<(u64, u64)> = app.gap.data.iter().map(|(&g, &c)| (g, c)).collect();
        all_freq.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let total_gaps: u64 = all_freq.iter().map(|(_, c)| *c).sum();

        // ヒストグラム描画用（x 軸順にソート）
        let mut bins: Vec<(u64, u64)> = app.gap.data.iter().map(|(g, c)| (*g, *c)).collect();
        bins.sort_by_key(|(g, _)| *g);

        if bins.is_empty() {
            return;
        }

        let max_count = bins.iter().map(|(_, c)| *c).max().unwrap_or(1).max(1);

        // グラフ領域を共通ヘルパーで計算
        let margins = GraphMargins::default();
        let graph_rect = compute_graph_rect(rect, &margins);

        // ズーム・パン入力処理（共通設定）
        handle_zoom_and_pan(
            ui,
            graph_rect,
            &response,
            &mut app.gap.view,
            &DEFAULT_ZOOM_CONFIG,
        );

        let hover_pos = response.hover_pos();

        // 軸描画（共通ヘルパー）
        let n_bins = bins.len();
        let axis_labels = if n_bins > 0 {
            AxisLabels {
                y_max: format!("{}", max_count),
                y_min: "0".to_string(),
                x_min: format!("{}", bins.first().map(|(g, _)| *g).unwrap_or(0)),
                x_max: format!("{}", bins.last().map(|(g, _)| *g).unwrap_or(0)),
            }
        } else {
            AxisLabels::default()
        };
        draw_axes(
            &painter,
            graph_rect,
            &app.gap.view,
            &axis_labels,
            colors::TEXT_SECONDARY,
        );

        // バー情報を構築
        let bin_count = bins.len() as f32;
        let bin_width = if bin_count > 0.0 {
            graph_rect.width() / bin_count
        } else {
            0.0
        };

        // 対数スケール用の最大値計算
        let log_max = (max_count as f32 + 1.0).log10();

        let bar_infos: Vec<BarInfo> = bins
            .iter()
            .enumerate()
            .map(|(i, (_, count))| {
                let i_f = i as f32;
                let x0 = graph_rect.min.x + i_f * bin_width + bin_width * 0.1;
                let x1 = graph_rect.min.x + (i_f + 1.0) * bin_width - bin_width * 0.1;
                // 最小高さを4pxに設定し、出現数1でも見えるようにする
                let min_bar_height = 4.0;
                let ratio = if app.gap.log_scale {
                    // 対数スケール: log10(count+1) / log10(max_count+1)
                    (*count as f32 + 1.0).log10() / log_max
                } else {
                    // 線形スケール
                    *count as f32 / max_count as f32
                };
                let h = (ratio * graph_rect.height()).max(min_bar_height);
                let y1 = graph_rect.max.y;
                let y0 = y1 - h;

                BarInfo {
                    center_x: (x0 + x1) * 0.5,
                    center_y: (y0 + y1) * 0.5,
                    half_width: (x1 - x0) * 0.5,
                    half_height: (y1 - y0) * 0.5,
                }
            })
            .collect();

        // バー描画（共通ヘルパー）
        let bar_rects: Vec<egui::Rect> = bar_infos
            .iter()
            .map(|bar| {
                crate::ui_graph_utils::draw_bar(
                    &painter,
                    graph_rect,
                    &app.gap.view,
                    bar,
                    colors::ACCENT,
                    2.0,
                )
            })
            .collect();

        // ホバー判定（共通ヘルパー）
        let hover_info: Option<(egui::Pos2, String)> =
            pick_hovered_bar(hover_pos, &bar_rects).map(|idx| {
                let (gap, count) = bins[idx];
                let ratio = if total_gaps > 0 {
                    count as f64 / total_gaps as f64 * 100.0
                } else {
                    0.0
                };
                let text = format!("gap = {}\ncount = {} ({:.2}%)", gap, count, ratio);
                (hover_pos.unwrap(), text)
            });

        // 右上にトップ10ランキング（gap, count, ratio）を小さく表示（位置は固定のまま）
        if total_gaps > 0 && !all_freq.is_empty() {
            let max_rank = usize::min(10, all_freq.len());
            let mut y = graph_rect.min.y + 4.0;
            let x = graph_rect.max.x - 6.0;

            painter.text(
                egui::pos2(x, y),
                egui::Align2::RIGHT_TOP,
                "Top gaps",
                egui::FontId::proportional(10.0),
                colors::TEXT_SECONDARY,
            );
            y += 12.0;

            for (rank, (gap, count)) in all_freq.iter().take(max_rank).enumerate() {
                let ratio = (*count as f64 / total_gaps as f64) * 100.0;
                let line = format!("{}. {}: {} ({:.1}%)", rank + 1, gap, count, ratio);
                painter.text(
                    egui::pos2(x, y),
                    egui::Align2::RIGHT_TOP,
                    line,
                    egui::FontId::proportional(9.0),
                    colors::TEXT_SECONDARY,
                );
                y += 11.0;
            }
        }

        // ツールチップ描画（カード外にはみ出しても表示されるようオーバーレイペインタを使用）
        if let Some((pos, text)) = hover_info {
            let style = GraphTooltipStyle::default();
            let overlay_painter = ui.painter_at(ui.max_rect());
            draw_graph_tooltip(&overlay_painter, pos, &text, &style);
        }
    });
}

/// Gap 統計テキストカード
fn render_gap_stats(ui: &mut egui::Ui, app: &MyApp) {
    card_frame().show(ui, |ui| {
        // Histogram カードと同様に、残り高さいっぱいを使う
        ui.set_min_height(ui.available_height());

        ui.label(section_title("Statistics"));
        ui.add_space(8.0);

        if app.gap.data.is_empty() {
            ui.label(
                egui::RichText::new("No data yet")
                    .size(font_sizes::LABEL)
                    .color(colors::TEXT_SECONDARY),
            );
            return;
        }

        let mut min_gap: Option<u64> = None;
        let mut total_gaps: u64 = 0;
        let mut total_weighted: u128 = 0;
        let mut mode_gap: u64 = 0;
        let mut mode_count: u64 = 0;
        let mut twin_count: u64 = 0;

        let mut sorted: Vec<(u64, u64)> = app.gap.data.iter().map(|(&g, &c)| (g, c)).collect();
        sorted.sort_by_key(|(g, _)| *g);

        for (gap, count) in sorted.iter() {
            let gap = *gap;
            let count = *count;
            min_gap = Some(min_gap.map_or(gap, |m| m.min(gap)));
            total_gaps += count;
            total_weighted += (gap as u128) * (count as u128);
            if count > mode_count {
                mode_count = count;
                mode_gap = gap;
            }
            if gap == 2 {
                twin_count = count;
            }
        }

        let avg_gap = if total_gaps > 0 {
            (total_weighted as f64) / (total_gaps as f64)
        } else {
            0.0
        };

        // median gap（離散分布の中央値）
        let median_gap = if total_gaps > 0 {
            let target = (total_gaps + 1) / 2; // 1-based 中央
            let mut acc = 0u64;
            let mut med = 0u64;
            for (gap, count) in sorted.iter() {
                acc += *count;
                if acc >= target {
                    med = *gap;
                    break;
                }
            }
            med
        } else {
            0
        };

        let twin_ratio = if total_gaps > 0 {
            twin_count as f64 / total_gaps as f64
        } else {
            0.0
        };

        // カード内を左右 2 カラムに分けて、縦方向の詰まりを軽減する
        ui.columns(2, |columns| {
            // 左カラム: Prime / Gaps / Min / Twin primes
            columns[0].vertical(|ui| {
                ui.label(field_label("Prime count"));
                ui.label(
                    egui::RichText::new(format!("{}", app.gap.prime_count))
                        .size(font_sizes::BODY)
                        .color(colors::TEXT_PRIMARY),
                );

                ui.add_space(8.0);

                ui.label(field_label("Gaps count"));
                ui.label(
                    egui::RichText::new(format!("{}", total_gaps))
                        .size(font_sizes::BODY)
                        .color(colors::TEXT_PRIMARY),
                );

                ui.add_space(8.0);

                ui.label(field_label("Min gap"));
                ui.label(
                    egui::RichText::new(
                        min_gap
                            .map(|g| g.to_string())
                            .unwrap_or_else(|| "—".to_string()),
                    )
                    .size(font_sizes::BODY)
                    .color(colors::TEXT_PRIMARY),
                );

                ui.add_space(8.0);

                ui.label(field_label("Twin primes (gap = 2)"));
                ui.label(
                    egui::RichText::new(if total_gaps > 0 {
                        format!("{} ({:.2}% of gaps)", twin_count, twin_ratio * 100.0)
                    } else {
                        "—".to_string()
                    })
                    .size(font_sizes::BODY)
                    .color(colors::TEXT_PRIMARY),
                );
            });

            // 右カラム: Max / Average / Median / Mode
            columns[1].vertical(|ui| {
                ui.label(field_label("Max gap"));
                ui.label(
                    egui::RichText::new(if app.gap.max_gap_value > 0 {
                        format!(
                            "{} (between p = {} and {})",
                            app.gap.max_gap_value,
                            app.gap.max_gap_prev_prime,
                            app.gap.max_gap_prime
                        )
                    } else {
                        "—".to_string()
                    })
                    .size(font_sizes::BODY)
                    .color(colors::TEXT_PRIMARY),
                );

                ui.add_space(8.0);

                ui.label(field_label("Average gap"));
                ui.label(
                    egui::RichText::new(format!("{:.2}", avg_gap))
                        .size(font_sizes::BODY)
                        .color(colors::TEXT_PRIMARY),
                );

                ui.add_space(8.0);

                ui.label(field_label("Median gap"));
                ui.label(
                    egui::RichText::new(if total_gaps > 0 {
                        format!("{}", median_gap)
                    } else {
                        "—".to_string()
                    })
                    .size(font_sizes::BODY)
                    .color(colors::TEXT_PRIMARY),
                );

                ui.add_space(8.0);

                ui.label(field_label("Mode gap"));
                ui.label(
                    egui::RichText::new(if mode_count > 0 {
                        format!("{} ({} times)", mode_gap, mode_count)
                    } else {
                        "—".to_string()
                    })
                    .size(font_sizes::BODY)
                    .color(colors::TEXT_PRIMARY),
                );
            });
        });
    });
}
