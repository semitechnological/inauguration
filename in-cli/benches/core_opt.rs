use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use inauguration::core_ir::*;
use inauguration::core_opt::optimize;

fn generate_test_decls() -> Vec<Decl> {
    let mut decls = Vec::new();

    // Add some small functions to inline
    for i in 0..10 {
        decls.push(Decl::Function {
            name: format!("small_func_{}", i),
            params: vec![("x".to_string(), Typ::Int), ("y".to_string(), Typ::Int)],
            ret: Typ::Int,
            body: vec![Stmt::Return(Some(Expr::Binary {
                op: "+".to_string(),
                lhs: Box::new(Expr::Ident("x".to_string())),
                rhs: Box::new(Expr::Ident("y".to_string())),
            }))],
            type_params: vec![],
        });
    }

    // Add a large function that calls the small functions repeatedly
    let mut stmts = Vec::new();
    for i in 0..100 {
        stmts.push(Stmt::Let(
            format!("var_{}", i),
            Some(Typ::Int),
            Expr::Call {
                callee: Box::new(Expr::Ident(format!("small_func_{}", i % 10))),
                args: vec![Expr::IntLit(i as i64), Expr::IntLit((i + 1) as i64)],
            },
        ));
    }

    decls.push(Decl::Function {
        name: "main_func".to_string(),
        params: vec![],
        ret: Typ::Void,
        body: stmts,
        type_params: vec![],
    });

    decls
}

fn bench_core_opt(c: &mut Criterion) {
    let decls = generate_test_decls();

    c.bench_function("core_opt_optimize", |b| {
        b.iter(|| {
            let mut d = decls.clone();
            optimize(black_box(&mut d));
        })
    });
}

criterion_group!(benches, bench_core_opt);
criterion_main!(benches);
