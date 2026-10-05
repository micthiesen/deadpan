//! Nearest-rank distribution shared by `cargo xtask perf` and the
//! `deadpan-cli` `perf` example (included there by path), so both report the
//! same percentile definition. No dependencies.

/// Sorted-sample summary. Percentiles use nearest rank: the smallest sample
/// with at least `p` of the samples at or below it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Distribution {
    pub n: usize,
    pub min: f64,
    pub p50: f64,
    pub p95: f64,
    pub max: f64,
    pub mean: f64,
}

pub fn distribution(samples: &[f64]) -> Option<Distribution> {
    if samples.is_empty() {
        return None;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let rank = |p: f64| {
        let index = (p * sorted.len() as f64).ceil() as usize;
        sorted[index.clamp(1, sorted.len()) - 1]
    };
    Some(Distribution {
        n: sorted.len(),
        min: sorted[0],
        p50: rank(0.50),
        p95: rank(0.95),
        max: sorted[sorted.len() - 1],
        mean: sorted.iter().sum::<f64>() / sorted.len() as f64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_rank_percentiles() {
        let samples: Vec<f64> = (1..=20).rev().map(f64::from).collect();
        let summary = distribution(&samples).unwrap();
        assert_eq!((summary.n, summary.min, summary.max), (20, 1.0, 20.0));
        assert_eq!((summary.p50, summary.p95), (10.0, 19.0));
        assert_eq!(distribution(&[]), None);
        assert_eq!(distribution(&[3.0]).unwrap().p95, 3.0);
    }
}
