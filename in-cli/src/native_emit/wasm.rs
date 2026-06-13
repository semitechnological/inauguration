pub const WASM32_UNKNOWN_TRIPLE: &str = "wasm32-unknown-unknown";

pub struct WasmModule {
    pub export_name: String,
    pub value: u8,
}

pub fn write_scalar_i32_module(module: &WasmModule, out: &mut Vec<u8>) {
    out.clear();
    out.extend_from_slice(b"\0asm");
    out.extend_from_slice(&1u32.to_le_bytes());
    write_section(out, 1, &[0x01, 0x60, 0x00, 0x01, 0x7F]);
    write_section(out, 3, &[0x01, 0x00]);
    let mut export = Vec::new();
    write_name(&mut export, &module.export_name);
    export.push(0x00);
    export.push(0x00);
    let mut exports = vec![0x01];
    exports.extend_from_slice(&export);
    write_section(out, 7, &exports);
    let body = [0x00, 0x41, module.value, 0x0B];
    let mut code = vec![0x01];
    write_leb_u32(&mut code, body.len() as u32);
    code.extend_from_slice(&body);
    write_section(out, 10, &code);
}

fn write_section(out: &mut Vec<u8>, id: u8, payload: &[u8]) {
    out.push(id);
    write_leb_u32(out, payload.len() as u32);
    out.extend_from_slice(payload);
}

fn write_name(out: &mut Vec<u8>, name: &str) {
    write_leb_u32(out, name.len() as u32);
    out.extend_from_slice(name.as_bytes());
}

fn write_leb_u32(out: &mut Vec<u8>, mut value: u32) {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_wasm_module_with_export() {
        let module = WasmModule {
            export_name: "answer".to_string(),
            value: 42,
        };
        let mut out = Vec::new();
        write_scalar_i32_module(&module, &mut out);
        assert_eq!(&out[0..4], b"\0asm");
        assert_eq!(&out[4..8], &1u32.to_le_bytes());
        assert!(out.windows(6).any(|window| window == b"answer"));
        assert!(out.windows(3).any(|window| window == [0x41, 42, 0x0B]));
    }
}
