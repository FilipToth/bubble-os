#![no_std]
#![feature(lang_items)]
#![feature(ptr_internals)]
#![feature(custom_test_frameworks)]
#![feature(allocator_api)]
#![feature(global_allocator)]
#![feature(const_mut_refs)]

extern crate multiboot2;
extern crate rlibc;
extern crate spinning_top;
extern crate alloc;

#[macro_use]
extern crate bitflags;

mod io;
mod mem;
mod test;
mod utils;

use core::alloc::Layout;
use core::borrow::BorrowMut;
use core::panic::PanicInfo;

use alloc::boxed::Box;
use alloc::vec;
use mem::heap::LinkedListHeap;
use x86_64::registers::control::{Cr0, Cr0Flags};
use x86_64::registers::model_specific::{Efer, EferFlags};

use crate::io::print;
use crate::mem::heap;
use crate::utils::safe;

#[global_allocator]
static mut HEAP_ALLOCATOR: safe::Safe<LinkedListHeap> = safe::Safe::new(LinkedListHeap::empty());

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

    enable_nxe_bit();
    enable_write_protect_bit();

    mem::init(&boot_info);

    unsafe {
        heap::init_heap();
    }

    print!("[ OK ] Initialized kernel heap...\n");

    let mut v = vec![5, 3, 1, 9, 8, 12, 19, 81, 12, 44, 22];
    let mut v = vec![5, 3, 1, 9, 8, 12, 19, 81, 12, 44, 22];
    let mut v = vec![5, 3, 1, 9, 8, 12, 19, 81, 12, 44, 22];
    let mut v = vec![5, 3, 1, 9, 8, 12, 19, 81, 12, 44, 22];

    let l = Layout::new::<u8>();
    let ptr = unsafe { alloc::alloc::alloc(l) };
    unsafe { ptr.write(5 as u8); };
    print!("[ OK ] p1_test_value: {:?}\n", unsafe { ptr.read() });

    let l2 = Layout::new::<u8>();
    let ptr2 = unsafe { alloc::alloc::alloc(l2) };
    unsafe { ptr2.write(7 as u8); };

    print!("[ OK ] p2_test_addr: 0x{:x}\n", ptr2 as usize);
    let mut v = vec![5, 3, 1, 9, 8, 12, 19, 81, 12, 44, 22];
    print!("[ OK ] p2_test_value: {:?}\n", unsafe { ptr2.read() } );

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

fn enable_nxe_bit() {
    unsafe {
        Efer::update(|efer| *efer |= EferFlags::NO_EXECUTE_ENABLE);
    };
}

fn enable_write_protect_bit() {
    // makes .code and .rodata immutable,
    // write page flags are ignored by the
    // CPU in ring 0.

    let write_protect = Cr0::read() | Cr0Flags::WRITE_PROTECT;
    unsafe {
        Cr0::write(write_protect);
    }
}
