//! Analyze Tab: Validation
//!
//! NOTE:
//! - types + engine + markdown + UI をこのファイルに集約する。
//! - 現段階では、エンジンのエントリポイントのみを `validation_engine` から re-export する。

pub use crate::analyze::validation_engine::analyze_validation_binary_file;

/// Validation: 整合性チェック
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct IntegrityCheck {
    pub c2_sum: u64,
    pub expected_triplets: u64,
    pub c1_c2_consistent: bool,
    pub row_sums_ok: bool,
}

/// Validation: 理論値比較（P(k|j) が 1/φ(M) に近いか）
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct TheoryComparison {
    /// 期待値（= 1 / 状態数）。mod30: 0.125, mod210: 1/48
    pub expected_uniform: f64,
    /// 最大偏差（max |p - expected|）
    pub max_deviation: f64,
    /// χ² 統計量（行ごとに期待一様分布を仮定）
    pub chi_squared: f64,
    /// 自由度（df）
    pub df: u64,
    /// p 値（上側確率）。計算できない場合は NaN
    pub p_value: f64,
}

/// Validation: Wheel 乱数との比較結果
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct WheelComparison {
    /// ベースライン名（例: "iid_residues", "wheel_thinned_candidates"）
    pub baseline: String,
    pub wheel_sample_size: u64,
    /// wheel-thinned の推定採択確率 q（適用しない場合は NaN）
    pub accept_prob_q: f64,
    pub prime_cmi: f64,
    pub wheel_cmi: f64,
    pub delta_cmi: f64,
    pub prime_delta_ll: f64,
    pub wheel_delta_ll: f64,
    pub delta_delta_ll: f64,
    /// 結論（例: "sieve-derived" / "prime-specific" / "inconclusive"）
    pub conclusion: String,
}

/// Validation: 範囲依存性（max p で切ったときの指標推移）
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct RangeResult {
    pub max_p: u64,
    pub triplets: u64,
    pub cmi: f64,
    pub delta_ll: f64,
}

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct RangeDependency {
    pub ranges: Vec<RangeResult>,
}

/// Lemke Oliver–Soundararajan (2016) 理論比較（連続素数の遷移バイアス）
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct LemkeOliverComparison {
    /// 法（10 または 30）
    pub modulus: u64,
    /// 素数の上限（この比較で使った最大 p）
    pub prime_max: u64,
    /// χ² 統計量
    pub chi_squared: f64,
    /// 自由度
    pub df: u64,
    /// p値（上側確率）
    pub p_value: f64,
    /// 各ペア詳細
    pub pairwise: Vec<PairwiseResult>,
    /// スケーリング解析（Bin/Range別の c 推定）
    pub scaling: Option<ScalingAnalysis>,
    /// Final verdict: "Consistent" / "QualitativeMatch" / "PartialMatch" / "TheoryInconsistent" / "InsufficientData"
    pub verdict: String,
}

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct PairwiseResult {
    pub from_residue: u64,
    pub to_residue: u64,
    pub p_observed: f64,
    pub p_theory: f64,
    pub delta: f64,
    pub z_score: f64,
    /// 推定 c(a,b) = (P_obs - 1/φ(q)) * log(x)
    pub estimated_c: f64,
    /// 期待される c の符号/相対（-1,0,+1）
    pub expected_c_sign: i8,
}

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct BinScalingData {
    /// prefix の上限（max p）
    pub max_p: u64,
    /// log10(max_p)
    pub log10_mid: f64,
    /// 推定 c(a,b) 行列（a,b は剰余インデックス）
    pub estimated_c: Vec<Vec<f64>>,
    /// 理論的な符号パターンと概ね一致しているか
    pub sign_match: bool,
}

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScalingAnalysis {
    pub bin_data: Vec<BinScalingData>,
    pub convergence_ok: bool,
}

/// Validation の集約結果
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct ValidationResult {
    pub modulus: u64,
    pub integrity: IntegrityCheck,
    pub theory: TheoryComparison,
    /// Lemke Oliver–Soundararajan (2016) 理論比較（mod10/mod30）
    pub lemke_oliver: Option<Vec<LemkeOliverComparison>>,
    /// 複数ベースライン（iid / wheel-thinned 等）
    pub wheel: Vec<WheelComparison>,
    /// 代表結論（デフォルトは wheel-thinned を優先）
    pub overall_conclusion: String,
    pub range_dep: RangeDependency,
}

/// Validation: mod 30 と mod 210 をまとめた結果。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ValidationBundle {
    pub mod30: ValidationResult,
    pub mod30_triplets: u64,
    pub mod210: ValidationResult,
    pub mod210_triplets: u64,
}

impl Default for ValidationBundle {
    fn default() -> Self {
        Self {
            mod30: ValidationResult {
                modulus: 30,
                ..Default::default()
            },
            mod30_triplets: 0,
            mod210: ValidationResult {
                modulus: 210,
                ..Default::default()
            },
            mod210_triplets: 0,
        }
    }
}
