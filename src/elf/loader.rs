use crate::{
    mem::{
        paging::{entry::EntryFlags, Page},
        Region, GLOBAL_MEMORY_CONTROLLER, PAGE_SIZE,
    },
    print,
};

#[repr(C)]
/// Represents a 32-bit ELF Header.
/// `ph` stands for the program header.
/// `sh` stands for the section header.
struct ElfHeader64 {
    ident: [u8; 16],
    elf_type: u16,
    machine: u16,
    version: u32,
    entry_addr: u64,
    ph_offset: u64,
    sh_offset: u64,
    flags: u32,
    eh_size: u16,
    ph_entry_size: u16,
    ph_num: u16,
    sh_entry_size: u16,
    sh_num: u16,
    sh_str_hdr_index: u16,
}


#[repr(C)]
struct ElfProgramHeader64 {
    ph_type: u32,
    flags: u32,
    offset: u64,
    virt_addr: u64,
    phys_addr: u64,
    file_size: u64,
    memory_size: u64,
    align: u64,
}

fn load_ph_headers(header: &ElfHeader64, elf_ptr: *mut u8) {
    let ph_ptr = unsafe { elf_ptr.add(header.ph_offset as usize) };
    for i in 0..header.ph_num {
        let ph_offset = (i * header.ph_entry_size) as usize;
        let entry_ptr = unsafe { ph_ptr.add(ph_offset) };

        let entry = unsafe { &*(entry_ptr as *mut ElfProgramHeader64) };
        if entry.ph_type != 1 {
            // not a LOAD entry
            continue;
        }

        // map memory
        let mut controller = GLOBAL_MEMORY_CONTROLLER.lock();
        let controller = controller.as_mut().unwrap();

        let start_page = Page::for_address(entry.virt_addr as usize);
        let end_page = Page::for_address((entry.virt_addr + entry.memory_size - 1) as usize);

        let start_map_addr = start_page.start_address();
        let end_map_addr = end_page.start_address() + PAGE_SIZE;

        controller.map(start_page, end_page, EntryFlags::WRITABLE);

        // load entry into memory
        print!("[ ELF ] Found LOAD entry, vaddr: 0x{:x}, file_size: 0x{:x}, memory_size: 0x{:x}, start_map: 0x{:x}, end_map: 0x{:x}\n", entry.virt_addr, entry.file_size, entry.memory_size, start_map_addr, end_map_addr);

        let ph_file_src = unsafe { elf_ptr.add(entry.offset as usize) };
        let destination_ptr = entry.virt_addr as *mut u8;

        unsafe {
            core::ptr::copy_nonoverlapping(ph_file_src, destination_ptr, entry.file_size as usize);
        }

        // check if BSS exists
        let bss_size = (entry.memory_size - entry.file_size) as usize;
        if bss_size > 0 {
            // zero BSS
            let bss_ptr = unsafe { destination_ptr.add(entry.file_size as usize) };
            unsafe { core::ptr::write_bytes(bss_ptr, 0, bss_size) };
        }
    }
}

pub fn load(elf: Region) {
    let header = unsafe { &*(elf.ptr as *const ElfHeader64) };
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

    load_ph_headers(header, elf.ptr);

    let entry = header.entry_addr as usize;
    unsafe {
        core::arch::asm!(
            "jmp {entry}",
            entry = in(reg) entry
        );
    }
}
