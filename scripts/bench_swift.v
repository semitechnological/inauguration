module main

import os
import time
import json

struct RunResult {
	ok bool
	ms f64
	stderr string
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
	speed_ratio_in_over_swiftpkg f64
	swiftc_error string
	swiftpkg_error string
	in_error string
}

struct BenchDoc {
	rows []BenchRow
}

fn run_timed(cwd string, cmd string) RunResult {
	start := time.now()
	wrapped := 'cd "${cwd}" && ${cmd}'
	res := os.execute(wrapped)
	elapsed_ms := f64(time.since(start).microseconds()) / 1000.0
	return RunResult{
		ok: res.exit_code == 0
		ms: elapsed_ms
		stderr: res.output.trim_space()
	}
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

fn main() {
	root := os.getenv_opt('BENCH_ROOT') or { os.getwd() }
	aurorality_root := os.getenv_opt('AURORALITY_ROOT') or { os.join_path(root, '..', 'aurorality') }
	in_bin := os.getenv_opt('IN_BIN') or { os.join_path(os.getenv('HOME'), '.cargo', 'bin', 'in') }
	out_dir := os.join_path(root, 'docs', 'benchmarks')
	out_md := os.join_path(out_dir, 'swift-vs-in.md')
	out_json := os.join_path(out_dir, 'swift-vs-in.json')
	os.mkdir_all(out_dir) or { panic(err) }

	examples := [
		os.join_path(aurorality_root, 'examples', 'counter', 'Sources', 'App.swift') + ':Counter',
		os.join_path(aurorality_root, 'examples', 'basic', 'Sources', 'App.swift') + ':Basic',
		os.join_path(aurorality_root, 'examples', 'hyperchat', 'Sources', 'HyperChatRootView.swift') + ':HyperChat',
	]

	mut rows := []BenchRow{}
	for entry in examples {
		parts := entry.split(':')
		if parts.len < 2 {
			continue
		}
		path := parts[0]
		module_name := parts[1]
		if !os.exists(path) {
			rows << BenchRow{
				example: path
				module: module_name
			}
			continue
		}

		swiftc := run_timed(root, 'swiftc -typecheck "${path}"')
		pkg_root := find_package_root(path)
		swiftpkg := if pkg_root != '' { run_timed(pkg_root, 'swift build') } else { RunResult{} }
		inrun := run_timed(root, '"${in_bin}" build --path "${path}" --module-id "${module_name}"')
		ratio := if swiftpkg.ms > 0 { inrun.ms / swiftpkg.ms } else { 0.0 }

		rows << BenchRow{
			example: path
			module: module_name
			swiftc_ok: swiftc.ok
			swiftc_ms: swiftc.ms
			swiftpkg_ok: swiftpkg.ok
			swiftpkg_ms: swiftpkg.ms
			in_ok: inrun.ok
			in_ms: inrun.ms
			speed_ratio_in_over_swiftpkg: ratio
			swiftc_error: short_err(swiftc.stderr)
			swiftpkg_error: short_err(swiftpkg.stderr)
			in_error: short_err(inrun.stderr)
		}
	}

	doc := BenchDoc{rows: rows}
	os.write_file(out_json, json.encode_pretty(doc)) or { panic(err) }

	mut md := '# Swift Compiler vs in Pipeline Benchmark\n\n'
	md += 'Measured with: raw `swiftc -typecheck`, package-context `swift build`, and `in build`.\n\n'
	md += '| Example | swiftc(ms) | swift build(ms) | in(ms) | in/swift-build | swift build ok | in ok |\n'
	md += '|---|---:|---:|---:|---:|:---:|:---:|\n'
	for row in rows {
		md += '| `${row.example}` | ${row.swiftc_ms:.2f} | ${row.swiftpkg_ms:.2f} | ${row.in_ms:.2f} | ${row.speed_ratio_in_over_swiftpkg:.3f} | ${if row.swiftpkg_ok { '✅' } else { '❌' }} | ${if row.in_ok { '✅' } else { '❌' }} |\n'
	}
	os.write_file(out_md, md) or { panic(err) }

	println('wrote ${out_md}')
	println('wrote ${out_json}')
}
