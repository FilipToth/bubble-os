use alloc::{format, string::String};

#[repr(C, packed)]
pub struct FatBootSector {
    pub bootjmp: [u8; 3],
    pub oem_name: [u8; 8],
    pub bytes_per_sector: u16,
    pub sectors_per_cluster: u8,
    pub reserved_sector_count: u16,
    pub table_count: u8,
    pub root_entry_count: u16,
    pub total_sectors_16: u16,
    pub media_type: u8,
    pub table_size_16: u16,
    pub sectors_per_track: u16,
    pub head_side_count: u16,
    pub hidden_sector_count: u32,
    pub total_sectors_32: u32,
}

#[repr(C, packed)]
pub struct Fat32ExtendedBootSector {
    pub table_size: u32,
    pub extended_flags: u16,
    pub fat_version: u16,
    pub root_cluster: u32,
    pub fat_info: u16,
    pub backup_bs_sector: u16,
    pub reserved_0: [u8; 12],
    pub drive_number: u8,
    pub reserved_1: u8,
    pub boot_signature: u8,
    pub volume_id: u32,
    pub volume_label: [u8; 11],
    pub fat_type_label: [u8; 8],
}

#[repr(C, packed)]
#[derive(Clone, Debug)]
pub struct DirectoryEntry {
    pub name: [u8; 11],
    pub attributes: u8,
    pub reserved: u8,
    pub creation_time_centiseconds: u8,
    pub creation_time: u16,
    pub creation_date: u16,
    pub last_accessed_date: u16,
    pub first_cluster_high: u16,
    pub modified_time: u16,
    pub modified_date: u16,
    pub first_cluster_low: u16,
    pub size: u32,
}

impl DirectoryEntry {
    pub fn is_end_marker(&self) -> bool {
        self.name[0] == 0x00
    }

    pub fn is_deleted(&self) -> bool {
        self.name[0] == 0xE5
    }

    pub fn is_directory(&self) -> bool {
        self.is_normal_entry() && self.attributes & 0x10 != 0
    }

    pub fn is_long_filename(&self) -> bool {
        self.attributes & 0x3F == 0x0F
    }

    pub fn is_volume_label(&self) -> bool {
        self.attributes & 0x08 != 0
    }

    pub fn is_regular_file(&self) -> bool {
        self.is_normal_entry() && !self.is_directory()
    }

    fn is_normal_entry(&self) -> bool {
        !self.is_end_marker()
            && !self.is_deleted()
            && !self.is_long_filename()
            && !self.is_volume_label()
    }

    pub fn get_cluster(&self) -> usize {
        (((self.first_cluster_high as u32) << 16) | self.first_cluster_low as u32) as usize
    }

    /// Updates the first cluster fields for this directory entry.
    ///
    /// ## Arguments
    ///
    /// - `cluster` the first cluster in the file's cluster chain
    pub fn set_cluster(&mut self, cluster: usize) {
        self.first_cluster_high = ((cluster >> 16) & 0xFFFF) as u16;
        self.first_cluster_low = (cluster & 0xFFFF) as u16;
    }
}

pub fn get_fat_filename(filename: &str) -> Option<[u8; 11]> {
    let mut split = filename.splitn(2, '.');
    let name = split.next()?.to_uppercase();
    let ext = split.next().unwrap_or("").to_uppercase();

    if name.len() > 8 || ext.len() > 3 {
        return None;
    }

    let mut filename = [b' '; 11];
    for (index, char) in name.bytes().enumerate().take(8) {
        filename[index] = char;
    }

    for (index, char) in ext.bytes().enumerate().take(3) {
        filename[index + 8] = char;
    }

    Some(filename)
}

pub fn get_filename_from_fat(filename: &[u8; 11]) -> String {
    let name = &filename[0..8];
    let ext = &filename[8..11];

    let name = core::str::from_utf8(name)
        .unwrap_or("")
        .trim_end()
        .to_lowercase();

    let ext = core::str::from_utf8(ext)
        .unwrap_or("")
        .trim_end()
        .to_lowercase();

    if !ext.is_empty() {
        format!("{}.{}", name, ext)
    } else {
        name
    }
}
