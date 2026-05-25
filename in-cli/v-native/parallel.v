module parallel

@[export: "inc_wave_plan"]
fn inc_wave_plan(job_count int, worker_count int, out_boundaries &int, out_len int) int {
	max_waves := if out_len < job_count { out_len } else { job_count }
	if max_waves <= 0 || worker_count <= 0 {
		return 0
	}
	base := job_count / max_waves
	rem := job_count % max_waves
	mut pos := 0
	for i in 0 .. max_waves {
		chunk := base + if i < rem { 1 } else { 0 }
		unsafe { out_boundaries[i] = pos + chunk }
		pos += chunk
	}
	return max_waves
}

@[export: "inc_merge_timings"]
fn inc_merge_timings(timings &u64, count int, out_min &u64, out_max &u64, out_mean &u64) {
	if count <= 0 {
		unsafe {
			*out_min = 0
			*out_max = 0
			*out_mean = 0
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
	unsafe {
		*out_min = min_val
		*out_max = max_val
		*out_mean = sum / u64(count)
	}
}
