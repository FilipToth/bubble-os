#![no_std]
#![feature(lang_items)]
#![feature(ptr_internals)]
#![feature(custom_test_frameworks)]

extern crate multiboot2;
extern crate rlibc;

#[macro_use]
extern crate bitflags;

mod io;
mod mem;
mod test;

use core::panic::PanicInfo;

use crate::io::print;
use crate::mem::paging::remap_kernel;
use crate::mem::{PageFrameAllocator, SimplePageFrameAllocator};

#[no_mangle]
pub extern "C" fn rust_main(boot_info_addr: usize) {
    let boot_info_load_res = unsafe {
        multiboot2::BootInformation::load(
            boot_info_addr as *const multiboot2::BootInformationHeader,
        )
    };
    let boot_info = match boot_info_load_res {
        Ok(info) => {
            print!("[ OK ] Boot info successfully loaded!\n");
            info
        }
        Err(e) => {
            print!(
                "Couldn't load boot info at addr: {:x}\nErr: {:?}\n",
                boot_info_addr, e
            );
            return;
        }
    };

    let map_tag = boot_info.memory_map_tag().unwrap();
    print!("\n[ OK ] Kernel Init Done, Entering Rust 64-Bit Mode\n");

    let elf_sections = boot_info.elf_sections().unwrap();
    let kernel_start = elf_sections
        .clone()
        .map(|s| s.start_address())
        .min()
        .unwrap();
    let kernel_end = elf_sections
        .clone()
        .map(|s| s.start_address() + s.size())
        .max()
        .unwrap();
    let count = elf_sections.count();
    // print!("[ OK ] elf: {:#?}\n", count);

    let multiboot_start = boot_info_addr;
    let multiboot_end = multiboot_start + (boot_info.total_size() as usize);

    print!(
        "[ OK ] Identified kernel at start: 0x{:x} end: 0x{:x}\n",
        kernel_start, kernel_end
    );
    print!(
        "[ OK ] Identified multiboot info at start: 0x{:x} end: 0x{:x}\n",
        multiboot_start, multiboot_end
    );

    // memory

    // for some reason when getting the last memory area,
    // it's always padded to 4GB, the second last area
    // actually corresponds to the memory available

    let mem_areas = map_tag.memory_areas();
    let memory_end = mem_areas[mem_areas.len() - 2].end_address();

    print!("[ OK ] Memory end: 0x{:x}\n", memory_end);

    let mut allocator = SimplePageFrameAllocator::new(multiboot_end as usize, memory_end as usize);

    // for some reason I have to allocate
    // and empty page here or else it
    // panics and faults

    let _ = allocator.falloc().unwrap();
    remap_kernel(&mut allocator, &boot_info);

    print!("[ OK ] RAN KERNEL REMAP\n");

    loop {}
}

#[no_mangle]
pub extern "C" fn rust_main_test(boot_info_addr: usize) {
    test::run_tests(boot_info_addr);
    loop {}
}

#[no_mangle]
#[lang = "eh_personality"]
pub extern "C" fn eh_personality() {}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let location = info.location().unwrap();
    let file = location.file();
    let line = location.line() + 1;

    print!("PANIC on line {:?} in {:?}\n\n\n", line, file);
    loop {}
}
