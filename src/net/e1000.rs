use crate::{
    arch::x86_64::acpi::pci::{BarType, PciDevice, PciDeviceHeaderType0},
    print,
};

pub struct E1000Driver {
    bar_type: BarType,
}

impl E1000Driver {
    pub fn new(eth: &PciDevice) -> E1000Driver {
        let addr = eth.pci_base_addr;
        let header = unsafe { &mut *(addr as *mut PciDeviceHeaderType0) };

        let bar0_type = header.get_bar_type(0).unwrap();
        print!("[ ETH ] vendor id: 0x{:X}\n", header.subsystem_vendor_id);
        print!("[ ETH ] bar0: {:#?}\n", bar0_type);

        let bar_size = unsafe { header.probe_bar_size(0) }.unwrap();
        print!("[ ETH ] bar0 size: 0x{:X}\n", bar_size);

        E1000Driver {
            bar_type: bar0_type,
        }
    }
}
