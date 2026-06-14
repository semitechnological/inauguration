#[cfg(test)]
const DOS_MAGIC: u16 = 0x5A4D;
const PE_SIGNATURE: u32 = 0x0000_4550;
const MACHINE_AMD64: u16 = 0x8664;
const IMAGE_FILE_DLL: u16 = 0x2000;
const IMAGE_FILE_EXECUTABLE_IMAGE: u16 = 0x0002;
const IMAGE_SCN_CNT_CODE: u32 = 0x0000_0020;
const IMAGE_SCN_MEM_EXECUTE: u32 = 0x2000_0000;
const IMAGE_SCN_MEM_READ: u32 = 0x4000_0000;
const IMAGE_DIRECTORY_ENTRY_IMPORT: usize = 1;
const OPTIONAL_MAGIC_PE32_PLUS: u16 = 0x20B;
const SUBSYSTEM_WINDOWS_CUI: u16 = 3;
const DLL_CHARACTERISTICS_DYNAMIC_BASE: u16 = 0x0040;
const DLL_CHARACTERISTICS_NX_COMPAT: u16 = 0x0100;
const SECTION_ALIGNMENT: u32 = 0x1000;
const FILE_ALIGNMENT: u32 = 0x200;
const SIZE_OF_OPTIONAL_HEADER: u16 = 240;
const SIZE_OF_HEADERS: u32 = 0x200;
const TEXT_RVA: u32 = 0x1000;
const PE_OFFSET: u32 = 0x80;

pub const COFF_WINDOWS_TRIPLE: &str = "x86_64-pc-windows-msvc";

pub struct CoffDll {
    pub code: Vec<u8>,
    pub entry_offset: u32,
}

pub struct CoffExe {
    pub code: Vec<u8>,
    pub entry_offset: u32,
}

struct PeDirectory {
    rva: u32,
    size: u32,
}

struct PeImage<'a> {
    code: &'a [u8],
    entry_offset: u32,
    image_flags: u16,
    import_directory: Option<PeDirectory>,
}

pub fn write_dll(dll: &CoffDll, out: &mut Vec<u8>) {
    write_pe_image(
        PeImage {
            code: &dll.code,
            entry_offset: dll.entry_offset,
            image_flags: IMAGE_FILE_DLL,
            import_directory: None,
        },
        out,
    );
}

pub fn write_exe(exe: &CoffExe, out: &mut Vec<u8>) {
    write_pe_image(
        PeImage {
            code: &exe.code,
            entry_offset: exe.entry_offset,
            image_flags: 0,
            import_directory: None,
        },
        out,
    );
}

pub fn x86_64_exit_process_code(status: u8) -> Vec<u8> {
    let code_len = 16u32;
    let import_offset = align_to(code_len, 8);
    let descriptor_offset = import_offset;
    let ilt_offset = descriptor_offset + 40;
    let iat_offset = ilt_offset + 16;
    let hint_name_offset = iat_offset + 16;
    let dll_name_offset = align_to(hint_name_offset + 2 + "ExitProcess".len() as u32 + 1, 2);
    let hint_name_rva = TEXT_RVA + hint_name_offset;
    let iat_rva = TEXT_RVA + iat_offset;
    let call_next_offset = 15u32;
    let disp = iat_offset as i32 - call_next_offset as i32;
    let mut code = Vec::new();
    code.extend_from_slice(&[0x48, 0x83, 0xEC, 0x28]);
    code.push(0xB9);
    code.extend_from_slice(&(status as u32).to_le_bytes());
    code.extend_from_slice(&[0xFF, 0x15]);
    code.extend_from_slice(&disp.to_le_bytes());
    code.push(0xCC);
    while code.len() < descriptor_offset as usize {
        code.push(0);
    }
    code.extend_from_slice(&(TEXT_RVA + ilt_offset).to_le_bytes());
    code.extend_from_slice(&0u32.to_le_bytes());
    code.extend_from_slice(&0u32.to_le_bytes());
    code.extend_from_slice(&(TEXT_RVA + dll_name_offset).to_le_bytes());
    code.extend_from_slice(&iat_rva.to_le_bytes());
    code.extend_from_slice(&[0u8; 20]);
    code.extend_from_slice(&(hint_name_rva as u64).to_le_bytes());
    code.extend_from_slice(&0u64.to_le_bytes());
    code.extend_from_slice(&(hint_name_rva as u64).to_le_bytes());
    code.extend_from_slice(&0u64.to_le_bytes());
    code.extend_from_slice(&0u16.to_le_bytes());
    code.extend_from_slice(b"ExitProcess\0");
    while code.len() < dll_name_offset as usize {
        code.push(0);
    }
    code.extend_from_slice(b"KERNEL32.dll\0");
    code
}

pub fn write_exit_process_exe(status: u8, out: &mut Vec<u8>) {
    let code = x86_64_exit_process_code(status);
    let import_offset = align_to(16, 8);
    write_pe_image(
        PeImage {
            code: &code,
            entry_offset: 0,
            image_flags: 0,
            import_directory: Some(PeDirectory {
                rva: TEXT_RVA + import_offset,
                size: 40,
            }),
        },
        out,
    );
}

fn write_pe_image(image: PeImage<'_>, out: &mut Vec<u8>) {
    let dos_stub: [u8; 64] = [
        0x4D, 0x5A, 0x90, 0x00, 0x03, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0x00,
        0x00, 0xB8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x80, 0x00, 0x00, 0x00,
    ];
    let text_fileoff = SIZE_OF_HEADERS;
    let text_raw_size = (image.code.len() as u32).div_ceil(FILE_ALIGNMENT) * FILE_ALIGNMENT;
    let image_size = align_to(TEXT_RVA + image.code.len() as u32, SECTION_ALIGNMENT);
    let entry_rva = TEXT_RVA + image.entry_offset;

    out.clear();
    out.extend_from_slice(&dos_stub);
    while (out.len() as u32) < PE_OFFSET {
        out.push(0);
    }
    out.extend_from_slice(&PE_SIGNATURE.to_le_bytes());
    out.extend_from_slice(&MACHINE_AMD64.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&SIZE_OF_OPTIONAL_HEADER.to_le_bytes());
    out.extend_from_slice(&(image.image_flags | IMAGE_FILE_EXECUTABLE_IMAGE).to_le_bytes());
    out.extend_from_slice(&OPTIONAL_MAGIC_PE32_PLUS.to_le_bytes());
    out.extend_from_slice(&[0u8; 2]);
    out.extend_from_slice(&text_raw_size.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&entry_rva.to_le_bytes());
    out.extend_from_slice(&TEXT_RVA.to_le_bytes());
    out.extend_from_slice(&0x1400_0000u64.to_le_bytes());
    out.extend_from_slice(&SECTION_ALIGNMENT.to_le_bytes());
    out.extend_from_slice(&FILE_ALIGNMENT.to_le_bytes());
    out.extend_from_slice(&6u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&6u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&image_size.to_le_bytes());
    out.extend_from_slice(&SIZE_OF_HEADERS.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&SUBSYSTEM_WINDOWS_CUI.to_le_bytes());
    out.extend_from_slice(
        &(DLL_CHARACTERISTICS_DYNAMIC_BASE | DLL_CHARACTERISTICS_NX_COMPAT).to_le_bytes(),
    );
    out.extend_from_slice(&0x100000u64.to_le_bytes());
    out.extend_from_slice(&0x1000u64.to_le_bytes());
    out.extend_from_slice(&0x100000u64.to_le_bytes());
    out.extend_from_slice(&0x1000u64.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&16u32.to_le_bytes());
    for index in 0..16 {
        if index == IMAGE_DIRECTORY_ENTRY_IMPORT {
            if let Some(import) = &image.import_directory {
                out.extend_from_slice(&import.rva.to_le_bytes());
                out.extend_from_slice(&import.size.to_le_bytes());
            } else {
                out.extend_from_slice(&0u32.to_le_bytes());
                out.extend_from_slice(&0u32.to_le_bytes());
            }
        } else {
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
        }
    }

    write_section_name(out, ".text");
    out.extend_from_slice(&(image.code.len() as u32).to_le_bytes());
    out.extend_from_slice(&TEXT_RVA.to_le_bytes());
    out.extend_from_slice(&text_raw_size.to_le_bytes());
    out.extend_from_slice(&text_fileoff.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(
        &(IMAGE_SCN_CNT_CODE | IMAGE_SCN_MEM_EXECUTE | IMAGE_SCN_MEM_READ).to_le_bytes(),
    );

    while out.len() < text_fileoff as usize {
        out.push(0);
    }
    out.extend_from_slice(image.code);
    while out.len() < (text_fileoff + text_raw_size) as usize {
        out.push(0);
    }
}

fn align_to(value: u32, align: u32) -> u32 {
    value.div_ceil(align) * align
}

fn write_section_name(out: &mut Vec<u8>, name: &str) {
    let mut buf = [0u8; 8];
    let bytes = name.as_bytes();
    let len = bytes.len().min(8);
    buf[..len].copy_from_slice(&bytes[..len]);
    out.extend_from_slice(&buf);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_dos_and_pe_headers() {
        let dll = CoffDll {
            code: vec![
                0x48, 0xC7, 0xC0, 0x3C, 0x00, 0x00, 0x00, 0x48, 0xC7, 0xC7, 0x2A, 0x00, 0x00, 0x00,
                0x0F, 0x05,
            ],
            entry_offset: 0,
        };
        let mut out = Vec::new();
        write_dll(&dll, &mut out);
        assert_eq!(u16::from_le_bytes([out[0], out[1]]), DOS_MAGIC);
        let pe_off = u32::from_le_bytes([out[0x3C], out[0x3D], out[0x3E], out[0x3F]]) as usize;
        assert_eq!(&out[pe_off..pe_off + 4], &PE_SIGNATURE.to_le_bytes());
        let characteristics = u16::from_le_bytes([out[pe_off + 22], out[pe_off + 23]]);
        assert_eq!(characteristics & IMAGE_FILE_DLL, IMAGE_FILE_DLL);
        assert_eq!(
            u16::from_le_bytes([out[pe_off + 4], out[pe_off + 5]]),
            MACHINE_AMD64
        );
    }

    #[test]
    fn writes_exe_without_dll_characteristic() {
        let exe = CoffExe {
            code: vec![0xB8, 0x2A, 0x00, 0x00, 0x00, 0xC3],
            entry_offset: 0,
        };
        let mut out = Vec::new();
        write_exe(&exe, &mut out);
        assert_eq!(u16::from_le_bytes([out[0], out[1]]), DOS_MAGIC);
        let pe_off = u32::from_le_bytes([out[0x3C], out[0x3D], out[0x3E], out[0x3F]]) as usize;
        assert_eq!(&out[pe_off..pe_off + 4], &PE_SIGNATURE.to_le_bytes());
        let characteristics = u16::from_le_bytes([out[pe_off + 22], out[pe_off + 23]]);
        assert_eq!(
            characteristics & IMAGE_FILE_EXECUTABLE_IMAGE,
            IMAGE_FILE_EXECUTABLE_IMAGE
        );
        assert_eq!(characteristics & IMAGE_FILE_DLL, 0);
        assert!(
            out.windows(6)
                .any(|window| window == [0xB8, 0x2A, 0x00, 0x00, 0x00, 0xC3])
        );
    }

    #[test]
    fn writes_exit_process_import_exe() {
        let mut out = Vec::new();
        write_exit_process_exe(42, &mut out);
        assert_eq!(u16::from_le_bytes([out[0], out[1]]), DOS_MAGIC);
        let pe_off = u32::from_le_bytes([out[0x3C], out[0x3D], out[0x3E], out[0x3F]]) as usize;
        assert_eq!(&out[pe_off..pe_off + 4], &PE_SIGNATURE.to_le_bytes());
        let optional = pe_off + 24;
        let import_dir = optional + 112 + 8;
        assert_eq!(
            u32::from_le_bytes(out[import_dir..import_dir + 4].try_into().unwrap()),
            TEXT_RVA + 16
        );
        assert!(out.windows(12).any(|window| window == b"KERNEL32.dll"));
        assert!(out.windows(11).any(|window| window == b"ExitProcess"));
        assert!(out.windows(15).any(|window| window
            == [
                0x48, 0x83, 0xEC, 0x28, 0xB9, 0x2A, 0x00, 0x00, 0x00, 0xFF, 0x15, 0x39, 0x00, 0x00,
                0x00
            ]));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn roundtrip_dll_layout() {
        let code = vec![
            0x48, 0xC7, 0xC0, 0x3C, 0x00, 0x00, 0x00, 0x48, 0xC7, 0xC7, 0x2A, 0x00, 0x00, 0x00,
            0x0F, 0x05,
        ];
        let dll = CoffDll {
            code,
            entry_offset: 0,
        };
        let path = std::path::PathBuf::from("inauguration-coff-roundtrip.dll");
        let mut out = Vec::new();
        write_dll(&dll, &mut out);
        std::fs::write(&path, &out).unwrap();
        let _ = std::fs::remove_file(path);
    }
}
