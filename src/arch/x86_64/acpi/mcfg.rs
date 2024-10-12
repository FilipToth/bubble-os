use alloc::vec::Vec;

use crate::print;

use super::{complies_table_checksum, AcpiSDTHeader};

#[repr(C)]
pub struct McfgDeviceEntry {
    pub pcie_config_addr: u64,
    pub pci_segment_group: u16,
    pub bus_start_num: u8,
    pub bus_end_num: u8,
    pub reserved: u32,
}

pub struct Mcfg {
    pub entries: Vec<&'static McfgDeviceEntry>,
}

pub fn parse_mcfg(mcfg: &'static AcpiSDTHeader) -> Mcfg {
    let mcfg_ptr = mcfg as *const AcpiSDTHeader as *const u8;

    // checksum
    let slice = unsafe { core::slice::from_raw_parts(mcfg_ptr, mcfg.length as usize) };
    if !complies_table_checksum(slice) {
        print!("[ ERR ] MCFG doesn't match checksum!\n");
        unreachable!()
    }

    // create pointers, note there're
    // additional 8 reserved bytes
    let length = mcfg.length as usize;
    let mcfg_size = core::mem::size_of::<AcpiSDTHeader>();
    let entry_size = core::mem::size_of::<McfgDeviceEntry>();
    let num_entries = (length - mcfg_size - 8) / entry_size;

    let mut entries: Vec<&'static McfgDeviceEntry> = Vec::new();

    let mut curr_addr = mcfg_ptr as usize + mcfg_size as usize + 8;
    for _ in 0..num_entries {
        let entry = unsafe { &*(curr_addr as *const McfgDeviceEntry) };
        entries.push(&entry);

        curr_addr += entry_size;
    }

    Mcfg { entries: entries }
}
