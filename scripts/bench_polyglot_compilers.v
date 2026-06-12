module main

import json
import os
import time

struct RunResult {
	ok     bool
	ms     f64
	output string
}

struct BenchEnv {
	generated_at_utc string
	bench_runs       int
	warmup_runs      int
	host_os          string
	host_kernel      string
	cpu              string
	memory           string
	in_bin           string
}

struct CompilerCase {
	language   string
	example    string
	module     string
	compiler   string
	mode       string
	native_cmd string
	in_env     string
}

struct BenchRow {
	language                   string
	example                    string
	module                     string
	compiler                   string
	mode                       string
	compiler_available         bool
	native_ok                  bool
	native_ms                  f64
	native_ms_min              f64
	native_ms_max              f64
	in_ok                      bool
	in_ms                      f64
	in_ms_min                  f64
	in_ms_max                  f64
	speed_ratio_in_over_native f64
	native_error               string
	in_error                   string
}

struct BenchDoc {
	env           BenchEnv
	rows          []BenchRow
	easy_markdown string
}

fn run_timed(cwd string, cmd string) RunResult {
	start := time.now()
	wrapped := 'cd "${cwd}" && ${cmd}'
	res := os.execute(wrapped)
	elapsed_ms := f64(time.since(start).microseconds()) / 1000.0
	return RunResult{
		ok:     res.exit_code == 0
		ms:     elapsed_ms
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

fn command_available(cwd string, command string) bool {
	res := run_timed(cwd, 'command -v "${command}" >/dev/null 2>&1')
	return res.ok
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

fn min_max_ms(values []f64) (f64, f64) {
	if values.len == 0 {
		return 0.0, 0.0
	}
	mut lo := values[0]
	mut hi := values[0]
	for v in values {
		if v < lo {
			lo = v
		}
		if v > hi {
			hi = v
		}
	}
	return lo, hi
}

fn short_err(s string) string {
	if s.len <= 500 {
		return s
	}
	return s[..500]
}

fn gather_env(root string, bench_runs int, warmup_runs int, in_bin string) BenchEnv {
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
		command_output(root, "free -h | awk '/Mem:/ {print $2}'")
	}
	return BenchEnv{
		generated_at_utc: command_output(root, 'date -u +"%Y-%m-%dT%H:%M:%SZ"')
		bench_runs:       bench_runs
		warmup_runs:      warmup_runs
		host_os:          host_os
		host_kernel:      host_kernel
		cpu:              cpu
		memory:           memory
		in_bin:           in_bin
	}
}

fn in_compile_cmd(in_bin string, example string, module string, out_path string, in_env string) string {
	env_prefix := if in_env == '' { '' } else { '${in_env} ' }
	return '${env_prefix}"${in_bin}" compile --path "${example}" --target native --entry answer --out "${out_path}" --json >/dev/null'
}

fn main() {
	root := os.getenv_opt('BENCH_ROOT') or { os.getwd() }
	default_in_bin := os.join_path(root, 'in-cli', 'target', 'debug', if os.user_os() == 'windows' {
		'in.exe'
	} else {
		'in'
	})
	println('building local in-cli binary for benchmark...')
	build_local := run_with_status('cargo build in-cli', os.join_path(root, 'in-cli'),
		'cargo build')
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
	out_dir := os.join_path(root, 'docs', 'benchmarks')
	out_md := os.join_path(out_dir, 'polyglot-compilers.md')
	out_json := os.join_path(out_dir, 'polyglot-compilers.json')
	bench_runs := os.getenv_opt('BENCH_RUNS') or { '3' }.int()
	warmup_runs := os.getenv_opt('BENCH_WARMUP_RUNS') or { '1' }.int()
	target_dir := os.join_path(root, 'target', 'bench-polyglot-compilers')
	os.mkdir_all(out_dir) or { panic(err) }
	os.mkdir_all(target_dir) or { panic(err) }

	cases := [
		CompilerCase{
			language:   'C'
			example:    'apps/polyglot-sample/sample.c'
			module:     'SampleC'
			compiler:   'clang'
			mode:       'object'
			native_cmd: 'clang -c "__PATH__" -o "__NATIVE_OUT__"'
		},
		CompilerCase{
			language:   'C++'
			example:    'apps/polyglot-sample/sample.cpp'
			module:     'SampleCpp'
			compiler:   'clang++'
			mode:       'object'
			native_cmd: 'clang++ -c "__PATH__" -o "__NATIVE_OUT__"'
		},
		CompilerCase{
			language:   'Rust'
			example:    'apps/polyglot-sample/sample.rs'
			module:     'SampleRust'
			compiler:   'rustc'
			mode:       'object'
			native_cmd: 'rustc --emit=obj "__PATH__" -o "__NATIVE_OUT__"'
		},
		CompilerCase{
			language:   'Go'
			example:    'apps/polyglot-sample/sample.go'
			module:     'SampleGo'
			compiler:   'go'
			mode:       'object'
			native_cmd: 'go tool compile -o "__NATIVE_OUT__" "__PATH__"'
		},
		CompilerCase{
			language:   'Swift'
			example:    'apps/polyglot-sample/sample.swift'
			module:     'SampleSwift'
			compiler:   'swiftc'
			mode:       'object'
			native_cmd: 'swiftc -parse-as-library -c "__PATH__" -o "__NATIVE_OUT__"'
			in_env:     'IN_NATIVE_SWIFT_SIL=only'
		},
		CompilerCase{
			language:   'V'
			example:    'apps/polyglot-sample/sample.v'
			module:     'SampleV'
			compiler:   'v'
			mode:       'syntax'
			native_cmd: 'v -check-syntax "__PATH__"'
		},
		CompilerCase{
			language:   'JavaScript'
			example:    'apps/polyglot-sample/sample.js'
			module:     'SampleJavaScript'
			compiler:   'node'
			mode:       'syntax'
			native_cmd: 'node --check "__PATH__"'
		},
		CompilerCase{
			language:   'TypeScript'
			example:    'apps/polyglot-sample/sample.ts'
			module:     'SampleTypeScript'
			compiler:   'bun'
			mode:       'typecheck'
			native_cmd: 'bunx --bun tsc --noEmit --allowJs false "__PATH__"'
		},
		CompilerCase{
			language:   'Python'
			example:    'apps/polyglot-sample/sample.py'
			module:     'SamplePython'
			compiler:   'python3'
			mode:       'bytecode'
			native_cmd: 'python3 -c "import py_compile; py_compile.compile(\'__PATH__\', cfile=\'__NATIVE_OUT__\', doraise=True)"'
		},
		CompilerCase{
			language:   'Ruby'
			example:    'apps/polyglot-sample/sample.rb'
			module:     'SampleRuby'
			compiler:   'ruby'
			mode:       'syntax'
			native_cmd: 'ruby -c "__PATH__"'
		},
		CompilerCase{
			language:   'Zig'
			example:    'apps/polyglot-sample/sample.zig'
			module:     'SampleZig'
			compiler:   'zig'
			mode:       'syntax'
			native_cmd: 'zig ast-check "__PATH__"'
		},
		CompilerCase{
			language:   'PHP'
			example:    'apps/polyglot-sample/sample.php'
			module:     'SamplePhp'
			compiler:   'php'
			mode:       'syntax'
			native_cmd: 'php -l "__PATH__"'
		},
		CompilerCase{
			language:   'Java'
			example:    'apps/polyglot-sample/Sample.java'
			module:     'SampleJava'
			compiler:   'javac'
			mode:       'bytecode'
			native_cmd: 'javac -d "__NATIVE_OUT_DIR__" "__PATH__"'
		},
		CompilerCase{
			language:   'Nim'
			example:    'apps/polyglot-sample/sample.nim'
			module:     'SampleNim'
			compiler:   'nim'
			mode:       'typecheck'
			native_cmd: 'nim check --hints:off "__PATH__"'
		},
		CompilerCase{
			language:   'D'
			example:    'apps/polyglot-sample/sample.d'
			module:     'SampleD'
			compiler:   'ldc2'
			mode:       'object'
			native_cmd: 'ldc2 -o- -c "__PATH__"'
		},
	]

	mut rows := []BenchRow{}
	for idx, case in cases {
		path := os.join_path(root, case.example)
		println('[${idx + 1}/${cases.len}] benchmarking ${case.language} (${case.example})')
		compiler_available := command_available(root, case.compiler)
		if !compiler_available {
			println('  -- skipped ${case.compiler}: not found')
			rows << BenchRow{
				language: case.language
				example:  case.example
				module:   case.module
				compiler: case.compiler
				mode:     case.mode
			}
			continue
		}
		if !os.exists(path) {
			rows << BenchRow{
				language:           case.language
				example:            case.example
				module:             case.module
				compiler:           case.compiler
				mode:               case.mode
				compiler_available: true
				native_error:       'sample file missing'
				in_error:           'sample file missing'
			}
			continue
		}
		case_dir := os.join_path(target_dir, case.module)
		native_out_dir := os.join_path(case_dir, 'native')
		in_out := os.join_path(case_dir, 'in.out')
		native_out := os.join_path(native_out_dir, 'native.out')
		os.mkdir_all(native_out_dir) or { panic(err) }
		native_cmd := case.native_cmd.replace('__PATH__', path).replace('__NATIVE_OUT__',
			native_out).replace('__NATIVE_OUT_DIR__', native_out_dir)
		in_cmd := in_compile_cmd(in_bin, path, case.module, in_out, case.in_env)

		for warm_idx in 0 .. warmup_runs {
			warm_name := '${warm_idx + 1}/${warmup_runs}'
			_ = run_with_status('warm native ${warm_name}', root, native_cmd)
			_ = run_with_status('warm in compile ${warm_name}', root, in_cmd)
		}

		mut native_samples := []f64{}
		mut in_samples := []f64{}
		mut native_last := RunResult{}
		mut in_last := RunResult{}
		mut native_all_ok := true
		mut in_all_ok := true
		for run_idx in 0 .. bench_runs {
			run_name := '${run_idx + 1}/${bench_runs}'
			native_last = run_with_status('native ${run_name}', root, native_cmd)
			native_samples << native_last.ms
			if !native_last.ok {
				native_all_ok = false
			}
			in_last = run_with_status('in compile ${run_name}', root, in_cmd)
			in_samples << in_last.ms
			if !in_last.ok {
				in_all_ok = false
			}
		}
		native_ms := median_ms(native_samples)
		in_ms := median_ms(in_samples)
		native_lo, native_hi := min_max_ms(native_samples)
		in_lo, in_hi := min_max_ms(in_samples)
		ratio := if native_ms > 0 { in_ms / native_ms } else { 0.0 }
		rows << BenchRow{
			language:                   case.language
			example:                    case.example
			module:                     case.module
			compiler:                   case.compiler
			mode:                       case.mode
			compiler_available:         true
			native_ok:                  native_last.ok && native_all_ok
			native_ms:                  native_ms
			native_ms_min:              native_lo
			native_ms_max:              native_hi
			in_ok:                      in_last.ok && in_all_ok
			in_ms:                      in_ms
			in_ms_min:                  in_lo
			in_ms_max:                  in_hi
			speed_ratio_in_over_native: ratio
			native_error:               short_err(native_last.output)
			in_error:                   short_err(in_last.output)
		}
		println('  == summary ${case.language}: native=${native_ms:.2f}ms in=${in_ms:.2f}ms ratio=${ratio:.3f}')
	}

	env := gather_env(root, bench_runs, warmup_runs, in_bin)
	mut easy_md := '| Language | Native compiler | Native mode | Native median (min-max ms) | in median (min-max ms) | in/native | Status |\n'
	easy_md += '|---|---|---|---:|---:|---:|---|\n'
	for row in rows {
		status := if !row.compiler_available {
			'skipped'
		} else if row.native_ok && row.in_ok {
			'ok'
		} else if !row.native_ok {
			'native failed'
		} else {
			'in failed'
		}
		easy_md += '| ${row.language} | `${row.compiler}` | ${row.mode} | ${row.native_ms:.2f} (${row.native_ms_min:.2f}-${row.native_ms_max:.2f}) | ${row.in_ms:.2f} (${row.in_ms_min:.2f}-${row.in_ms_max:.2f}) | ${row.speed_ratio_in_over_native:.3f} | ${status} |\n'
	}

	doc := BenchDoc{
		env:           env
		rows:          rows
		easy_markdown: easy_md
	}
	os.write_file(out_json, json.encode_pretty(doc)) or { panic(err) }

	mut md := '# Polyglot Compiler Matrix Benchmark\n\n'
	md += 'Measured against installed native compiler checks and `in compile --target native --entry answer --json` for the same polyglot sample files. The native mode column distinguishes object compilation, bytecode compilation, typechecking, and syntax-only checks; ratios are only directly comparable within the same mode. Missing native compilers are skipped; installed compiler failures are recorded and fail the script.\n'
	md += 'Wall times: median over `${bench_runs}` timed runs; min-max across those runs shown in parentheses.\n\n'
	md += '## Benchmark Environment\n\n'
	md += '- Generated (UTC): `${env.generated_at_utc}`\n'
	md += '- Host OS: `${env.host_os}`\n'
	md += '- Kernel: `${env.host_kernel}`\n'
	md += '- CPU: `${env.cpu}`\n'
	md += '- Memory: `${env.memory}`\n'
	md += '- BENCH_RUNS: `${env.bench_runs}`\n'
	md += '- BENCH_WARMUP_RUNS: `${env.warmup_runs}`\n'
	md += '- in binary: `${env.in_bin}`\n\n'
	md += '## Results\n\n'
	md += easy_md + '\n'
	os.write_file(out_md, md) or { panic(err) }

	println('')
	println('Polyglot compiler matrix')
	println(easy_md.trim_space())
	println('')
	println('wrote ${out_md}')
	println('wrote ${out_json}')
	mut failed := false
	for row in rows {
		if row.compiler_available && (!row.native_ok || !row.in_ok) {
			failed = true
		}
	}
	if failed {
		eprintln('benchmark failed: one or more installed compiler runs failed')
		exit(1)
	}
}
