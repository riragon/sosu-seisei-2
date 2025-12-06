#![cfg(not(windows))]

use sosu_seisei_main2::prime_pi_engine::compute_prime_pi;

/// 小さい x に対して、既知の π(x) の値と一致することを確認する。
#[test]
fn prime_pi_small_values_match_known_results() {
    // 出典: 標準的な素数表 / OEIS A006880 など
    let cases: &[(u64, u64)] = &[
        (0, 0),
        (1, 0),
        (2, 1),
        (3, 2),
        (10, 4),
        (100, 25),
        (1_000, 168),
        (10_000, 1_229),
        (100_000, 9_592),
        (1_000_000, 78_498),
    ];

    for &(x, expected) in cases {
        let pi = compute_prime_pi(x).expect("primecount_pi failed");
        assert_eq!(pi, expected, "pi({x}) should be {expected}, got {pi}");
    }
}

/// 素朴なエラトステネスの篩実装と比較し、ある程度の範囲で一致することを確認する。
#[test]
fn prime_pi_matches_naive_sieve_up_to_1e6() {
    let test_points: &[u64] = &[10, 100, 1_000, 10_000, 100_000, 1_000_000];

    for &x in test_points {
        let expected = prime_pi_naive(x as usize) as u64;
        let pi = compute_prime_pi(x).expect("primecount_pi failed");
        assert_eq!(pi, expected, "pi({x}) should equal naive sieve result");
    }
}

/// WolframAlpha などでよく使われる代表値 π(10^11) をテストする。
///
/// 実行時間が比較的長くなる可能性があるため、デフォルトでは無視しておき、
/// 必要なときに `cargo test -- --ignored` で明示的に回す想定。
#[test]
#[ignore]
fn prime_pi_1e11_matches_reference() {
    let x = 100_000_000_000_u64;
    let expected = 4_118_054_813_u64;
    let pi = compute_prime_pi(x).expect("primecount_pi failed");
    assert_eq!(pi, expected, "pi({x}) should match known reference value");
}

/// 単純なエラトステネスの篩による π(x) 実装（テスト専用）。
fn prime_pi_naive(limit: usize) -> usize {
    if limit < 2 {
        return 0;
    }

    let mut is_prime = vec![true; limit + 1];
    is_prime[0] = false;
    is_prime[1] = false;

    let mut p = 2usize;
    while p * p <= limit {
        if is_prime[p] {
            let mut multiple = p * p;
            while multiple <= limit {
                is_prime[multiple] = false;
                multiple += p;
            }
        }
        p += 1;
    }

    is_prime.iter().take(limit + 1).filter(|&&b| b).count()
}
