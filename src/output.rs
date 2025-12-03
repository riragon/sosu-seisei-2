use std::fs::{create_dir_all, File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

use chrono::Local;

use crate::config::{Config, OutputFormat};

/// 素数生成のメタデータ
#[derive(Debug, Clone)]
pub struct OutputMetadata {
    pub range: (u64, u64),
    pub count: u64,
    pub pi_x_verified: bool,
    pub execution_time_ms: u64,
    pub generated_at: String,
    pub tool_version: String,
    /// 出力されたファイル名一覧（相対パスまたはファイル名）
    pub output_files: Vec<String>,
    /// primecount バージョン（使用している Rust クレート / C++ 実装の情報）
    pub primecount_version: Option<String>,
    /// primecount アルゴリズムモード（`pi(x)` がどのモードで呼ばれているかの説明）
    pub primecount_mode: Option<String>,
}

impl OutputMetadata {
    /// メタデータを新規作成
    pub fn new(
        range: (u64, u64),
        count: u64,
        pi_x_verified: bool,
        execution_time_ms: u64,
        output_files: Vec<String>,
        primecount_version: Option<String>,
        primecount_mode: Option<String>,
    ) -> Self {
        Self {
            range,
            count,
            pi_x_verified,
            execution_time_ms,
            generated_at: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
            output_files,
            primecount_version,
            primecount_mode,
        }
    }

    /// メタデータをTXTファイルに書き出す
    ///
    /// - `cfg` の内容も併せて出力し、再現性のための設定スナップショットとする。
    /// - `timestamp_prefix` が与えられた場合、レポートファイル名の先頭にも付与する。
    pub fn write_to_file(
        &self,
        output_dir: &str,
        cfg: &Config,
        timestamp_prefix: Option<&str>,
    ) -> io::Result<PathBuf> {
        let base_dir = PathBuf::from(output_dir);
        if !output_dir.is_empty() {
            create_dir_all(&base_dir)?;
        }

        let prefix = timestamp_prefix.unwrap_or("");
        let meta_name = format!("{prefix}primes.meta.txt");
        let meta_path = base_dir.join(meta_name);
        let file = File::create(&meta_path)?;
        let mut writer = BufWriter::new(file);

        writeln!(writer, "=== Prime Generation Report ===")?;
        writeln!(writer, "Range: {} - {}", self.range.0, self.range.1)?;
        writeln!(writer, "Count: {}", self.count)?;
        writeln!(
            writer,
            "π(x) Verified: {}",
            if self.pi_x_verified { "OK" } else { "MISMATCH" }
        )?;
        writeln!(writer, "Execution Time: {} ms", self.execution_time_ms)?;
        writeln!(writer, "Generated: {}", self.generated_at)?;
        writeln!(writer, "Tool Version: {}", self.tool_version)?;

        // 出力ファイル一覧
        if !self.output_files.is_empty() {
            writeln!(writer)?;
            writeln!(writer, "--- Output Files ---")?;
            for f in &self.output_files {
                writeln!(writer, "{f}")?;
            }
        }

        // primecount 情報
        if self.primecount_version.is_some() || self.primecount_mode.is_some() {
            writeln!(writer)?;
            writeln!(writer, "--- primecount Info ---")?;
            if let Some(ref v) = self.primecount_version {
                writeln!(writer, "primecount_version = {v}")?;
            }
            if let Some(ref m) = self.primecount_mode {
                writeln!(writer, "primecount_mode = {m}")?;
            }
        }

        // 設定スナップショット
        writeln!(writer)?;
        writeln!(writer, "--- Settings Snapshot ---")?;
        writeln!(writer, "prime_min = {}", cfg.prime_min)?;
        writeln!(writer, "prime_max = {}", cfg.prime_max)?;
        writeln!(writer, "prime_pi_x = {}", cfg.prime_pi_x)?;
        writeln!(writer, "segment_size = {}", cfg.segment_size)?;
        writeln!(writer, "writer_buffer_size = {}", cfg.writer_buffer_size)?;
        writeln!(writer, "output_format = {:?}", cfg.output_format)?;
        writeln!(writer, "output_dir = {}", cfg.output_dir)?;
        writeln!(writer, "split_count = {}", cfg.split_count)?;
        writeln!(writer, "last_prime_only = {}", cfg.last_prime_only)?;
        writeln!(writer, "wheel_type = {:?}", cfg.wheel_type)?;
        writer.flush()?;

        Ok(meta_path)
    }
}

pub trait PrimeWriter {
    fn write_prime(&mut self, p: u64) -> io::Result<()>;
    fn finish(&mut self) -> io::Result<()>;
}

pub struct FilePrimeWriter {
    format: OutputFormat,
    base_dir: PathBuf,
    split_count: u64,
    buf_size: usize,
    timestamp_prefix: Option<String>,

    current_writer: Option<BufWriter<std::fs::File>>,
    current_count: u64,
    file_index: u64,
    first_item_in_json: bool,
    /// これまでに書き込まれた素数の総数（ファイル分割をまたいだ合計）
    total_count: u64,
    /// 実際に書き出したファイルパス一覧
    output_files: Vec<PathBuf>,
}

impl FilePrimeWriter {
    pub fn new(
        output_dir: &str,
        format: OutputFormat,
        split_count: u64,
        buf_size: usize,
        timestamp_prefix: Option<String>,
    ) -> io::Result<Self> {
        let base_dir = PathBuf::from(output_dir);
        if !output_dir.is_empty() {
            create_dir_all(&base_dir)?;
        }

        let mut writer = Self {
            format,
            base_dir,
            split_count,
            buf_size,
            timestamp_prefix,
            current_writer: None,
            current_count: 0,
            file_index: 1,
            first_item_in_json: true,
            total_count: 0,
            output_files: Vec::new(),
        };

        writer.open_next_file()?;
        Ok(writer)
    }

    fn open_next_file(&mut self) -> io::Result<()> {
        if let Some(mut w) = self.current_writer.take() {
            if let OutputFormat::JSON = self.format {
                write!(w, "]")?;
            }
            w.flush()?;
        }

        let (base_name, ext) = match self.format {
            OutputFormat::Text => ("primes", "txt"),
            OutputFormat::CSV => ("primes", "csv"),
            OutputFormat::JSON => ("primes", "json"),
            OutputFormat::Binary => ("primes", "bin"),
        };

        let prefix = self.timestamp_prefix.as_deref().unwrap_or("");
        let file_name = if self.split_count > 0 {
            format!("{prefix}{base_name}_{}.{ext}", self.file_index)
        } else {
            format!("{prefix}{base_name}.{ext}")
        };

        let full_path = self.base_dir.join(Path::new(&file_name));
        let file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&full_path)?;

        let mut writer = BufWriter::with_capacity(self.buf_size, file);
        if let OutputFormat::JSON = self.format {
            write!(writer, "[")?;
            self.first_item_in_json = true;
        }

        self.current_writer = Some(writer);
        self.current_count = 0;
        self.file_index += 1;
        self.output_files.push(full_path);
        Ok(())
    }

    /// ファイルに書き込まれた素数の総数を返します。
    ///
    /// - `split_count` によるファイル分割をまたいだ全体の件数です。
    /// - 計算中も「これまでに書き出された素数の数」として参照できます。
    pub fn total_primes_written(&self) -> u64 {
        self.total_count
    }

    /// 出力されたファイルパス一覧を取得する。
    pub fn output_file_paths(&self) -> &[PathBuf] {
        &self.output_files
    }
}

impl PrimeWriter for FilePrimeWriter {
    fn write_prime(&mut self, p: u64) -> io::Result<()> {
        let writer = self
            .current_writer
            .as_mut()
            .expect("FilePrimeWriter not initialized");

        match self.format {
            OutputFormat::Text => {
                writeln!(writer, "{p}")?;
            }
            OutputFormat::CSV => {
                writeln!(writer, "{p},")?;
            }
            OutputFormat::JSON => {
                if !self.first_item_in_json {
                    write!(writer, ",{p}")?;
                } else {
                    write!(writer, "{p}")?;
                    self.first_item_in_json = false;
                }
            }
            OutputFormat::Binary => {
                writer.write_all(&p.to_le_bytes())?;
            }
        }

        self.current_count += 1;
        self.total_count += 1;
        if self.split_count > 0 && self.current_count >= self.split_count {
            self.open_next_file()?;
        }

        Ok(())
    }

    fn finish(&mut self) -> io::Result<()> {
        if let Some(mut w) = self.current_writer.take() {
            if let OutputFormat::JSON = self.format {
                write!(w, "]")?;
            }
            w.flush()?;
        }
        Ok(())
    }
}

/// 最後の素数だけを保持し、ファイル書き出しは一切しない Writer。
/// ディスク I/O がゼロになるため、大規模計算で劇的に高速化します。
pub struct LastPrimeWriter {
    last_prime: Option<u64>,
    /// これまでに検出された素数の総数
    total_count: u64,
}

impl LastPrimeWriter {
    pub fn new() -> Self {
        Self {
            last_prime: None,
            total_count: 0,
        }
    }

    /// これまでに書き込まれた最後の素数を取得します。
    pub fn get_last_prime(&self) -> Option<u64> {
        self.last_prime
    }

    /// これまでに検出された素数の総数を返します。
    ///
    /// - `LastPrimeWriter` は最後の 1 個しか保持しませんが、
    ///   ここで返す値は検出された素数の「個数」を表します。
    pub fn total_primes_written(&self) -> u64 {
        self.total_count
    }
}

impl Default for LastPrimeWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl PrimeWriter for LastPrimeWriter {
    fn write_prime(&mut self, p: u64) -> io::Result<()> {
        self.last_prime = Some(p);
        self.total_count += 1;
        Ok(())
    }

    fn finish(&mut self) -> io::Result<()> {
        Ok(())
    }
}


