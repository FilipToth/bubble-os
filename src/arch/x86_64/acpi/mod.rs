use core::{borrow::BorrowMut, cell::RefCell, ptr::NonNull};

use acpi::{AcpiHandler, AcpiTables, PhysicalMapping};
use multiboot2::{BootInformation, MaybeDynSized, TagHeader};
use rsdt::{parse_rsdt, AcpiSDTHeader};

use crate::{
    mem::{
        paging::{entry::EntryFlags, Page},
        MemoryController, PageFrame, GLOBAL_MEMORY_CONTROLLER, PAGE_SIZE,
    },
    print,
};

mod rsdt;

pub fn init_acpi(boot_info: &BootInformation) {
    let Some(rsdp) = boot_info.rsdp_v1_tag() else {
        print!("[ ERR ] Cannot find RSDP v1");
        loop {}
    };

    if !rsdp.checksum_is_valid() {
        print!("[ ERR ] Invalid RSDP v2 checksum");
        loop {}
    }

    let rsdt_address = rsdp.rsdt_address();
    acpi_mapping(rsdt_address, PAGE_SIZE);

    let rsdt = parse_rsdt(rsdt_address);
    print!(
        "[ OK ] Got RSDT with length: 0x{:x}, sizeof header: 0x{:x}\n",
        rsdt.length,
        core::mem::size_of::<AcpiSDTHeader>()
    );
}

fn acpi_mapping(physical_address: usize, size: usize) {
    let start_frame = PageFrame::from_address(physical_address);
    let end_frame = PageFrame::from_address(physical_address + size);

    let mut controller = GLOBAL_MEMORY_CONTROLLER.lock();
    let controller = controller.as_mut().unwrap();

    let allocator = &mut controller.frame_allocator;
    let table = &mut controller.active_table;

    let page = Page::for_address(physical_address);
    let unused = table.is_unused(page, allocator);
    if !unused {
        // already mapped
        return;
    }

    // table.map_range_identity(start_frame, end_frame, EntryFlags::WRITABLE, allocator);

    let range = PageFrame::range(start_frame, end_frame);
    for frame in range {
        table.map_identity(frame, EntryFlags::PRESENT, allocator);
    }
}
