use crate::{arch::x86_64::acpi::pci::PciDevice, net::e1000::E1000Driver};

mod e1000;

pub fn init(eth: &PciDevice) -> E1000Driver {
    E1000Driver::new(eth)
}
