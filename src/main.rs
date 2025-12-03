#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use eframe::NativeOptions;
use sosu_seisei_main2::prime_pi_engine::compute_prime_pi;

fn main() -> eframe::Result<()> {
    env_logger::init();

    // CLI モード: `--prime-pi <x>` が指定されている場合は GUI を起動せず、
    // primecount 経由で π(x) を計算して標準出力に表示して終了する。
    if try_handle_prime_pi_cli() {
        return Ok(());
    }

    let options = NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([900.0, 700.0])
            .with_min_inner_size([700.0, 550.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Sosu-Seisei Settings 2",
        options,
        Box::new(|cc| Ok(Box::new(sosu_seisei_main2::app::MyApp::new(cc)))),
    )
}

/// `--prime-pi <x>` 形式の CLI オプションを処理する。
///
/// - 対応例:
///   - `sosu-seisei-main2 --prime-pi 100000000000`
/// - 誤った引数の場合はエラーメッセージを標準エラーに出力し、true を返す（GUI は起動しない）。
fn try_handle_prime_pi_cli() -> bool {
    let mut args = std::env::args().skip(1);
    let Some(first) = args.next() else {
        return false;
    };

    if first != "--prime-pi" {
        return false;
    }

    let Some(x_str) = args.next() else {
        eprintln!("Usage: sosu-seisei-main2 --prime-pi <x>");
        return true;
    };

    let x = match x_str.parse::<u64>() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Invalid x for --prime-pi: {x_str} ({e})");
            return true;
        }
    };

    match compute_prime_pi(x) {
        Ok(pi) => {
            println!("pi({x}) = {pi}");
        }
        Err(e) => {
            eprintln!("Error while computing pi({x}): {e}");
        }
    }

    true
}
