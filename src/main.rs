#![no_std]
#![no_main]

#[macro_use]
extern crate lazy_static;

mod core_requirements;
mod efi;
mod io;
mod print;
mod serial;
mod gdt;

use core::panic::PanicInfo;
use efi::{EfiHandle, EfiSystemTable};
use core::arch::asm;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    print!("\n\nPanic!\n");
    loop {}
}

#[no_mangle]
extern "C" fn efi_main(image_handle: EfiHandle, system_table: *mut EfiSystemTable) {
    serial::serial_init();

    // register efi system table
    unsafe {
        efi::register_efi_system_table(system_table);
    }

    // enter long mode

    // disable interrupts
    unsafe {
        // asm: cli
        asm!("cli");
    }

    // gdt
    gdt::load_gdt();
    print!("GDT loaded...\n");

    // renenable interrups
    unsafe {
        asm!("sti");
    }

    // interrupts

    // memory handling

    // acpi stuff

    // get acpi table before exiting boot services
    let acpi = efi::get_acpi_table().unwrap();

    let memory = efi::get_memory_descriptor().unwrap();
    efi::exit_boot_servies(image_handle, memory.map_key);

    print!("Exited boot services! - serial\n");
    print!("free memory: {} bytes\n", memory.free_memory);

    loop {}
}
