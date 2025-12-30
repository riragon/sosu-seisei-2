//! Sosu-Analyze（Analyze Tabs）共通定義。

/// Sosu-Analyze モードの状態をまとめた構造体内で使う「分析タブ」。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum AnalyzeTab {
    /// Last digit (1/3/7/9) analysis
    #[default]
    LastDigit,
    /// mod 30 distribution + transition analysis
    Mod30,
    /// mod 30 PRHS (Prime Residue History Study)
    Mod30PRHS,
    /// mod 210 PRHS (Prime Residue History Study)
    Mod210PRHS,
    /// Validation (integrity/theory/wheel/range-dependency)
    Validation,
}

/// mod 30 の剰余類（2,3,5 の倍数を除外した 8 種類）。
///
/// - インデックス 0..7 の意味:
///   0=1, 1=7, 2=11, 3=13, 4=17, 5=19, 6=23, 7=29
pub const MOD30_RESIDUES: [u64; 8] = [1, 7, 11, 13, 17, 19, 23, 29];

/// mod 210 の剰余類（2,3,5,7 と互いに素な 48 種類）。
///
/// - インデックス 0..47 の順序はこの配列順（昇順）に固定する。
pub const MOD210_RESIDUES: [u64; 48] = [
    1, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61, 67, 71, 73, 79, 83, 89, 97, 101,
    103, 107, 109, 113, 121, 127, 131, 137, 139, 143, 149, 151, 157, 163, 167, 169, 173, 179, 181,
    187, 191, 193, 197, 199, 209,
];

/// PRHS のビン分割方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum PRHSBinMode {
    /// 対数ビン: log10(p) の等間隔
    #[default]
    Log10,
    /// 等量ビン: 同数の素数（フィルタ後の prime index）で分割
    EqualCount,
}
