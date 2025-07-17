use alloc::sync::Arc;
use spin::Mutex;

use crate::{mem::Region, scheduling::process::ProcessEntry};

use super::ElfRegion;

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

fn load_ph_headers(header: &ElfHeader64, elf_ptr: *mut u8) -> Option<Arc<Mutex<ElfRegion>>> {
    let ph_ptr = unsafe { elf_ptr.add(header.ph_offset as usize) };
    let mut start_region: Option<Arc<Mutex<ElfRegion>>> = None;
    let mut last_region: Option<Arc<Mutex<ElfRegion>>> = None;

    for i in 0..header.ph_num {
        let ph_offset = (i * header.ph_entry_size) as usize;
        let entry_ptr = unsafe { ph_ptr.add(ph_offset) };

        let entry = unsafe { &*(entry_ptr as *mut ElfProgramHeader64) };
        if entry.ph_type != 1 {
            // not a LOAD entry
            continue;
        }

        let addr = entry.virt_addr as usize;
        let size = entry.memory_size as usize;

        let ph_file_src = unsafe { elf_ptr.add(entry.offset as usize) };
        let ph_file_addr = ph_file_src as usize;
        let ph_file_size = entry.file_size as usize;
        let ph_file_region = Region::new(ph_file_addr, ph_file_size);

        // construct ELF region structure
        let mem_region = Region::new(addr, size - 1);
        let elf_region = ElfRegion::new(mem_region, None, ph_file_region);
        let elf_region = Arc::new(Mutex::new(elf_region));

        match &mut last_region {
            Some(last_region) => {
                last_region.lock().next = Some(elf_region);
            }
            None => {
                // initializing whole linked list
                start_region = Some(elf_region);
                last_region = start_region.clone();
            }
        }
    }

    start_region
}

pub fn load(elf: Region) -> Option<ProcessEntry> {
    let header = unsafe { &*(elf.addr as *const ElfHeader64) };
    let elf_type = header.elf_type;

    // vibe check magic :D
    let magic = ((header.ident[0] as u32) << 24)
        | ((header.ident[1] as u32) << 16)
        | ((header.ident[2] as u32) << 8)
        | (header.ident[3] as u32);

    if magic != 0x7f454c46 {
        return None;
    }

    // TODO: Do further ELF validation

    let ptr = elf.get_ptr();
    let start_region = load_ph_headers(header, ptr);

    let entry = header.entry_addr as usize;
    Some(ProcessEntry {
        entry: entry,
        start_region: start_region.unwrap(),
        ring3_page_table: None,
    })
}
