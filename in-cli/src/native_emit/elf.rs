const ELF_MAGIC: [u8; 4] = [0x7F, b'E', b'L', b'F'];
const ELFCLASS32: u8 = 1;
const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;
const EV_CURRENT: u8 = 1;
const ET_EXEC: u16 = 2;
const ET_REL: u16 = 1;
const EM_386: u16 = 3;
const EM_ARM: u16 = 40;
const EM_X86_64: u16 = 62;
const EM_AARCH64: u16 = 183;
const PT_LOAD: u32 = 1;
const PF_R: u32 = 4;
const PF_X: u32 = 1;
const EHDR32_SIZE: u32 = 52;
const EHDR_SIZE: u64 = 64;
const PHDR_SIZE: u64 = 56;
const TEXT_VADDR: u64 = 0x400_000;
const PAGE_SIZE: u64 = 0x1000;

pub const ELF_LINUX_TRIPLE: &str = "x86_64-unknown-linux-gnu";
pub const AARCH64_LINUX_TRIPLE: &str = "aarch64-unknown-linux-gnu";
pub const ARMV7_LINUX_GNUEABIHF_TRIPLE: &str = "armv7-unknown-linux-gnueabihf";

pub struct ElfExecutable {
    pub code: Vec<u8>,
    pub entry_offset: u32,
}

pub struct ElfObject {
    pub code: Vec<u8>,
    pub export_name: String,
    /// Additional exported symbols (name, offset).
    /// The primary export_name is always included as the first export.
    pub exports: Vec<(String, u32)>,
}

pub fn x86_64_return_i32_object_code(value: u8) -> Vec<u8> {
    vec![0xB8, value, 0x00, 0x00, 0x00, 0xC3]
}

pub fn aarch64_return_i32_object_code(value: u8) -> Vec<u8> {
    let mut code = Vec::new();
    code.extend_from_slice(&(0xD280_0000u32 | ((value as u32) << 5)).to_le_bytes());
    code.extend_from_slice(&0xD65F_03C0u32.to_le_bytes());
    code
}

pub fn arm32_return_i32_object_code(value: u8) -> Vec<u8> {
    let mut code = Vec::new();
    code.extend_from_slice(&(0xE3A0_0000u32 | value as u32).to_le_bytes());
    code.extend_from_slice(&0xE12F_FF1Eu32.to_le_bytes());
    code
}

pub fn x86_64_linux_exit_code(status: u8) -> Vec<u8> {
    vec![
        0x48, 0xC7, 0xC0, 0x3C, 0x00, 0x00, 0x00, 0x48, 0xC7, 0xC7, status, 0x00, 0x00, 0x00, 0x0F,
        0x05,
    ]
}

pub fn aarch64_linux_exit_code(status: u8) -> Vec<u8> {
    let mut code = Vec::new();
    code.extend_from_slice(&(0xD280_0000u32 | (93u32 << 5) | 8).to_le_bytes());
    code.extend_from_slice(&(0xD280_0000u32 | ((status as u32) << 5)).to_le_bytes());
    code.extend_from_slice(&0xD400_0001u32.to_le_bytes());
    code
}

pub fn arm32_linux_exit_code(status: u8) -> Vec<u8> {
    let mut code = Vec::new();
    code.extend_from_slice(&(0xE3A0_7000u32 | 1).to_le_bytes());
    code.extend_from_slice(&(0xE3A0_0000u32 | status as u32).to_le_bytes());
    code.extend_from_slice(&0xEF00_0000u32.to_le_bytes());
    code
}

pub fn write_x86_64_relocatable_object(object: &ElfObject, out: &mut Vec<u8>) {
    write_elf64_relocatable_object(object, EM_X86_64, out);
}

pub fn write_aarch64_relocatable_object(object: &ElfObject, out: &mut Vec<u8>) {
    write_elf64_relocatable_object(object, EM_AARCH64, out);
}

fn write_elf64_relocatable_object(object: &ElfObject, machine: u16, out: &mut Vec<u8>) {
    let shstrtab = b"\0.text\0.symtab\0.strtab\0.shstrtab\0";
    // Build combined export list: primary + additional
    let mut all_exports: Vec<(&str, u64)> = vec![(&object.export_name, 0)];
    let mut export_offsets: Vec<u64> = vec![0u64];
    for (name, offset) in &object.exports {
        all_exports.push((name, *offset as u64));
        export_offsets.push(*offset as u64);
    }
    // Build string table: null + all export names
    let mut strtab: Vec<u8> = vec![0u8];
    let mut name_indices: Vec<u32> = vec![0u32];
    for (name, _) in &all_exports {
        let idx = strtab.len() as u32;
        name_indices.push(idx);
        strtab.extend_from_slice(name.as_bytes());
        strtab.push(0u8);
    }
    let sym_count = all_exports.len() as u64 + 1;
    let symtab_entry_size = 24u64;
    let symtab_size = sym_count * symtab_entry_size;
    let text_name = 1u32;
    let symtab_name = 7u32;
    let strtab_name = 15u32;
    let shstrtab_name = 23u32;
    let text_offset = EHDR_SIZE;
    let symtab_offset = text_offset + object.code.len() as u64;
    let strtab_offset = symtab_offset + symtab_size;
    let shstrtab_offset = strtab_offset + strtab.len() as u64;
    let shoff = shstrtab_offset + shstrtab.len() as u64;
    let shentsize = 64u16;
    let shnum = 5u16;
    let shstrndx = 4u16;

    out.clear();
    out.extend_from_slice(&ELF_MAGIC);
    out.push(ELFCLASS64);
    out.push(ELFDATA2LSB);
    out.push(EV_CURRENT);
    out.extend_from_slice(&[0u8; 9]);
    out.extend_from_slice(&ET_REL.to_le_bytes());
    out.extend_from_slice(&machine.to_le_bytes());
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
    write_symbol(out, 0, 0, 0, 0, 0, 0);
    for i in 0..all_exports.len() {
        let (name_idx_val, size) = if i + 1 < all_exports.len() {
            (name_indices[i + 1], all_exports[i + 1].1 - all_exports[i].1)
        } else {
            (name_indices[i + 1], object.code.len() as u64 - all_exports[i].1)
        };
        write_symbol(out, name_idx_val, 0x12, 0, 1, all_exports[i].1, size);
    }
    out.extend_from_slice(&strtab);
    out.extend_from_slice(shstrtab);

    write_section_header(out, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0);
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
        0,
        0,
    );
    write_section_header(
        out,
        symtab_name,
        2,
        0,
        0,
        symtab_offset,
        symtab_size,
        8,
        24,
        3,
        1,
    );
    write_section_header(
        out,
        strtab_name,
        3,
        0,
        0,
        strtab_offset,
        strtab.len() as u64,
        1,
        0,
        0,
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
        0,
        0,
    );
}

pub fn write_arm32_relocatable_object(object: &ElfObject, out: &mut Vec<u8>) {
    let shstrtab = b"\0.text\0.symtab\0.strtab\0.shstrtab\0";
    let strtab = format!("\0{}\0", object.export_name).into_bytes();
    let symtab_size = 32u32;
    let text_name = 1u32;
    let symtab_name = 7u32;
    let strtab_name = 15u32;
    let shstrtab_name = 23u32;
    let text_offset = EHDR32_SIZE;
    let symtab_offset = text_offset + object.code.len() as u32;
    let strtab_offset = symtab_offset + symtab_size;
    let shstrtab_offset = strtab_offset + strtab.len() as u32;
    let shoff = shstrtab_offset + shstrtab.len() as u32;

    out.clear();
    out.extend_from_slice(&ELF_MAGIC);
    out.push(ELFCLASS32);
    out.push(ELFDATA2LSB);
    out.push(EV_CURRENT);
    out.extend_from_slice(&[0u8; 9]);
    out.extend_from_slice(&ET_REL.to_le_bytes());
    out.extend_from_slice(&EM_ARM.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&shoff.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(EHDR32_SIZE as u16).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&40u16.to_le_bytes());
    out.extend_from_slice(&5u16.to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());

    out.extend_from_slice(&object.code);
    write_symbol32(out, 0, 0, 0, 0, 0, 0);
    write_symbol32(out, 1, 0, object.code.len() as u32, 0x12, 0, 1);
    out.extend_from_slice(&strtab);
    out.extend_from_slice(shstrtab);

    write_section_header32(out, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0);
    write_section_header32(
        out,
        text_name,
        1,
        0x6,
        0,
        text_offset,
        object.code.len() as u32,
        0,
        0,
        4,
        0,
    );
    write_section_header32(
        out,
        symtab_name,
        2,
        0,
        0,
        symtab_offset,
        symtab_size,
        3,
        1,
        4,
        16,
    );
    write_section_header32(
        out,
        strtab_name,
        3,
        0,
        0,
        strtab_offset,
        strtab.len() as u32,
        0,
        0,
        1,
        0,
    );
    write_section_header32(
        out,
        shstrtab_name,
        3,
        0,
        0,
        shstrtab_offset,
        shstrtab.len() as u32,
        0,
        0,
        1,
        0,
    );
}

fn write_symbol32(
    out: &mut Vec<u8>,
    name: u32,
    value: u32,
    size: u32,
    info: u8,
    other: u8,
    shndx: u16,
) {
    out.extend_from_slice(&name.to_le_bytes());
    out.extend_from_slice(&value.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out.push(info);
    out.push(other);
    out.extend_from_slice(&shndx.to_le_bytes());
}

fn write_symbol(
    out: &mut Vec<u8>,
    name: u32,
    info: u8,
    other: u8,
    shndx: u16,
    value: u64,
    size: u64,
) {
    out.extend_from_slice(&name.to_le_bytes());
    out.push(info);
    out.push(other);
    out.extend_from_slice(&shndx.to_le_bytes());
    out.extend_from_slice(&value.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
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
    link: u32,
    info: u32,
) {
    out.extend_from_slice(&name.to_le_bytes());
    out.extend_from_slice(&typ.to_le_bytes());
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&addr.to_le_bytes());
    out.extend_from_slice(&offset.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&link.to_le_bytes());
    out.extend_from_slice(&info.to_le_bytes());
    out.extend_from_slice(&addralign.to_le_bytes());
    out.extend_from_slice(&entsize.to_le_bytes());
}

fn write_section_header32(
    out: &mut Vec<u8>,
    name: u32,
    typ: u32,
    flags: u32,
    addr: u32,
    offset: u32,
    size: u32,
    link: u32,
    info: u32,
    addralign: u32,
    entsize: u32,
) {
    out.extend_from_slice(&name.to_le_bytes());
    out.extend_from_slice(&typ.to_le_bytes());
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&addr.to_le_bytes());
    out.extend_from_slice(&offset.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&link.to_le_bytes());
    out.extend_from_slice(&info.to_le_bytes());
    out.extend_from_slice(&addralign.to_le_bytes());
    out.extend_from_slice(&entsize.to_le_bytes());
}

pub fn write_executable(exe: &ElfExecutable, out: &mut Vec<u8>) {
    write_elf64_executable(exe, EM_X86_64, out);
}

pub fn write_aarch64_executable(exe: &ElfExecutable, out: &mut Vec<u8>) {
    write_elf64_executable(exe, EM_AARCH64, out);
}

fn write_elf64_executable(exe: &ElfExecutable, machine: u16, out: &mut Vec<u8>) {
    let text_fileoff = PAGE_SIZE;
    let text_size = exe.code.len() as u64;
    let file_size = text_fileoff + text_size;
    let entry_vaddr = TEXT_VADDR + u64::from(exe.entry_offset);

    out.clear();
    out.extend_from_slice(&ELF_MAGIC);
    out.push(ELFCLASS64);
    out.push(ELFDATA2LSB);
    out.push(EV_CURRENT);
    out.extend_from_slice(&[0u8; 9]);
    out.extend_from_slice(&ET_EXEC.to_le_bytes());
    out.extend_from_slice(&machine.to_le_bytes());
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
    out.extend_from_slice(&0u16.to_le_bytes());

    out.extend_from_slice(&PT_LOAD.to_le_bytes());
    out.extend_from_slice(&(PF_R | PF_X).to_le_bytes());
    out.extend_from_slice(&text_fileoff.to_le_bytes());
    out.extend_from_slice(&TEXT_VADDR.to_le_bytes());
    out.extend_from_slice(&TEXT_VADDR.to_le_bytes());
    out.extend_from_slice(&text_size.to_le_bytes());
    out.extend_from_slice(&text_size.to_le_bytes());
    out.extend_from_slice(&PAGE_SIZE.to_le_bytes());

    while (out.len() as u64) < text_fileoff {
        out.push(0);
    }
    out.extend_from_slice(&exe.code);
    while (out.len() as u64) < file_size {
        out.push(0);
    }
}

pub fn write_i386_relocatable_object(object: &ElfObject, out: &mut Vec<u8>) {
    let shstrtab = b"\0.text\0.symtab\0.strtab\0.shstrtab\0";
    let strtab = format!("\0{}\0", object.export_name).into_bytes();
    let symtab_size = 32u32;
    let text_name = 1u32;
    let symtab_name = 7u32;
    let strtab_name = 15u32;
    let shstrtab_name = 23u32;
    let text_offset = EHDR32_SIZE;
    let symtab_offset = text_offset + object.code.len() as u32;
    let strtab_offset = symtab_offset + symtab_size;
    let shstrtab_offset = strtab_offset + strtab.len() as u32;
    let shoff = shstrtab_offset + shstrtab.len() as u32;

    out.clear();
    out.extend_from_slice(&ELF_MAGIC);
    out.push(ELFCLASS32);
    out.push(ELFDATA2LSB);
    out.push(EV_CURRENT);
    out.extend_from_slice(&[0u8; 9]);
    out.extend_from_slice(&ET_REL.to_le_bytes());
    out.extend_from_slice(&EM_386.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&shoff.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(EHDR32_SIZE as u16).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&40u16.to_le_bytes());
    out.extend_from_slice(&5u16.to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());

    out.extend_from_slice(&object.code);
    write_symbol32(out, 0, 0, 0, 0, 0, 0);
    write_symbol32(out, 1, 0, object.code.len() as u32, 0x12, 0, 1);
    out.extend_from_slice(&strtab);
    out.extend_from_slice(shstrtab);

    write_section_header32(out, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0);
    write_section_header32(
        out,
        text_name,
        1,
        0x6,
        0,
        text_offset,
        object.code.len() as u32,
        0,
        0,
        4,
        0,
    );
    write_section_header32(
        out,
        symtab_name,
        2,
        0,
        0,
        symtab_offset,
        symtab_size,
        3,
        1,
        4,
        16,
    );
    write_section_header32(
        out,
        strtab_name,
        3,
        0,
        0,
        strtab_offset,
        strtab.len() as u32,
        0,
        0,
        1,
        0,
    );
    write_section_header32(
        out,
        shstrtab_name,
        3,
        0,
        0,
        shstrtab_offset,
        shstrtab.len() as u32,
        0,
        0,
        1,
        0,
    );
}

pub fn write_arm32_executable(exe: &ElfExecutable, out: &mut Vec<u8>) {
    let text_fileoff = 0x1000u32;
    let text_vaddr = 0x10000u32;
    let text_size = exe.code.len() as u32;
    let file_size = text_fileoff + text_size;
    let entry_vaddr = text_vaddr + exe.entry_offset;

    out.clear();
    out.extend_from_slice(&ELF_MAGIC);
    out.push(ELFCLASS32);
    out.push(ELFDATA2LSB);
    out.push(EV_CURRENT);
    out.extend_from_slice(&[0u8; 9]);
    out.extend_from_slice(&ET_EXEC.to_le_bytes());
    out.extend_from_slice(&EM_ARM.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&entry_vaddr.to_le_bytes());
    out.extend_from_slice(&EHDR32_SIZE.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0x0500_0000u32.to_le_bytes());
    out.extend_from_slice(&(EHDR32_SIZE as u16).to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());

    out.extend_from_slice(&PT_LOAD.to_le_bytes());
    out.extend_from_slice(&text_fileoff.to_le_bytes());
    out.extend_from_slice(&text_vaddr.to_le_bytes());
    out.extend_from_slice(&text_vaddr.to_le_bytes());
    out.extend_from_slice(&text_size.to_le_bytes());
    out.extend_from_slice(&text_size.to_le_bytes());
    out.extend_from_slice(&(PF_R | PF_X).to_le_bytes());
    out.extend_from_slice(&0x1000u32.to_le_bytes());

    while (out.len() as u32) < text_fileoff {
        out.push(0);
    }
    out.extend_from_slice(&exe.code);
    while (out.len() as u32) < file_size {
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
        assert_eq!(out.len(), PAGE_SIZE as usize + exe.code.len());
        assert_eq!(out[4], ELFCLASS64);
        assert_eq!(u16::from_le_bytes([out[16], out[17]]), ET_EXEC);
        assert_eq!(u16::from_le_bytes([out[18], out[19]]), EM_X86_64);
    }

    #[test]
    fn writes_x86_64_relocatable_object() {
        let object = ElfObject {
            code: x86_64_return_i32_object_code(42),
            export_name: "answer".to_string(),
        exports: vec![],
    };
        let mut out = Vec::new();
        write_x86_64_relocatable_object(&object, &mut out);
        assert_eq!(&out[0..4], &ELF_MAGIC);
        assert_eq!(u16::from_le_bytes([out[16], out[17]]), ET_REL);
        assert_eq!(u16::from_le_bytes([out[18], out[19]]), EM_X86_64);
        assert!(out.windows(5).any(|window| window == b".text"));
        assert!(out.windows(7).any(|window| window == b".symtab"));
        assert!(out.windows(7).any(|window| window == b".strtab"));
        assert!(out.windows(9).any(|window| window == b".shstrtab"));
        assert!(out.windows(6).any(|window| window == b"answer"));
    }

    #[test]
    fn writes_aarch64_relocatable_object() {
        let object = ElfObject {
            code: aarch64_return_i32_object_code(42),
            export_name: "answer".to_string(),
        exports: vec![],
    };
        let mut out = Vec::new();
        write_aarch64_relocatable_object(&object, &mut out);
        assert_eq!(&out[0..4], &ELF_MAGIC);
        assert_eq!(out[4], ELFCLASS64);
        assert_eq!(u16::from_le_bytes([out[16], out[17]]), ET_REL);
        assert_eq!(u16::from_le_bytes([out[18], out[19]]), EM_AARCH64);
        assert!(out.windows(5).any(|window| window == b".text"));
        assert!(out.windows(7).any(|window| window == b".symtab"));
        assert!(out.windows(7).any(|window| window == b".strtab"));
        assert!(out.windows(9).any(|window| window == b".shstrtab"));
        assert!(out.windows(6).any(|window| window == b"answer"));
        assert!(
            out.windows(8)
                .any(|window| window == [0x40, 0x05, 0x80, 0xD2, 0xC0, 0x03, 0x5F, 0xD6])
        );
    }

    #[test]
    fn writes_arm32_relocatable_object() {
        let object = ElfObject {
            code: arm32_return_i32_object_code(42),
            export_name: "answer".to_string(),
        exports: vec![],
    };
        let mut out = Vec::new();
        write_arm32_relocatable_object(&object, &mut out);
        assert_eq!(&out[0..4], &ELF_MAGIC);
        assert_eq!(out[4], ELFCLASS32);
        assert_eq!(u16::from_le_bytes([out[16], out[17]]), ET_REL);
        assert_eq!(u16::from_le_bytes([out[18], out[19]]), EM_ARM);
        assert!(out.windows(5).any(|window| window == b".text"));
        assert!(out.windows(7).any(|window| window == b".symtab"));
        assert!(out.windows(7).any(|window| window == b".strtab"));
        assert!(out.windows(9).any(|window| window == b".shstrtab"));
        assert!(out.windows(6).any(|window| window == b"answer"));
        assert!(
            out.windows(8)
                .any(|window| window == [0x2A, 0x00, 0xA0, 0xE3, 0x1E, 0xFF, 0x2F, 0xE1])
        );
    }

    #[test]
    fn writes_x86_64_exit_executable() {
        let exe = ElfExecutable {
            code: x86_64_linux_exit_code(42),
            entry_offset: 0,
        };
        let mut out = Vec::new();
        write_executable(&exe, &mut out);
        assert_eq!(&out[0..4], &ELF_MAGIC);
        assert_eq!(u16::from_le_bytes([out[16], out[17]]), ET_EXEC);
        assert_eq!(u16::from_le_bytes([out[18], out[19]]), EM_X86_64);
        assert_eq!(u32::from_le_bytes(out[64..68].try_into().unwrap()), PT_LOAD);
        assert_eq!(
            u32::from_le_bytes(out[68..72].try_into().unwrap()),
            PF_R | PF_X
        );
        assert!(out.windows(16).any(|window| window
            == [
                0x48, 0xC7, 0xC0, 0x3C, 0x00, 0x00, 0x00, 0x48, 0xC7, 0xC7, 0x2A, 0x00, 0x00, 0x00,
                0x0F, 0x05
            ]));
    }

    #[test]
    fn writes_aarch64_exit_executable() {
        let exe = ElfExecutable {
            code: aarch64_linux_exit_code(42),
            entry_offset: 0,
        };
        let mut out = Vec::new();
        write_aarch64_executable(&exe, &mut out);
        assert_eq!(&out[0..4], &ELF_MAGIC);
        assert_eq!(out[4], ELFCLASS64);
        assert_eq!(u16::from_le_bytes([out[16], out[17]]), ET_EXEC);
        assert_eq!(u16::from_le_bytes([out[18], out[19]]), EM_AARCH64);
        assert_eq!(u32::from_le_bytes(out[64..68].try_into().unwrap()), PT_LOAD);
        assert_eq!(
            u32::from_le_bytes(out[68..72].try_into().unwrap()),
            PF_R | PF_X
        );
        assert!(out.windows(12).any(|window| window
            == [
                0xA8, 0x0B, 0x80, 0xD2, 0x40, 0x05, 0x80, 0xD2, 0x01, 0x00, 0x00, 0xD4
            ]));
    }

    #[test]
    fn writes_arm32_exit_executable() {
        let exe = ElfExecutable {
            code: arm32_linux_exit_code(42),
            entry_offset: 0,
        };
        let mut out = Vec::new();
        write_arm32_executable(&exe, &mut out);
        assert_eq!(&out[0..4], &ELF_MAGIC);
        assert_eq!(out[4], ELFCLASS32);
        assert_eq!(u16::from_le_bytes([out[16], out[17]]), ET_EXEC);
        assert_eq!(u16::from_le_bytes([out[18], out[19]]), EM_ARM);
        assert_eq!(u32::from_le_bytes(out[52..56].try_into().unwrap()), PT_LOAD);
        assert_eq!(
            u32::from_le_bytes(out[76..80].try_into().unwrap()),
            PF_R | PF_X
        );
        assert!(out.windows(12).any(|window| window
            == [
                0x01, 0x70, 0xA0, 0xE3, 0x2A, 0x00, 0xA0, 0xE3, 0x00, 0x00, 0x00, 0xEF
            ]));
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
