#![allow(clippy::needless_borrows_for_generic_args)]

fn main() {
    let src_dir = std::path::Path::new("src");
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let lib_name = "tree-sitter-v";
    let lib_path = out_dir.join(format!("lib{lib_name}.a"));
    let parser_path = src_dir.join("parser.c");
    let obj_path = out_dir.join("parser.o");

    println!("cargo:rerun-if-changed={}", parser_path.to_str().unwrap());

    let compiler = cc::Build::new()
        .include(&src_dir)
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-unused-but-set-variable")
        .flag_if_supported("-Wno-trigraphs")
        .get_compiler();
    let mut cmd = compiler.to_command();
    cmd.arg("-c").arg(&parser_path).arg("-o").arg(&obj_path);
    let status = cmd.status().expect("cc compile failed");
    assert!(status.success(), "C compiler did not exit successfully");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS");
    if target_os.is_ok_and(|v| v == "macos") {
        let status = std::process::Command::new("libtool")
            .args(&[
                "-static",
                "-o",
                &lib_path.to_string_lossy(),
                &obj_path.to_string_lossy(),
            ])
            .status()
            .expect("libtool failed");
        assert!(status.success(), "libtool failed");
    } else {
        let status = std::process::Command::new("ar")
            .args(&[
                "crs",
                &lib_path.to_string_lossy(),
                &obj_path.to_string_lossy(),
            ])
            .status()
            .expect("ar failed");
        assert!(status.success(), "ar failed");
    }

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static={}", lib_name);
}
