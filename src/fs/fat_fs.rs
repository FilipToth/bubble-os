use core::alloc::Layout;

use alloc::{
    alloc::{alloc, dealloc},
    vec::Vec,
};

use crate::{
    ahci::port::AHCIPort,
    fs::fat::{Fat32ExtendedBootSector, FatBootSector},
    mem::Region,
    print,
};

use super::fat::DirectoryEntry;

pub struct FATFileSystem<'a> {
    port: &'a mut AHCIPort,
    fat: FatBuffer,
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
            let fat_buff = match read_fat(port, &bs, &bs_32) {
                Some(f) => f,
                None => panic!(),
            };

            print!("\n");

            let root_cluster = bs_32.root_cluster as usize;
            let mut fs = FATFileSystem {
                port: port,
                fat: fat_buff,
                bs: bs,
                bs_32: bs_32,
            };

            // read root directory
            let root_entries = fs.read_directory(root_cluster);
            print!("[ FS ] Num root dir entries: {}\n", root_entries.len());

            let first = &root_entries[0];
            let file = fs.read_file(first).unwrap();

            let contents = file.to_string();
            print!("[ FS ] Contents: {}\n", contents);

            Some(fs)
        }
    }

    pub fn read_directory(&mut self, dir_cluster: usize) -> Vec<DirectoryEntry> {
        let mut cluster = dir_cluster;
        let mut entries: Vec<DirectoryEntry> = Vec::new();

        loop {
            match self.read_directory_internal(cluster) {
                Some((dir, num_entries)) => {
                    // copy directory entries
                    let dir_arr = unsafe { core::slice::from_raw_parts_mut(dir, num_entries) };
                    for entry in dir_arr {
                        if entry.attributes == 0 {
                            continue;
                        }

                        entries.push(entry.clone());
                    }

                    // follow fat chain
                    match self.fat.next_cluster(cluster) {
                        Some(n) => cluster = n,
                        None => break,
                    };
                }
                None => break,
            }
        }

        entries
    }

    fn read_directory_internal(
        &mut self,
        dir_cluster: usize,
    ) -> Option<(*mut DirectoryEntry, usize)> {
        self.read_cluster(dir_cluster).map(|region| {
            let num_entries = region.size / core::mem::size_of::<DirectoryEntry>();
            (region.ptr as *mut DirectoryEntry, num_entries)
        })
    }

    pub fn read_file(&mut self, file: &DirectoryEntry) -> Option<Region> {
        if file.attributes != 32 {
            return None;
        }

        let filesize = file.size as usize;
        let layout = Layout::array::<u8>(filesize).unwrap();
        let file_buffer = unsafe { alloc(layout) };

        if file_buffer.is_null() {
            return None;
        }

        let mut bytes_read: usize = 0;
        let mut cluster =
            (((file.first_cluster_high as u32) << 16) | file.first_cluster_low as u32) as usize;

        loop {
            match self.read_cluster(cluster) {
                Some(region) => {
                    let to_append = if (bytes_read + region.size) > filesize {
                        filesize - bytes_read
                    } else {
                        region.size
                    };

                    let head = unsafe { file_buffer.add(bytes_read) };
                    unsafe { core::ptr::copy(region.ptr, head, to_append) };
                    bytes_read += to_append;

                    // deallocate partial read buffer
                    let region_layout = region.construct_layout();
                    unsafe { dealloc(region.ptr, region_layout) };
                }
                None => break,
            };

            // follow FAT chain
            match self.fat.next_cluster(cluster) {
                Some(n) => cluster = n,
                None => break,
            };
        }

        let region = Region::new(file_buffer, filesize);

        Some(region)
    }

    fn get_sector(&self, cluster: usize) -> usize {
        let first_data_sector = self.bs.reserved_sector_count as usize
            + (self.bs.table_count as usize * self.bs_32.table_size as usize);

        first_data_sector + (cluster - 2) * self.bs.sectors_per_cluster as usize
    }

    fn read_cluster(&mut self, cluster: usize) -> Option<Region> {
        let dir_sector = self.get_sector(cluster);

        let num_sectors = self.bs.sectors_per_cluster as usize;
        let buffer_size = (self.bs.bytes_per_sector as usize) * num_sectors;
        let layout = Layout::array::<u8>(buffer_size).unwrap();

        let buffer = unsafe { alloc(layout) };
        if buffer.is_null() {
            print!("[ FS ] Filesystem read buffer is null\n");
            panic!();
        }

        unsafe { core::ptr::write_bytes(buffer, 0, buffer_size) };

        let status = self.port.read(dir_sector, num_sectors, buffer);
        if !status {
            print!("[ FS ] ERROR: Cannot cluster {}!\n", cluster);
            None
        } else {
            let region = Region::new(buffer, buffer_size);
            Some(region)
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
) -> Option<FatBuffer> {
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
        let fat_buffer = FatBuffer::new(buffer as *mut u32, num_fat_entries);
        Some(fat_buffer)
    }
}

struct FatBuffer {
    fat: *mut u32,
    entries: usize,
}

impl FatBuffer {
    pub fn new(fat_ptr: *mut u32, entries: usize) -> Self {
        Self {
            fat: fat_ptr,
            entries: entries,
        }
    }

    pub fn exists(&self, cluster: usize) -> bool {
        match self.next_cluster(cluster) {
            Some(n) => n != 0,
            None => false,
        }
    }

    pub fn next_cluster(&self, curr: usize) -> Option<usize> {
        if curr >= self.entries {
            None
        } else {
            let next = unsafe { *self.fat.add(curr) };
            if next >= 0x0FFFFFF8 || next == 0 {
                None
            } else {
                Some(next as usize)
            }
        }
    }
}
