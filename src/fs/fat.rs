use alloc::{borrow::Cow, format, string::String, vec::Vec};

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
#[derive(Clone)]
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
    pub fn get_filename(&self) -> String {
        get_filename_from_fat(&self.name)
    }

    pub fn get_fat_filename(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.name)
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
