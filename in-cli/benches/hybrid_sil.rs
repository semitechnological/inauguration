use criterion::{Criterion, criterion_group, criterion_main};
use inauguration::hybrid_sil::{extract_call_graph, parse_textual_sil, remove_debug_insts};
use std::hint::black_box;

fn small_subset_sil() -> String {
    [
        "sil @helper",
        "bb0:",
        "%0 = integer_literal $Builtin.Int64, 1",
        "bb1:",
        "return %0 : $Builtin.Int64",
        "sil @main",
        "bb0:",
        "%1 = function_ref @helper : $@convention(thin)",
        "%2 = integer_literal $Builtin.Int64, 0",
        "bb1:",
        "return %2 : $Builtin.Int64",
    ]
    .join("\n")
}

fn multi_function_sil() -> String {
    let mut out = String::new();
    for i in 0..32 {
        out.push_str(&format!(
            "sil @helper_{i}\nbb0:\ndebug_value %{i}\n%{i} = function_ref @leaf_{i} : $@convention(thin)\n"
        ));
    }
    out.push_str("sil @main\nbb0:\n");
    for i in 0..32 {
        out.push_str(&format!(
            "%{} = function_ref @helper_{i} : $@convention(thin)\n",
            i + 100
        ));
    }
    out
}

fn representative_swiftc_sil_blob() -> String {
    let mut out = String::new();
    for i in 0..128 {
        out.push_str(&format!("sil hidden @$s4App{i}ViewV4bodyQrvg\nbb0:\n"));
        out.push_str(&format!("debug_value %{i}\n"));
        out.push_str(&format!(
            "%{} = function_ref @$s4App{i}ViewV6helperyyF : $@convention(method)\n",
            i + 1
        ));
        out.push_str(&format!(
            "%{} = integer_literal $Builtin.Int64, {i}\n",
            i + 2
        ));
        out.push_str("bb1:\n");
        out.push_str(&format!("return %{} : $Builtin.Int64\n", i + 2));
    }
    out
}

fn bench_hybrid_sil(c: &mut Criterion) {
    let small = small_subset_sil();
    let multi = multi_function_sil();
    let representative = representative_swiftc_sil_blob();

    for (name, sil) in [
        ("small_subset", small.as_str()),
        ("multi_function", multi.as_str()),
        ("representative_swiftc", representative.as_str()),
    ] {
        c.bench_function(&format!("parse_textual_sil/{name}"), |b| {
            b.iter(|| parse_textual_sil(black_box(sil)))
        });
        c.bench_function(&format!("remove_debug_insts/{name}"), |b| {
            let artifact = parse_textual_sil(sil);
            b.iter(|| remove_debug_insts(black_box(&artifact)))
        });
        c.bench_function(&format!("extract_call_graph/{name}"), |b| {
            let artifact = remove_debug_insts(&parse_textual_sil(sil));
            b.iter(|| extract_call_graph(black_box(&artifact)))
        });
    }
}

criterion_group!(benches, bench_hybrid_sil);
criterion_main!(benches);
