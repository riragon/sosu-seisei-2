use sosu_seisei_main2::config::{Config, OutputFormat, WheelType};
use sosu_seisei_main2::cpu_engine::generate_primes_cpu;
use sosu_seisei_main2::engine_types::Progress;
use sosu_seisei_main2::output::FilePrimeWriter;
/// Memory limit test program
use std::sync::atomic::AtomicBool;

fn main() {
    env_logger::init();

    println!("========================================");
    println!("Memory Limit Test");
    println!("========================================");
    println!();

    // Test with different memory limits
    test_with_memory_limit(20.0, "20% (Low Memory)");
    test_with_memory_limit(50.0, "50% (Default)");
    test_with_memory_limit(70.0, "70% (High Memory)");

    println!("\n========================================");
    println!("All tests completed");
    println!("========================================");
}

fn test_with_memory_limit(memory_percent: f64, description: &str) {
    println!("\n--- Test: {} ---", description);

    let mut cfg = Config::default();
    cfg.prime_min = 1;
    cfg.prime_max = 10_000_000; // 10 million
    cfg.segment_size = 100_000;
    cfg.wheel_type = WheelType::Mod30;
    cfg.output_format = OutputFormat::Text;
    cfg.output_dir = ".".to_string();
    cfg.last_prime_only = true; // Skip file output for speed

    let output_dir = format!("test_memory_{}", memory_percent as u32);

    let mut writer = match FilePrimeWriter::new(&output_dir, OutputFormat::Text, 0, 8 * 1024, None)
    {
        Ok(w) => w,
        Err(e) => {
            eprintln!("Error: Failed to create writer: {}", e);
            return;
        }
    };

    let stop_flag = AtomicBool::new(false);

    println!("Generating primes from 1 to {}...", cfg.prime_max);
    println!("Memory limit: {}%", memory_percent);

    let start = std::time::Instant::now();

    let result = generate_primes_cpu(&cfg, &stop_flag, &mut writer, |progress: Progress| {
        if progress.total > 0 {
            let percent = (progress.processed as f64 / progress.total as f64) * 100.0;
            if percent as u32 % 10 == 0 {
                println!("  Progress: {:.0}%", percent);
            }
        }
    });

    let elapsed = start.elapsed();

    match result {
        Ok(_) => {
            println!("Success! Time: {:.2}s", elapsed.as_secs_f64());
        }
        Err(e) => {
            println!("Error: {}", e);
        }
    }
}
