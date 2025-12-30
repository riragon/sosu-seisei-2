use crate::engine_types::PrimeResult;

/// `n` 以下の最大の整数平方根を 2 分探索で求める。
pub fn integer_sqrt(n: u64) -> u64 {
    let mut low = 0u64;
    let mut high = n;
    while low <= high {
        let mid = (low + high) >> 1;
        match mid.checked_mul(mid) {
            Some(val) if val == n => return mid,
            Some(val) if val < n => low = mid + 1,
            _ => high = mid - 1,
        }
    }
    high
}

/// 単純なエラトステネスの篩で `[2, limit]` の素数を列挙する。
pub fn simple_sieve(limit: u64) -> PrimeResult<Vec<u64>> {
    if limit < 2 {
        return Ok(Vec::new());
    }

    let size = (limit + 1) as usize;
    let mut is_prime = vec![true; size];
    is_prime[0] = false;
    if limit >= 1 {
        is_prime[1] = false;
    }

    let lim_sqrt = integer_sqrt(limit);
    for i in 2..=lim_sqrt as usize {
        if is_prime[i] {
            let mut j = i * i;
            while j <= limit as usize {
                is_prime[j] = false;
                j += i;
            }
        }
    }

    let mut primes = Vec::new();
    for (i, &flag) in is_prime.iter().enumerate().take(limit as usize + 1).skip(2) {
        if flag {
            primes.push(i as u64);
        }
    }
    Ok(primes)
}
