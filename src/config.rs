use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::path::Path;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Text,
    CSV,
    JSON,
    Binary,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum WheelType {
    /// 奇数のみ (2を除外) - メモリ使用量 1/2
    Odd,
    /// mod 6 ホイール (2, 3を除外) - メモリ使用量 1/3
    Mod6,
    /// mod 30 ホイール (2, 3, 5を除外) - メモリ使用量 8/30
    Mod30,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    pub prime_min: u64,
    pub prime_max: u64,
    #[serde(default = "default_prime_pi_x")]
    pub prime_pi_x: u64,
    pub segment_size: u64,
    pub writer_buffer_size: usize,
    pub output_format: OutputFormat,
    pub output_dir: String,
    #[serde(default)]
    pub split_count: u64,
    #[serde(default)]
    pub last_prime_only: bool,
    #[serde(default = "default_wheel_type")]
    pub wheel_type: WheelType,
    #[serde(default = "default_use_timestamp_prefix")]
    pub use_timestamp_prefix: bool,
}

fn default_wheel_type() -> WheelType {
    WheelType::Mod30
}

fn default_prime_pi_x() -> u64 {
    1_000_000_000
}

fn default_use_timestamp_prefix() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self {
            prime_min: 1,
            prime_max: 1_000_000_000,
            prime_pi_x: default_prime_pi_x(),
            segment_size: 10_000_000,
            writer_buffer_size: 8 * 1024 * 1024,
            output_format: OutputFormat::Binary,
            output_dir: ".".to_string(),
            split_count: 0,
            last_prime_only: true,
            wheel_type: WheelType::Mod30,
            use_timestamp_prefix: default_use_timestamp_prefix(),
        }
    }
}

const SETTINGS_FILE: &str = "settings.toml";

pub fn load_or_create_config() -> Result<Config, Box<dyn std::error::Error + Send + Sync>> {
    if Path::new(SETTINGS_FILE).exists() {
        let mut file = File::open(SETTINGS_FILE)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let cfg = toml::from_str(&contents)?;
        Ok(cfg)
    } else {
        let cfg = Config::default();
        save_config(&cfg)?;
        Ok(cfg)
    }
}

pub fn save_config(cfg: &Config) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let toml_str = toml::to_string_pretty(cfg)?;
    let file = File::create(SETTINGS_FILE)?;
    let mut writer = BufWriter::new(file);
    writer.write_all(toml_str.as_bytes())?;
    Ok(())
}


