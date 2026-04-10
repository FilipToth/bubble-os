use alloc::sync::Arc;
use spin::Mutex;
use x86_64::structures::paging::page;

use crate::{
    mem::{
        paging::{entry::EntryFlags, Page},
        Region, GLOBAL_MEMORY_CONTROLLER,
    },
    print,
    scheduling::process::ProcessEntry,
};

mod loader;

/// Linked List of ELF Mapped Memory Regions
#[derive(Clone)]
pub struct ElfRegion {
    pub region: Region,
    pub next: Option<Arc<Mutex<ElfRegion>>>,

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
    pub fn new(region: Region, next: Option<Arc<Mutex<ElfRegion>>>, origin_buffer: Region) -> Self {
        ElfRegion {
            region: region,
            next: next,
            origin_buffer: origin_buffer,
        }
    }
}

pub fn unmap(start_region: &Arc<Mutex<ElfRegion>>) {
    let mut mem_controller = GLOBAL_MEMORY_CONTROLLER.lock();
    let mem_controller = mem_controller.as_mut().unwrap();

    let iter = ElfRegionIterator::from(start_region.clone());
    for region in iter {
        let region = region.lock();
        let ptr = region.region.get_ptr::<u8>();
        unsafe { core::ptr::write_bytes(ptr, 0, region.region.size) };

        let start_page = Page::for_address(region.region.addr);
        let end_page = Page::for_address((region.region.addr + region.region.size - 1) as usize);

        mem_controller.unmap(start_page, end_page);
    }
}

pub fn load(elf: Region) -> Option<ProcessEntry> {
    let mut entry = loader::load(elf)?;
    let start_region = entry.start_region.clone();

    let mut mc = GLOBAL_MEMORY_CONTROLLER.lock();
    let mc = mc.as_mut().unwrap();

    let mut ring3_table = mc.clone_kernel_table()?;
    let Some(prev_table) = mc.switch_table(&ring3_table) else {
        panic!("Cannot switch to new page table clone on ELF load!\n");
    };

    let iter = ElfRegionIterator::from(start_region.clone());
    for region in iter {
        let region = region.lock();

        let addr = region.region.addr;
        let size = region.region.size;

        let start_page = Page::for_address(addr);
        let end_page = Page::for_address(addr + size);

        let flags = EntryFlags::WRITABLE | EntryFlags::RING3_ACCESSIBLE;
        ring3_table.map_range(
            start_page,
            end_page,
            flags,
            &mut mc.frame_allocator,
            &mut mc.slot_allocator,
            &mut mc.temp_mapper,
        );

        // inspect mapped pages
        print!("\n");

        for page in Page::range(start_page, end_page) {
            ring3_table.inspect_page(page, &mut mc.temp_mapper);
            print!("\n");
        }
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
    let stack = mc.stack_allocator.alloc(
        &mut ring3_table,
        &mut mc.frame_allocator,
        &mut mc.slot_allocator,
        &mut mc.temp_mapper,
        10,
        EntryFlags::WRITABLE | EntryFlags::RING3_ACCESSIBLE,
    )?;

    // Switch back to root table
    mc.switch_table(&prev_table);

    entry.ring3_page_table = Some(ring3_table);
    entry.stack = Some(stack);

    Some(entry)
}
