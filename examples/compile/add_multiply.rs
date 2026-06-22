fn add(a: i64, b: i64) -> i64 { return a + b; }
fn multiply(a: i64, b: i64) -> i64 { return a * b; }
fn main() -> i64 { return multiply(add(10, 20), 2); }
