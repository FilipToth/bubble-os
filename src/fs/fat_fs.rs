use core::alloc::Layout;

use alloc::{alloc::alloc, string::String};

use crate::{
    ahci::port::AHCIPort,
    fs::fat::{self, Fat32ExtendedBootSector, FatBootSector},
    print,
};

use super::fat::DirectoryEntry;

pub struct FATFileSystem<'a> {
    port: &'a mut AHCIPort,
    fat: &'static mut [u32],
    bs: FatBootSector,
    bs_32: Fat32ExtendedBootSector,
}

impl<'a> FATFileSystem<'a> {
    pub fn new(port: &'a mut AHCIPort) -> Option<FATFileSystem<'a>> {
        let (bs, bs_32) = match read_boot_sector(port) {
            Some(bs) => bs,
            None => return None,
        };

        print!("\n");

        {
            // Scoped mutable borrow
            let alloc_table = match read_fat(port, &bs, &bs_32) {
                Some(f) => f,
                None => panic!(),
            };

            print!("\n");

            let root_cluster = bs_32.root_cluster as usize;
            let mut fs = FATFileSystem {
                port: port,
                fat: alloc_table,
                bs: bs,
                bs_32: bs_32,
            };

            // read root directory
            fs.read_directory(root_cluster);

            Some(fs)
        }
    }

    fn read_directory(&mut self, dir_cluster: usize) {
        print!("root cluster: {}\n", dir_cluster);
        let first_data_sector = self.bs.reserved_sector_count as usize
            + (self.bs.table_count as usize * self.bs_32.table_size as usize);

        let dir_sector =
            first_data_sector + (dir_cluster - 2) * self.bs.sectors_per_cluster as usize;

        let num_sectors = self.bs.sectors_per_cluster as usize;
        let buffer_size = (self.bs.bytes_per_sector as usize) * num_sectors;
        let layout = Layout::array::<u8>(buffer_size).unwrap();

        let buffer = unsafe { alloc(layout) };
        if buffer.is_null() {
            panic!()
        }

        unsafe { core::ptr::write_bytes(buffer, 0, buffer_size) };

        let status = self.port.read(dir_sector, num_sectors, buffer);
        if !status {
            print!("[ FS ] ERROR: Cannot read directory entries!\n");
            return;
        }

        let num_entries = buffer_size / core::mem::size_of::<DirectoryEntry>();
        let entries =
            unsafe { core::slice::from_raw_parts_mut(buffer as *mut DirectoryEntry, num_entries) };

        print!("[ FS ] Found {} root dir entries\n", entries.len());

        for entry in entries {
            let name = String::from_utf8_lossy(&entry.name);
            print!(
                "[ FS ] First dir entry name: {}, attr: {}\n",
                name, entry.attributes
            );
        }
    }
}

fn read_boot_sector(port: &mut AHCIPort) -> Option<(FatBootSector, Fat32ExtendedBootSector)> {
    let layout = Layout::array::<u8>(512).unwrap();
    let buffer = unsafe { alloc(layout) };

    if buffer.is_null() {
        return None;
    }

    unsafe {
        core::ptr::write_bytes(buffer, 0, 512);
    }

    let status = port.read(0, 1, buffer);
    if !status {
        print!("[ FS ] Failed to read FAT32 Boot Sector\n");
        None
    } else {
        let bs = unsafe { &*(buffer as *const FatBootSector) };
        let bytes_per_sector = bs.bytes_per_sector;

        let bs_size = core::mem::size_of::<FatBootSector>();
        let bs_32 = unsafe { &*(buffer.add(bs_size) as *const Fat32ExtendedBootSector) };
        let version = bs_32.fat_version;

        print!("[ FS ] Bytes per sector: {}\n", bytes_per_sector);
        print!("[ FS ] FAT version: {}\n", version);

        let bs = unsafe { core::ptr::read(bs) };
        let bs_32 = unsafe { core::ptr::read(bs_32) };

        Some((bs, bs_32))
    }
}

fn read_fat(
    port: &mut AHCIPort,
    bs: &FatBootSector,
    bs_32: &Fat32ExtendedBootSector,
) -> Option<&'static mut [u32]> {
    let fat_start = bs.reserved_sector_count as usize;
    let sectors_per_fat = if bs.table_size_16 != 0 {
        bs.table_size_16 as usize
    } else {
        bs_32.table_size as usize
    };

    let fat_size_bytes = sectors_per_fat * bs.bytes_per_sector as usize;
    let layout = Layout::array::<u8>(fat_size_bytes).unwrap();
    let buffer = unsafe { alloc(layout) };

    if buffer.is_null() {
        return None;
    }

    unsafe { core::ptr::write_bytes(buffer, 0, fat_size_bytes) }

    print!(
        "[ FS ] Reading fat with, start = 0x{:x}, sectors = 0x{:x}, buffer addr: 0x{:X}\n",
        fat_start, sectors_per_fat, buffer as usize
    );

    let status = port.read(fat_start, 1, buffer);
    if !status {
        print!("[ FS ] Failed to read FAT\n");
        None
    } else {
        let num_fat_entries = fat_size_bytes / core::mem::size_of::<u32>();
        let fat_array =
            unsafe { core::slice::from_raw_parts_mut(buffer as *mut u32, num_fat_entries) };

        Some(fat_array)
    }
}

/*
fn read_root_dir(
    port: &mut AHCIPort,
    bs: &FatBootSector,
    bs_32: &Fat32ExtendedBootSector,
) -> Option<(*mut DirectoryEntry, usize)> {
    let first_data_sector = bs.reserved_sector_count as usize + (bs_32.table_size as usize * bs.table_count as usize);

    let root_cluster = bs_32.root_cluster as usize;
    let sectors_per_cluster = bs.sectors_per_cluster as usize;

    // Sector offset of root directory = first_data_sector + (root_cluster - 2) * sectors_per_cluster
    let root_dir_start = first_data_sector + (root_cluster - 2) * sectors_per_cluster;


    let root_dir_size = core::mem::size_of::<DirectoryEntry>() * (bs_32.root_cluster as usize);
    let mut sectors = root_dir_size / (bs.bytes_per_sector as usize);

    // need to align read size
    if root_dir_size % (bs.bytes_per_sector as usize) > 0 {
        sectors += 1;
    }

    let layout = Layout::array::<u8>(root_dir_size).unwrap();
    let buffer = unsafe { alloc(layout) };
    print!(
        "[ FS ] Created root dir buffer at 0x{:X}\n",
        buffer as usize
    );

    if buffer.is_null() {
        return None;
    }

    unsafe { core::ptr::write_bytes(buffer, 0, root_dir_size) }

    print!("[ FS ] Reading root directory with, start = 0x{:x}, sectors = 0x{:x}, buffer addr: 0x{:X}\n", root_dir_start, sectors, buffer as usize);

    let status = port.read(root_dir_start, sectors, buffer);
    if !status {
        print!("[ FS ] Failed to read root directory\n");
        None
    } else {
        let ptr = buffer as *mut DirectoryEntry;
        let data_start = root_dir_start + root_dir_size;
        Some((ptr, data_start))
    }
}
*/

/* fn read_root_dir(
    port: &mut AHCIPort,
    bs: &FatBootSector,
    bs_32: &Fat32ExtendedBootSector,
) -> Option<(*mut DirectoryEntry, usize)> {
    let root_cluster = bs_32.root_cluster as usize;
    let root_sector = root_cluster * bs.sectors_per_cluster as usize;

    let num_sectors = bs.sectors_per_cluster;
    let buffer_size = (bs.bytes_per_sector as usize) * (num_sectors as usize);
    let layout = Layout::array::<u8>(buffer_size).unwrap();

    loop {
        let buffer = unsafe { alloc(layout) };
        if buffer.is_null() {
            break;
        }

        let status = port.read(, sector_count, buffer)
    }

    return None;
}
 */
