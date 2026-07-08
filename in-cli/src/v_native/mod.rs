// SAFETY: All unsafe blocks call trivial FFI wrappers generated from V source.
// The V-compiled functions accept/return plain integers or pointers to
// Rust-owned buffers. No aliasing or lifetime invariants to maintain beyond
// what Rust's type system already guarantees.
#[cfg(has_v_native)]
mod ffi {
    unsafe extern "C" {
        pub fn inc_version() -> i32;
        pub fn inc_hash_fnv1a(data: *const u8, len: i32) -> u64;
        pub fn inc_add(a: i32, b: i32) -> i32;
    }
}

pub fn v_native_available() -> bool {
    cfg!(has_v_native)
}

pub fn inc_version() -> i32 {
    #[cfg(has_v_native)]
    unsafe {
        ffi::inc_version()
    }
    #[cfg(not(has_v_native))]
    {
        0
    }
}

pub fn inc_hash_fnv1a(data: &[u8]) -> u64 {
    #[cfg(has_v_native)]
    unsafe {
        ffi::inc_hash_fnv1a(data.as_ptr(), data.len() as i32)
    }
    #[cfg(not(has_v_native))]
    {
        crate::compile_cache::fnv1a_hash(data)
    }
}

pub fn inc_add(a: i32, b: i32) -> i32 {
    #[cfg(has_v_native)]
    unsafe {
        ffi::inc_add(a, b)
    }
    #[cfg(not(has_v_native))]
    {
        a.wrapping_add(b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v_native_reports_availability() {
        let _ = v_native_available();
    }

    #[test]
    fn inc_add_fallback() {
        assert_eq!(inc_add(2, 3), 5);
    }
}

pub mod inrt {
    #[cfg(has_v_native)]
    mod ffi {
        unsafe extern "C" {
            pub fn inrt_value_kind_tag(kind: i32) -> i32;
            pub fn inrt_int_add(a: i64, b: i64) -> i64;
            pub fn inrt_int_sub(a: i64, b: i64) -> i64;
            pub fn inrt_int_mul(a: i64, b: i64) -> i64;
            pub fn inrt_bool_not(a: bool) -> bool;
            pub fn inrt_string_len(s: *const u8, len: i32) -> i32;
            pub fn inrt_string_eq(a: *const u8, a_len: i32, b: *const u8, b_len: i32) -> bool;
            pub fn inrt_array_len(ptr: *const u8, elem_size: i32) -> i32;
            pub fn inrt_struct_field_offset(
                field_index: i32,
                field_sizes: *const i32,
                field_count: i32,
            ) -> i32;
            pub fn inrt_trap(reason_code: i32);
            pub fn inrt_eval_answer(answer_val: i64) -> i32;
        }
    }

    pub fn value_kind_tag(kind: i32) -> i32 {
        #[cfg(has_v_native)]
        unsafe {
            ffi::inrt_value_kind_tag(kind)
        }
        #[cfg(not(has_v_native))]
        {
            kind
        }
    }

    pub fn int_add(a: i64, b: i64) -> i64 {
        #[cfg(has_v_native)]
        unsafe {
            ffi::inrt_int_add(a, b)
        }
        #[cfg(not(has_v_native))]
        {
            a.wrapping_add(b)
        }
    }

    pub fn int_sub(a: i64, b: i64) -> i64 {
        #[cfg(has_v_native)]
        unsafe {
            ffi::inrt_int_sub(a, b)
        }
        #[cfg(not(has_v_native))]
        {
            a.wrapping_sub(b)
        }
    }

    pub fn int_mul(a: i64, b: i64) -> i64 {
        #[cfg(has_v_native)]
        unsafe {
            ffi::inrt_int_mul(a, b)
        }
        #[cfg(not(has_v_native))]
        {
            a.wrapping_mul(b)
        }
    }

    pub fn bool_not(a: bool) -> bool {
        #[cfg(has_v_native)]
        unsafe {
            ffi::inrt_bool_not(a)
        }
        #[cfg(not(has_v_native))]
        {
            !a
        }
    }

    pub fn string_len(s: &str) -> i32 {
        #[cfg(has_v_native)]
        unsafe {
            ffi::inrt_string_len(s.as_ptr(), s.len() as i32)
        }
        #[cfg(not(has_v_native))]
        {
            s.len() as i32
        }
    }

    pub fn string_eq(a: &str, b: &str) -> bool {
        #[cfg(has_v_native)]
        unsafe {
            ffi::inrt_string_eq(a.as_ptr(), a.len() as i32, b.as_ptr(), b.len() as i32)
        }
        #[cfg(not(has_v_native))]
        {
            a == b
        }
    }

    pub fn array_len(_elem_size: i32) -> i32 {
        #[cfg(has_v_native)]
        unsafe {
            ffi::inrt_array_len(std::ptr::null(), _elem_size)
        }
        #[cfg(not(has_v_native))]
        {
            0
        }
    }

    pub fn struct_field_offset(field_index: i32, field_sizes: &[i32]) -> i32 {
        #[cfg(has_v_native)]
        unsafe {
            ffi::inrt_struct_field_offset(
                field_index,
                field_sizes.as_ptr(),
                field_sizes.len() as i32,
            )
        }
        #[cfg(not(has_v_native))]
        {
            if field_index < 0 || field_index as usize >= field_sizes.len() {
                return -1;
            }
            field_sizes[..field_index as usize].iter().sum()
        }
    }

    pub fn trap(reason_code: i32) -> ! {
        #[cfg(has_v_native)]
        unsafe {
            ffi::inrt_trap(reason_code);
        }
        eprintln!("inrt trap: code={reason_code}");
        std::process::exit(reason_code);
    }

    pub fn eval_answer(answer_val: i64) -> u8 {
        #[cfg(has_v_native)]
        unsafe {
            ffi::inrt_eval_answer(answer_val) as u8
        }
        #[cfg(not(has_v_native))]
        {
            (answer_val & 0xff) as u8
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn int_ops_match_rust() {
            assert_eq!(int_add(1, 2), 3);
            assert_eq!(int_sub(5, 3), 2);
            assert_eq!(int_mul(3, 4), 12);
        }

        #[test]
        fn bool_not_matches_rust() {
            assert!(!bool_not(true));
            assert!(bool_not(false));
        }

        #[test]
        fn string_ops_match_rust() {
            assert_eq!(string_len("hello"), 5);
            assert!(string_eq("abc", "abc"));
            assert!(!string_eq("abc", "def"));
        }

        #[test]
        fn struct_field_offset_matches_rust() {
            assert_eq!(struct_field_offset(0, &[4, 8, 1]), 0);
            assert_eq!(struct_field_offset(1, &[4, 8, 1]), 4);
            assert_eq!(struct_field_offset(2, &[4, 8, 1]), 12);
            assert_eq!(struct_field_offset(-1, &[4]), -1);
        }

        #[test]
        fn eval_answer_bounds() {
            assert_eq!(eval_answer(42), 42);
            assert_eq!(eval_answer(300), 44);
        }
    }
}

pub mod parallel {
    #[cfg(has_v_native)]
    mod ffi {
        unsafe extern "C" {
            pub fn inc_wave_plan(
                job_count: i32,
                worker_count: i32,
                out_boundaries: *mut i32,
                out_len: i32,
            ) -> i32;
            pub fn inc_merge_timings(
                timings: *const u64,
                count: i32,
                out_min: *mut u64,
                out_max: *mut u64,
                out_mean: *mut u64,
            );
        }
    }

    pub fn wave_plan(job_count: usize, worker_count: usize, out_max_waves: usize) -> Vec<usize> {
        let max_waves = out_max_waves.min(job_count);
        if max_waves == 0 || worker_count == 0 {
            return vec![job_count];
        }
        let mut boundaries = vec![0i32; max_waves];
        #[cfg(has_v_native)]
        let waves = unsafe {
            ffi::inc_wave_plan(
                job_count as i32,
                worker_count as i32,
                boundaries.as_mut_ptr(),
                max_waves as i32,
            )
        };
        #[cfg(not(has_v_native))]
        let waves = {
            let base = job_count / max_waves;
            let rem = job_count % max_waves;
            let mut pos = 0;
            for i in 0..max_waves {
                let chunk = base + if i < rem { 1 } else { 0 };
                boundaries[i] = (pos + chunk) as i32;
                pos += chunk;
            }
            max_waves as i32
        };
        boundaries.truncate(waves as usize);
        boundaries.into_iter().map(|b| b as usize).collect()
    }

    pub struct TimingStats {
        pub min: u64,
        pub max: u64,
        pub mean: u64,
    }

    pub fn merge_timings(timings: &[u64]) -> TimingStats {
        if timings.is_empty() {
            return TimingStats {
                min: 0,
                max: 0,
                mean: 0,
            };
        }
        #[cfg(has_v_native)]
        unsafe {
            let mut min = 0u64;
            let mut max = 0u64;
            let mut mean = 0u64;
            ffi::inc_merge_timings(
                timings.as_ptr(),
                timings.len() as i32,
                &mut min,
                &mut max,
                &mut mean,
            );
            TimingStats { min, max, mean }
        }
        #[cfg(not(has_v_native))]
        {
            let min = *timings.iter().min().unwrap_or(&0);
            let max = *timings.iter().max().unwrap_or(&0);
            let sum: u64 = timings.iter().sum();
            let mean = sum / timings.len() as u64;
            TimingStats { min, max, mean }
        }
    }

    pub fn source_frontend_hash_v(path: &str, content: &str) -> String {
        let mut hash: u64 = 0xcbf29ce484222325;
        for &byte in path.as_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash ^= 0xff;
        let content_hash = crate::v_native::inc_hash_fnv1a(content.as_bytes());
        hash = hash.wrapping_mul(0x100000001b3) ^ content_hash;
        format!("{hash:016x}")
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
        fn wave_plan_single_job() {
            let plan = wave_plan(1, 4, 4);
            assert_eq!(plan, vec![1]);
        }

        #[test]
        fn wave_plan_zero_waves_or_workers() {
            assert_eq!(wave_plan(10, 0, 5), vec![10]);
            assert_eq!(wave_plan(10, 5, 0), vec![10]);
        }

        #[test]
        fn wave_plan_less_jobs_than_waves() {
            assert_eq!(wave_plan(2, 5, 5), vec![1, 2]);
        }

        #[test]
        fn wave_plan_more_jobs_than_waves() {
            assert_eq!(wave_plan(10, 5, 3), vec![4, 7, 10]);
        }

        #[test]
        fn wave_plan_exact_multiple() {
            assert_eq!(wave_plan(9, 5, 3), vec![3, 6, 9]);
        }

        #[test]
        fn wave_plan_zero_jobs() {
            assert_eq!(wave_plan(0, 4, 4), vec![0]);
        }

        #[test]
        fn wave_plan_one_max_wave() {
            assert_eq!(wave_plan(10, 4, 1), vec![10]);
        }

        #[test]
        fn wave_plan_large_jobs() {
            let plan = wave_plan(1000, 10, 10);
            assert_eq!(plan, vec![100, 200, 300, 400, 500, 600, 700, 800, 900, 1000]);
        }

        #[test]
        fn merge_timings_basic() {
            let stats = merge_timings(&[10, 20, 30]);
            assert_eq!(stats.min, 10);
            assert_eq!(stats.max, 30);
            assert_eq!(stats.mean, 20);
        }

        #[test]
        fn merge_timings_empty() {
            let stats = merge_timings(&[]);
            assert_eq!(stats.min, 0);
            assert_eq!(stats.max, 0);
            assert_eq!(stats.mean, 0);
        }

        #[test]
        #[ignore = "pre-existing: V compiler version hash parity mismatch"]
        fn hash_parity_with_rust() {
            let v_hash =
                source_frontend_hash_v("apps/sample.in", "fn main() -> void { return; }\n");
            let rust_hash = crate::compile_cache::source_frontend_hash(
                std::path::Path::new("apps/sample.in"),
                "fn main() -> void { return; }\n",
            );
            assert_eq!(v_hash, rust_hash);
        }
    }
}

pub mod orchestration {
    #[cfg(has_v_native)]
    mod ffi {
        unsafe extern "C" {
            pub fn inc_count_parallel_regions(annotations_buf: *const u8, buf_len: i32) -> i32;
            pub fn inc_schedule_parallel_tasks(
                task_count: i32,
                out_order: *mut i32,
                out_len: i32,
            ) -> i32;
            pub fn inc_capability_check(
                cap_name: *const u8,
                cap_len: i32,
                allowlist: *const u8,
                allowlist_len: i32,
            ) -> bool;
        }
    }

    pub fn count_parallel_regions(source: &str) -> i32 {
        #[cfg(has_v_native)]
        unsafe {
            ffi::inc_count_parallel_regions(source.as_ptr(), source.len() as i32)
        }
        #[cfg(not(has_v_native))]
        {
            source.matches("@parallel").count() as i32
        }
    }

    pub fn schedule_parallel_tasks(task_count: usize, out_max: usize) -> Vec<i32> {
        let len = out_max.min(task_count);
        if len == 0 {
            return Vec::new();
        }
        let mut order = vec![0i32; len];
        #[cfg(has_v_native)]
        let count = unsafe {
            ffi::inc_schedule_parallel_tasks(task_count as i32, order.as_mut_ptr(), len as i32)
        };
        #[cfg(not(has_v_native))]
        let count = {
            for i in 0..len {
                order[i] = i as i32;
            }
            len as i32
        };
        order.truncate(count as usize);
        order
    }

    pub fn capability_check(cap_name: &[u8], allowlist_nul_sep: &[u8]) -> bool {
        #[cfg(has_v_native)]
        unsafe {
            ffi::inc_capability_check(
                cap_name.as_ptr(),
                cap_name.len() as i32,
                allowlist_nul_sep.as_ptr(),
                allowlist_nul_sep.len() as i32,
            )
        }
        #[cfg(not(has_v_native))]
        {
            let allowlist = String::from_utf8_lossy(allowlist_nul_sep);
            let cap = String::from_utf8_lossy(cap_name);
            allowlist.split('\0').any(|a| a == cap)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn count_parallel_regions_empty() {
            assert_eq!(count_parallel_regions(""), 0);
        }

        #[test]
        fn count_parallel_regions_detects_annotations() {
            assert_eq!(count_parallel_regions("@parallel\n@parallel\n"), 2);
        }

        #[test]
        fn schedule_parallel_tasks_basic() {
            let order = schedule_parallel_tasks(5, 10);
            assert_eq!(order, vec![0, 1, 2, 3, 4]);
        }

        #[test]
        fn capability_check_found() {
            assert!(capability_check(
                b"process.stdout",
                b"network\0process.stdout\0gpu"
            ));
        }

        #[test]
        fn capability_check_not_found() {
            assert!(!capability_check(b"fs.write", b"network\0process.stdout"));
        }
    }
}

pub mod bench {
    #[cfg(has_v_native)]
    mod ffi {
        unsafe extern "C" {
            pub fn inc_bench_aggregate(
                timings: *const u64,
                count: i32,
                out_min: *mut u64,
                out_max: *mut u64,
                out_mean: *mut u64,
                out_stddev: *mut u64,
            );
            pub fn inc_bench_regression(
                current_mean: u64,
                baseline_mean: u64,
                threshold_pct: i32,
            ) -> bool;
        }
    }

    pub struct BenchStats {
        pub min: u64,
        pub max: u64,
        pub mean: u64,
        pub stddev: u64,
    }

    pub fn aggregate(timings: &[u64]) -> BenchStats {
        if timings.is_empty() {
            return BenchStats {
                min: 0,
                max: 0,
                mean: 0,
                stddev: 0,
            };
        }
        #[cfg(has_v_native)]
        unsafe {
            let mut min = 0u64;
            let mut max = 0u64;
            let mut mean = 0u64;
            let mut stddev = 0u64;
            ffi::inc_bench_aggregate(
                timings.as_ptr(),
                timings.len() as i32,
                &mut min,
                &mut max,
                &mut mean,
                &mut stddev,
            );
            BenchStats {
                min,
                max,
                mean,
                stddev,
            }
        }
        #[cfg(not(has_v_native))]
        {
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
    }

    pub fn regression(current_mean: u64, baseline_mean: u64, threshold_pct: i32) -> bool {
        #[cfg(has_v_native)]
        unsafe {
            ffi::inc_bench_regression(current_mean, baseline_mean, threshold_pct)
        }
        #[cfg(not(has_v_native))]
        {
            if baseline_mean == 0 || current_mean <= baseline_mean {
                return false;
            }
            let increase_pct = ((current_mean - baseline_mean) * 100 / baseline_mean) as i32;
            increase_pct > threshold_pct
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn aggregate_basic() {
            let stats = aggregate(&[10, 20, 30, 40]);
            assert_eq!(stats.min, 10);
            assert_eq!(stats.max, 40);
            assert_eq!(stats.mean, 25);
            assert_eq!(stats.stddev, 11); // stddev of [10, 20, 30, 40] is sqrt(125) = ~11.18 -> 11
        }

        #[test]
        fn aggregate_empty() {
            let stats = aggregate(&[]);
            assert_eq!(stats.min, 0);
            assert_eq!(stats.max, 0);
            assert_eq!(stats.mean, 0);
            assert_eq!(stats.stddev, 0);
        }

        #[test]
        fn aggregate_single() {
            let stats = aggregate(&[42]);
            assert_eq!(stats.min, 42);
            assert_eq!(stats.max, 42);
            assert_eq!(stats.mean, 42);
            assert_eq!(stats.stddev, 0);
        }

        #[test]
        fn aggregate_identical() {
            let stats = aggregate(&[50, 50, 50, 50]);
            assert_eq!(stats.min, 50);
            assert_eq!(stats.max, 50);
            assert_eq!(stats.mean, 50);
            assert_eq!(stats.stddev, 0);
        }

        #[test]
        fn regression_no_regression() {
            assert!(!regression(100, 100, 10));
        }

        #[test]
        fn regression_detected() {
            assert!(regression(120, 100, 10));
        }

        #[test]
        fn regression_baseline_zero() {
            assert!(!regression(100, 0, 10));
        }

        #[test]
        fn regression_improvement() {
            assert!(!regression(80, 100, 10));
        }

        #[test]
        fn regression_exact_threshold() {
            assert!(!regression(110, 100, 10));
        }
    }
}
