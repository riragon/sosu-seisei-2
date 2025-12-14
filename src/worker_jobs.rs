//! バックグラウンド処理の補助関数。
//!
//! 現在このモジュールで利用されているのは、UI にメモリ使用量を送る
//! `start_resource_monitor` です。

use std::sync::mpsc;

use crate::worker_message::WorkerMessage;

/// メモリ使用量を 500ms ごとにポーリングし、`WorkerMessage::MemUsage` として送信する。
///
/// - このスレッドはメインの計算とは独立して動作し、UI の「Memory Usage」表示を更新します。
/// - sender 側がドロップされた場合（計算終了・画面クローズなど）はループを終了します。
pub fn start_resource_monitor(
    sender: mpsc::Sender<WorkerMessage>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut sys = sysinfo::System::new_all();
        sys.refresh_memory();

        loop {
            std::thread::sleep(std::time::Duration::from_millis(500));
            sys.refresh_memory();

            let mem_usage = sys.used_memory();

            if sender.send(WorkerMessage::MemUsage(mem_usage)).is_err() {
                break;
            }
        }
    })
}


