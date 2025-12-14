//! GUI アプリケーション本体と、ワーカースレッドとのメッセージプロトコルを定義するモジュールです。
//!
//! このモジュールは `eframe::App` の実装（`update` ループ）のみを保持し、
//! アプリケーション状態やワーカー起動ロジックは `app_state` / `app_workers` に分割されています。

use eframe::{egui, App};

use crate::worker_message::WorkerMessage;

// 外部からは従来どおり `crate::app::MyApp` などでアクセスできるようにする。
pub use crate::app_state::{AppTab, ExploreGraphMode, MyApp, SpiralGridShape};

impl App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // ワーカーからのメッセージをすべて処理し、UI に即時反映する。
        // ここでの処理順序（ログ → 進捗 → ETA → メモリ使用量）は
        // 「常に最新の状態が見える」ことを保証するための一部です。
        if let Some(ref receiver) = self.receiver {
            let mut remove_receiver = false;
            while let Ok(message) = receiver.try_recv() {
                match message {
                    WorkerMessage::Log(msg) => {
                        self.log.push_str(&msg);
                        if !msg.ends_with('\n') {
                            self.log.push('\n');
                        }
                    }
                    WorkerMessage::Progress { current, total } => {
                        let p = if total > 0 {
                            current as f32 / total as f32
                        } else {
                            0.0
                        };

                        if self.explore_running {
                            // Explore タブ専用の進捗
                            self.explore_progress = p;
                            self.explore_processed = current;
                            self.explore_total = total;
                        } else if self.gap_running {
                            // Gap タブ専用の進捗
                            self.gap_progress = p;
                            self.gap_processed = current;
                            self.gap_total = total;
                        } else if self.density_running {
                            // Density タブ専用の進捗
                            self.density_progress = p;
                            self.density_processed = current;
                            self.density_total = total;
                        } else if self.spiral_running {
                            self.spiral_processed = current;
                            self.spiral_total = total;
                        } else {
                            // Generator / π(x) 用の進捗
                            self.progress = p;
                            self.current_processed = current;
                            self.total_range = total;
                        }
                    }
                    WorkerMessage::Eta(eta_str) => {
                        self.eta = eta_str;
                    }
                    WorkerMessage::MemUsage(mem) => {
                        self.mem_usage = mem;
                    }
                    WorkerMessage::Done => {
                        self.is_running = false;
                        self.explore_running = false;
                        self.gap_running = false;
                        self.density_running = false;
                        self.spiral_running = false;
                        remove_receiver = true;
                    }
                    WorkerMessage::Stopped => {
                        self.is_running = false;
                        self.explore_running = false;
                        self.gap_running = false;
                        self.density_running = false;
                        self.spiral_running = false;
                        remove_receiver = true;
                        self.log.push_str("Process stopped by user.\n");
                    }
                    WorkerMessage::ExploreData { x, pi_x } => {
                        // x/log(x) を計算
                        let x_f = x as f64;
                        let x_log_x = if x > 1 {
                            x_f / x_f.ln()
                        } else {
                            0.0
                        };
                        self.explore_data.push((x_f, pi_x as f64, x_log_x));
                        self.explore_current_x = x;
                    }
                    WorkerMessage::GapData { prime, prev_prime, gap } => {
                        *self.gap_data.entry(gap).or_insert(0) += 1;
                        self.gap_current_x = prime;
                        self.gap_last_prime = prime;
                        self.gap_prime_count = self.gap_prime_count.saturating_add(1);

                        // 最大ギャップ情報を更新
                        if gap > self.gap_max_gap_value {
                            self.gap_max_gap_value = gap;
                            self.gap_max_gap_prev_prime = prev_prime;
                            self.gap_max_gap_prime = prime;
                        }
                    }
                    WorkerMessage::DensityData {
                        interval_start,
                        count,
                    } => {
                        self.density_data.push((interval_start, count));
                        self.density_current_interval = interval_start;
                        // density_processed は Progress メッセージで更新されるので、ここでは更新しない
                        self.density_total_primes =
                            self.density_total_primes.saturating_add(count);
                    }
                    WorkerMessage::SpiralData { primes, size } => {
                        self.spiral_primes = primes;
                        self.spiral_size = size;
                        self.spiral_generated = true;
                    }
                }
            }
            if remove_receiver {
                self.receiver = None;
            }
        }

        // キーボードショートカット: n キーで π(x) を実行
        if ctx.input(|i| i.key_pressed(egui::Key::N)) && !self.is_running {
            self.start_prime_pi();
        }

        // パネル描画は `ui_panels` モジュール経由にまとめる
        crate::ui_panels::render_header(self, ctx);
        crate::ui_panels::render_advanced_options_window(self, ctx);
        crate::ui_panels::render_main_panel(self, ctx);

        ctx.request_repaint();
    }
}


