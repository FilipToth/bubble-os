use crate::{
    mem::{paging::Page, Region, GLOBAL_MEMORY_CONTROLLER},
    scheduling::process::ProcessEntry,
};

mod loader;

pub fn unmap(elf: Region) {
    let mut mem_controller = GLOBAL_MEMORY_CONTROLLER.lock();
    let mem_controller = mem_controller.as_mut().unwrap();

    unsafe { core::ptr::write_bytes(elf.ptr, 0, elf.size) };

    let addr = elf.ptr as usize;
    let start_page = Page::for_address(addr);
    let end_page = Page::for_address((addr + elf.size - 1) as usize);

    mem_controller.unmap(start_page, end_page);
}

pub fn load(elf: Region) -> Option<ProcessEntry> {
    loader::load(elf)
}
