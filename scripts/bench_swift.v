module main

import os
import time
import json

struct RunResult {
	ok bool
	ms f64
	output string
}

struct InStageBreakdown {
mut:
	ast_refresh_ms f64
	swift_frontend_ms f64
	sil_analysis_ms f64
	total_ms f64
}

struct BenchRow {
	example string
	module string
	swiftc_ok bool
	swiftc_ms f64
	swiftpkg_ok bool
	swiftpkg_ms f64
	in_ok bool
	in_ms f64
	hybrid_cli_ok bool
	hybrid_cli_ms f64
	speed_ratio_in_over_swiftpkg f64
	in_stage_ast_refresh_ms f64
	in_stage_swift_frontend_ms f64
	in_stage_sil_analysis_ms f64
	in_stage_total_ms f64
	in_driver_overhead_ms f64
	in_wrapper_overhead_ms f64
	loss_bucket string
	swiftc_error string
	swiftpkg_error string
	in_error string
}

struct BenchDoc {
	env BenchEnv
	rows []BenchRow
}

struct BenchEnv {
	generated_at_utc string
	bench_runs int
	host_os string
	host_kernel string
	cpu string
	memory string
	swift_version string
	rustc_version string
	cargo_version string
	v_version string
	in_bin string
	hybrid_cli_bin string
}

fn run_timed(cwd string, cmd string) RunResult {
	start := time.now()
	wrapped := 'cd "${cwd}" && ${cmd}'
	res := os.execute(wrapped)
	elapsed_ms := f64(time.since(start).microseconds()) / 1000.0
	return RunResult{
		ok: res.exit_code == 0
		ms: elapsed_ms
		output: res.output.trim_space()
	}
}

fn run_with_status(label string, cwd string, cmd string) RunResult {
	println('  -> ${label}: start')
	res := run_timed(cwd, cmd)
	status := if res.ok { 'ok' } else { 'fail' }
	println('  <- ${label}: ${status} ${res.ms:.2f}ms')
	return res
}

fn command_output(cwd string, cmd string) string {
	res := run_timed(cwd, cmd)
	if !res.ok {
		return 'unavailable'
	}
	return res.output
}

fn median_ms(values []f64) f64 {
	if values.len == 0 {
		return 0.0
	}
	mut sorted := values.clone()
	sorted.sort()
	mid := sorted.len / 2
	if sorted.len % 2 == 1 {
		return sorted[mid]
	}
	return (sorted[mid - 1] + sorted[mid]) / 2.0
}

fn find_package_root(path string) string {
	mut cur := os.dir(path)
	for {
		if os.exists(os.join_path(cur, 'Package.swift')) {
			return cur
		}
		parent := os.dir(cur)
		if parent == cur {
			return ''
		}
		cur = parent
	}
	return ''
}

fn short_err(s string) string {
	if s.len <= 500 {
		return s
	}
	return s[..500]
}

fn parse_stage_value(line string, key string) f64 {
	prefix := '${key}='
	if !line.starts_with(prefix) {
		return 0.0
	}
	value := line[prefix.len..].trim_space()
	return value.f64()
}

fn parse_in_stages(output string) InStageBreakdown {
	mut stages := InStageBreakdown{}
	for line in output.split_into_lines() {
		trimmed := line.trim_space()
		if trimmed.starts_with('stage.ast_refresh_ms=') {
			stages.ast_refresh_ms = parse_stage_value(trimmed, 'stage.ast_refresh_ms')
		} else if trimmed.starts_with('stage.swift_frontend_ms=') {
			stages.swift_frontend_ms = parse_stage_value(trimmed, 'stage.swift_frontend_ms')
		} else if trimmed.starts_with('stage.sil_analysis_ms=') {
			stages.sil_analysis_ms = parse_stage_value(trimmed, 'stage.sil_analysis_ms')
		} else if trimmed.starts_with('stage.total_ms=') {
			stages.total_ms = parse_stage_value(trimmed, 'stage.total_ms')
		}
	}
	return stages
}

fn classify_loss(swiftpkg_ms f64, in_ms f64, in_driver_overhead_ms f64, in_stages InStageBreakdown) string {
	if swiftpkg_ms <= 0 {
		return 'unknown'
	}
	if in_ms <= swiftpkg_ms {
		return 'win'
	}
	delta := in_ms - swiftpkg_ms
	if in_driver_overhead_ms > delta * 0.5 {
		return 'driver-overhead'
	}
	if in_stages.swift_frontend_ms >= in_stages.ast_refresh_ms && in_stages.swift_frontend_ms >= in_stages.sil_analysis_ms {
		return 'swift-frontend-stage'
	}
	if in_stages.sil_analysis_ms >= in_stages.ast_refresh_ms && in_stages.sil_analysis_ms >= in_stages.swift_frontend_ms {
		return 'sil-analysis-stage'
	}
	return 'ast-refresh-stage'
}

fn gather_env(root string, bench_runs int, in_bin string, hybrid_cli_bin string) BenchEnv {
	host_os := command_output(root, 'uname -s')
	host_kernel := command_output(root, 'uname -r')
	cpu := if os.user_os() == 'macos' {
		command_output(root, 'sysctl -n machdep.cpu.brand_string')
	} else {
		command_output(root, 'uname -m')
	}
	memory := if os.user_os() == 'macos' {
		command_output(root, 'sysctl -n hw.memsize')
	} else {
		command_output(root, 'free -h | awk \'/Mem:/ {print $2}\'')
	}
	return BenchEnv{
		generated_at_utc: command_output(root, 'date -u +"%Y-%m-%dT%H:%M:%SZ"')
		bench_runs: bench_runs
		host_os: host_os
		host_kernel: host_kernel
		cpu: cpu
		memory: memory
		swift_version: command_output(root, 'swift --version')
		rustc_version: command_output(root, 'rustc --version')
		cargo_version: command_output(root, 'cargo --version')
		v_version: command_output(root, 'v version')
		in_bin: in_bin
		hybrid_cli_bin: hybrid_cli_bin
	}
}

fn main() {
	root := os.getenv_opt('BENCH_ROOT') or { os.getwd() }
	aurorality_root := os.getenv_opt('AURORALITY_ROOT') or { os.join_path(root, '..', 'aurorality') }
	default_in_bin := os.join_path(root, 'in-cli', 'target', 'debug', if os.user_os() == 'windows' { 'in.exe' } else { 'in' })
	println('building local in-cli binary for benchmark...')
	build_local := run_with_status('cargo build in-cli', os.join_path(root, 'in-cli'), 'cargo build')
	if !build_local.ok {
		eprintln('failed to build local in-cli binary')
		exit(1)
	}
	in_bin := os.getenv_opt('IN_BIN') or {
		if os.exists(default_in_bin) {
			default_in_bin
		} else {
			os.join_path(os.getenv('HOME'), '.cargo', 'bin', 'in')
		}
	}
	hybrid_cli_bin := os.join_path(root, 'compiler', 'rust-driver', 'target', 'debug', if os.user_os() == 'windows' {
		'hybrid-cli.exe'
	} else {
		'hybrid-cli'
	})
	println('building hybrid-cli binary for benchmark...')
	build_hybrid := run_with_status('cargo build hybrid-cli', os.join_path(root, 'compiler', 'rust-driver'),
		'cargo build -p hybrid-cli')
	if !build_hybrid.ok {
		eprintln('failed to build hybrid-cli binary')
		exit(1)
	}
	out_dir := os.join_path(root, 'docs', 'benchmarks')
	out_md := os.join_path(out_dir, 'swift-vs-in.md')
	out_json := os.join_path(out_dir, 'swift-vs-in.json')
	bench_runs := os.getenv_opt('BENCH_RUNS') or { '3' }.int()
	os.mkdir_all(out_dir) or { panic(err) }

	examples := [
		os.join_path(aurorality_root, 'examples', 'counter', 'Sources', 'App.swift') + ':Counter',
		os.join_path(aurorality_root, 'examples', 'basic', 'Sources', 'App.swift') + ':Basic',
		os.join_path(aurorality_root, 'examples', 'hyperchat', 'Sources', 'HyperChatRootView.swift') + ':HyperChat',
	]

	mut rows := []BenchRow{}
	for idx, entry in examples {
		parts := entry.split(':')
		if parts.len < 2 {
			continue
		}
		path := parts[0]
		module_name := parts[1]
		println('[${idx + 1}/${examples.len}] benchmarking ${module_name} (${path})')
		if !os.exists(path) {
			rows << BenchRow{
				example: path
				module: module_name
			}
			continue
		}
		pkg_root := find_package_root(path)
		mut swiftc_samples := []f64{}
		mut swiftpkg_samples := []f64{}
		mut in_samples := []f64{}
		mut swiftc_last := RunResult{}
		mut swiftpkg_last := RunResult{}
		mut in_last := RunResult{}
		mut hybrid_last := RunResult{}
		mut in_stage_samples := []InStageBreakdown{}
		mut hybrid_samples := []f64{}
		mut swiftc_all_ok := true
		mut swiftpkg_all_ok := true
		mut in_all_ok := true
		mut hybrid_all_ok := true

		for run_idx in 0 .. bench_runs {
			run_name := '${run_idx + 1}/${bench_runs}'
			swiftc_last = run_with_status('swiftc ${run_name}', root, 'swiftc -typecheck "${path}"')
			swiftc_samples << swiftc_last.ms
			if !swiftc_last.ok {
				swiftc_all_ok = false
			}

			if pkg_root != '' {
				swiftpkg_last = run_with_status('swift build ${run_name}', pkg_root, 'swift build')
				swiftpkg_samples << swiftpkg_last.ms
				if !swiftpkg_last.ok {
					swiftpkg_all_ok = false
				}
			}

			in_last = run_with_status('in build ${run_name}', root, '"${in_bin}" build --path "${path}" --module-id "${module_name}"')
			in_samples << in_last.ms
			in_stage_samples << parse_in_stages(in_last.output)
			if !in_last.ok {
				in_all_ok = false
			}
			hybrid_last = run_with_status('hybrid-cli ${run_name}', os.join_path(root, 'compiler', 'rust-driver'),
				'"${hybrid_cli_bin}" --path "${path}" --module-id "${module_name}"')
			hybrid_samples << hybrid_last.ms
			if !hybrid_last.ok {
				hybrid_all_ok = false
			}
		}
		swiftc_ms := median_ms(swiftc_samples)
		swiftpkg_ms := median_ms(swiftpkg_samples)
		in_ms := median_ms(in_samples)
		hybrid_ms := median_ms(hybrid_samples)
		swiftpkg_ok := if pkg_root != '' { swiftpkg_last.ok && swiftpkg_all_ok } else { false }
		in_ok := in_last.ok && in_all_ok
		hybrid_ok := hybrid_last.ok && hybrid_all_ok
		swiftc_ok := swiftc_last.ok && swiftc_all_ok
		ratio := if swiftpkg_ms > 0 { in_ms / swiftpkg_ms } else { 0.0 }
		mut stage_total_samples := []f64{}
		mut stage_ast_samples := []f64{}
		mut stage_frontend_samples := []f64{}
		mut stage_sil_samples := []f64{}
		for sample in in_stage_samples {
			stage_total_samples << sample.total_ms
			stage_ast_samples << sample.ast_refresh_ms
			stage_frontend_samples << sample.swift_frontend_ms
			stage_sil_samples << sample.sil_analysis_ms
		}
		in_stage_total_ms := median_ms(stage_total_samples)
		in_stage_ast_ms := median_ms(stage_ast_samples)
		in_stage_frontend_ms := median_ms(stage_frontend_samples)
		in_stage_sil_ms := median_ms(stage_sil_samples)
		in_driver_overhead_ms := if in_ms > in_stage_total_ms { in_ms - in_stage_total_ms } else { 0.0 }
		in_wrapper_overhead_ms := if in_ms > hybrid_ms { in_ms - hybrid_ms } else { 0.0 }
		loss_bucket := classify_loss(
			swiftpkg_ms,
			in_ms,
			in_driver_overhead_ms,
			InStageBreakdown{
				ast_refresh_ms: in_stage_ast_ms
				swift_frontend_ms: in_stage_frontend_ms
				sil_analysis_ms: in_stage_sil_ms
				total_ms: in_stage_total_ms
			},
		)
		speed_status := if ratio > 0 && ratio <= 1.0 { 'in faster/equal vs swift build' } else { 'in slower vs swift build' }
		println('  == summary ${module_name}: swiftc=${swiftc_ms:.2f}ms swift-build=${swiftpkg_ms:.2f}ms in=${in_ms:.2f}ms ratio=${ratio:.3f} (${speed_status})')
		println('     in-stages: ast=${in_stage_ast_ms:.3f} frontend=${in_stage_frontend_ms:.3f} sil=${in_stage_sil_ms:.3f} total=${in_stage_total_ms:.3f} driver-overhead=${in_driver_overhead_ms:.3f} wrapper-overhead=${in_wrapper_overhead_ms:.3f} loss=${loss_bucket}')

		rows << BenchRow{
			example: path
			module: module_name
			swiftc_ok: swiftc_ok
			swiftc_ms: swiftc_ms
			swiftpkg_ok: swiftpkg_ok
			swiftpkg_ms: swiftpkg_ms
			in_ok: in_ok
			in_ms: in_ms
			hybrid_cli_ok: hybrid_ok
			hybrid_cli_ms: hybrid_ms
			speed_ratio_in_over_swiftpkg: ratio
			in_stage_ast_refresh_ms: in_stage_ast_ms
			in_stage_swift_frontend_ms: in_stage_frontend_ms
			in_stage_sil_analysis_ms: in_stage_sil_ms
			in_stage_total_ms: in_stage_total_ms
			in_driver_overhead_ms: in_driver_overhead_ms
			in_wrapper_overhead_ms: in_wrapper_overhead_ms
			loss_bucket: loss_bucket
			swiftc_error: short_err(swiftc_last.output)
			swiftpkg_error: short_err(swiftpkg_last.output)
			in_error: short_err(in_last.output)
		}
	}

	env := gather_env(root, bench_runs, in_bin, hybrid_cli_bin)
	doc := BenchDoc{
		env: env
		rows: rows
	}
	os.write_file(out_json, json.encode_pretty(doc)) or { panic(err) }

	mut md := '# Swift Compiler vs in Pipeline Benchmark\n\n'
	md += 'Measured with: raw `swiftc -typecheck`, package-context `swift build`, and `in build`.\n'
	md += 'Each metric uses median of `${bench_runs}` runs.\n\n'
	md += '## Benchmark Environment\n\n'
	md += '- Generated (UTC): `${env.generated_at_utc}`\n'
	md += '- Host OS: `${env.host_os}`\n'
	md += '- Kernel: `${env.host_kernel}`\n'
	md += '- CPU: `${env.cpu}`\n'
	md += '- Memory: `${env.memory}`\n'
	md += '- Swift: `${env.swift_version}`\n'
	md += '- Rustc: `${env.rustc_version}`\n'
	md += '- Cargo: `${env.cargo_version}`\n'
	md += '- V: `${env.v_version}`\n'
	md += '- BENCH_RUNS: `${env.bench_runs}`\n'
	md += '- in binary: `${env.in_bin}`\n'
	md += '- hybrid-cli binary: `${env.hybrid_cli_bin}`\n\n'
	md += '| Example | swiftc(ms) | swift build(ms) | in(ms) | hybrid-cli(ms) | in/swift-build | in-stage-total(ms) | in-driver-overhead(ms) | in-wrapper-overhead(ms) | loss bucket | swift build ok | in ok |\n'
	md += '|---|---:|---:|---:|---:|---:|---:|---:|---:|---|:---:|:---:|\n'
	for row in rows {
		md += '| `${row.example}` | ${row.swiftc_ms:.2f} | ${row.swiftpkg_ms:.2f} | ${row.in_ms:.2f} | ${row.hybrid_cli_ms:.2f} | ${row.speed_ratio_in_over_swiftpkg:.3f} | ${row.in_stage_total_ms:.3f} | ${row.in_driver_overhead_ms:.3f} | ${row.in_wrapper_overhead_ms:.3f} | ${row.loss_bucket} | ${if row.swiftpkg_ok { '✅' } else { '❌' }} | ${if row.in_ok { '✅' } else { '❌' }} |\n'
	}
	os.write_file(out_md, md) or { panic(err) }

	println('wrote ${out_md}')
	println('wrote ${out_json}')
}
