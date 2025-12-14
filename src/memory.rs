use sysinfo::System;
use crate::config::WheelType;

/// システムの物理メモリ総量を取得（バイト単位）
pub fn get_total_memory() -> u64 {
    let mut sys = System::new_all();
    sys.refresh_memory();
    sys.total_memory()
}

/// ホイールタイプに応じたメモリ圧縮率を取得
pub fn get_wheel_compression_ratio(wheel_type: WheelType) -> f64 {
    match wheel_type {
        WheelType::Odd => 0.5,      // 1/2
        WheelType::Mod6 => 0.333,   // 1/3
        WheelType::Mod30 => 0.267,  // 8/30
    }
}

/// セグメントあたりのメモリ使用量を推定（バイト単位）
/// segment_size: セグメントに含まれる数値の範囲
/// wheel_type: 使用するホイールタイプ
pub fn estimate_segment_memory(segment_size: u64, wheel_type: WheelType) -> u64 {
    let compression = get_wheel_compression_ratio(wheel_type);
    // ビット配列: segment_size * compression / 8 (1バイト=8ビット)
    // + Vec のオーバーヘッド等を考慮して 1.2 倍
    let bits = (segment_size as f64 * compression) as u64;
    let bytes = (bits / 8).max(1);
    (bytes as f64 * 1.2) as u64
}

/// メモリ制限に基づいて最適なセグメントサイズを計算
/// memory_usage_percent: システムメモリの何%まで使用するか (10.0 ~ 90.0)
/// num_threads: 並列スレッド数
/// wheel_type: 使用するホイールタイプ
/// returns: 推奨セグメントサイズ
pub fn calculate_optimal_segment_size(
    memory_usage_percent: f64,
    num_threads: usize,
    wheel_type: WheelType,
) -> u64 {
    let total_memory = get_total_memory();
    
    // メモリ使用率を 10.0 ~ 90.0 の範囲にクランプ
    let percent = memory_usage_percent.clamp(10.0, 90.0);
    
    // 許容メモリ量
    let allowed_memory = (total_memory as f64 * percent / 100.0) as u64;
    
    // 安全係数 (他のプロセスやシステムのために余裕を持たせる)
    let safety_factor = 2.0;
    
    // スレッドあたりの許容メモリ
    let per_thread_memory = allowed_memory / (num_threads as u64).max(1);
    let safe_memory = (per_thread_memory as f64 / safety_factor) as u64;
    
    // セグメントサイズの逆算
    // estimate_segment_memory(size, wheel) ≈ size * compression * 1.2 / 8 = safe_memory
    // size = safe_memory * 8 / (compression * 1.2)
    let compression = get_wheel_compression_ratio(wheel_type);
    let segment_size = (safe_memory as f64 * 8.0 / (compression * 1.2)) as u64;
    
    // 最小値と最大値を設定
    let min_size = 1_000_000u64;       // 最小 100万
    let max_size = 100_000_000u64;     // 最大 1億
    
    segment_size.clamp(min_size, max_size)
}

/// メモリ使用量の情報を表示用に取得
pub fn get_memory_info(
    segment_size: u64,
    num_threads: usize,
    wheel_type: WheelType,
) -> MemoryInfo {
    let total_memory = get_total_memory();
    let segment_memory = estimate_segment_memory(segment_size, wheel_type);
    let estimated_total = segment_memory * num_threads as u64;
    let usage_percent = (estimated_total as f64 / total_memory as f64) * 100.0;
    
    MemoryInfo {
        total_memory,
        segment_memory,
        estimated_total,
        usage_percent,
    }
}

#[derive(Debug, Clone)]
pub struct MemoryInfo {
    pub total_memory: u64,
    pub segment_memory: u64,
    pub estimated_total: u64,
    pub usage_percent: f64,
}

impl MemoryInfo {
    pub fn format(&self) -> String {
        format!(
            "メモリ: システム {:.1}GB, セグメント {:.1}MB, 推定使用量 {:.1}MB ({:.1}%)",
            self.total_memory as f64 / (1024.0 * 1024.0 * 1024.0),
            self.segment_memory as f64 / (1024.0 * 1024.0),
            self.estimated_total as f64 / (1024.0 * 1024.0),
            self.usage_percent
        )
    }
}

