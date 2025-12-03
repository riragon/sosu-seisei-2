//! 再利用可能な UI コンポーネント／ヘルパー関数。
//!
//! - テキスト入力欄やカードフレーム、ラベル装飾などをまとめています。
//! - `app.rs` や 教育モード用パネルから共有して使えるように分離しています。

use eframe::egui;

use crate::ui_theme::{colors, font_sizes, layout};

/// 縦中央揃えのテキスト入力欄を作成
pub fn styled_text_edit(text: &mut String) -> egui::TextEdit<'_> {
    egui::TextEdit::singleline(text).font(egui::TextStyle::Body).margin(egui::Margin {
        left: 8.0,
        right: 8.0,
        top: 11.0,
        bottom: 5.0,
    })
}

/// セクション見出しラベルを作成
pub fn section_title(text: &str) -> egui::RichText {
    egui::RichText::new(text)
        .size(font_sizes::SECTION)
        .color(colors::TEXT_PRIMARY)
}

/// フィールドラベルを作成
pub fn field_label(text: &str) -> egui::RichText {
    egui::RichText::new(text)
        .size(font_sizes::LABEL)
        .color(colors::TEXT_SECONDARY)
}

/// カードフレームを作成
pub fn card_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(colors::CARD_BG)
        .rounding(egui::Rounding::same(layout::CARD_ROUNDING))
        .inner_margin(egui::Margin::same(layout::CARD_PADDING))
}

/// グラフ用ツールチップのスタイル
pub struct GraphTooltipStyle {
    pub bg: egui::Color32,
    pub border: egui::Color32,
    pub text: egui::Color32,
}

impl Default for GraphTooltipStyle {
    fn default() -> Self {
        Self {
            bg: colors::SURFACE_BG,
            border: colors::TEXT_SECONDARY,
            text: egui::Color32::WHITE,
        }
    }
}

impl GraphTooltipStyle {
    /// Spiral で素数セルに使うスタイル（背景: ACCENT, 枠: ACCENT, 文字: 白）
    pub fn prime() -> Self {
        Self {
            bg: colors::ACCENT,
            border: colors::ACCENT,
            text: egui::Color32::WHITE,
        }
    }
}

/// グラフ用の簡易ツールチップを描画
pub fn draw_graph_tooltip(
    painter: &egui::Painter,
    pos: egui::Pos2,
    text: &str,
    style: &GraphTooltipStyle,
) {
    let font_id = egui::FontId::proportional(14.0);

    // 複数行テキストを想定し、行ごとに長さを測って最大幅を求める
    let lines: Vec<&str> = text.lines().collect();
    let line_count = lines.len().max(1);
    let max_chars = lines
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(1);

    // 簡易的にテキスト幅・高さを推定（1文字あたり ~8px, 1行あたり ~18px）
    let text_width = (max_chars as f32) * 8.0;
    let line_height = 18.0;
    let text_height = line_height * (line_count as f32);
    // 上下の余白をやや広めにとって、文字が枠線に近づき過ぎないようにする
    let padding_x = 6.0;
    let padding_y = 6.0;

    // マウス位置の少し上にラベルを表示
    let bg_rect = egui::Rect::from_min_max(
        egui::pos2(
            pos.x - text_width / 2.0 - padding_x,
            pos.y - text_height - padding_y,
        ),
        egui::pos2(pos.x + text_width / 2.0 + padding_x, pos.y + padding_y),
    );

    painter.rect_filled(bg_rect, 4.0, style.bg);
    painter.rect_stroke(
        bg_rect,
        4.0,
        egui::Stroke::new(1.0, style.border),
    );

    // 各行を中央揃えで縦方向に並べる
    for (i, line) in lines.iter().enumerate() {
        let y = bg_rect.min.y + padding_y + line_height * (i as f32 + 0.5);
        painter.text(
            egui::pos2(bg_rect.center().x, y),
            egui::Align2::CENTER_CENTER,
            *line,
            font_id.clone(),
            style.text,
        );
    }
}

/// 進捗パーセントを計算するヘルパー
pub fn calc_percent(processed: u64, total: u64) -> f32 {
    if total > 0 {
        (processed as f32 / total as f32) * 100.0
    } else {
        0.0
    }
}

/// 10^k 表示ラベルを描画するヘルパー
///
/// 入力値が n = d × 10^k の形式（末尾に0が続く）の場合、
/// d = 1 なら "= 10^k"、それ以外なら "= d × 10^k" を表示する。
fn render_power_of_ten_label(ui: &mut egui::Ui, value: &str) {
    if let Ok(v) = value.trim().parse::<u64>() {
        if v > 0 {
            let mut x = v;
            let mut exp: u32 = 0;
            while x % 10 == 0 {
                x /= 10;
                exp += 1;
            }
            if exp > 0 {
                let text = if x == 1 {
                    format!("= 10^{}", exp)
                } else {
                    format!("= {} × 10^{}", x, exp)
                };
                ui.label(
                    egui::RichText::new(text)
                        .size(font_sizes::LABEL)
                        .color(colors::ACCENT),
                );
            }
        }
    }
}

/// Min/Max の 2 つの数値入力フィールドを横並びで描画するヘルパー
pub fn render_range_input_pair(
    ui: &mut egui::Ui,
    min_label_text: &str,
    max_label_text: &str,
    min_value: &mut String,
    max_value: &mut String,
    min_width: f32,
    max_width: f32,
) {
    ui.horizontal(|ui| {
        // Minimum
        ui.vertical(|ui| {
            ui.label(field_label(min_label_text));
            ui.add_space(4.0);
            ui.add_sized(
                [min_width, layout::INPUT_HEIGHT],
                styled_text_edit(min_value),
            );
            ui.add_space(4.0);
            render_power_of_ten_label(ui, min_value);
        });

        ui.add_space(16.0);

        // Maximum
        ui.vertical(|ui| {
            ui.label(field_label(max_label_text));
            ui.add_space(4.0);
            ui.add_sized(
                [max_width, layout::INPUT_HEIGHT],
                styled_text_edit(max_value),
            );
            ui.add_space(4.0);
            render_power_of_ten_label(ui, max_value);
        });
    });
}

/// Speed スライダーを描画するヘルパー（3段階: 1x / 3x / MAX）
pub fn render_speed_slider(ui: &mut egui::Ui, label: &str, speed: &mut f32) {
    ui.horizontal(|ui| {
        ui.label(field_label(label));

        // speed を 0.0, 1.0, 2.0 の 3 段階インデックスとして扱う
        if *speed < 0.0 {
            *speed = 0.0;
        } else if *speed > 2.0 {
            *speed = 2.0;
        }

        ui.add(
            egui::Slider::new(speed, 0.0..=2.0)
                .step_by(1.0)
                .show_value(false)
                .clamping(egui::SliderClamping::Always),
        );

        let label_text = match (*speed).round() as i32 {
            0 => "1x",
            1 => "3x",
            2 => "MAX",
            _ => "1x",
        };
        ui.label(
            egui::RichText::new(label_text)
                .size(font_sizes::BODY)
                .color(colors::TEXT_PRIMARY),
        );
    });
}

/// パーセント表示 + プログレスバーを描画するヘルパー
pub fn render_progress_header(ui: &mut egui::Ui, percent: f32, progress: f32) {
    ui.label(
        egui::RichText::new(format!("{:.1}%", percent.max(0.0)))
            .size(font_sizes::HERO)
            .color(colors::TEXT_PRIMARY),
    );

    ui.add_space(8.0);

    ui.add(
        egui::ProgressBar::new(progress.clamp(0.0, 1.0))
            .fill(colors::ACCENT)
            .desired_height(8.0),
    );
}

/// ズーム・パン状態を保持する汎用構造体
#[derive(Debug, Clone, Copy)]
pub struct ZoomPanState {
    pub zoom: f32,
    pub pan_x: f32,
    pub pan_y: f32,
}

impl Default for ZoomPanState {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
        }
    }
}

/// ズーム・パンの制約／感度設定
#[derive(Debug, Clone, Copy)]
pub struct ZoomPanConfig {
    pub min_zoom: f32,
    pub max_zoom: f32,
    /// ホイールスクロール 1.0 あたりのズーム係数（Spiral と同じ 0.001 程度推奨）
    pub zoom_speed: f32,
}

/// 汎用的なズーム・パン入力処理（マウスホイール＋ドラッグ）
pub fn handle_zoom_and_pan(
    ui: &egui::Ui,
    rect: egui::Rect,
    response: &egui::Response,
    state: &mut ZoomPanState,
    cfg: &ZoomPanConfig,
) {
    // マウスホイールでズーム
    if response.hovered() {
        let scroll_delta = ui.input(|i| i.raw_scroll_delta.y);
        if scroll_delta != 0.0 {
            let zoom_factor = 1.0 + scroll_delta * cfg.zoom_speed;
            let mut current_zoom = state.zoom;
            if current_zoom <= 0.0 {
                current_zoom = 1.0;
            }
            let new_zoom = (current_zoom * zoom_factor).clamp(cfg.min_zoom, cfg.max_zoom);

            // ズームの中心をマウス位置にする
            if let Some(mouse_pos) = ui.input(|i| i.pointer.hover_pos()) {
                if rect.contains(mouse_pos) {
                    let center = rect.center();
                    let mouse_offset_x = mouse_pos.x - center.x;
                    let mouse_offset_y = mouse_pos.y - center.y;

                    let scale_change = new_zoom / current_zoom;
                    state.pan_x =
                        state.pan_x * scale_change + mouse_offset_x * (1.0 - scale_change);
                    state.pan_y =
                        state.pan_y * scale_change + mouse_offset_y * (1.0 - scale_change);
                }
            }

            state.zoom = new_zoom;
        }
    }

    // ドラッグでパン
    if response.dragged() {
        let delta = response.drag_delta();
        state.pan_x += delta.x;
        state.pan_y += delta.y;
    }
}

/// グラフ領域 rect の中心を基準に、ズーム・パンを適用した点を返す
pub fn apply_zoom_pan_to_point(
    point: egui::Pos2,
    rect: egui::Rect,
    state: &ZoomPanState,
) -> egui::Pos2 {
    let center = rect.center();
    let dx = point.x - center.x;
    let dy = point.y - center.y;
    egui::pos2(
        center.x + dx * state.zoom + state.pan_x,
        center.y + dy * state.zoom + state.pan_y,
    )
}



