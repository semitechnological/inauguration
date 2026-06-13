const ELF_MAGIC: [u8; 4] = [0x7F, b'E', b'L', b'F'];
const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;
const EV_CURRENT: u8 = 1;
const ET_EXEC: u16 = 2;
const ET_REL: u16 = 1;
const EM_X86_64: u16 = 62;
const PT_LOAD: u32 = 1;
const PF_R: u32 = 4;
const PF_X: u32 = 1;
const EHDR_SIZE: u64 = 64;
const PHDR_SIZE: u64 = 56;
const TEXT_VADDR: u64 = 0x400_000;
const PAGE_SIZE: u64 = 0x1000;

pub const ELF_LINUX_TRIPLE: &str = "x86_64-unknown-linux-gnu";

pub struct ElfExecutable {
    pub code: Vec<u8>,
    pub entry_offset: u32,
}

pub struct ElfObject {
    pub code: Vec<u8>,
}

pub fn x86_64_return_i32_object_code(value: u8) -> Vec<u8> {
    vec![0xB8, value, 0x00, 0x00, 0x00, 0xC3]
}

pub fn write_x86_64_relocatable_object(object: &ElfObject, out: &mut Vec<u8>) {
    let shstrtab = b"\0.text\0.shstrtab\0";
    let text_name = 1u32;
    let shstrtab_name = 7u32;
    let text_offset = EHDR_SIZE;
    let shstrtab_offset = text_offset + object.code.len() as u64;
    let shoff = shstrtab_offset + shstrtab.len() as u64;
    let shentsize = 64u16;
    let shnum = 3u16;
    let shstrndx = 2u16;

    out.clear();
    out.extend_from_slice(&ELF_MAGIC);
    out.push(ELFCLASS64);
    out.push(ELFDATA2LSB);
    out.push(EV_CURRENT);
    out.extend_from_slice(&[0u8; 9]);
    out.extend_from_slice(&ET_REL.to_le_bytes());
    out.extend_from_slice(&EM_X86_64.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&0u64.to_le_bytes());
    out.extend_from_slice(&0u64.to_le_bytes());
    out.extend_from_slice(&shoff.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(EHDR_SIZE as u16).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&shentsize.to_le_bytes());
    out.extend_from_slice(&shnum.to_le_bytes());
    out.extend_from_slice(&shstrndx.to_le_bytes());

    out.extend_from_slice(&object.code);
    out.extend_from_slice(shstrtab);

    write_section_header(out, 0, 0, 0, 0, 0, 0, 0, 0);
    write_section_header(
        out,
        text_name,
        1,
        0x6,
        0,
        text_offset,
        object.code.len() as u64,
        16,
        0,
    );
    write_section_header(
        out,
        shstrtab_name,
        3,
        0,
        0,
        shstrtab_offset,
        shstrtab.len() as u64,
        1,
        0,
    );
}

fn write_section_header(
    out: &mut Vec<u8>,
    name: u32,
    typ: u32,
    flags: u64,
    addr: u64,
    offset: u64,
    size: u64,
    addralign: u64,
    entsize: u64,
) {
    out.extend_from_slice(&name.to_le_bytes());
    out.extend_from_slice(&typ.to_le_bytes());
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&addr.to_le_bytes());
    out.extend_from_slice(&offset.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&addralign.to_le_bytes());
    out.extend_from_slice(&entsize.to_le_bytes());
}

pub fn write_executable(exe: &ElfExecutable, out: &mut Vec<u8>) {
    let text_fileoff = PAGE_SIZE;
    let file_size = text_fileoff + PAGE_SIZE;
    let entry_vaddr = TEXT_VADDR + u64::from(exe.entry_offset);

    out.clear();
    out.extend_from_slice(&ELF_MAGIC);
    out.push(ELFCLASS64);
    out.push(ELFDATA2LSB);
    out.push(EV_CURRENT);
    out.extend_from_slice(&[0u8; 9]);
    out.extend_from_slice(&ET_EXEC.to_le_bytes());
    out.extend_from_slice(&EM_X86_64.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&entry_vaddr.to_le_bytes());
    out.extend_from_slice(&EHDR_SIZE.to_le_bytes());
    out.extend_from_slice(&0u64.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(EHDR_SIZE as u16).to_le_bytes());
    out.extend_from_slice(&(PHDR_SIZE as u16).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());

    out.extend_from_slice(&PT_LOAD.to_le_bytes());
    out.extend_from_slice(&(PF_R | PF_X).to_le_bytes());
    out.extend_from_slice(&text_fileoff.to_le_bytes());
    out.extend_from_slice(&TEXT_VADDR.to_le_bytes());
    out.extend_from_slice(&TEXT_VADDR.to_le_bytes());
    out.extend_from_slice(&(exe.code.len() as u64).to_le_bytes());
    out.extend_from_slice(&PAGE_SIZE.to_le_bytes());
    out.extend_from_slice(&PAGE_SIZE.to_le_bytes());

    while (out.len() as u64) < text_fileoff {
        out.push(0);
    }
    out.extend_from_slice(&exe.code);
    while (out.len() as u64) < file_size {
        out.push(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_magic_and_program_headers() {
        let exe = ElfExecutable {
            code: vec![
                0x48, 0xC7, 0xC0, 0x3C, 0x00, 0x00, 0x00, 0x48, 0xC7, 0xC7, 0x2A, 0x00, 0x00, 0x00,
                0x0F, 0x05,
            ],
            entry_offset: 0,
        };
        let mut out = Vec::new();
        write_executable(&exe, &mut out);
        assert_eq!(&out[0..4], &ELF_MAGIC);
        assert_eq!(out.len(), (PAGE_SIZE * 2) as usize);
        assert_eq!(out[4], ELFCLASS64);
        assert_eq!(u16::from_le_bytes([out[16], out[17]]), ET_EXEC);
        assert_eq!(u16::from_le_bytes([out[18], out[19]]), EM_X86_64);
    }

    #[test]
    fn writes_x86_64_relocatable_object() {
        let object = ElfObject {
            code: x86_64_return_i32_object_code(42),
        };
        let mut out = Vec::new();
        write_x86_64_relocatable_object(&object, &mut out);
        assert_eq!(&out[0..4], &ELF_MAGIC);
        assert_eq!(u16::from_le_bytes([out[16], out[17]]), ET_REL);
        assert_eq!(u16::from_le_bytes([out[18], out[19]]), EM_X86_64);
        assert!(out.windows(5).any(|window| window == b".text"));
        assert!(out.windows(9).any(|window| window == b".shstrtab"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn roundtrip_answer_code_layout() {
        let code = vec![
            0x48, 0xC7, 0xC0, 0x3C, 0x00, 0x00, 0x00, 0x48, 0xC7, 0xC7, 0x2A, 0x00, 0x00, 0x00,
            0x0F, 0x05,
        ];
        let exe = ElfExecutable {
            code,
            entry_offset: 0,
        };
        let path = std::path::PathBuf::from("/tmp/inauguration-elf-roundtrip");
        let mut out = Vec::new();
        write_executable(&exe, &mut out);
        std::fs::write(&path, &out).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        let status = std::process::Command::new(&path).status().expect("run");
        match status.code() {
            Some(42) => {}
            other => panic!("unexpected native exit {other:?}"),
        }
        let _ = std::fs::remove_file(path);
    }
}
