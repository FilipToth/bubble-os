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
extern "C" fn efi_main(image_handle: EfiHandle, system_table: *mut EfiSystemTable) {
    // register efi system table
    unsafe {
        efi::register_efi_system_table(system_table);
    }

    print!("Hello, bubble!\n");
    let memory = efi::get_memory_descriptor().unwrap();
    efi::exit_boot_servies(image_handle, memory.map_key);
    loop {}
}
