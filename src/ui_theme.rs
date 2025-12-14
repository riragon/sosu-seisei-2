//! UI テーマ定数（カラーパレット・フォントサイズ・レイアウト）。
//!
//! - 他のモジュールからは `crate::ui_theme::{colors, font_sizes, layout}`
//!   として参照します。
//! - もともと `app.rs` に内包されていた定数群を切り出しており、
//!   教育モード用の画面からも共有できるようにしています。

/// カラーパレット
pub mod colors {
    use eframe::egui;

    /// アクセントカラー（iOS システムブルー風）
    pub const ACCENT: egui::Color32 = egui::Color32::from_rgb(0x00, 0x7A, 0xFF);
    /// 危険アクション用の赤
    pub const DANGER: egui::Color32 = egui::Color32::from_rgb(0xFF, 0x45, 0x3A);
    /// カード背景
    pub const CARD_BG: egui::Color32 = egui::Color32::from_rgb(0x1C, 0x1C, 0x1E);
    /// サーフェス背景（純黒）
    pub const SURFACE_BG: egui::Color32 = egui::Color32::from_rgb(0x00, 0x00, 0x00);
    /// プライマリテキスト
    pub const TEXT_PRIMARY: egui::Color32 = egui::Color32::from_rgb(0xF5, 0xF5, 0xF7);
    /// セカンダリテキスト
    pub const TEXT_SECONDARY: egui::Color32 = egui::Color32::from_rgb(0x86, 0x86, 0x8B);
}

/// フォントサイズ（論理ピクセル）
///
/// eframe/egui は DPI スケーリングを自動で行うため、
/// ここでは「論理ピクセル」で指定すれば FHD/4K どちらでも適切なサイズになる。
pub mod font_sizes {
    /// 大見出し（進捗パーセント）
    pub const HERO: f32 = 42.0;
    /// タイトル
    pub const TITLE: f32 = 22.0;
    /// セクション見出し
    pub const SECTION: f32 = 16.0;
    /// 本文
    pub const BODY: f32 = 14.0;
    /// ラベル
    pub const LABEL: f32 = 12.0;
}

/// レイアウト定数（論理ピクセル）
pub mod layout {
    /// カード間のギャップ
    pub const CARD_GAP: f32 = 12.0;
    /// パネルマージン
    pub const PANEL_MARGIN: f32 = 16.0;
    /// カード内パディング
    pub const CARD_PADDING: f32 = 16.0;
    /// カード角丸
    pub const CARD_ROUNDING: f32 = 10.0;
    /// 入力欄の高さ
    pub const INPUT_HEIGHT: f32 = 32.0;
    /// ボタンの高さ
    pub const BUTTON_HEIGHT: f32 = 32.0;
    /// カード高さ計算時のオフセット
    pub const CARD_HEIGHT_OFFSET: f32 = 32.0;
    /// 上部 Range/Progress カードの標準高さ
    pub const TOP_CARD_HEIGHT: f32 = 220.0;
    /// 小さい入力欄の標準幅
    pub const INPUT_WIDTH_SMALL: f32 = 120.0;
    /// 中サイズ入力欄の標準幅
    pub const INPUT_WIDTH_MEDIUM: f32 = 150.0;
}


