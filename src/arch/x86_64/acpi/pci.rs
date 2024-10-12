use alloc::vec::Vec;

use crate::{mem::PAGE_SIZE, print};

use super::{acpi_mapping, mcfg::Mcfg};

#[repr(C)]
struct PciDeviceHeader {
    vendor_id: u16,
    device_id: u16,
    command: u16,
    status: u16,
    rev_id: u8,
    prog_interface_type: u8,
    subclass: u8,
    dev_class: u8,
    cache_line_size: u8,
    latency_timer: u8,
    header_type: u8,
    bist: u8,
}

#[derive(Debug, PartialEq)]
pub enum PciDeviceClass {
    Unknown,
    NonVGACompatibleUnclassifiedDevice,
    VGACompatibleUnclassifiedDevice,
    SATAController,
    EthernetController,
    VGACompatibleController,
    HostBridge,
    ISABridge,
    SMBusController,
}

pub struct PciDevice {
    pub pci_base_addr: usize,
    pub vendor: u16,
    pub device_class: PciDeviceClass,
}

pub struct PciDevices {
    pub devices: Vec<PciDevice>,
}

impl PciDevices {
    fn new() -> PciDevices {
        PciDevices {
            devices: Vec::new(),
        }
    }

    fn add_device(&mut self, dev: PciDevice) {
        self.devices.push(dev);
    }

    pub fn get_device(&self, class: PciDeviceClass) -> Option<&PciDevice> {
        for device in &self.devices {
            if device.device_class != class {
                continue;
            }

            return Some(device);
        }

        None
    }
}

fn get_device_class(class: u8, subclass: u8) -> PciDeviceClass {
    match class {
        0x00 => match subclass {
            0x00 => PciDeviceClass::NonVGACompatibleUnclassifiedDevice,
            0x01 => PciDeviceClass::VGACompatibleUnclassifiedDevice,
            _ => PciDeviceClass::Unknown,
        },
        0x01 => match subclass {
            0x06 => PciDeviceClass::SATAController,
            _ => PciDeviceClass::Unknown,
        },
        0x02 => match subclass {
            0x00 => PciDeviceClass::EthernetController,
            _ => PciDeviceClass::Unknown,
        },
        0x03 => match subclass {
            0x00 => PciDeviceClass::VGACompatibleController,
            _ => PciDeviceClass::Unknown,
        },
        0x06 => match subclass {
            0x00 => PciDeviceClass::HostBridge,
            0x01 => PciDeviceClass::ISABridge,
            _ => PciDeviceClass::Unknown,
        },
        0x0C => match subclass {
            0x05 => PciDeviceClass::SMBusController,
            _ => PciDeviceClass::Unknown,
        },
        _ => PciDeviceClass::Unknown,
    }
}

fn enumerate_function(dev_addr: usize, function: usize, devices: &mut PciDevices) {
    let offset = (function as usize) << 12;
    let func_addr = dev_addr + offset;
    acpi_mapping(func_addr, PAGE_SIZE);

    let header = unsafe { &*(func_addr as *const PciDeviceHeader) };
    if header.device_id == 0xFFFF {
        // function not present
        return;
    }

    let device_class = get_device_class(header.dev_class, header.subclass);
    print!("[ PCI ] Found {:?}\n", device_class);

    let device = PciDevice {
        pci_base_addr: func_addr,
        vendor: header.vendor_id,
        device_class,
    };

    devices.add_device(device);
}

fn enumerate_device(bus_addr: usize, device: usize, devices: &mut PciDevices) {
    let offset = (device as usize) << 15;
    let dev_addr = bus_addr + offset;
    acpi_mapping(dev_addr, PAGE_SIZE);

    let header = unsafe { &*(dev_addr as *const PciDeviceHeader) };
    if header.device_id == 0xFFFF {
        // device not present
        return;
    }

    for function in 0..8 {
        enumerate_function(dev_addr, function, devices);
    }
}

fn enumerate_bus(base_addr: usize, bus: u8, devices: &mut PciDevices) {
    let offset = (bus as usize) << 20;
    let bus_addr = base_addr + offset;
    acpi_mapping(bus_addr, PAGE_SIZE);

    let header = unsafe { &*(bus_addr as *const PciDeviceHeader) };
    if header.device_id == 0xFFFF {
        // bus not present
        return;
    }

    for device in 0..32 {
        enumerate_device(bus_addr, device, devices);
    }
}

pub fn enumerate_pci(mcfg: Mcfg) -> PciDevices {
    let mut devices = PciDevices::new();
    for entry in mcfg.entries {
        let start = entry.bus_start_num;
        let end = entry.bus_end_num;

        for bus in start..end {
            enumerate_bus(entry.pcie_config_addr as usize, bus, &mut devices);
        }
    }

    devices
}
