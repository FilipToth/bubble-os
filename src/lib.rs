#![no_std]
#![feature(lang_items)]
#![feature(ptr_internals)]

extern crate rlibc;
extern crate multiboot2;

#[macro_use]
extern crate bitflags;

mod io;
mod mem;

use core::{panic::PanicInfo};

use crate::io::print;
use crate::mem::{SimplePageFrameAllocator, PageFrameAllocator};

#[no_mangle]
pub extern fn rust_main(boot_info_addr: usize) {
    let boot_info_load_res = unsafe { multiboot2::BootInformation::load(boot_info_addr as *const multiboot2::BootInformationHeader) };
    let boot_info = match boot_info_load_res {
        Ok(info) => {
            print!("[ OK ] Boot info successfully loaded!\n");
            info
        },
        Err(e) => {
            print!("Couldn't load boot info at addr: {:x}\nErr: {:?}\n", boot_info_addr, e);
            return;
        }
    };

    let map_tag = boot_info.memory_map_tag().unwrap();
    for mem_area in map_tag.memory_areas() {
        print!("    Memory Area: start: 0x{:x}, len: 0x{:x}\n", mem_area.start_address(), mem_area.size());
    }

    print!("\n[ OK ] Kernel Init Done, Entering Rust 64-Bit Mode\n");

    let mut count = 0;
    let elf_sections = boot_info.elf_sections().unwrap();
    for section in elf_sections {
        let addr = section.start_address();
        let length = section.size();
        let flags = section.flags().bits();

        print!("    ELF Section at 0x{:x}, with length 0x{:x} and flags 0x{:x}\n", addr, length, flags);
        count += 1;
    }

    print!("\n[ OK ] ELF Section Count: {:}\n", count);

    let kernel_start = boot_info.elf_sections()
                                .unwrap()
                                .map(|s| s.start_address())
                                .min()
                                .unwrap();
    
    let kernel_end = boot_info.elf_sections()
                              .unwrap()
                              .map(|s| s.start_address() + s.size())
                              .max()
                              .unwrap();

    let multiboot_start = boot_info_addr as u64;
    let multiboot_end = (boot_info_addr + boot_info.total_size()) as u64;

    print!("[ OK ] Identified kernel at start: 0x{:x} end: 0x{:x}\n", kernel_start, kernel_end);
    print!("[ OK ] Identified multiboot info at start: 0x{:x} end: 0x{:x}\n", multiboot_start, multiboot_end);

    // memory

    // for some reason when getting the last memory area,
    // it's always padded to 4GB, the second last area
    // actually corresponds to the memory available
    let mem_areas = map_tag.memory_areas();
    let memory_end = mem_areas[mem_areas.len() - 2].end_address();
    
    print!("[ OK ] Memory end: 0x{:x}\n", memory_end);

    let mut allocator = SimplePageFrameAllocator::new(multiboot_end as usize, memory_end as usize);
    for i in 0.. {
        let alloc_res = allocator.falloc();
        match alloc_res {
            Some(_) => continue,
            None => {
                print!("[ OK ] Allocated {} page frames to fill memory map", i + 1);
                break;
            }
        }
    }

    loop {};
}

#[no_mangle]
#[lang = "eh_personality"]
pub extern fn eh_personality() {}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let location = info.location().unwrap();
    let file = location.file();
    let line = location.line() + 1;

    print!("PANIC on line {:?} in {:?}", line, file);
    loop {}
}
