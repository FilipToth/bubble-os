use hba::HBAMemory;
use port::probe_ports;

use crate::{
    arch::x86_64::acpi::{
        acpi_mapping,
        pci::{PciDevice, PciDeviceHeaderType0},
    },
    mem::PAGE_SIZE,
    print,
};

mod port;
mod fis;
mod hba;

pub fn init_ahci(controller: &PciDevice) {
    let addr = controller.pci_base_addr;
    let header = unsafe { &*(addr as *const PciDeviceHeaderType0) };

    // TODO: proper memory management
    let hba_mem = unsafe { &*(header.bar5 as *const HBAMemory) };
    acpi_mapping(header.bar5 as usize, PAGE_SIZE);

    let ports = probe_ports(hba_mem);
    print!("[ AHCI ] Found {} SATA Port/s\n", ports.len());
}
