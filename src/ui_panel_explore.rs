use eframe::egui;

use crate::app::{ExploreGraphMode, MyApp};
use crate::ui_components::{
    calc_percent, card_frame, draw_graph_tooltip, field_label, handle_zoom_and_pan,
    render_progress_header, render_range_input_pair, render_speed_slider, section_title,
    GraphTooltipStyle, ZoomPanState,
};
use crate::ui_graph_utils::{
    apply_view_transform, data_to_screen, draw_axes, draw_polyline, pick_closest_point,
    AxisLabels, LegendItem, DEFAULT_ZOOM_CONFIG,
};
use crate::ui_theme::{colors, font_sizes, layout};

/// Explore モードのパネル（π(x) vs x/log x グラフ）
pub fn render_explore_panel(app: &mut MyApp, ctx: &egui::Context) {
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
                render_explore_range_card(&mut columns[0], app, top_card_height);

                // 右: Progress カード
                render_explore_progress_card(&mut columns[1], app, top_card_height);
            });

            ui.add_space(layout::CARD_GAP);

            // 下部: グラフ
            render_explore_graph_card(ui, app);
        });
}

/// Explore の Range カード
fn render_explore_range_card(ui: &mut egui::Ui, app: &mut MyApp, height: f32) {
    card_frame().show(ui, |ui| {
        // Explore の Range カードと Progress カードで同じ高さになるように固定
        ui.set_min_height(height);

        ui.label(section_title("Range"));
        ui.add_space(12.0);

        // Min/Max 入力と、それぞれの直下に 10^k を表示
        render_range_input_pair(
            ui,
            "Minimum",
            "Maximum",
            &mut app.explore_min_input,
            &mut app.explore_max_input,
            layout::INPUT_WIDTH_MEDIUM,
            layout::INPUT_WIDTH_MEDIUM,
        );

        ui.add_space(12.0);

        // Speed スライダー（共通コンポーネント）
        render_speed_slider(ui, "Speed:", &mut app.explore_speed);
    });
}

/// Explore の Progress カード
fn render_explore_progress_card(ui: &mut egui::Ui, app: &MyApp, height: f32) {
    card_frame().show(ui, |ui| {
        // Range カードと同じ高さを維持
        ui.set_min_height(height);

        let percent = calc_percent(app.explore_processed, app.explore_total);

        // 進捗ヘッダー（パーセント + プログレスバー）
        render_progress_header(ui, percent, app.explore_progress);

        ui.add_space(12.0);

        // 詳細情報
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(field_label("Processed"));
                ui.label(
                    egui::RichText::new(if app.explore_total > 0 {
                        format!("{} / {}", app.explore_processed, app.explore_total)
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
                    egui::RichText::new(if app.explore_running || !app.explore_data.is_empty() {
                        format!("{}", app.explore_current_x)
                    } else {
                        "—".to_string()
                    })
                    .size(font_sizes::BODY)
                    .color(colors::ACCENT),
                );
            });

            ui.add_space(24.0);

            ui.vertical(|ui| {
                ui.label(field_label("Data Points"));
                ui.label(
                    egui::RichText::new(format!("{}", app.explore_data.len()))
                        .size(font_sizes::BODY)
                        .color(colors::TEXT_SECONDARY),
                );
            });
        });
    });
}

/// Explore のグラフカード
fn render_explore_graph_card(ui: &mut egui::Ui, app: &mut MyApp) {
    // 上段の Progress カードの右端と揃うように、中央パネルの幅に合わせてカードを配置する
    let available_height = ui.available_height();

    card_frame().show(ui, |ui| {
        // 幅は親の利用可能幅いっぱいに任せ、右端がはみ出さないようにする
        ui.set_min_height(available_height - layout::CARD_HEIGHT_OFFSET);

        // ヘッダー: グラフ切り替えボタン + オプション
        ui.horizontal(|ui| {
            // グラフモード切り替えボタン
            let tab_size = egui::vec2(100.0, 24.0);

            let pi_selected = app.explore_graph_mode == ExploreGraphMode::PiVsXLogX;
            let pi_fill = if pi_selected { colors::ACCENT } else { egui::Color32::TRANSPARENT };
            let pi_text = if pi_selected { egui::Color32::WHITE } else { colors::TEXT_SECONDARY };
            if ui.add(
                egui::Button::new(egui::RichText::new("π(x) vs x/logx").size(12.0).color(pi_text))
                    .fill(pi_fill)
                    .min_size(tab_size),
            ).clicked() {
                app.explore_graph_mode = ExploreGraphMode::PiVsXLogX;
            }

            let ratio_selected = app.explore_graph_mode == ExploreGraphMode::Ratio;
            let ratio_fill = if ratio_selected { colors::ACCENT } else { egui::Color32::TRANSPARENT };
            let ratio_text = if ratio_selected { egui::Color32::WHITE } else { colors::TEXT_SECONDARY };
            if ui.add(
                egui::Button::new(egui::RichText::new("Ratio").size(12.0).color(ratio_text))
                    .fill(ratio_fill)
                    .min_size(egui::vec2(60.0, 24.0)),
            ).clicked() {
                app.explore_graph_mode = ExploreGraphMode::Ratio;
            }

            ui.add_space(16.0);

            // 追跡モードチェックボックス
            ui.checkbox(&mut app.explore_follow_mode, "");
            ui.label(
                egui::RichText::new("Follow")
                    .size(12.0)
                    .color(colors::TEXT_PRIMARY),
            );

            ui.add_space(8.0);

            // ウィンドウサイズスライダー（追跡モード時のみ有効）
            if app.explore_follow_mode {
                ui.label(
                    egui::RichText::new("Window:")
                        .size(12.0)
                        .color(colors::TEXT_SECONDARY),
                );
                let mut window_f = app.explore_window_size as f32;
                ui.add(
                    egui::Slider::new(&mut window_f, 20.0..=200.0)
                        .show_value(false)
                        .clamping(egui::SliderClamping::Always),
                );
                app.explore_window_size = window_f as usize;
                ui.label(
                    egui::RichText::new(format!("{}", app.explore_window_size))
                        .size(12.0)
                        .color(colors::TEXT_PRIMARY),
                );
            }
        });

        // ズーム表示 + Reset ボタン（右寄せの 2 行目）
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(egui::Button::new("Reset View").min_size(egui::vec2(80.0, 24.0)))
                    .clicked()
                {
                    app.explore_view = ZoomPanState::default();
                }
                ui.label(
                    egui::RichText::new(format!("{:.0}%", app.explore_view.zoom * 100.0))
                        .size(font_sizes::LABEL)
                        .color(colors::TEXT_SECONDARY),
                );
            });
        });

        ui.add_space(8.0);

        // グラフ描画エリア
        let graph_rect = ui.available_rect_before_wrap();
        let response = ui.allocate_rect(graph_rect, egui::Sense::click_and_drag());

        // グラフを描画
        render_pi_graph(app, ui, graph_rect, &response);
    });
}

/// Render pi(x) vs x/log x or ratio graph（ズーム・ツールチップ対応）
fn render_pi_graph(app: &mut MyApp, ui: &mut egui::Ui, rect: egui::Rect, response: &egui::Response) {
    let painter = ui.painter_at(rect);

    // 背景
    painter.rect_filled(rect, 0.0, colors::CARD_BG);

    if app.explore_data.is_empty() {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "Press Run to start visualization\n\nMouse wheel: Zoom\nDrag: Pan",
            egui::FontId::proportional(16.0),
            colors::TEXT_SECONDARY,
        );
        return;
    }

    // Follow mode: show only recent data points
    let data: Vec<(f64, f64, f64)> = if app.explore_follow_mode {
        let len = app.explore_data.len();
        let start = len.saturating_sub(app.explore_window_size);
        app.explore_data[start..].to_vec()
    } else {
        app.explore_data.clone()
    };

    if data.is_empty() {
        return;
    }

    // グラフ領域（マージン）
    let margin = 50.0;
    let graph_rect = egui::Rect::from_min_max(
        egui::pos2(rect.min.x + margin, rect.min.y + 20.0),
        egui::pos2(rect.max.x - margin, rect.max.y - 30.0),
    );

    // ズーム・パン入力処理（共通設定）
    handle_zoom_and_pan(
        ui,
        graph_rect,
        response,
        &mut app.explore_view,
        &DEFAULT_ZOOM_CONFIG,
    );

    let hover_pos = response.hover_pos();
    let mut tooltip: Option<(egui::Pos2, String)> = None;

    let axis_color = colors::TEXT_SECONDARY;

    match app.explore_graph_mode {
        ExploreGraphMode::PiVsXLogX => {
            render_pi_vs_xlogx_graph(
                &painter,
                &data,
                graph_rect,
                axis_color,
                &app.explore_view,
                hover_pos,
                &mut tooltip,
            );
        }
        ExploreGraphMode::Ratio => {
            render_ratio_graph(
                &painter,
                &data,
                graph_rect,
                axis_color,
                &app.explore_view,
                hover_pos,
                &mut tooltip,
            );
        }
    }

    // ツールチップ描画（カード外にはみ出しても表示されるようオーバーレイペインタを使用）
    if let Some((pos, text)) = tooltip {
        let style = GraphTooltipStyle::default();
        let overlay_painter = ui.painter_at(ui.max_rect());
        draw_graph_tooltip(&overlay_painter, pos, &text, &style);
    }
}

/// π(x) vs x/log x のグラフを描画
fn render_pi_vs_xlogx_graph(
    painter: &egui::Painter,
    data: &[(f64, f64, f64)],
    graph_rect: egui::Rect,
    axis_color: egui::Color32,
    view: &ZoomPanState,
    hover_pos: Option<egui::Pos2>,
    tooltip: &mut Option<(egui::Pos2, String)>,
) {
    if data.len() < 2 {
        return;
    }

    // データ範囲を計算
    let min_x = data.iter().map(|(x, _, _)| *x).fold(f64::INFINITY, f64::min);
    let max_x = data.iter().map(|(x, _, _)| *x).fold(0.0_f64, f64::max);
    let max_y = data
        .iter()
        .map(|(_, pi, xlx)| pi.max(*xlx))
        .fold(0.0_f64, f64::max);
    let min_y = 0.0_f64;

    if max_x <= min_x || max_y <= min_y {
        return;
    }

    let data_range = (min_x, max_x, min_y, max_y);

    // 軸描画（共通ヘルパー）
    let axis_labels = AxisLabels {
        y_max: format!("{:.0}", max_y),
        y_min: "0".to_string(),
        x_min: format!("{:.0}", min_x),
        x_max: format!("{:.0}", max_x),
    };
    draw_axes(painter, graph_rect, view, &axis_labels, axis_color);

    // π(x) の線（青）
    let pi_screen_points: Vec<egui::Pos2> = data
        .iter()
        .map(|(x, pi, _)| data_to_screen(*x, *pi, data_range, graph_rect))
        .collect();
    draw_polyline(
        painter,
        graph_rect,
        view,
        &pi_screen_points,
        egui::Stroke::new(2.0, colors::ACCENT),
    );

    // x/log x の線（グレー）
    let xlx_color = egui::Color32::from_rgb(0x88, 0x88, 0x88);
    let xlx_screen_points: Vec<egui::Pos2> = data
        .iter()
        .map(|(x, _, xlx)| data_to_screen(*x, *xlx, data_range, graph_rect))
        .collect();
    draw_polyline(
        painter,
        graph_rect,
        view,
        &xlx_screen_points,
        egui::Stroke::new(2.0, xlx_color),
    );

    // 凡例（共通ヘルパー）
    crate::ui_graph_utils::draw_legend(
        painter,
        graph_rect,
        &[
            LegendItem {
                label: "π(x)",
                color: colors::ACCENT,
            },
            LegendItem {
                label: "x/logx",
                color: xlx_color,
            },
        ],
    );

    // Show current values（位置は固定のまま）
    if let Some((x, pi, xlx)) = data.last() {
        let info_y = graph_rect.max.y + 15.0;
        painter.text(
            egui::pos2(graph_rect.center().x, info_y),
            egui::Align2::CENTER_CENTER,
            format!(
                "x = {:.0}  |  pi(x) = {:.0}  |  x/logx = {:.1}  |  diff = {:.1}",
                x,
                pi,
                xlx,
                pi - xlx
            ),
            egui::FontId::proportional(11.0),
            colors::TEXT_PRIMARY,
        );
    }

    // ツールチップ（共通ヘルパーで最近傍点を選択）
    if let Some((idx, pos)) =
        pick_closest_point(hover_pos, graph_rect, view, &pi_screen_points, f32::INFINITY)
    {
        let (x, pi, xlx) = data[idx];
        let text = format!(
            "x = {:.0}\npi(x) = {:.0}\nx/logx = {:.1}\ndiff = {:.1}",
            x,
            pi,
            xlx,
            pi - xlx
        );
        *tooltip = Some((pos, text));
    }
}

/// Render ratio pi(x) / (x/log x) graph
fn render_ratio_graph(
    painter: &egui::Painter,
    data: &[(f64, f64, f64)],
    graph_rect: egui::Rect,
    axis_color: egui::Color32,
    view: &ZoomPanState,
    hover_pos: Option<egui::Pos2>,
    tooltip: &mut Option<(egui::Pos2, String)>,
) {
    // Calculate ratio
    let ratio_data: Vec<(f64, f64)> = data
        .iter()
        .filter(|(_, _, xlx)| *xlx > 0.0)
        .map(|(x, pi, xlx)| (*x, *pi / *xlx))
        .collect();

    if ratio_data.len() < 2 {
        return;
    }

    // データ範囲を計算（x軸のみデータから取得）
    let min_x = ratio_data
        .iter()
        .map(|(x, _)| *x)
        .fold(f64::INFINITY, f64::min);
    let max_x = ratio_data.iter().map(|(x, _)| *x).fold(0.0_f64, f64::max);

    // 縦軸は固定: 0.3 〜 1.3 (小さい x では ratio < 1 になるため)
    let min_r = 0.3_f64;
    let max_r = 1.3_f64;

    if max_x <= min_x {
        return;
    }

    let data_range = (min_x, max_x, min_r, max_r);

    // 軸描画（共通ヘルパー）
    let axis_labels = AxisLabels {
        y_max: "1.3".to_string(),
        y_min: "0.3".to_string(),
        x_min: format!("{:.0}", min_x),
        x_max: format!("{:.0}", max_x),
    };
    draw_axes(painter, graph_rect, view, &axis_labels, axis_color);

    // 中央付近に 1.0 のラベル（追加）
    let y_one_label =
        graph_rect.max.y - ((1.0 - min_r) / (max_r - min_r)) as f32 * graph_rect.height();
    let y_one_pos = apply_view_transform(
        egui::pos2(graph_rect.min.x - 5.0, y_one_label),
        graph_rect,
        view,
    );
    painter.text(
        y_one_pos,
        egui::Align2::RIGHT_CENTER,
        "1.0",
        egui::FontId::proportional(10.0),
        colors::TEXT_SECONDARY,
    );

    // r = 1.0 の基準線
    let y_one = data_to_screen(min_x, 1.0, data_range, graph_rect).y;
    let baseline_points = [
        egui::pos2(graph_rect.min.x, y_one),
        egui::pos2(graph_rect.max.x, y_one),
    ];
    draw_polyline(
        painter,
        graph_rect,
        view,
        &baseline_points,
        egui::Stroke::new(1.5, egui::Color32::from_rgb(0x66, 0x66, 0x66)),
    );
    painter.text(
        egui::pos2(graph_rect.max.x + 5.0, y_one),
        egui::Align2::LEFT_CENTER,
        "1.0",
        egui::FontId::proportional(10.0),
        egui::Color32::from_rgb(0x99, 0x99, 0x99),
    );

    // Ratio line (yellow)
    let ratio_color = egui::Color32::from_rgb(0xFF, 0xC0, 0x00);
    let ratio_screen_points: Vec<egui::Pos2> = ratio_data
        .iter()
        .map(|(x, r)| data_to_screen(*x, *r, data_range, graph_rect))
        .collect();
    draw_polyline(
        painter,
        graph_rect,
        view,
        &ratio_screen_points,
        egui::Stroke::new(2.0, ratio_color),
    );

    // 凡例（位置は固定のまま）
    painter.text(
        egui::pos2(graph_rect.max.x - 10.0, graph_rect.min.y + 10.0),
        egui::Align2::RIGHT_CENTER,
        "π(x) / (x / log x)",
        egui::FontId::proportional(12.0),
        ratio_color,
    );

    // Show current values（位置は固定のまま）
    if let Some((x, r)) = ratio_data.last() {
        let info_y = graph_rect.max.y + 15.0;
        painter.text(
            egui::pos2(graph_rect.center().x, info_y),
            egui::Align2::CENTER_CENTER,
            format!(
                "x = {:.0}  |  ratio = {:.4}  |  diff from 1 = {:.4}",
                x,
                r,
                r - 1.0
            ),
            egui::FontId::proportional(11.0),
            colors::TEXT_PRIMARY,
        );
    }

    // ツールチップ（共通ヘルパーで最近傍点を選択）
    if let Some((idx, pos)) = pick_closest_point(
        hover_pos,
        graph_rect,
        view,
        &ratio_screen_points,
        f32::INFINITY,
    ) {
        let (x, r) = ratio_data[idx];
        let text = format!(
            "x = {:.0}\nratio = {:.4}\ndiff from 1 = {:.4}",
            x,
            r,
            r - 1.0
        );
        *tooltip = Some((pos, text));
    }
}


