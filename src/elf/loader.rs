use alloc::sync::Arc;
use spin::Mutex;

use crate::{io::LogType, log, mem::Region, scheduling::process::ProcessEntry};

use super::{ElfProgramHeaderFlags, ElfRegion};

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

fn load_ph_headers(header: &ElfHeader64, elf: &Region) -> Option<Arc<Mutex<ElfRegion>>> {
    let elf_ptr = elf.get_ptr::<u8>();
    let ph_table_size = (header.ph_num as usize).checked_mul(header.ph_entry_size as usize)?;
    let ph_table_end = (header.ph_offset as usize).checked_add(ph_table_size)?;
    if ph_table_end > elf.size {
        log!(
            LogType::ERR,
            "elf_loader: PH table out of file bounds, ph_offset: 0x{:X}, ph_num: {}, ph_entry_size: {}, elf_size: 0x{:X}",
            header.ph_offset,
            header.ph_num,
            header.ph_entry_size,
            elf.size
        );

        return None;
    }

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
        if size == 0 {
            log!(
                LogType::ERR,
                "elf_loader: LOAD ph {} has zero memory size",
                i
            );
            return None;
        }

        if entry.file_size > entry.memory_size {
            log!(
                LogType::ERR,
                "elf_loader: LOAD ph {} has file_size > memory_size, file_size: 0x{:X}, mem_size: 0x{:X}",
                i,
                entry.file_size,
                entry.memory_size
            );

            return None;
        }

        let ph_file_end = (entry.offset as usize).checked_add(entry.file_size as usize)?;
        if ph_file_end > elf.size {
            log!(
                LogType::ERR,
                "elf_loader: LOAD ph {} file range out of bounds, offset: 0x{:X}, file_size: 0x{:X}, elf_size: 0x{:X}",
                i,
                entry.offset,
                entry.file_size,
                elf.size
            );

            return None;
        }

        let ph_file_src = unsafe { elf_ptr.add(entry.offset as usize) };
        let ph_file_addr = ph_file_src as usize;
        let ph_file_size = entry.file_size as usize;
        let ph_file_region = Region::new(ph_file_addr, ph_file_size);
        let flags = ElfProgramHeaderFlags::from_bits_retain(entry.flags);

        // construct ELF region structure
        let mem_region = Region::new(addr, size);
        let elf_region = ElfRegion::new(mem_region, None, ph_file_region, flags);
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
    if elf.size < core::mem::size_of::<ElfHeader64>() {
        log!(
            LogType::ERR,
            "elf_loader: file too small for ELF header, size: 0x{:X}",
            elf.size
        );

        return None;
    }

    let header = unsafe { &*(elf.addr as *const ElfHeader64) };

    // vibe check magic :D
    let magic = ((header.ident[0] as u32) << 24)
        | ((header.ident[1] as u32) << 16)
        | ((header.ident[2] as u32) << 8)
        | (header.ident[3] as u32);

    if magic != 0x7f454c46 {
        log!(
            LogType::ERR,
            "elf_loader: invalid magic 0x{:X}, elf_addr: 0x{:X}, elf_size: 0x{:X}",
            magic,
            elf.addr,
            elf.size
        );

        return None;
    }

    // TODO: Do further ELF validation

    let Some(start_region) = load_ph_headers(header, &elf) else {
        log!(LogType::ERR, "elf_loader: failed to load program headers");
        return None;
    };

    let entry = header.entry_addr as usize;
    Some(ProcessEntry {
        entry: entry,
        start_region: start_region,
        ring3_page_table: None,
        stack: None,
        initial_rsp: 0,
    })
}
