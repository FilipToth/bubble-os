use core::alloc::Layout;

use alloc::alloc::alloc;

use crate::{ahci::port::AHCIPort, fs::fat::{Fat32ExtendedBootSector, FatBootSector}, print};

pub struct FATFileSystem<'a> {
    port: &'a mut AHCIPort
}

impl<'a> FATFileSystem<'a> {
    pub fn new(port: &mut AHCIPort) -> FATFileSystem {
        let mut fs = FATFileSystem {
            port: port
        };

        fs.read_boot_sector();
        fs
    }

    fn read_boot_sector(&mut self) {
        let layout = Layout::array::<u8>(512).unwrap();
        let buffer = unsafe { alloc(layout) };
        unsafe {
            core::ptr::write_bytes(buffer, 0, 512);
        }

        let status = self.port.read(0, 1, buffer);
        if !status {
            print!("[ FS ] Failed to read FAT32 Boot Sector\n");
            return;
        }

        let bs = unsafe { &*(buffer as *const FatBootSector) };
        let bytes_per_sector = bs.bytes_per_sector;

        let bs_size = core::mem::size_of::<FatBootSector>();
        let bs_32 = unsafe { &*(buffer.add(bs_size) as *const Fat32ExtendedBootSector) };
        let version = bs_32.fat_version;

        print!("[ FS ] Bytes per sector: {}\n", bytes_per_sector);
        print!("[ FS ] FAT version: {}\n", version);
    }
}
