#![no_std]
#![feature(lang_items)]

extern crate rlibc;

mod io;

use core::{panic::PanicInfo};

use io::print;

#[no_mangle]
pub extern fn rust_main(boot_info_addr: usize) {
    print!("Hello, rust!");
    loop {};
}

#[no_mangle]
#[lang = "eh_personality"]
pub extern fn eh_personality() {}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    loop {};
}
