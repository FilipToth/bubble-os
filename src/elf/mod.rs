use alloc::boxed::Box;

use crate::{
    mem::{paging::Page, Region, GLOBAL_MEMORY_CONTROLLER},
    scheduling::process::ProcessEntry,
};

mod loader;

/// Linked List of ELF Mapped Memory Regions
#[derive(Clone)]
pub struct ElfRegion {
    region: Region,
    next: Option<Box<ElfRegion>>,
}

impl ElfRegion {
    pub fn new(region: Region, next: Option<Box<ElfRegion>>) -> Self {
        ElfRegion {
            region: region,
            next: next,
        }
    }
}

pub fn unmap(start_region: &Box<ElfRegion>) {
    let mut mem_controller = GLOBAL_MEMORY_CONTROLLER.lock();
    let mem_controller = mem_controller.as_mut().unwrap();

    let mut current_region = Some(start_region);
    while let Some(region) = &current_region {
        let ptr = region.region.get_ptr::<u8>();
        unsafe { core::ptr::write_bytes(ptr, 0, region.region.size) };

        let start_page = Page::for_address(region.region.addr);
        let end_page = Page::for_address((region.region.addr + region.region.size - 1) as usize);

        mem_controller.unmap(start_page, end_page);
        current_region = region.next.as_ref();
    }
}

pub fn load(elf: Region) -> Option<ProcessEntry> {
    loader::load(elf)
}
