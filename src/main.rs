#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

#[macro_use]
extern crate lazy_static;

mod core_requirements;
mod efi;
mod gdt;
mod interrupts;
mod io;
mod print;
mod serial;

use core::arch::asm;
use core::panic::PanicInfo;
use efi::{EfiHandle, EfiSystemTable};

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    print!("\n\nPanic!\n");

    let payload = info.payload().downcast_ref::<&str>();
    if let Some(msg) = payload {
        print!("Payload info: {}", msg);
    } else {
        print!("No additional panic payload\n");
    }

    loop {
        unsafe {
            asm!("hlt");
        }
    }
}

#[no_mangle]
extern "C" fn efi_main(image_handle: EfiHandle, system_table: *mut EfiSystemTable) {
    serial::serial_init();

    // register efi system table
    unsafe {
        efi::register_efi_system_table(system_table);
    }

    // enter long mode

    unsafe {
        // disable interrupts
        asm!("cli");

        gdt::load_gdt();
        asm!("sti");

        print!("GDT loaded...\n");
        interrupts::init_interrupts();

        // reenable interrupts
        print!("Initialized idt, ret from func\n");

        // for some reason we crash when we do sti...
        asm!("sti");
        print!("reenabled interrupts\n");

        // call test interrupt
        asm!("int 0x34");
    }

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
