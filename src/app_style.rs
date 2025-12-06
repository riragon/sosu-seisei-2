//! egui スタイル設定まわりをまとめたモジュール。
//!
//! `MyApp::new` から呼び出され、アプリ全体の見た目（Apple 風ダークテーマ）を構成します。

use eframe::egui;

use crate::ui_theme::colors;

/// グローバルな egui スタイルを設定する。
///
/// - 余白や角丸を大きめにとった、Apple 風のミニマルなダークテーマ。
/// - テキストスタイルや選択範囲などもここで一括設定する。
pub fn setup_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();

    // 余白を大きめに取って呼吸感を出す
    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.button_padding = egui::vec2(20.0, 10.0);
    style.spacing.window_margin = egui::Margin::same(20.0);

    // Apple 風の純黒ベース
    let bg_surface = colors::SURFACE_BG;
    let bg_card = colors::CARD_BG;
    let accent = colors::ACCENT;

    style.visuals.dark_mode = true;
    style.visuals.panel_fill = bg_surface;
    style.visuals.extreme_bg_color = bg_surface;
    style.visuals.faint_bg_color = bg_card;
    style.visuals.code_bg_color = bg_card;

    // 枠線をほぼ消す（Apple 風）
    style.visuals.window_stroke = egui::Stroke::NONE;

    style.visuals.widgets.noninteractive.bg_fill = bg_card;
    style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::NONE;
    style.visuals.widgets.noninteractive.fg_stroke = egui::Stroke {
        width: 1.0,
        color: colors::TEXT_SECONDARY,
    };

    // 大きめの角丸で柔らかさを出す
    style.visuals.window_rounding = egui::Rounding::same(14.0);
    style.visuals.widgets.noninteractive.rounding = egui::Rounding::same(10.0);
    style.visuals.widgets.inactive.rounding = egui::Rounding::same(10.0);
    style.visuals.widgets.hovered.rounding = egui::Rounding::same(10.0);
    style.visuals.widgets.active.rounding = egui::Rounding::same(10.0);

    // インタラクティブ要素
    style.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(0x38, 0x38, 0x3A);
    style.visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
    style.visuals.widgets.inactive.fg_stroke = egui::Stroke {
        width: 1.0,
        color: colors::TEXT_PRIMARY,
    };

    style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(0x48, 0x48, 0x4A);
    style.visuals.widgets.hovered.bg_stroke = egui::Stroke::NONE;
    style.visuals.widgets.hovered.fg_stroke = egui::Stroke {
        width: 1.0,
        color: colors::TEXT_PRIMARY,
    };

    style.visuals.widgets.active.bg_fill = accent;
    style.visuals.widgets.active.bg_stroke = egui::Stroke::NONE;
    style.visuals.widgets.active.fg_stroke = egui::Stroke {
        width: 1.0,
        color: egui::Color32::WHITE,
    };

    // 選択範囲
    style.visuals.selection.bg_fill = accent.linear_multiply(0.4);
    style.visuals.selection.stroke = egui::Stroke::NONE;

    // テキストスタイル: SF Pro 風の階層
    // 見出しは軽く大きく、本文は読みやすく
    // 論理ピクセルで指定することで DPI スケーリングに対応
    style
        .text_styles
        .insert(egui::TextStyle::Heading, egui::FontId::proportional(24.0));
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Monospace, egui::FontId::monospace(13.0));
    style
        .text_styles
        .insert(egui::TextStyle::Small, egui::FontId::proportional(12.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));

    ctx.set_style(style);
}
