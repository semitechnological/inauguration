use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn crepus_render(bundle_json: &str) -> Result<String, JsValue> {
    crepuscularity_web::render_bundle(bundle_json).map_err(|e| JsValue::from_str(&e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    // Removed `wasm_bindgen_test_configure!(run_in_browser);` to run tests headlessly via Node

    #[wasm_bindgen_test]
    fn test_crepus_render_empty() {
        let result = crepus_render("");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().as_string().unwrap_or_default(), "render error: bundle JSON: EOF while parsing a value at line 1 column 0");
    }

    #[wasm_bindgen_test]
    fn test_crepus_render_invalid_json() {
        let result = crepus_render("{ invalid_json }");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().as_string().unwrap_or_default();
        assert!(err_msg.starts_with("render error"));
    }

    #[wasm_bindgen_test]
    fn test_crepus_render_valid() {
        let valid_bundle = r#"{"name": "test-bundle", "entry": "index.crp", "files": {"index.crp": "<div class=\"test\">hello world</div>"}}"#;
        let result = crepus_render(valid_bundle);
        assert!(result.is_ok());
        let html = result.unwrap();
        assert!(html.contains("<div class=\"test\">hello world</div>"));
    }
}
