module orchestration

@[export: "inc_count_parallel_regions"]
fn inc_count_parallel_regions(annotations_buf &u8, buf_len int) int {
	mut count := 0
	mut i := 0
	for i + 9 <= buf_len {
		b0 := unsafe { annotations_buf[i] }
		b1 := unsafe { annotations_buf[i+1] }
		b2 := unsafe { annotations_buf[i+2] }
		b3 := unsafe { annotations_buf[i+3] }
		b4 := unsafe { annotations_buf[i+4] }
		b5 := unsafe { annotations_buf[i+5] }
		b6 := unsafe { annotations_buf[i+6] }
		b7 := unsafe { annotations_buf[i+7] }
		b8 := unsafe { annotations_buf[i+8] }
		if b0 == u8(`@`)
		&& b1 == u8(`p`)
		&& b2 == u8(`a`)
		&& b3 == u8(`r`)
		&& b4 == u8(`a`)
		&& b5 == u8(`l`)
		&& b6 == u8(`l`)
		&& b7 == u8(`e`)
		&& b8 == u8(`l`)
		{
			count++
			i += 9
		} else {
			i++
		}
	}
	return count
}

@[export: "inc_schedule_parallel_tasks"]
fn inc_schedule_parallel_tasks(task_count int, out_order &int, out_len int) int {
	if out_len < task_count {
		return 0
	}
	for i in 0 .. task_count {
		unsafe { out_order[i] = i }
	}
	return task_count
}

@[export: "inc_capability_check"]
fn inc_capability_check(cap_name &u8, cap_len int, allowlist &u8, allowlist_len int) bool {
	mut pos := 0
	for pos < allowlist_len {
		mut end := pos
		for end < allowlist_len && unsafe { allowlist[end] } != 0 {
			end++
		}
		if end - pos == cap_len {
			mut matches := true
			for j in 0 .. cap_len {
				if unsafe { allowlist[pos + j] } != unsafe { cap_name[j] } {
					matches = false
					break
				}
			}
			if matches {
				return true
			}
		}
		pos = end + 1
	}
	return false
}
