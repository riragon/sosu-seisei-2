use eframe::egui;

use crate::app::MyApp;
use crate::ui_components::{
    calc_percent, card_frame, draw_graph_tooltip, field_label, handle_zoom_and_pan,
    render_progress_header, render_range_input_pair, render_speed_slider, section_title,
    styled_text_edit, GraphTooltipStyle, ZoomPanState,
};
use crate::ui_graph_utils::{
    compute_graph_rect, draw_axes, draw_expected_density_line, expected_line_color,
    pick_hovered_bar, AxisLabels, BarInfo, GraphMargins, DEFAULT_ZOOM_CONFIG,
};
use crate::ui_theme::{colors, font_sizes, layout};

/// Density モードのパネル（区間ごとの素数密度）
pub fn render_density_panel(app: &mut MyApp, ctx: &egui::Context) {
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
                render_density_range_card(&mut columns[0], app, top_card_height);

                // 右: Progress カード
                render_density_progress_card(&mut columns[1], app, top_card_height);
            });

            ui.add_space(layout::CARD_GAP);

            // 下部: 棒グラフ + 統計テキスト
            render_density_histogram_and_stats(ui, app);
        });
}

/// Density の Range カード
fn render_density_range_card(ui: &mut egui::Ui, app: &mut MyApp, height: f32) {
    card_frame().show(ui, |ui| {
        ui.set_min_height(height);

        ui.label(section_title("Range"));
        ui.add_space(12.0);

        // Min/Max 入力と、それぞれの直下に 10^k を表示
        render_range_input_pair(
            ui,
            "Minimum",
            "Maximum",
            &mut app.density.min_input,
            &mut app.density.max_input,
            layout::INPUT_WIDTH_SMALL,
            layout::INPUT_WIDTH_SMALL,
        );

        ui.add_space(8.0);
        ui.add_space(8.0);

        // Interval
        ui.horizontal(|ui| {
            ui.label(field_label("Interval"));
            ui.add_space(8.0);
            ui.add_sized(
                [120.0, layout::INPUT_HEIGHT],
                styled_text_edit(&mut app.density.interval_input),
            );
        });

        ui.add_space(8.0);

        // Speed スライダー（共通コンポーネント）
        render_speed_slider(ui, "Speed:", &mut app.density.speed);
    });
}

/// Density の Progress カード
fn render_density_progress_card(ui: &mut egui::Ui, app: &MyApp, height: f32) {
    card_frame().show(ui, |ui| {
        ui.set_min_height(height);

        let percent = calc_percent(app.density.processed, app.density.total);

        // 進捗ヘッダー（パーセント + プログレスバー）
        render_progress_header(ui, percent, app.density.progress);

        ui.add_space(12.0);

        // 詳細情報
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(field_label("Intervals"));
                ui.label(
                    egui::RichText::new(if app.density.total > 0 {
                        format!("{} / {}", app.density.processed, app.density.total)
                    } else {
                        "—".to_string()
                    })
                    .size(font_sizes::BODY)
                    .color(colors::TEXT_PRIMARY),
                );
            });

            ui.add_space(24.0);

            ui.vertical(|ui| {
                ui.label(field_label("Current interval"));
                ui.label(
                    egui::RichText::new(if app.density.running || !app.density.data.is_empty() {
                        format!("{}", app.density.current_interval)
                    } else {
                        "—".to_string()
                    })
                    .size(font_sizes::BODY)
                    .color(colors::ACCENT),
                );
            });

            ui.add_space(24.0);

            ui.vertical(|ui| {
                ui.label(field_label("Total primes"));
                ui.label(
                    egui::RichText::new(format!("{}", app.density.total_primes))
                        .size(font_sizes::BODY)
                        .color(colors::TEXT_SECONDARY),
                );
            });
        });
    });
}

/// Density の棒グラフ + 統計テキスト行
fn render_density_histogram_and_stats(ui: &mut egui::Ui, app: &mut MyApp) {
    ui.columns(2, |columns| {
        render_density_histogram(&mut columns[0], app);
        render_density_stats(&mut columns[1], app);
    });
}

/// Density 棒グラフを描画
fn render_density_histogram(ui: &mut egui::Ui, app: &mut MyApp) {
    card_frame().show(ui, |ui| {
        // 下段カードがウィンドウ下端まできれいに伸びるよう、残り高さいっぱいを使う
        ui.set_min_height(ui.available_height());

        // 1行目: タイトルのみ
        ui.horizontal(|ui| {
            ui.label(section_title("Density (primes per interval)"));
        });

        // 2行目: ズーム表示 + Reset ボタン + 横幅スケール（右寄せ）
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // 右端: Reset View ボタン
                if ui
                    .add(egui::Button::new("Reset View").min_size(egui::vec2(80.0, 24.0)))
                    .clicked()
                {
                    app.density.view = ZoomPanState::default();
                    app.density.bar_width_scale = 1.0;
                }

                // ズーム率表示
                ui.label(
                    egui::RichText::new(format!("{:.0}%", app.density.view.zoom * 100.0))
                        .size(font_sizes::LABEL)
                        .color(colors::TEXT_SECONDARY),
                );

                ui.add_space(16.0);

                // バー横幅スケール（0.5〜10.0）
                ui.label(
                    egui::RichText::new("Width")
                        .size(font_sizes::LABEL)
                        .color(colors::TEXT_SECONDARY),
                );
                let mut scale = app.density.bar_width_scale;
                ui.add(
                    egui::Slider::new(&mut scale, 0.5..=10.0)
                        .show_value(false)
                        .clamping(egui::SliderClamping::Always)
                        .drag_value_speed(0.01),
                );
                app.density.bar_width_scale = scale;
            });
        });

        ui.add_space(8.0);

        let rect = ui.available_rect_before_wrap();
        let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
        let painter = ui.painter_at(rect);

        painter.rect_filled(rect, 0.0, colors::CARD_BG);

        if app.density.data.is_empty() {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Press Run to start density visualization\n\nMouse wheel: Zoom\nDrag: Pan",
                egui::FontId::proportional(16.0),
                colors::TEXT_SECONDARY,
            );
            return;
        }

        let mut bins = app.density.data.clone();
        bins.sort_by_key(|(start, _)| *start);

        let max_count = bins.iter().map(|(_, c)| *c).max().unwrap_or(1).max(1);

        // グラフ領域を共通ヘルパーで計算
        let margins = GraphMargins::default();
        let graph_rect = compute_graph_rect(rect, &margins);

        // ズーム・パン入力処理（共通設定）
        handle_zoom_and_pan(
            ui,
            graph_rect,
            &response,
            &mut app.density.view,
            &DEFAULT_ZOOM_CONFIG,
        );

        let hover_pos = response.hover_pos();

        // 区間幅（ツールチップ用の密度計算に使用）
        let interval_size = app
            .density
            .interval_input
            .trim()
            .parse::<u64>()
            .unwrap_or(1)
            .max(1);

        // 軸描画（共通ヘルパー）
        let n_bins = bins.len();
        let axis_labels = if n_bins > 0 {
            AxisLabels {
                y_max: format!("{}", max_count),
                y_min: "0".to_string(),
                x_min: format!("{}", bins.first().map(|(s, _)| *s).unwrap_or(0)),
                x_max: format!("{}", bins.last().map(|(s, _)| *s).unwrap_or(0)),
            }
        } else {
            AxisLabels::default()
        };
        draw_axes(
            &painter,
            graph_rect,
            &app.density.view,
            &axis_labels,
            colors::TEXT_SECONDARY,
        );

        // バー情報を構築
        let bin_count = bins.len() as f32;
        let base_bin_width = if bin_count > 0.0 {
            graph_rect.width() / bin_count
        } else {
            0.0
        };
        // Width スライダーの効きをさらに強めるため、スケール値を 3 乗で反映する
        let width_scale = app.density.bar_width_scale.max(0.5);
        let width_factor = width_scale * width_scale * width_scale; // 1.0, 8.0, 27.0, ... 最大 1000
        let bin_width = base_bin_width * width_factor;

        let bar_infos: Vec<BarInfo> = bins
            .iter()
            .enumerate()
            .map(|(i, (_, count))| {
                let i_f = i as f32;
                let x0 = graph_rect.min.x + i_f * bin_width + bin_width * 0.1;
                let x1 = graph_rect.min.x + (i_f + 1.0) * bin_width - bin_width * 0.1;
                let h = (*count as f32 / max_count as f32) * graph_rect.height();
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
                    &app.density.view,
                    bar,
                    colors::ACCENT,
                    2.0,
                )
            })
            .collect();

        // 期待値線（理論密度 1/log x に基づく平均線）を描画（共通ヘルパー）
        if interval_size > 0 {
            draw_expected_density_line(
                &painter,
                graph_rect,
                &app.density.view,
                &bins,
                bin_width,
                interval_size,
                max_count,
                expected_line_color(),
            );
        }

        // ホバー判定（共通ヘルパー）
        let hover_info: Option<(egui::Pos2, String)> =
            pick_hovered_bar(hover_pos, &bar_rects).map(|idx| {
                let (start, count) = bins[idx];
                let end = start.saturating_add(interval_size.saturating_sub(1));
                let density = count as f64 / interval_size as f64;
                let text = format!(
                    "[{}, {}]\ncount = {}, density = {:.6}",
                    start, end, count, density
                );
                (hover_pos.unwrap(), text)
            });

        // ツールチップ描画（カードのクリップに制限されないよう、画面全体ペインタを使用）
        if let Some((pos, text)) = hover_info {
            let style = GraphTooltipStyle::default();
            let overlay_painter = ui.painter_at(ui.max_rect());
            draw_graph_tooltip(&overlay_painter, pos, &text, &style);
        }

        // ------------------------------------------------------------------
        // 横方向ナビゲーション用シークバー
        // ------------------------------------------------------------------
        // バー幅を大きくしたときにグラフ全体が横に非常に長くなるため、
        // 端から端まで一気に移動できるようにする。
        let zoom = app.density.view.zoom.max(0.01);
        let content_left = graph_rect.min.x + bin_width * 0.1;
        let content_right = graph_rect.min.x + bin_width * bin_count - bin_width * 0.1;
        let content_width = (content_right - content_left).max(1.0);
        let view_width_pre = graph_rect.width() / zoom;

        // コンテンツがビューより十分広い場合のみシークバーを表示
        if content_width > view_width_pre * 1.05 {
            let center = graph_rect.center();
            // 現在のパン量から「ズーム前の画面中心座標」を逆算
            let center_pre = center.x - app.density.view.pan_x / zoom;
            let left_view_pre = center_pre - view_width_pre * 0.5;
            let max_offset = (content_width - view_width_pre).max(0.0);

            let current_nav = if max_offset > 0.0 {
                ((left_view_pre - content_left) / max_offset).clamp(0.0, 1.0)
            } else {
                0.0
            };

            let mut nav_pos = current_nav;

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(field_label("Scroll"));
                // デフォルトより長め（おおよそ 2 倍）のスクロールバー幅にする
                let slider = egui::Slider::new(&mut nav_pos, 0.0..=1.0)
                    .show_value(false)
                    .clamping(egui::SliderClamping::Always);
                let resp = ui.add_sized([200.0, 20.0], slider);

                if resp.changed() {
                    // シークバー位置から新しいパン量を計算
                    let left_view_pre_new = content_left + max_offset * nav_pos;
                    let center_pre_new = left_view_pre_new + view_width_pre * 0.5;
                    let dx_pre = center_pre_new - center.x;
                    app.density.view.pan_x = -dx_pre * zoom;
                }
            });
        }
    });
}

/// Density 統計テキストカード
fn render_density_stats(ui: &mut egui::Ui, app: &MyApp) {
    card_frame().show(ui, |ui| {
        // Histogram カードと同様に、残り高さいっぱいを使う
        ui.set_min_height(ui.available_height());

        ui.label(section_title("Statistics"));
        ui.add_space(8.0);

        if app.density.data.is_empty() {
            ui.label(
                egui::RichText::new("No data yet")
                    .size(font_sizes::LABEL)
                    .color(colors::TEXT_SECONDARY),
            );
            return;
        }

        // 区間幅とレンジを取得
        let interval_size = app
            .density
            .interval_input
            .trim()
            .parse::<u64>()
            .unwrap_or(1)
            .max(1);

        let min_x = app.density.min_input.trim().parse::<u64>().unwrap_or(0);
        let max_x = app.density.max_input.trim().parse::<u64>().unwrap_or(min_x);

        let range_len = if max_x > min_x {
            (max_x - min_x) as f64
        } else {
            1.0
        };

        let mut bins = app.density.data.clone();
        bins.sort_by_key(|(start, _)| *start);

        let n_intervals = bins.len() as u64;
        let total_primes: u64 = bins.iter().map(|(_, c)| *c).sum();

        // 全体平均密度
        let avg_density_overall = if range_len > 0.0 {
            total_primes as f64 / range_len
        } else {
            0.0
        };

        // 先頭 / 末尾 10% の平均密度
        let first_k = (n_intervals / 10).max(1) as usize;
        let last_k = (n_intervals / 10).max(1) as usize;

        let avg_density_first = if !bins.is_empty() {
            let slice = &bins[..first_k.min(bins.len())];
            let sum: u64 = slice.iter().map(|(_, c)| *c).sum();
            let len = (slice.len() as u64 * interval_size) as f64;
            if len > 0.0 {
                sum as f64 / len
            } else {
                0.0
            }
        } else {
            0.0
        };

        let avg_density_last = if !bins.is_empty() {
            let slice = &bins[bins.len().saturating_sub(last_k)..];
            let sum: u64 = slice.iter().map(|(_, c)| *c).sum();
            let len = (slice.len() as u64 * interval_size) as f64;
            if len > 0.0 {
                sum as f64 / len
            } else {
                0.0
            }
        } else {
            0.0
        };

        // Max / Min density などの区間統計
        let mut max_density = -1.0_f64;
        let mut max_interval: Option<(u64, u64)> = None;

        let mut min_density = f64::INFINITY;
        let mut min_interval: Option<(u64, u64)> = None;

        for (start, count) in bins.iter() {
            let density = *count as f64 / interval_size as f64;
            let interval_start = *start;
            let interval_end = (*start + interval_size - 1).min(max_x);

            // max density（ゼロも含めて最大をとる）
            if density > max_density {
                max_density = density;
                max_interval = Some((interval_start, interval_end));
            }

            // min density（count > 0 の区間に限定）
            if *count > 0 && density < min_density {
                min_density = density;
                min_interval = Some((interval_start, interval_end));
            }
        }

        // Expected density (1/log x_mid) と Empirical / Expected
        let x_mid = ((min_x + max_x) / 2).max(2);
        let x_mid_f = x_mid as f64;
        let expected_density = if x_mid_f > 1.0 {
            1.0 / x_mid_f.ln()
        } else {
            0.0
        };
        let emp_over_exp = if expected_density > 0.0 {
            avg_density_overall / expected_density
        } else {
            0.0
        };

        // 表示（2カラムに分けて横幅のオーバーフローを防ぐ）
        ui.columns(2, |columns| {
            // 左カラム: 全体的な指標
            columns[0].vertical(|ui| {
                ui.label(field_label("Total primes"));
                ui.label(
                    egui::RichText::new(format!("{}", total_primes))
                        .size(font_sizes::BODY)
                        .color(colors::TEXT_PRIMARY),
                );

                ui.add_space(8.0);

                ui.label(field_label("Average density (overall)"));
                ui.label(
                    egui::RichText::new(format!("{:.6}", avg_density_overall))
                        .size(font_sizes::BODY)
                        .color(colors::TEXT_PRIMARY),
                );

                ui.add_space(8.0);

                ui.label(field_label("Expected density"));
                ui.label(
                    egui::RichText::new(format!("{:.6}  (x_mid = {})", expected_density, x_mid))
                        .size(font_sizes::BODY)
                        .color(colors::TEXT_PRIMARY),
                );

                ui.add_space(8.0);

                ui.label(field_label("Empirical / Expected"));
                ui.label(
                    egui::RichText::new(format!("{:.4}", emp_over_exp))
                        .size(font_sizes::BODY)
                        .color(colors::TEXT_PRIMARY),
                );
            });

            // 右カラム: 先頭/末尾10%, max/min など
            columns[1].vertical(|ui| {
                ui.label(field_label("Average density (first 10%)"));
                ui.label(
                    egui::RichText::new(format!("{:.6}", avg_density_first))
                        .size(font_sizes::BODY)
                        .color(colors::TEXT_PRIMARY),
                );

                ui.add_space(8.0);

                ui.label(field_label("Average density (last 10%)"));
                ui.label(
                    egui::RichText::new(format!("{:.6}", avg_density_last))
                        .size(font_sizes::BODY)
                        .color(colors::TEXT_PRIMARY),
                );

                ui.add_space(8.0);

                ui.label(field_label("Max density"));
                ui.label(
                    egui::RichText::new(if let Some((s, e)) = max_interval {
                        format!("{:.6}\n[{}, {}]", max_density, s, e)
                    } else {
                        "—".to_string()
                    })
                    .size(font_sizes::BODY)
                    .color(colors::TEXT_PRIMARY),
                );

                ui.add_space(8.0);

                ui.label(field_label("Min density (non-zero)"));
                ui.label(
                    egui::RichText::new(if let Some((s, e)) = min_interval {
                        format!("{:.6}\n[{}, {}]", min_density, s, e)
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
