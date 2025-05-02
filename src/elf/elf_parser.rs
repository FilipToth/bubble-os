use crate::{mem::Region, print};

#[repr(C)]
/// Represents a 32-bit ELF Header.
/// `ph` stands for the program header.
/// `sh` stands for the section header.
struct ElfHeader32 {
    ident: [u8; 16],
    elf_type: u16,
    machine: u16,
    version: u32,
    entry_addr: u32,
    ph_offset: u32,
    sh_offset: u32,
    flags: u32,
    elf_header_size: u16,
    ph_entry_size: u16,
    ph_num: u16,
    sh_entry_size: u16,
    sh_num: u16,
    sh_str_hdr_index: u16,
}

pub fn parse(elf: Region) {
    let header = unsafe { &*(elf.ptr as *const ElfHeader32) };
    let elf_type = header.elf_type;
    print!("[ ELF ] elf_type: {}\n", elf_type);

    // vibe check magic :D
    let magic = ((header.ident[0] as u32) << 24)
                | ((header.ident[1] as u32) << 16)
                | ((header.ident[2] as u32) << 8)
                | (header.ident[3] as u32);

    if magic != 0x7f454c46 {
        return;
    }

    print!("[ ELF ] Verified ELF Magic\n");
}