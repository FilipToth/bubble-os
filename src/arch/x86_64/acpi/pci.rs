use core::ptr::{addr_of, addr_of_mut, read_volatile, write_volatile};

use alloc::vec::Vec;

use crate::{mem::PAGE_SIZE, print};

use super::{acpi_mapping, mcfg::Mcfg};

#[repr(C)]
pub struct PciDeviceHeader {
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

#[derive(Debug)]
pub enum BarType {
    IO { address: u32 },
    Memory32 { address: u32, prefetchable: bool },
    Memory64 { address: u64, prefetchable: bool },
}

#[repr(C)]
pub struct PciDeviceHeaderType0 {
    pub header: PciDeviceHeader,
    pub bar0: u32,
    pub bar1: u32,
    pub bar2: u32,
    pub bar3: u32,
    pub bar4: u32,
    pub bar5: u32,
    pub cardbus_cis_ptr: u32,
    pub subsystem_vendor_id: u16,
    pub subsystem_id: u16,
    pub expansion_rom_base_addr: u32,
    pub capabilities_ptr: u8,
    pub rsv0: u8,
    pub rsv1: u16,
    pub rsv2: u32,
    pub interrupt_line: u8,
    pub interrupt_pin: u8,
    pub min_grant: u8,
    pub max_latency: u8,
}

impl PciDeviceHeaderType0 {
    pub fn get_bar_type(&self, bar_index: usize) -> Option<BarType> {
        match bar_index {
            0 => Self::parse_bar_type(self.bar0, Some(self.bar1)),
            1 => Self::parse_bar_type(self.bar1, Some(self.bar2)),
            2 => Self::parse_bar_type(self.bar2, Some(self.bar3)),
            3 => Self::parse_bar_type(self.bar3, Some(self.bar4)),
            4 => Self::parse_bar_type(self.bar4, Some(self.bar5)),
            5 => Self::parse_bar_type(self.bar5, None),
            _ => None
        }
    }

    pub fn parse_bar_type(bar: u32, next_bar: Option<u32>) -> Option<BarType> {
        if bar == 0 {
            return None;
        }

        if bar & 0x1 == 1 {
            Some(BarType::IO {
                address: bar & 0xFFFFFFFC,
            })
        } else {
            let mem_type = (bar >> 1) & 0x3;
            let prefetchable = (bar & 0x8) != 0;

            match mem_type {
                0b00 => Some(BarType::Memory32 {
                    address: bar & 0xFFFFFFF0,
                    prefetchable,
                }),
                0b10 => {
                    let next = next_bar?;
                    let addr = ((next as u64) << 32) | ((bar & 0xFFFFFFF0) as u64);
                    Some(BarType::Memory64 {
                        address: addr,
                        prefetchable,
                    })
                }
                _ => None,
            }
        }
    }

    pub unsafe fn probe_bar_size(&mut self, bar_index: usize) -> Option<u64> {
        if bar_index >= 6 {
            return None;
        }

        let command_ptr = addr_of_mut!(self.header.command);
        let bars_ptr = addr_of_mut!(self.bar0) as *mut u32;
        let bar_ptr = bars_ptr.add(bar_index);

        let command_orig = read_volatile(command_ptr);
        write_volatile(command_ptr, command_orig & !0x3);

        let original = read_volatile(bar_ptr);
        if original == 0 {
            write_volatile(command_ptr, command_orig);
            return None;
        }

        if (original & 0x1) != 0 {
            // IO BAR
            write_volatile(bar_ptr, 0xFFFF_FFFF);

            let probed = read_volatile(bar_ptr);
            write_volatile(bar_ptr, original);
            write_volatile(command_ptr, command_orig);

            let mask = probed & 0xFFFF_FFFC;
            if mask == 0 {
                return None;
            }

            let size = (!(mask as u64)).wrapping_add(1);
            return Some(size);
        }

        let mem_type = (original >> 1) & 0x3;
        match mem_type {
            0b00 => {
                // 32-bit MMIO BAR
                write_volatile(bar_ptr, 0xFFFF_FFFF);

                let probed = read_volatile(bar_ptr);
                write_volatile(bar_ptr, original);
                write_volatile(command_ptr, command_orig);
                
                let mask = probed & 0x0000_0000_FFFF_FFF0;
                if mask == 0 {
                    return None;
                }

                let size = (!(mask as u64)).wrapping_add(1);
                Some(size & 0x0000_0000_FFFF_FFFF)
            }
            0b10 => {
                // 64-bit MMIO BAR
                let next_ptr = bars_ptr.add(bar_index + 1);
                let original_hi = read_volatile(next_ptr);

                write_volatile(bar_ptr, 0xFFFF_FFFF);
                write_volatile(next_ptr, 0xFFFF_FFFF);

                let probed_lo = read_volatile(bar_ptr);
                let probed_hi = read_volatile(next_ptr);

                write_volatile(bar_ptr, original);
                write_volatile(next_ptr, original_hi);
                write_volatile(command_ptr, command_orig);

                let probed = ((probed_hi as u64) << 32) | ((probed_lo & 0xFFFF_FFF0) as u64);
                if probed == 0 {
                    return None;
                }

                let size = (!probed).wrapping_add(1);
                Some(size)
            }
            _ => {
                write_volatile(command_ptr, command_orig);
                None
            }
        }
    }

    pub unsafe fn read_bar_raw(&self, bar_index: usize) -> Option<u32> {
        if bar_index >= 6 {
            return None;
        }

        let bars_ptr = addr_of!(self.bar0) as *const u32;
        Some(read_volatile(bars_ptr.add(bar_index)))
    }
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
