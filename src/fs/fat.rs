use alloc::{format, string::String, vec::Vec};

/// Flag set in the order byte of the first physical long filename entry.
pub const LFN_LAST_ENTRY_FLAG: u8 = 0x40;

/// Mask extracting the sequence number from a long filename order byte.
pub const LFN_SEQUENCE_MASK: u8 = 0x3F;

/// Number of UTF-16 units stored in a single long filename entry.
pub const LFN_UNITS_PER_ENTRY: usize = 13;

/// Maximum number of UTF-16 units in a long filename.
pub const LFN_MAX_UNITS: usize = 255;

/// Maximum number of long filename entries in a single chain.
pub const LFN_MAX_ENTRIES: usize = (LFN_MAX_UNITS + LFN_UNITS_PER_ENTRY - 1) / LFN_UNITS_PER_ENTRY;

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

    /// Marks this directory entry as deleted.
    pub fn mark_deleted(&mut self) {
        self.name[0] = 0xE5;
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

/// A long filename directory entry as stored on disk.
///
/// Each entry stores 13 UTF-16 units of the long name. Entries are stored
/// directly before their short 8.3 entry in reverse order, i.e. the chunk
/// covering the end of the name comes first and carries
/// [`LFN_LAST_ENTRY_FLAG`] in its order byte.
#[repr(C, packed)]
#[derive(Clone)]
pub struct LongDirectoryEntry {
    pub order: u8,
    name1: [u8; 10],
    pub attributes: u8,
    pub entry_type: u8,
    pub checksum: u8,
    name2: [u8; 12],
    pub first_cluster_low: u16,
    name3: [u8; 4],
}

impl LongDirectoryEntry {
    /// Creates a long filename entry for one 13-unit chunk of a name.
    ///
    /// ## Arguments
    ///
    /// - `sequence` the 1-based chunk index within the name
    /// - `last` whether this chunk covers the end of the name
    /// - `checksum` the checksum of the accompanying short 8.3 name
    /// - `units` the UTF-16 units of this chunk, at most 13
    pub fn new(sequence: u8, last: bool, checksum: u8, units: &[u16]) -> Self {
        let mut padded = [0xFFFFu16; LFN_UNITS_PER_ENTRY];
        for (index, unit) in units.iter().enumerate().take(LFN_UNITS_PER_ENTRY) {
            padded[index] = *unit;
        }

        if units.len() < LFN_UNITS_PER_ENTRY {
            padded[units.len()] = 0x0000;
        }

        let order = if last {
            sequence | LFN_LAST_ENTRY_FLAG
        } else {
            sequence
        };

        let mut name1 = [0u8; 10];
        let mut name2 = [0u8; 12];
        let mut name3 = [0u8; 4];

        for (index, unit) in padded.iter().enumerate() {
            let bytes = unit.to_le_bytes();
            match index {
                0..=4 => {
                    name1[index * 2] = bytes[0];
                    name1[index * 2 + 1] = bytes[1];
                }
                5..=10 => {
                    name2[(index - 5) * 2] = bytes[0];
                    name2[(index - 5) * 2 + 1] = bytes[1];
                }
                _ => {
                    name3[(index - 11) * 2] = bytes[0];
                    name3[(index - 11) * 2 + 1] = bytes[1];
                }
            }
        }

        Self {
            order: order,
            name1: name1,
            attributes: 0x0F,
            entry_type: 0,
            checksum: checksum,
            name2: name2,
            first_cluster_low: 0,
            name3: name3,
        }
    }

    /// Reinterprets a raw directory entry as a long filename entry.
    pub fn from_directory_entry(entry: &DirectoryEntry) -> Self {
        unsafe { core::mem::transmute_copy(entry) }
    }

    /// Reinterprets this long filename entry as a raw directory entry.
    pub fn to_directory_entry(&self) -> DirectoryEntry {
        unsafe { core::mem::transmute_copy(self) }
    }

    pub fn sequence(&self) -> u8 {
        self.order & LFN_SEQUENCE_MASK
    }

    pub fn is_last(&self) -> bool {
        self.order & LFN_LAST_ENTRY_FLAG != 0
    }

    /// Collects the 13 UTF-16 units stored in this entry.
    pub fn name_units(&self) -> [u16; LFN_UNITS_PER_ENTRY] {
        let mut units = [0u16; LFN_UNITS_PER_ENTRY];

        for index in 0..5 {
            units[index] = u16::from_le_bytes([self.name1[index * 2], self.name1[index * 2 + 1]]);
        }

        for index in 0..6 {
            units[5 + index] =
                u16::from_le_bytes([self.name2[index * 2], self.name2[index * 2 + 1]]);
        }

        for index in 0..2 {
            units[11 + index] =
                u16::from_le_bytes([self.name3[index * 2], self.name3[index * 2 + 1]]);
        }

        units
    }
}

/// Computes the short-name checksum stored in long filename entries.
///
/// ## Arguments
///
/// - `short_name` the raw 8.3 name the long filename entries belong to
pub fn lfn_checksum(short_name: &[u8; 11]) -> u8 {
    short_name
        .iter()
        .fold(0u8, |sum, byte| sum.rotate_right(1).wrapping_add(*byte))
}

/// Encodes a long filename into UTF-16 units.
///
/// ## Arguments
///
/// - `filename` the long filename to encode
///
/// ## Returns
/// The UTF-16 units, or `None` when the name is empty, too long, or contains
/// characters that are invalid in a FAT long filename.
pub fn encode_long_filename(filename: &str) -> Option<Vec<u16>> {
    if filename.is_empty()
        || filename == "."
        || filename == ".."
        || filename.ends_with(' ')
        || filename.ends_with('.')
        || filename.chars().any(is_invalid_long_name_char)
    {
        return None;
    }

    let units: Vec<u16> = filename.encode_utf16().collect();
    if units.len() > LFN_MAX_UNITS {
        return None;
    }

    Some(units)
}

fn is_invalid_long_name_char(char: char) -> bool {
    (char as u32) < 0x20
        || matches!(
            char,
            '"' | '*' | '/' | ':' | '<' | '>' | '?' | '\\' | '|'
        )
}

/// Decodes the UTF-16 units collected from a long filename chain.
///
/// ## Arguments
///
/// - `units` the units in name order, including any terminator and padding
///
/// ## Returns
/// The decoded name, or `None` when the units are not valid UTF-16.
pub fn decode_long_filename(units: &[u16]) -> Option<String> {
    let end = units
        .iter()
        .position(|unit| *unit == 0x0000)
        .unwrap_or(units.len());

    let mut name = String::new();
    for char in char::decode_utf16(units[..end].iter().copied()) {
        name.push(char.ok()?);
    }

    if name.is_empty() {
        return None;
    }

    Some(name)
}

/// Generates a unique short 8.3 alias for a long filename.
///
/// The alias follows the classic `BASE~N.EXT` numeric-tail scheme.
///
/// ## Arguments
///
/// - `filename` the long filename to derive the alias from
/// - `taken` the raw short names already present in the directory
///
/// ## Returns
/// A short name not present in `taken`, or `None` when no free alias exists.
pub fn generate_short_alias(filename: &str, taken: &[[u8; 11]]) -> Option<[u8; 11]> {
    let (base, ext) = match filename.rfind('.') {
        Some(index) if index != 0 => (&filename[..index], &filename[index + 1..]),
        _ => (filename, ""),
    };

    let base = sanitize_short_component(base, 8);
    let ext = sanitize_short_component(ext, 3);

    for tail in 1..=999_999u32 {
        let candidate = compose_short_alias(&base, &ext, tail);
        if !taken.contains(&candidate) {
            return Some(candidate);
        }
    }

    None
}

fn sanitize_short_component(component: &str, max_len: usize) -> Vec<u8> {
    let mut bytes = Vec::new();

    for char in component.chars() {
        if bytes.len() == max_len {
            break;
        }

        if char == ' ' || char == '.' {
            continue;
        }

        let upper = char.to_ascii_uppercase();
        let byte = if upper.is_ascii() && !is_invalid_short_name_byte(upper as u8) {
            upper as u8
        } else {
            b'_'
        };

        bytes.push(byte);
    }

    bytes
}

fn compose_short_alias(base: &[u8], ext: &[u8], tail: u32) -> [u8; 11] {
    let mut digits = [0u8; 6];
    let mut digit_count = 0;
    let mut remaining = tail;

    while remaining > 0 {
        digits[digit_count] = b'0' + (remaining % 10) as u8;
        digit_count += 1;
        remaining /= 10;
    }

    let base_len = core::cmp::min(base.len(), 8 - 1 - digit_count);
    let mut name = [b' '; 11];

    name[..base_len].copy_from_slice(&base[..base_len]);
    name[base_len] = b'~';

    for index in 0..digit_count {
        name[base_len + 1 + index] = digits[digit_count - 1 - index];
    }

    name[8..8 + ext.len()].copy_from_slice(ext);
    name
}

pub fn get_fat_filename(filename: &str) -> Option<[u8; 11]> {
    let mut split = filename.splitn(2, '.');
    let name = split.next()?.to_uppercase();
    let ext = split.next().unwrap_or("").to_uppercase();

    if name.is_empty()
        || name.len() > 8
        || ext.len() > 3
        || filename.bytes().filter(|byte| *byte == b'.').count() > 1
        || !name.is_ascii()
        || !ext.is_ascii()
        || name
            .bytes()
            .chain(ext.bytes())
            .any(is_invalid_short_name_byte)
    {
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

fn is_invalid_short_name_byte(byte: u8) -> bool {
    byte <= b' '
        || matches!(
            byte,
            b'"' | b'*'
                | b'+'
                | b','
                | b'/'
                | b':'
                | b';'
                | b'<'
                | b'='
                | b'>'
                | b'?'
                | b'['
                | b'\\'
                | b']'
                | b'|'
        )
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
