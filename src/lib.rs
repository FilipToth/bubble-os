#![no_std]
#![feature(lang_items)]
#![feature(ptr_internals)]
#![feature(custom_test_frameworks)]
#![feature(allocator_api)]
#![feature(global_allocator)]

extern crate multiboot2;
extern crate rlibc;
extern crate spinning_top;
extern crate alloc;

#[macro_use]
extern crate bitflags;

mod io;
mod mem;
mod test;

use core::alloc::Layout;
use core::panic::PanicInfo;

use alloc::boxed::Box;
use alloc::vec;
use mem::heap::LockedHeapAllocator;
use x86_64::registers::control::{Cr0, Cr0Flags};
use x86_64::registers::model_specific::{Efer, EferFlags};

use crate::io::print;
use crate::mem::heap;

#[global_allocator]
static mut HEAP_ALLOCATOR: LockedHeapAllocator = LockedHeapAllocator::empty();

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

    print!("[ OK ] Initialized kernel heap...\n\n\n");

    let simple_layout = Layout::new::<u8>();
    let ptr = unsafe { alloc::alloc::alloc(simple_layout) };
    // unsafe { ptr.write(3 as u8 ) }

/*     let v = vec![4, 3, 2, 1];
    print!("[ OK ] first heap test: {:?}\n", v[0]); */

    // let mut heap_test = Box::new(20);
    // *heap_test -= 10;
    // print!("[ OK ] Ran second heap test: {:?}\n", *heap_test);


/*     let l = Layout::new::<u8>();
    let ptr = unsafe { alloc::alloc::alloc(l) }; */
    // unsafe { ptr.write(5 as u8); };

/*     print!("[ OK ] addr: {:?}\n", ptr as usize); */

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
