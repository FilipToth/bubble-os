use spin::Mutex;

use crate::{arch::x86_64::acpi::pci::PciDevice, net::e1000::E1000Driver};

pub mod dma_ptr;
mod e1000;

pub static ETH_DRIVER: Mutex<Option<E1000Driver>> = Mutex::new(None);

pub fn init(eth: &PciDevice) -> E1000Driver {
    E1000Driver::new(eth)
}

pub fn load(eth: E1000Driver) {
    *ETH_DRIVER.lock() = Some(eth);
}