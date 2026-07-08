//! Small pure helpers for owned-compile timing and bench summaries (Rust-only).

pub fn wave_plan(job_count: usize, worker_count: usize, out_max_waves: usize) -> Vec<usize> {
    let max_waves = out_max_waves.min(job_count);
    if max_waves == 0 || worker_count == 0 {
        return vec![job_count];
    }
    let base = job_count / max_waves;
    let rem = job_count % max_waves;
    let mut pos = 0usize;
    let mut boundaries = Vec::with_capacity(max_waves);
    for i in 0..max_waves {
        let chunk = base + if i < rem { 1 } else { 0 };
        pos += chunk;
        boundaries.push(pos);
    }
    boundaries
}

pub struct BenchStats {
    pub min: u64,
    pub max: u64,
    pub mean: u64,
    pub stddev: u64,
}

pub fn bench_aggregate(timings: &[u64]) -> BenchStats {
    if timings.is_empty() {
        return BenchStats {
            min: 0,
            max: 0,
            mean: 0,
            stddev: 0,
        };
    }
    let min = *timings.iter().min().unwrap_or(&0);
    let max = *timings.iter().max().unwrap_or(&0);
    let sum: u64 = timings.iter().sum();
    let mean = sum / timings.len() as u64;
    let sq_sum: f64 = timings
        .iter()
        .map(|&t| {
            let diff = t.abs_diff(mean);
            (diff as f64) * (diff as f64)
        })
        .sum();
    let stddev = (sq_sum / timings.len() as f64).sqrt() as u64;
    BenchStats {
        min,
        max,
        mean,
        stddev,
    }
}

pub fn bench_regression(current_mean: u64, baseline_mean: u64, threshold_pct: i32) -> bool {
    if baseline_mean == 0 || current_mean <= baseline_mean {
        return false;
    }
    let increase_pct = ((current_mean - baseline_mean) * 100 / baseline_mean) as i32;
    increase_pct > threshold_pct
}

pub fn count_parallel_regions(source: &str) -> i32 {
    source.matches("@parallel").count() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wave_plan_deterministic() {
        let a = wave_plan(10, 4, 4);
        let b = wave_plan(10, 4, 4);
        assert_eq!(a, b);
        assert!(a.len() <= 4);
    }

    #[test]
    fn wave_plan_zero_jobs() {
        assert_eq!(wave_plan(0, 4, 4), vec![0]);
    }

    #[test]
    fn wave_plan_large_jobs() {
        let plan = wave_plan(1000, 10, 10);
        assert_eq!(
            plan,
            vec![100, 200, 300, 400, 500, 600, 700, 800, 900, 1000]
        );
    }

    #[test]
    fn aggregate_unordered() {
        let stats = bench_aggregate(&[100, 10, 50, 20]);
        assert_eq!(stats.min, 10);
        assert_eq!(stats.max, 100);
        assert_eq!(stats.mean, 45);
        assert_eq!(stats.stddev, 35);
    }

    #[test]
    fn regression_detected() {
        assert!(bench_regression(120, 100, 10));
    }
}