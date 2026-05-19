#[test]
fn test_parse_in() {
    let source = r#"
func main() {
  return 42
}
"#;
    
    println!("[test] Parsing...");
    match inauguration::in_lang_parse::parse_in_source(&source) {
        Ok(module) => {
            println!("[test] Parse success");
        }
        Err(e) => {
            println!("[test] Parse error: {}", e);
        }
    }
    println!("[test] Done");
}
