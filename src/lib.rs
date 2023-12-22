#![no_std]
#![feature(lang_items)]

use core::{panic::PanicInfo};

#[no_mangle]
pub extern fn rust_main(boot_info_addr: usize) {
    loop {};
}

#[no_mangle]
#[lang = "eh_personality"]
pub extern fn eh_personality() {}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    loop {};
}
