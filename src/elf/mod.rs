use crate::{
    io::LogType,
    log,
    mem::{
        paging::{entry::EntryFlags, Page},
        MemoryController, Region, Stack, GLOBAL_MEMORY_CONTROLLER,
    },
    scheduling::process::ProcessEntry,
};
use alloc::sync::Arc;
use spin::Mutex;

mod loader;

const USER_STACK_PAGES: usize = 32;

bitflags! {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct ElfProgramHeaderFlags: u32 {
        const NONE = 0;
        const PF_X = 1;
        const PF_W = 1 << 1;
        const PF_R = 1 << 2;
    }
}

impl ElfProgramHeaderFlags {
    pub fn to_entry_flags(self) -> EntryFlags {
        let mut flags = EntryFlags::empty();

        if !self.is_empty() {
            flags |= EntryFlags::RING3_ACCESSIBLE;
        }

        if self.contains(ElfProgramHeaderFlags::PF_W) {
            flags |= EntryFlags::WRITABLE;
        }

        if !self.contains(ElfProgramHeaderFlags::PF_X) {
            flags |= EntryFlags::NO_EXECUTE;
        }

        flags
    }
}

/// Linked List of ELF Mapped Memory Regions
#[derive(Clone)]
pub struct ElfRegion {
    pub region: Region,
    pub next: Option<Arc<Mutex<ElfRegion>>>,
    pub flags: ElfProgramHeaderFlags,

    /// A buffer to copy the ELF sections from when loading
    /// into virtual memory. Only useful for the ELF loader.
    pub origin_buffer: Region,
}

pub struct ElfRegionIterator {
    current: Option<Arc<Mutex<ElfRegion>>>,
}

impl ElfRegionIterator {
    pub fn from(start: Arc<Mutex<ElfRegion>>) -> Self {
        Self {
            current: Some(start),
        }
    }
}

impl Iterator for ElfRegionIterator {
    type Item = Arc<Mutex<ElfRegion>>;

    fn next(&mut self) -> Option<Self::Item> {
        let curr = self.current.clone();

        self.current = match curr.clone() {
            Some(c) => {
                let c = c.lock();
                c.next.clone()
            }
            None => None,
        };

        curr
    }
}

impl ElfRegion {
    pub fn new(
        region: Region,
        next: Option<Arc<Mutex<ElfRegion>>>,
        origin_buffer: Region,
        flags: ElfProgramHeaderFlags,
    ) -> Self {
        ElfRegion {
            region: region,
            next: next,
            flags: flags,
            origin_buffer: origin_buffer,
        }
    }
}

pub fn unmap(start_region: &Arc<Mutex<ElfRegion>>) {
    let mut mem_controller = GLOBAL_MEMORY_CONTROLLER.lock();
    let Some(mem_controller) = mem_controller.as_mut() else {
        log!(
            LogType::ERR,
            "elf_unmap: memory controller is not initialized"
        );
        return;
    };

    unmap_regions_from_active_table(mem_controller, start_region);
}

fn unmap_regions_from_active_table(
    mem_controller: &mut MemoryController,
    start_region: &Arc<Mutex<ElfRegion>>,
) {
    let iter = ElfRegionIterator::from(start_region.clone());
    for region in iter {
        let region = region.lock();
        if region.region.size == 0 {
            continue;
        }

        unsafe {
            let ptr = region.region.get_ptr::<u8>();
            core::ptr::write_bytes(ptr, 0, region.region.size);
        }

        let start_page = Page::for_address(region.region.addr);
        let end_page = Page::for_address((region.region.addr + region.region.size - 1) as usize);

        mem_controller.unmap(start_page, end_page);
    }
}

/// Loads an ELF binary into a fresh process address space.
///
/// ## Arguments
///
/// - `elf` the raw ELF file contents
/// - `argv` the process arguments, starting with the program name
///
/// ## Returns
/// A process entry ready to be deployed.
pub fn load(elf: Region, argv: &[&str]) -> Option<ProcessEntry> {
    let Some(mut entry) = loader::load(elf) else {
        log!(LogType::ERR, "elf_load: loader::load failed");
        return None;
    };

    let start_region = entry.start_region.clone();

    let mut mc = GLOBAL_MEMORY_CONTROLLER.lock();
    let Some(mc) = mc.as_mut() else {
        log!(
            LogType::ERR,
            "elf_load: memory controller is not initialized"
        );

        return None;
    };

    let Some(mut ring3_table) = mc.clone_kernel_table() else {
        log!(LogType::ERR, "elf_load: failed to clone kernel page table");
        return None;
    };

    let Some(prev_table) = mc.switch_table(&ring3_table) else {
        log!(
            LogType::ERR,
            "elf_load: failed to switch to new ring3 table 0x{:X}",
            ring3_table.addr
        );

        return None;
    };

    let iter = ElfRegionIterator::from(start_region.clone());
    for region in iter {
        let region = region.lock();

        let addr = region.region.addr;
        let size = region.region.size;

        let start_page = Page::for_address(addr);
        let end_page = Page::for_address(addr + size - 1);

        let flags = region.flags.to_entry_flags();
        ring3_table.map_range(
            start_page,
            end_page,
            flags,
            &mut mc.frame_allocator,
            &mut mc.slot_allocator,
            &mut mc.temp_mapper,
        );

        // inspect mapped pages
        /*
        print!("\n");

        for page in Page::range(start_page, end_page) {
            ring3_table.inspect_page(page, &mut mc.temp_mapper);
            print!("\n");
        }
        */
    }

    // load elf regions
    let iter = ElfRegionIterator::from(start_region.clone());
    for region in iter {
        let region = region.lock();

        // load entry into memory
        let ph_file_src = region.origin_buffer.addr as *mut u8;
        let destination_ptr = region.region.addr as *mut u8;

        let ph_file_size = region.origin_buffer.size;
        let size = region.region.size;

        if ph_file_size > size {
            log!(
                LogType::ERR,
                "elf_load: refusing copy where file bytes exceed region size, dst: 0x{:X}, file_size: 0x{:X}, region_size: 0x{:X}",
                region.region.addr,
                ph_file_size,
                size
            );

            unmap_regions_from_active_table(mc, &start_region);
            mc.switch_table(&prev_table);
            return None;
        }

        unsafe {
            core::ptr::copy_nonoverlapping(ph_file_src, destination_ptr, ph_file_size);
        }

        // check if BSS exists
        let bss_size = (size as i64) - (ph_file_size as i64);
        if bss_size > 0 {
            // zero BSS
            let bss_ptr = unsafe { destination_ptr.add(ph_file_size as usize) };
            unsafe { core::ptr::write_bytes(bss_ptr, 0, bss_size as usize) };
        }
    }

    // allocate stack
    let Some(stack) = mc.stack_allocator.alloc(
        &mut ring3_table,
        &mut mc.frame_allocator,
        &mut mc.slot_allocator,
        &mut mc.temp_mapper,
        USER_STACK_PAGES,
        EntryFlags::WRITABLE | EntryFlags::RING3_ACCESSIBLE,
    ) else {
        log!(LogType::ERR, "elf_load: failed to allocate user stack");
        unmap_regions_from_active_table(mc, &start_region);
        mc.switch_table(&prev_table);
        return None;
    };

    // write the argument frame while the process page table
    // is still active
    let Some(initial_rsp) = write_args_frame(&stack, argv) else {
        log!(LogType::ERR, "elf_load: failed to write argument frame");
        mc.free_stack(&stack);
        unmap_regions_from_active_table(mc, &start_region);
        mc.switch_table(&prev_table);
        return None;
    };

    // Switch back to root table
    if mc.switch_table(&prev_table).is_none() {
        log!(
            LogType::ERR,
            "elf_load: failed to switch back to previous table 0x{:X}",
            prev_table.addr
        );

        return None;
    }

    entry.ring3_page_table = Some(ring3_table);
    entry.stack = Some(stack);
    entry.initial_rsp = initial_rsp;

    Some(entry)
}

/// Writes a System V style argument frame onto a fresh user stack.
///
/// The argument strings are copied NUL-terminated to the very top of the
/// stack, and below them sits the pointer frame the process starts with:
///
/// ```text
/// rsp -> [argc][argv[0]]..[argv[argc - 1]][NULL][NULL] .. [strings]
/// ```
///
/// The final `NULL` terminates the (empty) environment list. The initial
/// stack pointer is 16-byte aligned and points at `argc`, matching what a
/// standard libc `_start` expects.
///
/// Must be called while the process page table is active.
///
/// ## Arguments
///
/// - `stack` the user stack to write into
/// - `argv` the process arguments, starting with the program name
///
/// ## Returns
/// The initial user stack pointer, or `None` when the frame does not fit
/// into the stack.
fn write_args_frame(stack: &Stack, argv: &[&str]) -> Option<usize> {
    let string_bytes: usize = argv.iter().map(|arg| arg.len() + 1).sum();
    let strings_base = stack.top.checked_sub(string_bytes)?;

    // argc + argv pointers + argv NULL terminator + envp NULL terminator
    let frame_words = argv.len().checked_add(3)?;
    let frame_size = frame_words.checked_mul(core::mem::size_of::<usize>())?;
    let frame_base = strings_base.checked_sub(frame_size)? & !0xF;

    if frame_base < stack.bottom {
        return None;
    }

    let frame = frame_base as *mut usize;
    let mut string_addr = strings_base;

    unsafe {
        *frame = argv.len();

        for (index, arg) in argv.iter().enumerate() {
            *frame.add(1 + index) = string_addr;

            core::ptr::copy_nonoverlapping(arg.as_ptr(), string_addr as *mut u8, arg.len());
            *((string_addr + arg.len()) as *mut u8) = 0;
            string_addr += arg.len() + 1;
        }

        *frame.add(1 + argv.len()) = 0;
        *frame.add(2 + argv.len()) = 0;
    }

    Some(frame_base)
}
