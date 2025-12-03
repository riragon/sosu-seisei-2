use eframe::egui;
use std::f32::consts::PI;

use crate::app::{MyApp, SpiralGridShape};
use crate::ui_components::{
    card_frame, draw_graph_tooltip, field_label, render_speed_slider, section_title,
    styled_text_edit, GraphTooltipStyle,
};
use crate::ui_graph_utils::{handle_spiral_zoom_and_pan_input, DEFAULT_SPIRAL_ZOOM_CONFIG};
use crate::ui_theme::{colors, font_sizes, layout};

/// Spiral モードのパネル（Ulam Spiral）
pub fn render_spiral_panel(app: &mut MyApp, ctx: &egui::Context) {
    egui::CentralPanel::default()
        .frame(
            egui::Frame::none()
                .fill(colors::SURFACE_BG)
                .inner_margin(egui::Margin::same(layout::PANEL_MARGIN)),
        )
        .show(ctx, |ui| {
            ui.columns(2, |columns| {
                // 左側: Settings + Statistics を縦に並べる
                // 上の Settings は固定高さ、下の Statistics は「残り高さ」を使うようにして
                // 全体がウィンドウ内に収まるようにする
                let settings_height = layout::TOP_CARD_HEIGHT;

                render_spiral_settings_card(&mut columns[0], app, settings_height);
                columns[0].add_space(layout::CARD_GAP);

                // Settings 描画後の残り高さをそのまま Statistics に渡す
                let stats_height = columns[0].available_height();
                render_spiral_stats_card(&mut columns[0], app, stats_height);

                // 右側: Spiral Grid（ズーム・パン操作のため app を &mut で渡す）
                render_spiral_grid(&mut columns[1], app);
            });
        });
}

/// Spiral の Settings カード
fn render_spiral_settings_card(ui: &mut egui::Ui, app: &mut MyApp, height: f32) {
    card_frame().show(ui, |ui| {
        ui.set_min_height(height - layout::CARD_HEIGHT_OFFSET);

        ui.label(section_title("Settings"));
        ui.add_space(12.0);

        ui.horizontal(|ui| {
            // Center
            ui.vertical(|ui| {
                ui.label(field_label("Center"));
                ui.add_space(4.0);
                ui.add_sized(
                    [120.0, layout::INPUT_HEIGHT],
                    styled_text_edit(&mut app.spiral.center_input),
                );
            });

            ui.add_space(16.0);

            // Size (grid)
            ui.vertical(|ui| {
                ui.label(field_label("Size (grid)"));
                ui.add_space(4.0);
                ui.add_sized(
                    [120.0, layout::INPUT_HEIGHT],
                    styled_text_edit(&mut app.spiral.size_input),
                );
            });
        });

        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("Size: odd number, minimum 5 (very large sizes may be slow)")
                .size(font_sizes::LABEL)
                .color(colors::TEXT_SECONDARY),
        );

        ui.add_space(8.0);

        // Grid shape 切り替え
        ui.horizontal(|ui| {
            ui.label(field_label("Grid shape"));
            egui::ComboBox::new("spiral_grid_shape", "")
                .selected_text(match app.spiral.grid_shape {
                    SpiralGridShape::Square => "Square (Ulam)",
                    SpiralGridShape::Hex => "Hex (Honeycomb)",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut app.spiral.grid_shape,
                        SpiralGridShape::Square,
                        "Square",
                    );
                    ui.selectable_value(
                        &mut app.spiral.grid_shape,
                        SpiralGridShape::Hex,
                        "Hex (honeycomb)",
                    );
                });
        });

        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(
                "Square: Ulam spiral, Hex: prime spiral on honeycomb lattice",
            )
            .size(font_sizes::LABEL)
            .color(colors::TEXT_SECONDARY),
        );

        ui.add_space(8.0);

        // パス線の表示 ON/OFF
        ui.horizontal(|ui| {
            ui.label(field_label("Spiral path"));
            ui.checkbox(&mut app.spiral.show_path, "Show path line");
        });

        ui.add_space(8.0);

        // Speed スライダー（共通コンポーネント: 1x / 3x / MAX）
        render_speed_slider(ui, "Speed:", &mut app.spiral.speed);

        ui.add_space(8.0);

        // Progress 情報
        let processed = app.spiral.processed;
        let total = app.spiral.total;
        let percent = if total > 0 {
            (processed as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        ui.label(field_label("Progress"));
        ui.add_space(4.0);
        ui.add(
            egui::ProgressBar::new(percent as f32 / 100.0)
                .fill(colors::ACCENT)
                .desired_height(8.0),
        );
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(format!("{} / {} ({:.1}%)", processed, total, percent))
                .size(font_sizes::BODY)
                .color(colors::TEXT_PRIMARY),
        );
    });
}

/// Spiral の Statistics カード
fn render_spiral_stats_card(ui: &mut egui::Ui, app: &MyApp, height: f32) {
    card_frame().show(ui, |ui| {
        ui.set_min_height(height - layout::CARD_HEIGHT_OFFSET);

        ui.label(section_title("Statistics"));
        ui.add_space(12.0);

        let size = app.spiral.size;
        let center = app.spiral.center;

        if size == 0 || app.spiral.primes.is_empty() {
            ui.label(
                egui::RichText::new("No data yet")
                    .size(font_sizes::LABEL)
                    .color(colors::TEXT_SECONDARY),
            );
            return;
        }

        // 表示している範囲を計算
        // center が中心で、size x size のグリッド
        // 最小値: center - (size/2)^2 相当ではなく、スパイラルの開始値から計算
        // スパイラルは center から始まり、size^2 個のセルを持つ
        let total_cells = (size * size) as u64;
        // 中心から最も遠いセルまでの距離
        // スパイラルの最小値と最大値を計算
        // center が中心にあり、スパイラルは center から外側に広がる
        // 最小値: center - offset, 最大値: center + offset
        // ただし実際のスパイラルでは、中心から離れるほど値が増減する
        // Ulam spiral の場合、center を起点に 1, 2, 3, ... と増えていく
        // つまり range は [center, center + size^2 - 1]
        let range_min = center;
        let range_max = center + total_cells - 1;

        // 素数の数をカウント
        let prime_count: u64 = app.spiral.primes.iter().filter(|&&p| p).count() as u64;

        // 素数の割合
        let prime_ratio = if total_cells > 0 {
            prime_count as f64 / total_cells as f64
        } else {
            0.0
        };

        // 期待値との比較
        // 代表値 N として range の中央値を使用
        let n_mid = (range_min + range_max) / 2;
        let n_mid_f = n_mid.max(2) as f64;
        let expected_ratio = if n_mid_f > 1.0 {
            1.0 / n_mid_f.ln()
        } else {
            0.0
        };
        let emp_over_exp = if expected_ratio > 0.0 {
            prime_ratio / expected_ratio
        } else {
            0.0
        };

        // 2 カラムレイアウトで縦幅を圧縮
        ui.columns(2, |columns| {
            // 左カラム: Range / Prime count / Prime ratio
            columns[0].vertical(|ui| {
                ui.label(field_label("Range"));
                ui.label(
                    egui::RichText::new(format!(
                        "{} ~ {}² = {}",
                        range_min,
                        size,
                        range_max
                    ))
                    .size(font_sizes::BODY)
                    .color(colors::TEXT_PRIMARY),
                );

                ui.add_space(8.0);

                ui.label(field_label("Prime count"));
                ui.label(
                    egui::RichText::new(format!("{}", prime_count))
                        .size(font_sizes::BODY)
                        .color(colors::TEXT_PRIMARY),
                );

                ui.add_space(8.0);

                ui.label(field_label("Prime ratio"));
                ui.label(
                    egui::RichText::new(format!(
                        "{:.6} (= {} / {})",
                        prime_ratio, prime_count, total_cells
                    ))
                    .size(font_sizes::BODY)
                    .color(colors::TEXT_PRIMARY),
                );
            });

            // 右カラム: Expected ratio / Empirical / Expected
            columns[1].vertical(|ui| {
                ui.label(field_label("Expected ratio (1/log N)"));
                ui.label(
                    egui::RichText::new(format!("{:.6}  (N = {})", expected_ratio, n_mid))
                        .size(font_sizes::BODY)
                        .color(colors::TEXT_PRIMARY),
                );

                ui.add_space(8.0);

                ui.label(field_label("Empirical / Expected"));
                ui.label(
                    egui::RichText::new(format!("{:.4}", emp_over_exp))
                        .size(font_sizes::BODY)
                        .color(colors::ACCENT),
                );
            });
        });
    });
}

/// スクエアグリッド上のウラム螺旋パスを生成し、ステップ順にコールバックを呼び出す。
///
/// - `step` は 0 から始まる連番で、整数値 `n = center + step` に対応する。
/// - `(gx, gy)` は [0, size) x [0, size) のグリッドインデックス。
fn for_each_square_spiral_index<F>(size: usize, mut f: F)
where
    F: FnMut(u64, i32, i32),
{
    if size == 0 {
        return;
    }

    let total_cells = (size as u64).saturating_mul(size as u64);
    let c = (size / 2) as i32;

    // 螺旋パス生成用（原点を中心にして右→上→左→下→...）
    let dirs: [(i32, i32); 4] = [(1, 0), (0, -1), (-1, 0), (0, 1)];
    let mut dir_idx = 0usize;
    let mut x = 0i32;
    let mut y = 0i32;
    let mut leg_len = 1i32;
    let mut leg_step = 0i32;
    let mut leg_count = 0i32;

    let mut produced: u64 = 0;

    while produced < total_cells {
        let gx = c + x;
        let gy = c + y;
        if gx >= 0 && gy >= 0 && (gx as usize) < size && (gy as usize) < size {
            f(produced, gx, gy);
            produced += 1;
        }

        // 螺旋の次の 1 ステップへ進む
        let (dx, dy) = dirs[dir_idx];
        x += dx;
        y += dy;
        leg_step += 1;
        if leg_step == leg_len {
            leg_step = 0;
            dir_idx = (dir_idx + 1) % 4;
            leg_count += 1;
            if leg_count % 2 == 0 {
                leg_len += 1;
            }
        }
    }
}

/// ハニカム（六角格子）上の螺旋パスを生成し、ステップ順にコールバックを呼び出す。
///
/// - `step` は 0 から始まる連番で、整数値 `n = center + step` に対応する。
/// - `(q, r)` は軸座標（axial coordinates）。
fn for_each_hex_spiral_index<F>(total_cells: u64, mut f: F)
where
    F: FnMut(u64, i32, i32),
{
    if total_cells == 0 {
        return;
    }

    // pointy-top axial 座標系の 6 方向
    let dirs: [(i32, i32); 6] = [
        (1, 0),
        (1, -1),
        (0, -1),
        (-1, 0),
        (-1, 1),
        (0, 1),
    ];

    let mut produced: u64 = 0;

    // 中心セル（step = 0）
    let mut last_q: i32 = 0;
    let mut last_r: i32 = 0;
    f(produced, last_q, last_r);
    produced += 1;
    if produced >= total_cells {
        return;
    }

    // 半径 1, 2, 3, ... のリングを外側へ広げながら、常に
    // 「直前のセルと隣接するリングセル」から順にたどることで
    // 連続した渦巻き状パスを作る。
    let mut radius: u64 = 1;
    while produced < total_cells {
        let k = radius as i32;

        // 標準的なリング順（SW 始まり）で 6k 個の座標を生成
        let mut ring: Vec<(i32, i32)> = Vec::with_capacity((6 * radius) as usize);
        let mut q = dirs[4].0 * k;
        let mut r = dirs[4].1 * k;
        for dir in 0..6 {
            let (dq, dr) = dirs[dir];
            for _ in 0..radius {
                ring.push((q, r));
                q += dq;
                r += dr;
            }
        }

        if ring.is_empty() {
            radius += 1;
            continue;
        }

        // リングの開始インデックスを決める。
        // - 半径 1 のときは「右隣 (1, 0)」から始めて、
        //   0→1→2→3→4→5→6 と内側リングを一周する（反時計回り）。
        // - 半径 > 1 のときは「直前セル last に隣接する外側リングセル」のうち、
        //   画面上で最も右側（world_x が最大）にあるセルをスタートにする。
        //   これにより、7→8 が「7 の右隣」になるような自然な渦巻きを作る。
        let start_idx = if radius == 1 {
            if let Some(idx) = ring.iter().position(|&(q, r)| q == 1 && r == 0) {
                idx
            } else {
                0
            }
        } else {
            // last に隣接する外側リングセルをすべて探し、その中で最も右側にあるものを選ぶ
            let sqrt3 = 3.0_f32.sqrt();
            let world_x = |(q, r): (i32, i32)| -> f32 {
                let qf = q as f32;
                let rf = r as f32;
                sqrt3 * qf + (sqrt3 / 2.0) * rf
            };

            let mut best_idx = 0usize;
            let mut best_x = f32::NEG_INFINITY;
            for (idx, &(rq, rr)) in ring.iter().enumerate() {
                // last と隣接しているか？
                let mut adjacent = false;
                for (dq, dr) in dirs {
                    if last_q + dq == rq && last_r + dr == rr {
                        adjacent = true;
                        break;
                    }
                }
                if adjacent {
                    let x = world_x((rq, rr));
                    if x > best_x {
                        best_x = x;
                        best_idx = idx;
                    }
                }
            }
            best_idx
        };

        let ring_len = ring.len();
        if ring_len == 0 {
            radius += 1;
            continue;
        }

        // リングを反時計回り（CCW）に一周する
        for i in 0..ring_len {
            if produced >= total_cells {
                return;
            }
            let idx = (start_idx + i) % ring_len;
            let (rq, rr) = ring[idx];
            f(produced, rq, rr);
            produced += 1;
            last_q = rq;
            last_r = rr;
        }

        radius += 1;
    }
}

/// Spiral グリッドを描画（ズーム・パン対応）
fn render_spiral_grid(ui: &mut egui::Ui, app: &mut MyApp) {
    // ホバー情報をクロージャの外で保持（値 + 画面上の位置 + 素数フラグ）
    let mut hover_value: Option<(u64, egui::Pos2, bool)> = None;

    card_frame().show(ui, |ui| {
        render_spiral_header(ui, app);

        ui.add_space(8.0);

        let rect = ui.available_rect_before_wrap();
        // ドラッグとホバーを検知するため Sense::drag() を使用
        let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
        let painter = ui.painter_at(rect.intersect(ui.clip_rect()));

        painter.rect_filled(rect, 0.0, colors::CARD_BG);

        let size = app.spiral.size;
        if size == 0 || app.spiral.primes.is_empty() {
            draw_spiral_empty_message(&painter, rect);
            return;
        }

        let (offset_x, offset_y, cell_size) =
            handle_spiral_zoom_and_pan(ui, rect, &response, app);

        let hover_pos = response.hover_pos();
        let mut path_points: Vec<egui::Pos2> = Vec::new();

        let (visible_cells, visible_primes) = draw_spiral_cells(
            &painter,
            rect,
            app,
            offset_x,
            offset_y,
            cell_size,
            hover_pos,
            &mut hover_value,
            &mut path_points,
        );

        // セル中心を結ぶ細い線で螺旋パスを可視化（設定で ON/OFF）
        if app.spiral.show_path {
            draw_spiral_path(&painter, &path_points);
        }

        draw_spiral_center_highlight(&painter, rect, app, offset_x, offset_y, cell_size);
        draw_spiral_overlays(
            &painter,
            rect,
            visible_cells,
            visible_primes,
            &hover_value,
        );
    });
}

/// ヘッダー（タイトル + ズーム表示 + リセットボタン）を描画
fn render_spiral_header(ui: &mut egui::Ui, app: &mut MyApp) {
    ui.horizontal(|ui| {
        ui.label(section_title("Ulam Spiral"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // ズームリセットボタン
            if ui
                .add(egui::Button::new("Reset View").min_size(egui::vec2(80.0, 24.0)))
                .clicked()
            {
                app.spiral.zoom = 1.0;
                app.spiral.pan_x = 0.0;
                app.spiral.pan_y = 0.0;
            }
            // ズーム表示
            ui.label(
                egui::RichText::new(format!("{:.0}%", app.spiral.zoom * 100.0))
                    .size(font_sizes::LABEL)
                    .color(colors::TEXT_SECONDARY),
            );
        });
    });
}

/// データが無いときのメッセージを描画
fn draw_spiral_empty_message(painter: &egui::Painter, rect: egui::Rect) {
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "Press Run to generate spiral\n\nMouse wheel: Zoom\nDrag: Pan",
        egui::FontId::proportional(16.0),
        colors::TEXT_SECONDARY,
    );
}

/// ズーム・パン入力を処理し、オフセットとセルサイズを返す
fn handle_spiral_zoom_and_pan(
    ui: &egui::Ui,
    rect: egui::Rect,
    response: &egui::Response,
    app: &mut MyApp,
) -> (f32, f32, f32) {
    let size = app.spiral.size as f32;

    // 共通ヘルパーでズーム・パン入力を処理（統一された ZoomPanConfig を使用）
    handle_spiral_zoom_and_pan_input(
        ui,
        rect,
        response,
        &mut app.spiral.zoom,
        &mut app.spiral.pan_x,
        &mut app.spiral.pan_y,
        &DEFAULT_SPIRAL_ZOOM_CONFIG,
    );

    let padding = 12.0;
    let inner_rect = egui::Rect::from_min_max(
        egui::pos2(rect.min.x + padding, rect.min.y + padding),
        egui::pos2(rect.max.x - padding, rect.max.y - padding),
    );

    // ズームを適用したセルサイズ
    let base_cell_w = inner_rect.width() / size;
    let base_cell_h = inner_rect.height() / size;
    let base_cell_size = base_cell_w.min(base_cell_h).max(0.1);
    let cell_size = base_cell_size * app.spiral.zoom;

    // パンを適用したオフセット
    let offset_x = inner_rect.center().x - size * cell_size / 2.0 + app.spiral.pan_x;
    let offset_y = inner_rect.center().y - size * cell_size / 2.0 + app.spiral.pan_y;

    (offset_x, offset_y, cell_size)
}

/// スパイラルのセルを描画し、可視セルと素数セルの数を返す
fn draw_spiral_cells(
    painter: &egui::Painter,
    rect: egui::Rect,
    app: &MyApp,
    offset_x: f32,
    offset_y: f32,
    cell_size: f32,
    hover_pos: Option<egui::Pos2>,
    hover_value: &mut Option<(u64, egui::Pos2, bool)>,
    path_points: &mut Vec<egui::Pos2>,
) -> (u64, u64) {
    let size = app.spiral.size;
    if size == 0 {
        return (0, 0);
    }

    let total_cells = (size as u64).saturating_mul(size as u64);
    let primes = &app.spiral.primes;
    let total = total_cells.min(primes.len() as u64);

    // グリッド中心の画面座標
    let size_f = size as f32;
    let center_x = offset_x + size_f * cell_size / 2.0;
    let center_y = offset_y + size_f * cell_size / 2.0;

    let mut visible_cells: u64 = 0;
    let mut visible_primes: u64 = 0;

    match app.spiral.grid_shape {
        SpiralGridShape::Square => {
            let c = size_f / 2.0;
            for_each_square_spiral_index(size, |step, gx, gy| {
                if step >= total {
                    return;
                }
                let is_prime = primes[step as usize];

                // セル矩形（左上・右下）
                let dx = gx as f32 - c;
                let dy = gy as f32 - c;
                let px0 = egui::pos2(center_x + dx * cell_size, center_y + dy * cell_size);
                let px1 = egui::pos2(px0.x + cell_size, px0.y + cell_size);
                let cell_rect = egui::Rect::from_min_max(px0, px1);

                // パス用にセル中心を記録
                let cell_center = cell_rect.center();
                if (step as usize) < path_points.len() {
                    path_points[step as usize] = cell_center;
                } else {
                    path_points.push(cell_center);
                }

                if !rect.intersects(cell_rect) {
                    return;
                }

                visible_cells += 1;
                if is_prime {
                    visible_primes += 1;
                    painter.rect_filled(cell_rect, 0.0, colors::ACCENT);
                }

                // ホバー判定
                if let Some(mouse_pos) = hover_pos {
                    if cell_rect.contains(mouse_pos) {
                        let value = app.spiral.center.saturating_add(step);
                        *hover_value = Some((value, mouse_pos, is_prime));
                    }
                }
            });
        }
        SpiralGridShape::Hex => {
            let sqrt3 = 3.0_f32.sqrt();
            // pointy-top hex の半径。縦方向の中心間隔がおおよそ cell_size になるよう調整。
            let hex_r = cell_size / 1.5;

            for_each_hex_spiral_index(total, |step, q, r| {
                if step as usize >= primes.len() {
                    return;
                }
                let is_prime = primes[step as usize];

                let qf = q as f32;
                let rf = r as f32;
                let world_x = hex_r * (sqrt3 * qf + (sqrt3 / 2.0) * rf);
                let world_y = hex_r * (1.5 * rf);

                let cx = center_x + world_x;
                let cy = center_y + world_y;
                let cell_center = egui::pos2(cx, cy);

                // パス用にセル中心を記録
                if (step as usize) < path_points.len() {
                    path_points[step as usize] = cell_center;
                } else {
                    path_points.push(cell_center);
                }

                // おおまかなバウンディング矩形（表示領域判定用）
                let cell_rect = egui::Rect::from_center_size(
                    cell_center,
                    egui::vec2(hex_r * 2.0, hex_r * 2.0),
                );
                if !rect.intersects(cell_rect) {
                    return;
                }

                visible_cells += 1;
                if is_prime {
                    visible_primes += 1;

                    // 六角形ポリゴンを描画
                    let mut points = Vec::with_capacity(6);
                    for i in 0..6 {
                        let angle = PI / 180.0 * (60.0 * i as f32 - 30.0);
                        let x = cx + hex_r * angle.cos();
                        let y = cy + hex_r * angle.sin();
                        points.push(egui::pos2(x, y));
                    }
                    painter.add(egui::Shape::convex_polygon(
                        points,
                        colors::ACCENT,
                        egui::Stroke::NONE,
                    ));
                }

                // ホバー判定（簡易的に円判定）
                if let Some(mouse_pos) = hover_pos {
                    let dx = mouse_pos.x - cx;
                    let dy = mouse_pos.y - cy;
                    if dx * dx + dy * dy <= hex_r * hex_r {
                        let value = app.spiral.center.saturating_add(step);
                        *hover_value = Some((value, mouse_pos, is_prime));
                    }
                }
            });
        }
    }

    (visible_cells, visible_primes)
}

/// ステップ順に並んだセル中心を細い線で結び、螺旋パスを可視化する
fn draw_spiral_path(painter: &egui::Painter, path_points: &[egui::Pos2]) {
    if path_points.len() < 2 {
        return;
    }

    let stroke = egui::Stroke::new(1.0, colors::TEXT_SECONDARY);
    for pair in path_points.windows(2) {
        let p0 = pair[0];
        let p1 = pair[1];
        painter.line_segment([p0, p1], stroke);
    }
}

/// スパイラル中心セルをハイライト表示
fn draw_spiral_center_highlight(
    painter: &egui::Painter,
    rect: egui::Rect,
    app: &MyApp,
    offset_x: f32,
    offset_y: f32,
    cell_size: f32,
) {
    let size = app.spiral.size;
    if size == 0 {
        return;
    }
    let size_f = size as f32;
    let center_x = offset_x + size_f * cell_size / 2.0;
    let center_y = offset_y + size_f * cell_size / 2.0;

    match app.spiral.grid_shape {
        SpiralGridShape::Square => {
            let px0 = egui::pos2(center_x - cell_size / 2.0, center_y - cell_size / 2.0);
            let px1 = egui::pos2(px0.x + cell_size, px0.y + cell_size);
            let rect_center = egui::Rect::from_min_max(px0, px1);
            if rect.intersects(rect_center) {
                painter.rect_stroke(
                    rect_center,
                    0.0,
                    egui::Stroke::new(2.0, egui::Color32::from_rgb(0xFF, 0xFF, 0x00)),
                );
            }
        }
        SpiralGridShape::Hex => {
            let hex_r = cell_size / 1.5;
            let center_pos = egui::pos2(center_x, center_y);
            let bounds = egui::Rect::from_center_size(
                center_pos,
                egui::vec2(hex_r * 2.0, hex_r * 2.0),
            );
            if rect.intersects(bounds) {
                painter.circle_stroke(
                    center_pos,
                    hex_r * 1.1,
                    egui::Stroke::new(2.0, egui::Color32::from_rgb(0xFF, 0xFF, 0x00)),
                );
            }
        }
    }
}

/// オーバーレイ（可視素数数・操作ヒント・ホバー値）を描画
fn draw_spiral_overlays(
    painter: &egui::Painter,
    rect: egui::Rect,
    visible_cells: u64,
    visible_primes: u64,
    hover_value: &Option<(u64, egui::Pos2, bool)>,
) {
    // 画面に表示されている素数数と割合を左下に表示
    if visible_cells > 0 {
        let ratio = visible_primes as f64 / visible_cells as f64;
        painter.text(
            egui::pos2(rect.min.x + 8.0, rect.max.y - 8.0),
            egui::Align2::LEFT_BOTTOM,
            format!(
                "Visible primes: {} / {}  (ratio = {:.4})",
                visible_primes, visible_cells, ratio
            ),
            egui::FontId::proportional(10.0),
            colors::TEXT_SECONDARY,
        );
    }

    // 操作ヒントを右下に表示
    painter.text(
        egui::pos2(rect.max.x - 8.0, rect.max.y - 8.0),
        egui::Align2::RIGHT_BOTTOM,
        "Scroll: Zoom | Drag: Pan",
        egui::FontId::proportional(10.0),
        colors::TEXT_SECONDARY,
    );

    // ホバー中のセルの数値をカーソル付近に表示（背景付きラベル）
    if let Some((value, pos, is_prime)) = hover_value {
        let text = format!("{}", value);
        let style = if *is_prime {
            GraphTooltipStyle::prime()
        } else {
            GraphTooltipStyle::default()
        };
        draw_graph_tooltip(painter, *pos, &text, &style);
    }
}


