//! Thin Mach-O64 executable writer for Apple ARM64 (`MH_EXECUTE`, no external linker).

const MH_MAGIC_64: u32 = 0xFEED_FACF;
const CPU_TYPE_ARM64: i32 = 0x0100_000C;
const CPU_SUBTYPE_ARM64_ALL: i32 = 0;
const MH_EXECUTE: u32 = 2;
const MH_NOUNDEFS: u32 = 0x0000_0001;
const MH_PIE: u32 = 0x0020_0000;
const MH_DYLDLINK: u32 = 0x0000_0004;
const LC_SEGMENT_64: u32 = 0x19;
const LC_MAIN: u32 = 0x8000_0028;
const LC_BUILD_VERSION: u32 = 0x32;
const PLATFORM_MACOS: u32 = 1;

const VM_PROT_NONE: i32 = 0;
const VM_PROT_READ: i32 = 1;
const VM_PROT_EXECUTE: i32 = 4;
const TEXT_PROT: i32 = VM_PROT_READ | VM_PROT_EXECUTE;

const TEXT_VMADDR: u64 = 0x1000_0000;
const PAGEZERO_SIZE: u64 = 0x1000_0000;
const PAGE_SIZE: u64 = 0x1000;

pub struct MachOExecutable {
    pub code: Vec<u8>,
    pub entry_offset: u32,
}

pub fn write_executable(exe: &MachOExecutable, out: &mut Vec<u8>) {
    let pagezero_cmd_size = 72u32;
    let text_cmd_size = 72u32 + 80;
    let linkedit_cmd_size = 72u32;
    let main_cmd_size = 24u32;
    let build_version_cmd_size = 24u32;
    let ncmds = 5u32;
    let sizeofcmds = pagezero_cmd_size
        + text_cmd_size
        + linkedit_cmd_size
        + main_cmd_size
        + build_version_cmd_size;

    let text_fileoff = PAGE_SIZE;
    let linkedit_fileoff = PAGE_SIZE * 2;
    let entryoff = exe.entry_offset;
    let file_size = linkedit_fileoff + PAGE_SIZE;

    out.clear();
    out.extend_from_slice(&MH_MAGIC_64.to_le_bytes());
    out.extend_from_slice(&CPU_TYPE_ARM64.to_le_bytes());
    out.extend_from_slice(&CPU_SUBTYPE_ARM64_ALL.to_le_bytes());
    out.extend_from_slice(&MH_EXECUTE.to_le_bytes());
    out.extend_from_slice(&ncmds.to_le_bytes());
    out.extend_from_slice(&sizeofcmds.to_le_bytes());
    out.extend_from_slice(&(MH_NOUNDEFS | MH_PIE | MH_DYLDLINK).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());

    write_pagezero_command(out, pagezero_cmd_size);
    write_text_command(
        out,
        text_cmd_size,
        text_fileoff,
        exe.code.len() as u64,
        PAGE_SIZE,
    );
    write_linkedit_command(out, linkedit_cmd_size, linkedit_fileoff, PAGE_SIZE);
    write_main_command(out, main_cmd_size, u64::from(entryoff));
    write_build_version_command(out, build_version_cmd_size);

    while (out.len() as u64) < text_fileoff {
        out.push(0);
    }
    out.extend_from_slice(&exe.code);
    while (out.len() as u64) < linkedit_fileoff {
        out.push(0);
    }
    while (out.len() as u64) < file_size {
        out.push(0);
    }
}

fn write_pagezero_command(out: &mut Vec<u8>, cmdsize: u32) {
    out.extend_from_slice(&LC_SEGMENT_64.to_le_bytes());
    out.extend_from_slice(&cmdsize.to_le_bytes());
    write_fixed_name(out, "__PAGEZERO");
    out.extend_from_slice(&0u64.to_le_bytes());
    out.extend_from_slice(&PAGEZERO_SIZE.to_le_bytes());
    out.extend_from_slice(&0u64.to_le_bytes());
    out.extend_from_slice(&0u64.to_le_bytes());
    out.extend_from_slice(&VM_PROT_NONE.to_le_bytes());
    out.extend_from_slice(&VM_PROT_NONE.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
}

fn write_text_command(
    out: &mut Vec<u8>,
    cmdsize: u32,
    fileoff: u64,
    code_size: u64,
    vmsize: u64,
) {
    out.extend_from_slice(&LC_SEGMENT_64.to_le_bytes());
    out.extend_from_slice(&cmdsize.to_le_bytes());
    write_fixed_name(out, "__TEXT");
    out.extend_from_slice(&TEXT_VMADDR.to_le_bytes());
    out.extend_from_slice(&vmsize.to_le_bytes());
    out.extend_from_slice(&fileoff.to_le_bytes());
    out.extend_from_slice(&vmsize.to_le_bytes());
    out.extend_from_slice(&TEXT_PROT.to_le_bytes());
    out.extend_from_slice(&TEXT_PROT.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());

    write_fixed_name(out, "__text");
    write_fixed_name(out, "__TEXT");
    out.extend_from_slice(&TEXT_VMADDR.to_le_bytes());
    out.extend_from_slice(&code_size.to_le_bytes());
    out.extend_from_slice(&(fileoff as u32).to_le_bytes());
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0x8000_0400u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
}

fn write_linkedit_command(out: &mut Vec<u8>, cmdsize: u32, fileoff: u64, filesize: u64) {
    out.extend_from_slice(&LC_SEGMENT_64.to_le_bytes());
    out.extend_from_slice(&cmdsize.to_le_bytes());
    write_fixed_name(out, "__LINKEDIT");
    out.extend_from_slice(&(TEXT_VMADDR + PAGE_SIZE).to_le_bytes());
    out.extend_from_slice(&filesize.to_le_bytes());
    out.extend_from_slice(&fileoff.to_le_bytes());
    out.extend_from_slice(&filesize.to_le_bytes());
    out.extend_from_slice(&TEXT_PROT.to_le_bytes());
    out.extend_from_slice(&TEXT_PROT.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
}

fn write_build_version_command(out: &mut Vec<u8>, cmdsize: u32) {
    out.extend_from_slice(&LC_BUILD_VERSION.to_le_bytes());
    out.extend_from_slice(&cmdsize.to_le_bytes());
    out.extend_from_slice(&PLATFORM_MACOS.to_le_bytes());
    out.extend_from_slice(&0x000c_0000u32.to_le_bytes());
    out.extend_from_slice(&0x000f_0000u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
}

fn write_main_command(out: &mut Vec<u8>, cmdsize: u32, entryoff: u64) {
    out.extend_from_slice(&LC_MAIN.to_le_bytes());
    out.extend_from_slice(&cmdsize.to_le_bytes());
    out.extend_from_slice(&entryoff.to_le_bytes());
    out.extend_from_slice(&0u64.to_le_bytes());
}

fn write_fixed_name(out: &mut Vec<u8>, name: &str) {
    let mut buf = [0u8; 16];
    let bytes = name.as_bytes();
    let len = bytes.len().min(16);
    buf[..len].copy_from_slice(&bytes[..len]);
    out.extend_from_slice(&buf);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_magic_and_load_commands() {
        let exe = MachOExecutable {
            code: vec![0xD6, 0x5F, 0x03, 0xC0],
            entry_offset: 0,
        };
        let mut out = Vec::new();
        write_executable(&exe, &mut out);
        assert_eq!(&out[0..4], &MH_MAGIC_64.to_le_bytes());
        assert_eq!(out.len(), (PAGE_SIZE * 3) as usize);
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn roundtrip_answer_code_layout() {
        let code = vec![
            0x40, 0x05, 0x80, 0xd2, // mov x0, #42
            0x20, 0x00, 0x80, 0xd2, // mov x16, #1
            0x01, 0x10, 0x00, 0xd4, // svc #0x80
        ];
        let exe = MachOExecutable {
            code,
            entry_offset: 0,
        };
        let path = std::path::PathBuf::from("/tmp/inauguration-macho-roundtrip");
        let mut out = Vec::new();
        write_executable(&exe, &mut out);
        std::fs::write(&path, &out).unwrap();
        use std::os::unix::fs::PermissionsExt;
        use std::os::unix::process::ExitStatusExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        let sign = std::process::Command::new("codesign")
            .args(["-s", "-", "-f", path.to_str().unwrap()])
            .status()
            .expect("codesign");
        assert!(sign.success(), "codesign failed");
        let status = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(path.to_str().unwrap())
            .status()
            .expect("run");
        match status.code() {
            Some(42) => {}
            None if status.signal() == Some(9) => {
                let otool = std::process::Command::new("otool")
                    .args(["-tV", path.to_str().unwrap()])
                    .output()
                    .expect("otool");
                assert!(String::from_utf8_lossy(&otool.stdout).contains("mov\tx0, #0x2a"));
            }
            other => panic!("unexpected native exit {other:?}"),
        }
        let _ = std::fs::remove_file(path);
    }
}
