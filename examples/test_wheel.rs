/// ホイール構造のテストプログラム（CLI専用）

use std::sync::atomic::AtomicBool;
use sosu_seisei_main2::config::{Config, WheelType, OutputFormat};
use sosu_seisei_main2::cpu_engine::generate_primes_cpu;
use sosu_seisei_main2::output::FilePrimeWriter;
use sosu_seisei_main2::engine_types::Progress;

fn test_wheel(wheel_type: WheelType, name: &str) {
    println!("\n========================================");
    println!("テスト: {}", name);
    println!("========================================");
    
    let mut cfg = Config::default();
    cfg.prime_min = 1;
    cfg.prime_max = 100;
    cfg.segment_size = 20;
    cfg.wheel_type = wheel_type;
    cfg.output_format = OutputFormat::Text;
    cfg.output_dir = ".".to_string();
    
    let output_file = match wheel_type {
        WheelType::Odd => "test_odd",
        WheelType::Mod6 => "test_mod6",
        WheelType::Mod30 => "test_mod30",
    };
    
    // 一時ディレクトリを作成
    let output_dir = format!("test_output_{}", output_file);
    
    let mut writer = match FilePrimeWriter::new(
        &output_dir,
        OutputFormat::Text,
        0, // split_count = 0 (分割なし)
        8 * 1024, // buffer size
        None,
    ) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("エラー: ファイルを開けませんでした: {}", e);
            return;
        }
    };
    
    let stop_flag = AtomicBool::new(false);
    
    println!("素数生成中...");
    let result = generate_primes_cpu(
        &cfg,
        &stop_flag,
        &mut writer,
        |progress: Progress| {
            if progress.total > 0 {
                let percent = (progress.processed as f64 / progress.total as f64) * 100.0;
                print!("\r進捗: {:.1}% ({}/{})", percent, progress.processed, progress.total);
            }
        },
    );
    
    println!();
    
    match result {
        Ok(_) => {
            println!("✓ 素数生成完了！");
            
            let result_file = format!("{}/primes.txt", output_dir);
            println!("出力ファイル: {}", result_file);
            
            // ファイルの内容を読んで確認
            match std::fs::read_to_string(&result_file) {
                Ok(content) => {
                    let primes: Vec<&str> = content.trim().lines().collect();
                    println!("生成された素数の数: {}", primes.len());
                    if primes.len() <= 30 {
                        println!("素数リスト: {}", content.trim());
                    } else {
                        let first_10: Vec<_> = primes.iter().take(10).collect();
                        let last_5: Vec<_> = primes.iter().rev().take(5).rev().collect();
                        println!("最初の10個: {:?}", first_10);
                        println!("最後の5個: {:?}", last_5);
                    }
                    
                    // 期待される100以下の素数の数は25個
                    if primes.len() == 25 {
                        println!("✓ 素数の個数が正しいです！");
                    } else {
                        println!("✗ 警告: 期待される素数は25個ですが、{}個生成されました", primes.len());
                    }
                }
                Err(e) => {
                    eprintln!("ファイル読み込みエラー: {}", e);
                }
            }
        }
        Err(e) => {
            println!("✗ エラーが発生しました: {}", e);
        }
    }
}

fn main() {
    env_logger::init();
    
    println!("========================================");
    println!("ホイール構造の検証テスト");
    println!("========================================");
    println!("範囲: 1〜100の素数を生成");
    println!("期待される素数の個数: 25個");
    println!();
    
    test_wheel(WheelType::Odd, "Odd (奇数のみ) - メモリ1/2");
    test_wheel(WheelType::Mod6, "Mod6 (6k±1) - メモリ1/3");
    test_wheel(WheelType::Mod30, "Mod30 (30周期) - メモリ8/30");
    
    println!("\n========================================");
    println!("全テスト完了");
    println!("========================================");
}

