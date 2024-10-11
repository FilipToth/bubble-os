use alloc::vec::Vec;

use crate::print;

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

struct Rsdt {
    pointers: Vec<usize>
}

pub fn parse_rsdt(address: usize) -> &'static AcpiSDTHeader {
    let rsdt = unsafe { &*(address as *const AcpiSDTHeader) };
    let rsdt_ptr = rsdt as *const AcpiSDTHeader as *const u8;

    // checksum
    let slice = unsafe { core::slice::from_raw_parts(rsdt_ptr, rsdt.length as usize) };

    let mut sum: u8 = 0;
    for &byte in slice {
        sum = sum.wrapping_add(byte);
    }

    if sum != 0 {
        print!("[ ERR ] RSDT doesn't match checksum!\n");
        unreachable!()
    }

    // create pointers
    let length = rsdt.length as usize;
    let rsdt_size = core::mem::size_of::<AcpiSDTHeader>();
    let ptr_size = core::mem::size_of::<u32>();
    let num_entries = (length - rsdt_size) / ptr_size;

    let mut pointers: Vec<usize> = Vec::new();
    let mut curr_addr = address + rsdt_size as usize;

    for _ in 0..num_entries {
        // they're u32 pointers :D
        let ptr = unsafe { &*(curr_addr as *const u32) };
        pointers.push(*ptr as usize);

        curr_addr += ptr_size;
    }

    for pointer in pointers {
        print!("[ OK ] rsdt next table ptr: 0x{:x}\n", pointer);
    }

    return rsdt;
}
