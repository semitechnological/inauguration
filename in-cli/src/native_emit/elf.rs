const ELF_MAGIC: [u8; 4] = [0x7F, b'E', b'L', b'F'];
const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;
const EV_CURRENT: u8 = 1;
const ET_EXEC: u16 = 2;
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
