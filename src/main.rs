#![no_std]
#![no_main]

mod core_requirements;
mod efi;
mod print;

use core::panic::PanicInfo;
use efi::{EfiHandle, EfiSystemTable};

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

#[no_mangle]
extern "C" fn efi_main(_image_handle: EfiHandle, system_table: *mut EfiSystemTable) {
    // register efi system table
    unsafe {
        efi::register_efi_system_table(system_table);
    }

    print!("Hello, World?\n");
    loop {}
}
