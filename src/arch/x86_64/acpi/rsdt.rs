use crate::print;

use super::{complies_table_checksum, AcpiSDTHeader};

pub struct Rsdt {
    pub mcfg: Option<&'static AcpiSDTHeader>
}

pub fn parse_rsdt(address: usize) -> Rsdt {
    let rsdt = unsafe { &*(address as *const AcpiSDTHeader) };
    let rsdt_ptr = rsdt as *const AcpiSDTHeader as *const u8;

    // checksum
    let slice = unsafe { core::slice::from_raw_parts(rsdt_ptr, rsdt.length as usize) };
    if !complies_table_checksum(slice) {
        print!("[ ERR ] RSDT doesn't match checksum!\n");
        unreachable!()
    }

    // create pointers
    let length = rsdt.length as usize;
    let rsdt_size = core::mem::size_of::<AcpiSDTHeader>();
    let ptr_size = core::mem::size_of::<u32>();
    let num_entries = (length - rsdt_size) / ptr_size;

    let mut mcfg: Option<&'static AcpiSDTHeader> = None;

    let mut curr_addr = address + rsdt_size as usize;
    for _ in 0..num_entries {
        // they're u32 pointers :D
        let ptr = unsafe { &*(curr_addr as *const u32) };
        let header = unsafe { &*(*ptr as *const AcpiSDTHeader) };

        let signature = core::str::from_utf8(&header.signature).unwrap();
        print!("[ OK ] Found RSDT Table with Signature: {}, rev: {}\n", signature, header.revision);

        match signature {
            "MCFG" => {
                mcfg = Some(header);
            },
            _ => {}
        }

        curr_addr += ptr_size;
    }

    Rsdt { mcfg }
}
