#![no_std]
#![feature(lang_items)]
#![feature(ptr_internals)]
#![feature(custom_test_frameworks)]
#![feature(allocator_api)]
#![feature(strict_provenance)]
#![feature(abi_x86_interrupt)]
#![feature(naked_functions)]

extern crate alloc;
extern crate multiboot2;
extern crate rlibc;
extern crate spinning_top;

#[macro_use]
extern crate bitflags;

#[macro_use]
extern crate lazy_static;

mod ahci;
mod arch;
mod elf;
mod fs;
mod io;
mod mem;
mod scheduling;
mod syscall;
mod test;
mod utils;

use ahci::init_ahci;
use alloc::boxed::Box;
use arch::x86_64::acpi::pci::PciDeviceClass;
use core::panic::PanicInfo;
use fs::GLOBAL_FILESYSTEM;
use io::serial::serial_init;
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
            print!(
                "[ OK ] Successfully loaded boot info at addr: 0x{:x}\n",
                boot_info_addr
            );
            info
        }
        Err(e) => {
            print!(
                "Couldn't load boot info at addr: 0x{:x}\nErr: {:?}\n",
                boot_info_addr, e
            );
            return;
        }
    };

    serial_init();

    enable_nxe_bit();
    enable_write_protect_bit();

    mem::init(&boot_info);

    unsafe {
        heap::init_heap();
    }

    print!("[ OK ] Initialized kernel heap...\n");

    arch::x86_64::gdt::init_gdt();
    print!("[ OK ] Initialized kernel GDT\n");

    x86_64::instructions::interrupts::disable();

    arch::x86_64::idt::remap_pic();
    arch::x86_64::idt::load_idt();
    arch::x86_64::pit::init_pit();

    x86_64::instructions::interrupts::enable();
    print!("[ OK ] Initialized IDT\n");

    unsafe {
        core::arch::asm!("int 0x34");
    }

    print!("[ OK ] Returned from interrupt\n");

    let devices = arch::x86_64::acpi::init_acpi(&boot_info);
    let sata_controller = devices.get_device(PciDeviceClass::SATAController).unwrap();

    let mut ports = init_ahci(sata_controller);

    let port = ports.remove(0);
    let port = Box::new(port);
    fs::init(port);

    let shell_binary = {
        let mut fs = GLOBAL_FILESYSTEM.lock();
        let fs = fs.as_mut().unwrap();

        print!("\n");
        for entry in &fs.root_dir.clone() {
            let name = entry.get_filename();
            print!(
                "[ OK ] Root dir entry: {}, dir: {}\n",
                name,
                entry.is_directory()
            );

            if entry.is_directory() {
                let cluster = entry.get_cluster();
                let subfiles = fs.read_directory(cluster);

                for file in subfiles {
                    let name = file.get_filename();
                    print!("           Subfile: {}\n", name);
                }
            }
        }

        // load shell ELF binary :D
        let bin_entry = fs.get_file_in_root("shell.elf").unwrap();
        let elf_binary = fs.read_file(&bin_entry).unwrap();

        elf_binary.clone()
    };

    print!("[ OK ] Read Shell ELF binary\n");

    let shell_entry = elf::load(shell_binary).unwrap();
    scheduling::deploy(shell_entry);

    scheduling::enable();
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
    let msg = info.message();

    print!("PANIC on line {:?} in {:?}, {:?}\n\n\n", line, file, msg);
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
