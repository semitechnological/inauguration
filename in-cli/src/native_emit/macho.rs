const MH_MAGIC_64: u32 = 0xFEED_FACF;
const CPU_TYPE_ARM64: i32 = 0x0100_000C;
const CPU_SUBTYPE_ARM64_ALL: i32 = 0;
const MH_OBJECT: u32 = 1;
const MH_EXECUTE: u32 = 2;
const MH_DYLIB: u32 = 6;
const MH_NOUNDEFS: u32 = 0x0000_0001;
const MH_PIE: u32 = 0x0020_0000;
const MH_DYLDLINK: u32 = 0x0000_0004;
const LC_SEGMENT_64: u32 = 0x19;
const LC_SYMTAB: u32 = 0x2;
const LC_DYSYMTAB: u32 = 0xb;
const LC_ID_DYLIB: u32 = 0xd;
const LC_MAIN: u32 = 0x8000_0028;
const LC_BUILD_VERSION: u32 = 0x32;
const PLATFORM_MACOS: u32 = 1;
const N_SECT: u8 = 0x0e;
const N_EXT: u8 = 0x01;

const VM_PROT_NONE: i32 = 0;
const VM_PROT_READ: i32 = 1;
const VM_PROT_EXECUTE: i32 = 4;
const TEXT_PROT: i32 = VM_PROT_READ | VM_PROT_EXECUTE;

const TEXT_VMADDR: u64 = 0x1000_0000;
const PAGEZERO_SIZE: u64 = 0x1000_0000;
const PAGE_SIZE: u64 = 0x1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MachOLinkage {
    Executable,
    Dylib,
    StaticLib,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportSymbol {
    pub name: String,
    pub offset: u32,
}

#[derive(Debug, Clone)]
pub struct MachOImage {
    pub code: Vec<u8>,
    pub entry_offset: Option<u32>,
    pub exports: Vec<ExportSymbol>,
}

#[cfg(test)]
pub struct MachOExecutable {
    pub code: Vec<u8>,
    pub entry_offset: u32,
}

#[cfg(test)]
pub fn write_executable(exe: &MachOExecutable, out: &mut Vec<u8>) {
    let image = MachOImage {
        code: exe.code.clone(),
        entry_offset: Some(exe.entry_offset),
        exports: Vec::new(),
    };
    write_image(&image, MachOLinkage::Executable, "", out);
}

pub fn write_image(
    image: &MachOImage,
    linkage: MachOLinkage,
    install_name: &str,
    out: &mut Vec<u8>,
) {
    match linkage {
        MachOLinkage::StaticLib => write_static_archive(image, install_name, out),
        MachOLinkage::Executable | MachOLinkage::Dylib => {
            write_macho_image(image, linkage, install_name, out)
        }
    }
}

fn write_static_archive(image: &MachOImage, member_name: &str, out: &mut Vec<u8>) {
    let mut object = Vec::new();
    write_macho_image(image, MachOLinkage::StaticLib, "", &mut object);
    let name = if member_name.is_empty() {
        "object.o".to_string()
    } else if member_name.ends_with(".o") {
        member_name.to_string()
    } else {
        format!("{member_name}.o")
    };
    write_ar_member(out, &name, &object);
}

fn write_ar_member(out: &mut Vec<u8>, name: &str, payload: &[u8]) {
    if out.is_empty() {
        out.extend_from_slice(b"!<arch>\n");
    }
    let header_name = if name.len() <= 16 {
        let mut buf = [b' '; 16];
        buf[..name.len()].copy_from_slice(name.as_bytes());
        buf
    } else {
        let mut buf = [b' '; 16];
        let marker = format!("#1/{}", name.len());
        buf[..marker.len()].copy_from_slice(marker.as_bytes());
        buf
    };
    let timestamp = b"0           ";
    let uid = b"0           ";
    let gid = b"0           ";
    let mode: &[u8] = b"644       ";
    let size_field = format!(
        "{:10}",
        payload.len() + if name.len() > 16 { name.len() } else { 0 }
    );
    out.extend_from_slice(&header_name);
    out.extend_from_slice(timestamp);
    out.extend_from_slice(uid);
    out.extend_from_slice(gid);
    out.extend_from_slice(mode);
    out.extend_from_slice(size_field.as_bytes());
    out.extend_from_slice(b"`\n");
    if name.len() > 16 {
        out.extend_from_slice(name.as_bytes());
    }
    out.extend_from_slice(payload);
    if out.len() % 2 == 1 {
        out.push(b'\n');
    }
}

fn write_macho_image(
    image: &MachOImage,
    linkage: MachOLinkage,
    install_name: &str,
    out: &mut Vec<u8>,
) {
    let (file_type, flags, text_vmaddr) = match linkage {
        MachOLinkage::Executable => (MH_EXECUTE, MH_NOUNDEFS | MH_PIE | MH_DYLDLINK, TEXT_VMADDR),
        MachOLinkage::Dylib => (MH_DYLIB, MH_NOUNDEFS | MH_DYLDLINK, TEXT_VMADDR),
        MachOLinkage::StaticLib => (MH_OBJECT, MH_NOUNDEFS, 0),
    };

    let pagezero_cmd_size = if linkage == MachOLinkage::StaticLib {
        0
    } else {
        72
    };
    let text_cmd_size = 72 + 80;
    let linkedit_cmd_size = if image.exports.is_empty() && linkage != MachOLinkage::Executable {
        0
    } else {
        72
    };
    let main_cmd_size = if linkage == MachOLinkage::Executable {
        24
    } else {
        0
    };
    let id_dylib_cmd_size = if linkage == MachOLinkage::Dylib {
        align8(24 + install_name.len())
    } else {
        0
    };
    let symtab_cmd_size = if image.exports.is_empty() { 0 } else { 24 };
    let dysymtab_cmd_size = if image.exports.is_empty() { 0 } else { 80 };
    let build_version_cmd_size = if linkage == MachOLinkage::StaticLib {
        0
    } else {
        24
    };

    let ncmds = u32::from(pagezero_cmd_size > 0)
        + 1
        + u32::from(linkedit_cmd_size > 0)
        + u32::from(main_cmd_size > 0)
        + u32::from(id_dylib_cmd_size > 0)
        + u32::from(symtab_cmd_size > 0)
        + u32::from(dysymtab_cmd_size > 0)
        + u32::from(build_version_cmd_size > 0);
    let sizeofcmds = pagezero_cmd_size
        + text_cmd_size
        + linkedit_cmd_size
        + main_cmd_size
        + id_dylib_cmd_size
        + symtab_cmd_size
        + dysymtab_cmd_size
        + build_version_cmd_size;

    let text_fileoff = if linkage == MachOLinkage::StaticLib {
        u64::from(align8(sizeofcmds as usize))
    } else {
        PAGE_SIZE
    };
    let code_size = image.code.len() as u64;
    let text_vmsize = code_size.next_multiple_of(PAGE_SIZE).max(PAGE_SIZE);

    let linkedit = build_linkedit(image, text_vmaddr);
    let linkedit_fileoff = text_fileoff + text_vmsize;
    let linkedit_filesize = if image.exports.is_empty() && linkage == MachOLinkage::Executable {
        PAGE_SIZE
    } else {
        linkedit.total_size()
    };
    let file_size = linkedit_fileoff + linkedit_filesize.next_multiple_of(PAGE_SIZE);

    out.clear();
    out.extend_from_slice(&MH_MAGIC_64.to_le_bytes());
    out.extend_from_slice(&CPU_TYPE_ARM64.to_le_bytes());
    out.extend_from_slice(&CPU_SUBTYPE_ARM64_ALL.to_le_bytes());
    out.extend_from_slice(&file_type.to_le_bytes());
    out.extend_from_slice(&ncmds.to_le_bytes());
    out.extend_from_slice(&sizeofcmds.to_le_bytes());
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());

    if pagezero_cmd_size > 0 {
        write_pagezero_command(out, pagezero_cmd_size);
    }
    write_text_command(
        out,
        text_cmd_size,
        text_fileoff,
        code_size,
        text_vmsize,
        text_vmaddr,
    );
    if linkedit_cmd_size > 0 {
        write_linkedit_command(
            out,
            linkedit_cmd_size,
            linkedit_fileoff,
            linkedit_filesize,
            text_vmaddr + text_vmsize,
        );
    }
    if id_dylib_cmd_size > 0 {
        write_id_dylib_command(out, id_dylib_cmd_size, install_name);
    }
    if symtab_cmd_size > 0 {
        write_symtab_command(
            out,
            symtab_cmd_size,
            linkedit_fileoff,
            linkedit.symtab.len() as u32,
            linkedit_fileoff + linkedit.symtab.len() as u64,
            linkedit.strtab.len() as u32,
        );
    }
    if dysymtab_cmd_size > 0 {
        write_dysymtab_command(out, dysymtab_cmd_size, image.exports.len() as u32);
    }
    if main_cmd_size > 0 {
        let entryoff = image.entry_offset.unwrap_or(0);
        write_main_command(out, main_cmd_size, u64::from(entryoff));
    }
    if build_version_cmd_size > 0 {
        write_build_version_command(out, build_version_cmd_size);
    }

    while (out.len() as u64) < text_fileoff {
        out.push(0);
    }
    out.extend_from_slice(&image.code);
    while (out.len() as u64) < linkedit_fileoff {
        out.push(0);
    }
    if !image.exports.is_empty() {
        out.extend_from_slice(&linkedit.symtab);
        out.extend_from_slice(&linkedit.strtab);
    }
    while (out.len() as u64) < file_size {
        out.push(0);
    }
}

struct LinkEditTables {
    symtab: Vec<u8>,
    strtab: Vec<u8>,
}

impl LinkEditTables {
    fn total_size(&self) -> u64 {
        (self.symtab.len() + self.strtab.len()) as u64
    }
}

fn build_linkedit(image: &MachOImage, text_vmaddr: u64) -> LinkEditTables {
    let mut strtab = vec![0u8];
    let mut symtab = Vec::new();
    for export in &image.exports {
        let sym_name = format!("_{}", export.name);
        let strx = strtab.len() as u32;
        strtab.extend_from_slice(sym_name.as_bytes());
        strtab.push(0);
        symtab.extend_from_slice(&strx.to_le_bytes());
        symtab.push(N_SECT | N_EXT);
        symtab.push(1);
        symtab.extend_from_slice(&0u16.to_le_bytes());
        symtab.extend_from_slice(&(text_vmaddr + u64::from(export.offset)).to_le_bytes());
    }
    LinkEditTables { symtab, strtab }
}

fn align8(value: usize) -> u32 {
    value.next_multiple_of(8) as u32
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
    vmaddr: u64,
) {
    out.extend_from_slice(&LC_SEGMENT_64.to_le_bytes());
    out.extend_from_slice(&cmdsize.to_le_bytes());
    write_fixed_name(out, "__TEXT");
    out.extend_from_slice(&vmaddr.to_le_bytes());
    out.extend_from_slice(&vmsize.to_le_bytes());
    out.extend_from_slice(&fileoff.to_le_bytes());
    out.extend_from_slice(&vmsize.to_le_bytes());
    out.extend_from_slice(&TEXT_PROT.to_le_bytes());
    out.extend_from_slice(&TEXT_PROT.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());

    write_fixed_name(out, "__text");
    write_fixed_name(out, "__TEXT");
    out.extend_from_slice(&vmaddr.to_le_bytes());
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

fn write_linkedit_command(
    out: &mut Vec<u8>,
    cmdsize: u32,
    fileoff: u64,
    filesize: u64,
    vmaddr: u64,
) {
    out.extend_from_slice(&LC_SEGMENT_64.to_le_bytes());
    out.extend_from_slice(&cmdsize.to_le_bytes());
    write_fixed_name(out, "__LINKEDIT");
    out.extend_from_slice(&vmaddr.to_le_bytes());
    out.extend_from_slice(&filesize.to_le_bytes());
    out.extend_from_slice(&fileoff.to_le_bytes());
    out.extend_from_slice(&filesize.to_le_bytes());
    out.extend_from_slice(&TEXT_PROT.to_le_bytes());
    out.extend_from_slice(&TEXT_PROT.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
}

fn write_id_dylib_command(out: &mut Vec<u8>, cmdsize: u32, install_name: &str) {
    out.extend_from_slice(&LC_ID_DYLIB.to_le_bytes());
    out.extend_from_slice(&cmdsize.to_le_bytes());
    out.extend_from_slice(&24u32.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&0x0001_0000u32.to_le_bytes());
    out.extend_from_slice(&0x0001_0000u32.to_le_bytes());
    out.extend_from_slice(install_name.as_bytes());
    let padding = cmdsize as usize - 24 - install_name.len();
    out.extend(std::iter::repeat_n(0u8, padding));
}

fn write_symtab_command(
    out: &mut Vec<u8>,
    cmdsize: u32,
    symoff: u64,
    nsyms: u32,
    stroff: u64,
    strsize: u32,
) {
    out.extend_from_slice(&LC_SYMTAB.to_le_bytes());
    out.extend_from_slice(&cmdsize.to_le_bytes());
    out.extend_from_slice(&(symoff as u32).to_le_bytes());
    out.extend_from_slice(&nsyms.to_le_bytes());
    out.extend_from_slice(&(stroff as u32).to_le_bytes());
    out.extend_from_slice(&strsize.to_le_bytes());
}

fn write_dysymtab_command(out: &mut Vec<u8>, cmdsize: u32, nextdefsym: u32) {
    out.extend_from_slice(&LC_DYSYMTAB.to_le_bytes());
    out.extend_from_slice(&cmdsize.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&nextdefsym.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
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

    fn sample_image() -> MachOImage {
        MachOImage {
            code: vec![0xD6, 0x5F, 0x03, 0xC0],
            entry_offset: Some(0),
            exports: vec![ExportSymbol {
                name: "answer".into(),
                offset: 0,
            }],
        }
    }

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

    #[test]
    fn dylib_uses_mh_dylib_file_type() {
        let image = sample_image();
        let mut out = Vec::new();
        write_image(
            &image,
            MachOLinkage::Dylib,
            "@rpath/libsample.dylib",
            &mut out,
        );
        assert_eq!(&out[0..4], &MH_MAGIC_64.to_le_bytes());
        assert_eq!(
            u32::from_le_bytes(out[12..16].try_into().unwrap()),
            MH_DYLIB
        );
    }

    #[test]
    fn staticlib_writes_ar_archive() {
        let image = sample_image();
        let mut out = Vec::new();
        write_image(&image, MachOLinkage::StaticLib, "sample", &mut out);
        assert_eq!(&out[..8], b"!<arch>\n");
        let object_offset = 82;
        assert_eq!(
            u32::from_le_bytes(out[object_offset..object_offset + 4].try_into().unwrap()),
            MH_MAGIC_64
        );
        assert_eq!(
            i32::from_le_bytes(
                out[object_offset + 4..object_offset + 8]
                    .try_into()
                    .unwrap()
            ),
            CPU_TYPE_ARM64
        );
        assert_eq!(
            u32::from_le_bytes(
                out[object_offset + 12..object_offset + 16]
                    .try_into()
                    .unwrap()
            ),
            MH_OBJECT
        );
        assert!(String::from_utf8_lossy(&out).contains("_answer"));
    }

    #[test]
    fn dylib_exports_symbols_in_linkedit() {
        let image = sample_image();
        let mut out = Vec::new();
        write_image(
            &image,
            MachOLinkage::Dylib,
            "@rpath/libsample.dylib",
            &mut out,
        );
        let haystack = String::from_utf8_lossy(&out);
        assert!(haystack.contains("_answer"));
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn roundtrip_answer_code_layout() {
        let code = vec![
            0x40, 0x05, 0x80, 0xd2, 0x20, 0x00, 0x80, 0xd2, 0x01, 0x10, 0x00, 0xd4,
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
            .args(["-s", "-", "-f"])
            .arg(&path)
            .status()
            .expect("codesign");
        assert!(sign.success(), "codesign failed");
        let status = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(&path)
            .status()
            .expect("run");
        match status.code() {
            Some(42) => {}
            None if status.signal() == Some(9) => {
                let otool = std::process::Command::new("otool")
                    .arg("-tV")
                    .arg(&path)
                    .output()
                    .expect("otool");
                assert!(String::from_utf8_lossy(&otool.stdout).contains("mov\tx0, #0x2a"));
            }
            other => panic!("unexpected native exit {other:?}"),
        }
        let _ = std::fs::remove_file(path);
    }
}
