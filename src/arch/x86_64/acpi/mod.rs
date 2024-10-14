use mcfg::parse_mcfg;
use multiboot2::BootInformation;
use pci::{enumerate_pci, PciDevices};
use rsdt::parse_rsdt;

use crate::{
    mem::{
        paging::{entry::EntryFlags, Page},
        PageFrame, GLOBAL_MEMORY_CONTROLLER, PAGE_SIZE,
    },
    print,
};

mod mcfg;
pub mod pci;
mod rsdt;

#[repr(C)]
pub struct AcpiSDTHeader {
    pub signature: [u8; 4],
    pub length: u32,
    pub revision: u8,
    pub checksum: u8,
    pub oem_id: [u8; 6],
    pub oem_table_id: [u8; 8],
    pub oem_revision: u32,
    pub creator_id: u32,
    pub creator_revision: u32,
}

pub fn init_acpi(boot_info: &BootInformation) -> PciDevices {
    let Some(rsdp) = boot_info.rsdp_v1_tag() else {
        print!("[ ERR ] Cannot find RSDP v1\n");
        loop {}
    };

    if !rsdp.checksum_is_valid() {
        print!("[ ERR ] Invalid RSDP v2 checksum\n");
        loop {}
    }

    let rsdt_address = rsdp.rsdt_address();
    acpi_mapping(rsdt_address, PAGE_SIZE);

    let rsdt = parse_rsdt(rsdt_address);

    let mcfg = match rsdt.mcfg {
        Some(mcfg) => parse_mcfg(mcfg),
        None => {
            print!("[ ERR ] MCFG Not found\n");
            loop {}
        }
    };

    enumerate_pci(mcfg)
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

pub fn complies_table_checksum(slice: &[u8]) -> bool {
    let mut sum: u8 = 0;
    for &byte in slice {
        sum = sum.wrapping_add(byte);
    }

    sum == 0
}
