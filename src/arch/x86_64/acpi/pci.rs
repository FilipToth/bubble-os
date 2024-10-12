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

fn enumerate_function(dev_addr: usize, function: usize, device: usize, bus: u8) {
    let offset = (function as usize) << 12;
    let func_addr = dev_addr + offset;
    acpi_mapping(func_addr, PAGE_SIZE);

    let header = unsafe { &*(func_addr as *const PciDeviceHeader) };
    if header.device_id == 0xFFFF {
        // function not present
        return;
    }

    print!(
        "[ PCI ] function (f: {}, d: {}, b: {}), vendor: 0x{:X}, dev_id: 0x{:X}\n",
        function, device, bus, header.vendor_id, header.device_id
    );
}

fn enumerate_device(bus_addr: usize, device: usize, bus: u8) {
    let offset = (device as usize) << 15;
    let dev_addr = bus_addr + offset;
    acpi_mapping(dev_addr, PAGE_SIZE);

    let header = unsafe { &*(dev_addr as *const PciDeviceHeader) };
    if header.device_id == 0xFFFF {
        // device not present
        return;
    }

    print!("[ PCI ] device {}, bus {} PRESENT\n", device, bus);
    for function in 0..8 {
        enumerate_function(dev_addr, function, device, bus);
    }
}

fn enumerate_bus(base_addr: usize, bus: u8) {
    let offset = (bus as usize) << 20;
    let bus_addr = base_addr + offset;
    acpi_mapping(bus_addr, PAGE_SIZE);

    let header = unsafe { &*(bus_addr as *const PciDeviceHeader) };
    if header.device_id == 0xFFFF {
        // bus not present
        return;
    }

    print!("[ PCI ] bus {} PRESENT\n", bus);
    for device in 0..32 {
        enumerate_device(bus_addr, device, bus);
    }
}

pub fn enumerate_pci(mcfg: Mcfg) {
    for entry in mcfg.entries {
        let start = entry.bus_start_num;
        let end = entry.bus_end_num;

        for bus in start..end {
            enumerate_bus(entry.pcie_config_addr as usize, bus);
        }
    }
}
