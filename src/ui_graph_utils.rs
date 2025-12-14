//! グラフ描画用の共通ヘルパー関数群。
//!
//! Density / Gap / Explore / Spiral などのグラフパネルで共通して使う
//! 軸描画・折れ線描画・バー描画・ツールチップ選択などをまとめています。

use eframe::egui;

use crate::ui_components::ZoomPanState;
use crate::ui_theme::colors;

// =============================================================================
// 共通のズーム設定
// =============================================================================

/// グラフ共通のズーム・パン設定（デフォルト値）
pub const DEFAULT_ZOOM_CONFIG: crate::ui_components::ZoomPanConfig =
    crate::ui_components::ZoomPanConfig {
        min_zoom: 0.5,
        max_zoom: 20.0,
        zoom_speed: 0.001,
    };

// =============================================================================
// 座標変換ヘルパー
// =============================================================================

/// データ座標をグラフ内ピクセル座標に変換する
///
/// - `data_x`, `data_y`: データ空間での座標
/// - `data_range`: (min_x, max_x, min_y, max_y) のデータ範囲
/// - `graph_rect`: グラフ描画領域（ピクセル）
///
/// 戻り値はズーム・パン適用前のグラフ内ピクセル座標
pub fn data_to_screen(
    data_x: f64,
    data_y: f64,
    data_range: (f64, f64, f64, f64),
    graph_rect: egui::Rect,
) -> egui::Pos2 {
    let (min_x, max_x, min_y, max_y) = data_range;
    let range_x = (max_x - min_x).max(1e-9);
    let range_y = (max_y - min_y).max(1e-9);

    let nx = ((data_x - min_x) / range_x) as f32;
    let ny = ((data_y - min_y) / range_y) as f32;

    egui::pos2(
        graph_rect.min.x + nx * graph_rect.width(),
        graph_rect.max.y - ny * graph_rect.height(), // Y軸は上が大きい
    )
}

/// ズーム・パンを適用した座標を返す（`ui_components::apply_zoom_pan_to_point` の再エクスポート的役割）
pub fn apply_view_transform(
    point: egui::Pos2,
    graph_rect: egui::Rect,
    view: &ZoomPanState,
) -> egui::Pos2 {
    crate::ui_components::apply_zoom_pan_to_point(point, graph_rect, view)
}

// =============================================================================
// 軸描画ヘルパー
// =============================================================================

/// 軸ラベル情報
#[derive(Default)]
pub struct AxisLabels {
    /// Y軸の上端ラベル（例: "1000"）
    pub y_max: String,
    /// Y軸の下端ラベル（例: "0"）
    pub y_min: String,
    /// X軸の左端ラベル（例: "2"）
    pub x_min: String,
    /// X軸の右端ラベル（例: "1000000"）
    pub x_max: String,
}

/// 標準的な L 字型軸（左下原点）を描画する
///
/// - `painter`: 描画先
/// - `graph_rect`: グラフ領域
/// - `view`: ズーム・パン状態
/// - `labels`: 軸ラベル
/// - `axis_color`: 軸線の色
pub fn draw_axes(
    painter: &egui::Painter,
    graph_rect: egui::Rect,
    view: &ZoomPanState,
    labels: &AxisLabels,
    axis_color: egui::Color32,
) {
    // 軸線の始点・終点
    let x_axis_start = egui::pos2(graph_rect.min.x, graph_rect.max.y);
    let x_axis_end = egui::pos2(graph_rect.max.x, graph_rect.max.y);
    let y_axis_start = x_axis_start;
    let y_axis_end = egui::pos2(graph_rect.min.x, graph_rect.min.y);

    // ズーム・パン適用
    let x_axis_start_z = apply_view_transform(x_axis_start, graph_rect, view);
    let x_axis_end_z = apply_view_transform(x_axis_end, graph_rect, view);
    let y_axis_start_z = apply_view_transform(y_axis_start, graph_rect, view);
    let y_axis_end_z = apply_view_transform(y_axis_end, graph_rect, view);

    // 軸線描画
    painter.line_segment([x_axis_start_z, x_axis_end_z], egui::Stroke::new(1.0, axis_color));
    painter.line_segment([y_axis_start_z, y_axis_end_z], egui::Stroke::new(1.0, axis_color));

    let font_id = egui::FontId::proportional(10.0);

    // Y軸ラベル
    if !labels.y_max.is_empty() {
        let pos = apply_view_transform(
            egui::pos2(graph_rect.min.x, graph_rect.min.y),
            graph_rect,
            view,
        );
        painter.text(
            egui::pos2(pos.x - 5.0, pos.y),
            egui::Align2::RIGHT_CENTER,
            &labels.y_max,
            font_id.clone(),
            axis_color,
        );
    }
    if !labels.y_min.is_empty() {
        let pos = apply_view_transform(
            egui::pos2(graph_rect.min.x, graph_rect.max.y),
            graph_rect,
            view,
        );
        painter.text(
            egui::pos2(pos.x - 5.0, pos.y),
            egui::Align2::RIGHT_CENTER,
            &labels.y_min,
            font_id.clone(),
            axis_color,
        );
    }

    // X軸ラベル
    if !labels.x_min.is_empty() {
        let pos = apply_view_transform(
            egui::pos2(graph_rect.min.x, graph_rect.max.y),
            graph_rect,
            view,
        );
        painter.text(
            egui::pos2(pos.x, pos.y + 10.0),
            egui::Align2::CENTER_TOP,
            &labels.x_min,
            font_id.clone(),
            axis_color,
        );
    }
    if !labels.x_max.is_empty() {
        let pos = apply_view_transform(
            egui::pos2(graph_rect.max.x, graph_rect.max.y),
            graph_rect,
            view,
        );
        painter.text(
            egui::pos2(pos.x, pos.y + 10.0),
            egui::Align2::CENTER_TOP,
            &labels.x_max,
            font_id,
            axis_color,
        );
    }
}

// =============================================================================
// 折れ線描画ヘルパー
// =============================================================================

/// 折れ線を描画する
///
/// - `painter`: 描画先
/// - `graph_rect`: グラフ領域
/// - `view`: ズーム・パン状態
/// - `points`: グラフ内ピクセル座標の点列（ズーム前）
/// - `stroke`: 線のスタイル
pub fn draw_polyline(
    painter: &egui::Painter,
    graph_rect: egui::Rect,
    view: &ZoomPanState,
    points: &[egui::Pos2],
    stroke: egui::Stroke,
) {
    if points.len() < 2 {
        return;
    }

    let transformed: Vec<egui::Pos2> = points
        .iter()
        .map(|p| apply_view_transform(*p, graph_rect, view))
        .collect();

    for pair in transformed.windows(2) {
        painter.line_segment([pair[0], pair[1]], stroke);
    }
}

// =============================================================================
// バーチャート描画ヘルパー
// =============================================================================

/// バー情報（グラフ内ピクセル座標ベース）
pub struct BarInfo {
    /// バー中心の X 座標（ズーム前）
    pub center_x: f32,
    /// バー中心の Y 座標（ズーム前）
    pub center_y: f32,
    /// バーの半幅（ズーム前）
    pub half_width: f32,
    /// バーの半高さ（ズーム前）
    pub half_height: f32,
}

/// バーを描画し、ズーム後の矩形を返す
///
/// - `painter`: 描画先
/// - `graph_rect`: グラフ領域
/// - `view`: ズーム・パン状態
/// - `bar`: バー情報
/// - `color`: バーの色
/// - `rounding`: 角丸
///
/// 戻り値: ズーム後のバー矩形（ホバー判定用）
pub fn draw_bar(
    painter: &egui::Painter,
    graph_rect: egui::Rect,
    view: &ZoomPanState,
    bar: &BarInfo,
    color: egui::Color32,
    rounding: f32,
) -> egui::Rect {
    let center = apply_view_transform(
        egui::pos2(bar.center_x, bar.center_y),
        graph_rect,
        view,
    );

    let half_w = bar.half_width * view.zoom.max(0.01);
    let half_h = bar.half_height * view.zoom.max(0.01);

    let bar_rect = egui::Rect::from_min_max(
        egui::pos2(center.x - half_w, center.y - half_h),
        egui::pos2(center.x + half_w, center.y + half_h),
    );

    painter.rect_filled(bar_rect, rounding, color);

    bar_rect
}

// =============================================================================
// ツールチップ・ポイント選択ヘルパー
// =============================================================================

/// マウス位置に最も近い点を選択する
///
/// - `hover_pos`: マウス位置（None なら選択なし）
/// - `graph_rect`: グラフ領域
/// - `view`: ズーム・パン状態
/// - `screen_points`: グラフ内ピクセル座標の点列（ズーム前）
/// - `max_distance`: この距離以内の点のみ選択対象（ピクセル単位）
///
/// 戻り値: (インデックス, ズーム後のスクリーン座標)
pub fn pick_closest_point(
    hover_pos: Option<egui::Pos2>,
    graph_rect: egui::Rect,
    view: &ZoomPanState,
    screen_points: &[egui::Pos2],
    max_distance: f32,
) -> Option<(usize, egui::Pos2)> {
    let mouse = hover_pos?;

    let mut best_idx: Option<usize> = None;
    let mut best_dist = f32::INFINITY;
    let mut best_pos = egui::Pos2::ZERO;

    for (i, &pt) in screen_points.iter().enumerate() {
        let transformed = apply_view_transform(pt, graph_rect, view);
        let dx = transformed.x - mouse.x;
        let dy = transformed.y - mouse.y;
        let dist = (dx * dx + dy * dy).sqrt();

        if dist < best_dist && dist <= max_distance {
            best_dist = dist;
            best_idx = Some(i);
            best_pos = transformed;
        }
    }

    best_idx.map(|idx| (idx, best_pos))
}

/// バー矩形のリストからホバー中のバーを選択する
///
/// - `hover_pos`: マウス位置
/// - `bar_rects`: ズーム後のバー矩形リスト
///
/// 戻り値: ホバー中のバーのインデックス
pub fn pick_hovered_bar(
    hover_pos: Option<egui::Pos2>,
    bar_rects: &[egui::Rect],
) -> Option<usize> {
    let mouse = hover_pos?;

    for (i, rect) in bar_rects.iter().enumerate() {
        if rect.contains(mouse) {
            return Some(i);
        }
    }

    None
}

// =============================================================================
// グラフ領域計算ヘルパー
// =============================================================================

/// 標準的なマージン設定
pub struct GraphMargins {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

impl Default for GraphMargins {
    fn default() -> Self {
        Self {
            left: 40.0,
            right: 10.0,
            top: 20.0,
            bottom: 30.0,
        }
    }
}

/// カード内の利用可能領域からグラフ描画領域を計算する
pub fn compute_graph_rect(available_rect: egui::Rect, margins: &GraphMargins) -> egui::Rect {
    egui::Rect::from_min_max(
        egui::pos2(
            available_rect.min.x + margins.left,
            available_rect.min.y + margins.top,
        ),
        egui::pos2(
            available_rect.max.x - margins.right,
            available_rect.max.y - margins.bottom,
        ),
    )
}

/// 期待値線の標準色（黄色系）
pub fn expected_line_color() -> egui::Color32 {
    egui::Color32::from_rgb(0xFF, 0xC0, 0x00)
}

// =============================================================================
// Spiral 専用ズーム・パン処理
// =============================================================================

/// Spiral グリッド用のズーム・パン設定
pub struct SpiralZoomPanConfig {
    pub min_zoom: f32,
    pub max_zoom: f32,
    pub zoom_speed: f32,
}

impl Default for SpiralZoomPanConfig {
    fn default() -> Self {
        Self {
            min_zoom: 0.1,
            max_zoom: 50.0,
            zoom_speed: 0.001,
        }
    }
}

/// Spiral グリッド用のズーム・パン入力を処理する
///
/// 標準的なグラフとは異なり、Spiral は独自の座標系を使用するため、
/// ズーム中心をマウス位置にする処理が異なります。
///
/// - `ui`: egui::Ui 参照
/// - `rect`: グリッド描画領域
/// - `response`: allocate_rect の応答
/// - `zoom`: 現在のズーム値（更新される）
/// - `pan_x`, `pan_y`: 現在のパン値（更新される）
/// - `config`: ズーム設定
pub fn handle_spiral_zoom_and_pan_input(
    ui: &egui::Ui,
    rect: egui::Rect,
    response: &egui::Response,
    zoom: &mut f32,
    pan_x: &mut f32,
    pan_y: &mut f32,
    config: &SpiralZoomPanConfig,
) {
    // マウスホイールでズーム
    if response.hovered() {
        let scroll_delta = ui.input(|i| i.raw_scroll_delta.y);
        if scroll_delta != 0.0 {
            let zoom_factor = 1.0 + scroll_delta * config.zoom_speed;
            let new_zoom = (*zoom * zoom_factor).clamp(config.min_zoom, config.max_zoom);

            // ズームの中心をマウス位置にする
            if let Some(mouse_pos) = ui.input(|i| i.pointer.hover_pos()) {
                if rect.contains(mouse_pos) {
                    let center_x = rect.center().x;
                    let center_y = rect.center().y;
                    let mouse_offset_x = mouse_pos.x - center_x;
                    let mouse_offset_y = mouse_pos.y - center_y;

                    let scale_change = new_zoom / *zoom;
                    *pan_x = *pan_x * scale_change + mouse_offset_x * (1.0 - scale_change);
                    *pan_y = *pan_y * scale_change + mouse_offset_y * (1.0 - scale_change);
                }
            }

            *zoom = new_zoom;
        }
    }

    // ドラッグでパン
    if response.dragged() {
        let delta = response.drag_delta();
        *pan_x += delta.x;
        *pan_y += delta.y;
    }
}

// =============================================================================
// 凡例描画ヘルパー
// =============================================================================

/// 凡例アイテム
pub struct LegendItem<'a> {
    pub label: &'a str,
    pub color: egui::Color32,
}

/// 右上に凡例を描画する
pub fn draw_legend(
    painter: &egui::Painter,
    graph_rect: egui::Rect,
    items: &[LegendItem<'_>],
) {
    let font_id = egui::FontId::proportional(11.0);
    let line_height = 16.0;
    let legend_x = graph_rect.max.x - 10.0;
    let mut y = graph_rect.min.y + 10.0;

    for item in items {
        // 線サンプル
        painter.line_segment(
            [
                egui::pos2(legend_x - 50.0, y),
                egui::pos2(legend_x - 30.0, y),
            ],
            egui::Stroke::new(2.0, item.color),
        );
        // ラベル
        painter.text(
            egui::pos2(legend_x - 25.0, y),
            egui::Align2::LEFT_CENTER,
            item.label,
            font_id.clone(),
            colors::TEXT_PRIMARY,
        );
        y += line_height;
    }
}

