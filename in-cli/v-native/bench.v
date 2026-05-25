module bench

import math

@[export: "inc_bench_aggregate"]
fn inc_bench_aggregate(timings &u64, count int, out_min &u64, out_max &u64, out_mean &u64, out_stddev &u64) {
	if count <= 0 {
		unsafe {
			*out_min = 0
			*out_max = 0
			*out_mean = 0
			*out_stddev = 0
		}
		return
	}
	mut min_val := u64(0xffffffffffffffff)
	mut max_val := u64(0)
	mut sum := u64(0)
	for i in 0 .. count {
		val := unsafe { timings[i] }
		if val < min_val { min_val = val }
		if val > max_val { max_val = val }
		sum += val
	}
	mean := sum / u64(count)
	mut sq_sum := f64(0)
	for i in 0 .. count {
		val := unsafe { timings[i] }
		diff := if val > mean { f64(val - mean) } else { f64(mean - val) }
		sq_sum += diff * diff
	}
	stddev := u64(math.sqrt(sq_sum / f64(count)))
	unsafe {
		*out_min = min_val
		*out_max = max_val
		*out_mean = mean
		*out_stddev = stddev
	}
}

@[export: "inc_bench_regression"]
fn inc_bench_regression(current_mean u64, baseline_mean u64, threshold_pct int) bool {
	if baseline_mean == 0 {
		return false
	}
	increase_pct := int((current_mean - baseline_mean) * 100 / baseline_mean)
	return increase_pct > threshold_pct
}
