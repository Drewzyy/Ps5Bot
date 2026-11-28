//! Small statistics used to judge strategies: a deterministic bootstrap range for the mean return per trade.
//!
//! Why: a learning change used to be applied when 10 trades beat the main lane on P/L. With a few trades per side a gap shows up
//! by luck all the time (a 10 against 10 split can show a big "difference" from nothing). The rule now is: the holdout trades
//! (trades the change never saw) must have a mean return above 0 AND the lower end of the 95% bootstrap range of that mean must be
//! above 0.

use crate::curve::Rng;

pub fn mean(xs: &[f64]) -> f64 {
    if xs.is_empty() { 0.0 } else { xs.iter().sum::<f64>() / xs.len() as f64 }
}

/// 95% percentile bootstrap range of the mean: resample with replacement `resamples` times, take the 2.5% and 97.5% points of
/// the resampled means. Deterministic for a given `seed`. Returns (mean, low, high); None for fewer than 2 values.
pub fn bootstrap_mean_ci(xs: &[f64], resamples: u32, seed: u64) -> Option<(f64, f64, f64)> {
    if xs.len() < 2 || resamples < 100 {
        return None;
    }
    let n = xs.len();
    let mut rng = Rng::new(seed);
    let mut means: Vec<f64> = (0..resamples)
        .map(|_| {
            let mut s = 0.0;
            for _ in 0..n {
                s += xs[(rng.f64() * n as f64) as usize % n];
            }
            s / n as f64
        })
        .collect();
    means.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let at = |q: f64| means[(((means.len() - 1) as f64) * q).round() as usize];
    Some((mean(xs), at(0.025), at(0.975)))
}

/// Is this list of per-trade returns good enough to promote: at least `min_n` trades, mean above 0, lower 95% bound above 0.
/// Returns the verdict and a plain sentence with the numbers (returns are fractions: 0.02 = +2%).
pub fn holdout_verdict(returns: &[f64], min_n: u32, resamples: u32) -> (bool, String) {
    let n = returns.len() as u32;
    if n < min_n {
        return (false, format!("Not enough holdout trades: {n} (needs {min_n})."));
    }
    let seed = 0x5EED_u64 ^ (n as u64).wrapping_mul(0x9E37_79B9) ^ (returns.iter().sum::<f64>() * 1e9) as i64 as u64;
    match bootstrap_mean_ci(returns, resamples, seed) {
        None => (false, format!("Not enough holdout trades to measure: {n}.")),
        Some((m, lo, hi)) => {
            let ok = m > 0.0 && lo > 0.0;
            let range = format!("{n} trades, mean {:+.2}% per trade, 95% range {:+.2}% to {:+.2}%", m * 100.0, lo * 100.0, hi * 100.0);
            (ok, if ok { format!("{range}: the lower end is above 0.") } else if m <= 0.0 { format!("{range}: the mean is not above 0.") } else { format!("{range}: the lower end is not above 0, so this could be luck.") })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bootstrap_is_deterministic_and_brackets_the_mean() {
        let xs: Vec<f64> = (0..40).map(|i| if i % 4 == 0 { -0.01 } else { 0.02 }).collect();
        let a = bootstrap_mean_ci(&xs, 2000, 7).unwrap();
        let b = bootstrap_mean_ci(&xs, 2000, 7).unwrap();
        assert_eq!(a, b);
        assert!(a.1 < a.0 && a.0 < a.2, "{a:?}");
        assert!((a.0 - 0.0125).abs() < 1e-12);
        assert!(bootstrap_mean_ci(&xs[..1], 2000, 7).is_none());
        // a constant sample has a zero-width range
        let c = bootstrap_mean_ci(&[0.01; 30], 500, 1).unwrap();
        assert!((c.1 - 0.01).abs() < 1e-12 && (c.2 - 0.01).abs() < 1e-12);
    }

    #[test]
    fn a_clearly_positive_holdout_passes_and_a_lucky_looking_one_does_not() {
        let good: Vec<f64> = (0..30).map(|i| if i % 5 == 0 { -0.01 } else { 0.03 }).collect();
        let (ok, why) = holdout_verdict(&good, 20, 4000);
        assert!(ok, "{why}");
        // positive mean but noisy: 12 wins of +10%, 12 losses of -9%: mean +0.5%, range crosses 0
        let noisy: Vec<f64> = (0..24).map(|i| if i % 2 == 0 { 0.10 } else { -0.09 }).collect();
        assert!(mean(&noisy) > 0.0);
        let (ok, why) = holdout_verdict(&noisy, 20, 4000);
        assert!(!ok && why.contains("lower end is not above 0"), "{why}");
        let (ok, why) = holdout_verdict(&[-0.02; 30], 20, 4000);
        assert!(!ok && why.contains("mean is not above 0"), "{why}");
        let (ok, why) = holdout_verdict(&good[..10], 20, 4000);
        assert!(!ok && why.contains("Not enough holdout trades"), "{why}");
    }

    #[test]
    fn the_ten_against_ten_luck_case_is_not_promoted() {
        // 10 trades, 6 wins: the old "beats main" idea would call this a win. The range says it is noise.
        let r = [0.04, -0.03, 0.05, -0.04, 0.03, -0.02, 0.06, 0.02, -0.05, -0.01];
        assert!(mean(&r) > 0.0);
        assert!(!holdout_verdict(&r, 10, 4000).0);
    }
}
