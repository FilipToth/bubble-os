use core::{alloc::Layout, cmp::min};

use alloc::{
    alloc::{alloc, dealloc},
    boxed::Box,
    string::String,
    sync::{Arc, Weak},
    vec::Vec,
};
use spin::{Mutex, RwLock};

use crate::log;
use crate::{
    ahci::port::AHCIPort,
    fs::fat::{Fat32ExtendedBootSector, FatBootSector},
    mem::Region,
};

use super::{
    fat::{
        decode_long_filename, encode_long_filename, generate_short_alias, get_fat_filename,
        get_filename_from_fat, lfn_checksum, DirectoryEntry, LongDirectoryEntry, LFN_MAX_ENTRIES,
        LFN_UNITS_PER_ENTRY,
    },
    fs::{Directory, DirectoryItems, File},
};

const FAT_CLUSTER_FREE: u32 = 0x00000000;
const FAT_CLUSTER_EOF: u32 = 0x0FFFFFFF;

/// Identifies where a directory entry is stored on disk.
#[derive(Clone, Copy)]
pub struct DirectoryEntryLocation {
    /// The directory cluster containing the entry.
    pub cluster: usize,

    /// The index of the entry within the directory cluster.
    pub index: usize,

    /// The byte offset of the entry within the directory cluster.
    pub offset: usize,
}

/// A directory entry together with its on-disk location and decoded name.
///
/// When the entry carries a long filename, `name` holds it and
/// `lfn_locations` points at the long filename entries preceding the short
/// entry; otherwise `name` is the decoded 8.3 name and `lfn_locations` is
/// empty.
#[derive(Clone)]
struct LocatedDirectoryEntry {
    entry: DirectoryEntry,
    location: DirectoryEntryLocation,
    name: String,
    lfn_locations: Vec<DirectoryEntryLocation>,
}

#[derive(Clone)]
pub struct FATDirectory {
    entry: DirectoryEntry,
    name: String,
    pub fs: Weak<Mutex<FATFileSystem>>,
}

impl Directory for FATDirectory {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn list_dir(&self) -> DirectoryItems {
        let fs = match self.fs.upgrade() {
            Some(f) => f,
            None => {
                log!(crate::io::LogType::ROOTFS, "Failed to upgrade weak");
                panic!()
            }
        };

        let mut fs_guard = fs.lock();
        let (files, directories) = fs_guard.list_directory(&self.entry);

        let directories: Vec<Arc<dyn Directory>> = directories
            .into_iter()
            .map(|d| {
                let dir = FATDirectory::new(d.entry, d.name, self.fs.clone());
                Arc::new(dir) as Arc<dyn Directory>
            })
            .collect();

        let files: Vec<Arc<RwLock<dyn File>>> = files
            .into_iter()
            .map(|f| {
                let file = FATFile::new(f.entry, f.name, f.location, self.fs.clone());
                Arc::new(RwLock::new(file)) as Arc<RwLock<dyn File>>
            })
            .collect();

        (directories, files)
    }

    fn create_file(&self, name: &str) -> Option<Arc<RwLock<dyn File>>> {
        let fs = self.fs.upgrade()?;
        let mut fs_guard = fs.lock();
        let file = fs_guard.create_file(&self.entry, name)?;

        Some(Arc::new(RwLock::new(FATFile::new(
            file.entry,
            file.name,
            file.location,
            self.fs.clone(),
        ))))
    }

    fn create_directory(&self, name: &str) -> Option<()> {
        let fs = self.fs.upgrade()?;
        let mut fs_guard = fs.lock();
        fs_guard.create_directory(&self.entry, name)
    }

    fn unlink_file(&self, name: &str) -> Option<()> {
        let fs = self.fs.upgrade()?;
        let mut fs_guard = fs.lock();
        fs_guard.unlink_file(&self.entry, name)
    }

    fn remove_directory(&self, name: &str) -> Option<()> {
        let fs = self.fs.upgrade()?;
        let mut fs_guard = fs.lock();
        fs_guard.remove_directory(&self.entry, name)
    }
}

impl FATDirectory {
    pub fn new(entry: DirectoryEntry, name: String, fs: Weak<Mutex<FATFileSystem>>) -> Self {
        Self {
            entry: entry,
            name: name,
            fs: fs,
        }
    }
}

#[derive(Clone)]
pub struct FATFile {
    entry: DirectoryEntry,
    name: String,
    location: DirectoryEntryLocation,
    fs: Weak<Mutex<FATFileSystem>>,
}

impl File for FATFile {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn size(&self) -> usize {
        let Some(fs) = self.fs.upgrade() else {
            return self.entry.size as usize;
        };

        let mut fs_guard = fs.lock();
        fs_guard
            .read_directory_entry(self.location)
            .map(|entry| entry.size as usize)
            .unwrap_or(self.entry.size as usize)
    }

    fn read(&self) -> Option<Region> {
        let fs = self.fs.upgrade().unwrap();
        let mut fs_guard = fs.lock();
        let entry = fs_guard.read_directory_entry(self.location)?;
        fs_guard.read_file(&entry)
    }

    fn write(&self, offset: usize, bytes: &[u8]) -> Option<usize> {
        let fs = self.fs.upgrade().unwrap();
        let mut fs_guard = fs.lock();
        let entry = fs_guard.read_directory_entry(self.location)?;
        let bytes_written = fs_guard.write_existing_file(&entry, offset, bytes)?;
        fs_guard.persist_directory_entry(self.location, &entry)?;

        Some(bytes_written)
    }

    fn truncate(&mut self, size: usize) -> Option<()> {
        let fs = self.fs.upgrade().unwrap();
        let mut fs_guard = fs.lock();

        fs_guard.truncate_existing_file(&mut self.entry, self.location, size)
    }
}

impl FATFile {
    pub fn new(
        entry: DirectoryEntry,
        name: String,
        location: DirectoryEntryLocation,
        fs: Weak<Mutex<FATFileSystem>>,
    ) -> Self {
        Self {
            entry: entry,
            name: name,
            location: location,
            fs: fs,
        }
    }
}

pub struct FATFileSystem {
    port: Box<AHCIPort>,
    fat: FatBuffer,
    bs: FatBootSector,
    bs_32: Fat32ExtendedBootSector,
}

impl FATFileSystem {
    pub fn new(mut port: Box<AHCIPort>) -> Option<Self> {
        let (bs, bs_32) = read_boot_sector(&mut *port)?;
        let fat_buff = read_fat(&mut *port, &bs, &bs_32)?;

        let fs = FATFileSystem {
            port: port,
            fat: fat_buff,
            bs: bs,
            bs_32: bs_32,
        };

        Some(fs)
    }

    pub fn root_dir(self_arc: Arc<Mutex<FATFileSystem>>) -> FATDirectory {
        let fs_weak = Arc::downgrade(&self_arc);
        let root = self_arc.lock().root();
        FATDirectory::new(root, String::from("root"), fs_weak)
    }

    fn root(&self) -> DirectoryEntry {
        let root_cluster = self.bs_32.root_cluster;
        let root_name = get_fat_filename("root").unwrap();

        let cluster_high = ((root_cluster >> 16) & 0xFFF) as u16;
        let cluster_low = (root_cluster & 0xFFFF) as u16;

        DirectoryEntry {
            name: root_name,
            attributes: 0x10,
            reserved: 0,
            creation_time_centiseconds: 0,
            creation_time: 0,
            creation_date: 0,
            last_accessed_date: 0,
            first_cluster_high: cluster_high,
            modified_time: 0,
            modified_date: 0,
            first_cluster_low: cluster_low,
            size: 0,
        }
    }

    fn list_directory(
        &mut self,
        dir: &DirectoryEntry,
    ) -> (Vec<LocatedDirectoryEntry>, Vec<LocatedDirectoryEntry>) {
        let cluster = self.directory_cluster(dir);
        let Some((slots, _)) = self.load_directory_slots(cluster) else {
            return (Vec::new(), Vec::new());
        };

        let mut files: Vec<LocatedDirectoryEntry> = Vec::new();
        let mut subdirs: Vec<LocatedDirectoryEntry> = Vec::new();

        for parsed in Self::parse_directory_entries(&slots) {
            if parsed.entry.is_directory() {
                subdirs.push(parsed);
            } else if parsed.entry.is_regular_file() {
                files.push(parsed);
            }
        }

        (files, subdirs)
    }

    /// Loads every directory entry slot of a directory cluster chain.
    ///
    /// ## Arguments
    ///
    /// - `first_cluster` the first cluster of the directory
    ///
    /// ## Returns
    /// All slots with their locations, including deleted and end marker
    /// slots, along with the last cluster of the chain.
    fn load_directory_slots(
        &mut self,
        first_cluster: usize,
    ) -> Option<(Vec<(DirectoryEntry, DirectoryEntryLocation)>, usize)> {
        let entry_size = core::mem::size_of::<DirectoryEntry>();
        let mut slots = Vec::new();
        let mut cluster = first_cluster;

        loop {
            let region = self.read_directory_internal(cluster)?;
            let num_entries = region.size / entry_size;
            let entries = unsafe {
                core::slice::from_raw_parts(region.get_ptr::<DirectoryEntry>(), num_entries)
            };

            for (index, entry) in entries.iter().enumerate() {
                let location = DirectoryEntryLocation {
                    cluster: cluster,
                    index: index,
                    offset: index * entry_size,
                };

                slots.push((entry.clone(), location));
            }

            Self::deallocate_region(&region);

            match self.fat.next_cluster(cluster) {
                Some(next_cluster) => cluster = next_cluster,
                None => return Some((slots, cluster)),
            }
        }
    }

    /// Decodes directory slots into live entries with their names.
    ///
    /// Long filename chains are validated against their sequence numbers and
    /// short-name checksum; invalid or orphaned chains are discarded and the
    /// 8.3 name is used instead.
    fn parse_directory_entries(
        slots: &[(DirectoryEntry, DirectoryEntryLocation)],
    ) -> Vec<LocatedDirectoryEntry> {
        let mut parsed = Vec::new();
        let mut lfn_chunks: Vec<[u16; LFN_UNITS_PER_ENTRY]> = Vec::new();
        let mut lfn_locations: Vec<DirectoryEntryLocation> = Vec::new();
        let mut lfn_expected_sequence = 0u8;
        let mut lfn_checksum_value = 0u8;

        for (entry, location) in slots {
            if entry.is_end_marker() {
                break;
            }

            if entry.is_deleted() {
                lfn_chunks.clear();
                lfn_locations.clear();
                lfn_expected_sequence = 0;
                continue;
            }

            if entry.is_long_filename() {
                let lfn = LongDirectoryEntry::from_directory_entry(entry);
                let sequence = lfn.sequence();

                if lfn.is_last() {
                    lfn_chunks.clear();
                    lfn_locations.clear();
                    lfn_checksum_value = lfn.checksum;
                    lfn_expected_sequence =
                        if sequence >= 1 && sequence as usize <= LFN_MAX_ENTRIES {
                            sequence
                        } else {
                            0
                        };
                }

                if lfn_expected_sequence == 0
                    || sequence != lfn_expected_sequence
                    || lfn.checksum != lfn_checksum_value
                {
                    lfn_chunks.clear();
                    lfn_locations.clear();
                    lfn_expected_sequence = 0;
                    continue;
                }

                lfn_chunks.push(lfn.name_units());
                lfn_locations.push(*location);
                lfn_expected_sequence -= 1;
                continue;
            }

            if entry.is_volume_label() {
                lfn_chunks.clear();
                lfn_locations.clear();
                lfn_expected_sequence = 0;
                continue;
            }

            let short_name = entry.name;
            let lfn_complete = lfn_expected_sequence == 0
                && !lfn_chunks.is_empty()
                && lfn_checksum_value == lfn_checksum(&short_name);

            let long_name = if lfn_complete {
                let mut units: Vec<u16> = Vec::with_capacity(lfn_chunks.len() * LFN_UNITS_PER_ENTRY);
                for chunk in lfn_chunks.iter().rev() {
                    units.extend_from_slice(chunk);
                }

                decode_long_filename(&units)
            } else {
                None
            };

            let (name, lfn_entry_locations) = match long_name {
                Some(name) => (name, core::mem::take(&mut lfn_locations)),
                None => (get_filename_from_fat(&short_name), Vec::new()),
            };

            parsed.push(LocatedDirectoryEntry {
                entry: entry.clone(),
                location: *location,
                name: name,
                lfn_locations: lfn_entry_locations,
            });

            lfn_chunks.clear();
            lfn_locations.clear();
            lfn_expected_sequence = 0;
        }

        parsed
    }

    /// Finds a live directory entry by name.
    ///
    /// Both the long filename and the decoded 8.3 alias match,
    /// case-insensitively.
    fn find_parsed_entry<'a>(
        parsed: &'a [LocatedDirectoryEntry],
        name: &str,
    ) -> Option<&'a LocatedDirectoryEntry> {
        parsed.iter().find(|candidate| {
            let short_name = candidate.entry.name;
            candidate.name.eq_ignore_ascii_case(name)
                || get_filename_from_fat(&short_name).eq_ignore_ascii_case(name)
        })
    }

    fn find_entry_by_name(
        &mut self,
        dir: &DirectoryEntry,
        name: &str,
    ) -> Option<LocatedDirectoryEntry> {
        let cluster = self.directory_cluster(dir);
        let (slots, _) = self.load_directory_slots(cluster)?;
        let parsed = Self::parse_directory_entries(&slots);

        Self::find_parsed_entry(&parsed, name).cloned()
    }

    fn create_file(
        &mut self,
        dir: &DirectoryEntry,
        filename: &str,
    ) -> Option<LocatedDirectoryEntry> {
        let dir_cluster = self.directory_cluster(dir);
        let (slots, _) = self.load_directory_slots(dir_cluster)?;
        let parsed = Self::parse_directory_entries(&slots);

        if Self::find_parsed_entry(&parsed, filename).is_some() {
            return None;
        }

        let (short_name, lfn_entries) = Self::prepare_name_entries(&parsed, filename)?;
        let entry = DirectoryEntry {
            name: short_name,
            attributes: 0x20,
            reserved: 0,
            creation_time_centiseconds: 0,
            creation_time: 0,
            creation_date: 0,
            last_accessed_date: 0,
            first_cluster_high: 0,
            modified_time: 0,
            modified_date: 0,
            first_cluster_low: 0,
            size: 0,
        };

        let (location, lfn_locations) = self.persist_new_entry(dir_cluster, &lfn_entries, &entry)?;
        let name = if lfn_entries.is_empty() {
            get_filename_from_fat(&short_name)
        } else {
            String::from(filename)
        };

        Some(LocatedDirectoryEntry {
            entry: entry,
            location: location,
            name: name,
            lfn_locations: lfn_locations,
        })
    }

    fn create_directory(&mut self, parent: &DirectoryEntry, name: &str) -> Option<()> {
        let parent_cluster = self.directory_cluster(parent);
        let (slots, _) = self.load_directory_slots(parent_cluster)?;
        let parsed = Self::parse_directory_entries(&slots);

        if Self::find_parsed_entry(&parsed, name).is_some() {
            return None;
        }

        let (short_name, lfn_entries) = Self::prepare_name_entries(&parsed, name)?;
        let cluster = self.fat.allocate_chain(1)?;

        if !self.initialize_directory_cluster(cluster, parent_cluster) {
            let _ = self.fat.free_chain(cluster);
            return None;
        }

        if self.persist_fat().is_none() {
            self.rollback_allocated_chain(cluster);
            return None;
        }

        let entry = Self::directory_entry(short_name, cluster);
        if self
            .persist_new_entry(parent_cluster, &lfn_entries, &entry)
            .is_none()
        {
            self.rollback_allocated_chain(cluster);
            return None;
        }

        Some(())
    }

    /// Derives the on-disk name entries for a new directory entry.
    ///
    /// Names that fit the 8.3 format are stored as a plain short entry.
    /// Longer names get a chain of long filename entries plus a unique
    /// generated short alias.
    ///
    /// ## Arguments
    ///
    /// - `parsed` the live entries of the target directory
    /// - `filename` the requested name
    ///
    /// ## Returns
    /// The short 8.3 name and the long filename entries preceding it, or
    /// `None` when the name is invalid.
    fn prepare_name_entries(
        parsed: &[LocatedDirectoryEntry],
        filename: &str,
    ) -> Option<([u8; 11], Vec<LongDirectoryEntry>)> {
        if let Some(short_name) = get_fat_filename(filename) {
            return Some((short_name, Vec::new()));
        }

        let units = encode_long_filename(filename)?;
        let taken: Vec<[u8; 11]> = parsed.iter().map(|entry| entry.entry.name).collect();
        let short_name = generate_short_alias(filename, &taken)?;
        let checksum = lfn_checksum(&short_name);

        let chunk_count = (units.len() + LFN_UNITS_PER_ENTRY - 1) / LFN_UNITS_PER_ENTRY;
        let mut entries = Vec::with_capacity(chunk_count);

        for chunk_index in (0..chunk_count).rev() {
            let start = chunk_index * LFN_UNITS_PER_ENTRY;
            let end = min(start + LFN_UNITS_PER_ENTRY, units.len());
            let last = chunk_index == chunk_count - 1;

            entries.push(LongDirectoryEntry::new(
                (chunk_index + 1) as u8,
                last,
                checksum,
                &units[start..end],
            ));
        }

        Some((short_name, entries))
    }

    /// Persists a new directory entry along with its long filename entries.
    ///
    /// ## Arguments
    ///
    /// - `dir_cluster` the first cluster of the target directory
    /// - `lfn_entries` the long filename entries, in on-disk order
    /// - `entry` the short 8.3 entry
    ///
    /// ## Returns
    /// The location of the short entry and the locations of the long
    /// filename entries.
    fn persist_new_entry(
        &mut self,
        dir_cluster: usize,
        lfn_entries: &[LongDirectoryEntry],
        entry: &DirectoryEntry,
    ) -> Option<(DirectoryEntryLocation, Vec<DirectoryEntryLocation>)> {
        let locations = self.allocate_directory_slots(dir_cluster, lfn_entries.len() + 1)?;

        for (lfn, location) in lfn_entries.iter().zip(&locations) {
            self.persist_directory_entry(*location, &lfn.to_directory_entry())?;
        }

        let short_location = *locations.last()?;
        self.persist_directory_entry(short_location, entry)?;

        let lfn_locations = locations[..locations.len() - 1].to_vec();
        Some((short_location, lfn_locations))
    }

    fn unlink_file(&mut self, parent: &DirectoryEntry, name: &str) -> Option<()> {
        let file = self.find_entry_by_name(parent, name)?;
        if !file.entry.is_regular_file() {
            return None;
        }

        self.delete_directory_entry(file)
    }

    fn remove_directory(&mut self, parent: &DirectoryEntry, name: &str) -> Option<()> {
        let directory = self.find_entry_by_name(parent, name)?;
        let short_name = directory.entry.name;

        if !directory.entry.is_directory()
            || directory.entry.get_cluster() == 0
            || Self::is_dot_name(&short_name)
        {
            return None;
        }

        if !self.directory_is_empty(&directory.entry)? {
            return None;
        }

        self.delete_directory_entry(directory)
    }

    /// Finds a run of consecutive free directory entry slots, extending the
    /// directory with new clusters when needed.
    ///
    /// A slot is free when it is deleted or lies at or beyond the end
    /// marker. The run may span cluster boundaries.
    ///
    /// ## Arguments
    ///
    /// - `dir_cluster` the first cluster of the directory
    /// - `needed` the number of consecutive slots required
    ///
    /// ## Returns
    /// The locations of the free slots, in directory order.
    fn allocate_directory_slots(
        &mut self,
        dir_cluster: usize,
        needed: usize,
    ) -> Option<Vec<DirectoryEntryLocation>> {
        let (slots, mut last_cluster) = self.load_directory_slots(dir_cluster)?;
        let mut run: Vec<DirectoryEntryLocation> = Vec::new();
        let mut past_end = false;

        for (entry, location) in &slots {
            if !past_end && entry.is_end_marker() {
                past_end = true;
            }

            if past_end || entry.is_deleted() {
                run.push(*location);
                if run.len() == needed {
                    return Some(run);
                }
            } else {
                run.clear();
            }
        }

        // the remaining run is a suffix of the chain; extend the directory
        // until enough consecutive slots are available
        let entry_size = core::mem::size_of::<DirectoryEntry>();
        let entries_per_cluster = self.cluster_size() / entry_size;

        while run.len() < needed {
            let new_cluster = self.append_directory_cluster(last_cluster)?;
            last_cluster = new_cluster;

            for index in 0..entries_per_cluster {
                run.push(DirectoryEntryLocation {
                    cluster: new_cluster,
                    index: index,
                    offset: index * entry_size,
                });

                if run.len() == needed {
                    break;
                }
            }
        }

        Some(run)
    }

    /// Links a new zeroed cluster to the end of a directory cluster chain.
    ///
    /// ## Arguments
    ///
    /// - `last_cluster` the current last cluster of the directory
    ///
    /// ## Returns
    /// The newly linked cluster.
    fn append_directory_cluster(&mut self, last_cluster: usize) -> Option<usize> {
        let new_cluster = self.fat.allocate_chain(1)?;
        if self.fat.set(last_cluster, new_cluster as u32).is_none() {
            let _ = self.fat.free_chain(new_cluster);
            return None;
        }

        if self.persist_fat().is_none() || !self.clear_cluster(new_cluster) {
            let _ = self.fat.set(last_cluster, FAT_CLUSTER_EOF);
            let _ = self.fat.free_chain(new_cluster);
            let _ = self.persist_fat();
            return None;
        }

        Some(new_cluster)
    }

    fn initialize_directory_cluster(&mut self, cluster: usize, parent_cluster: usize) -> bool {
        let size = self.cluster_size();
        let layout = match Layout::array::<u8>(size) {
            Ok(layout) => layout,
            Err(_) => return false,
        };
        let buffer = unsafe { alloc(layout) };

        if buffer.is_null() {
            return false;
        }

        unsafe { core::ptr::write_bytes(buffer, 0, size) };

        let dot = Self::directory_entry(Self::dot_name(false), cluster);
        let dot_dot = Self::directory_entry(Self::dot_name(true), parent_cluster);
        let entry_size = core::mem::size_of::<DirectoryEntry>();

        unsafe {
            core::ptr::copy_nonoverlapping(
                &dot as *const DirectoryEntry as *const u8,
                buffer,
                entry_size,
            );
            core::ptr::copy_nonoverlapping(
                &dot_dot as *const DirectoryEntry as *const u8,
                buffer.add(entry_size),
                entry_size,
            );
        }

        let status = self.write_cluster(cluster, buffer);
        unsafe { dealloc(buffer, layout) };

        status
    }

    fn directory_is_empty(&mut self, dir: &DirectoryEntry) -> Option<bool> {
        let cluster = self.directory_cluster(dir);
        let (slots, _) = self.load_directory_slots(cluster)?;
        let parsed = Self::parse_directory_entries(&slots);

        let empty = parsed.iter().all(|entry| {
            let name = entry.entry.name;
            Self::is_dot_name(&name)
        });

        Some(empty)
    }

    fn delete_directory_entry(&mut self, located: LocatedDirectoryEntry) -> Option<()> {
        for lfn_location in &located.lfn_locations {
            let mut lfn_entry = self.read_directory_entry(*lfn_location)?;
            lfn_entry.mark_deleted();
            self.persist_directory_entry(*lfn_location, &lfn_entry)?;
        }

        let first_cluster = located.entry.get_cluster();
        let mut deleted_entry = located.entry;
        deleted_entry.mark_deleted();
        self.persist_directory_entry(located.location, &deleted_entry)?;

        if first_cluster == 0 {
            return Some(());
        }

        self.fat.free_chain(first_cluster)?;
        if self.persist_fat().is_none() {
            return Some(());
        }

        Some(())
    }

    fn rollback_allocated_chain(&mut self, cluster: usize) {
        let _ = self.fat.free_chain(cluster);
        let _ = self.persist_fat();
    }

    fn directory_cluster(&self, dir: &DirectoryEntry) -> usize {
        let cluster = dir.get_cluster();
        if cluster == 0 {
            self.root().get_cluster()
        } else {
            cluster
        }
    }

    fn directory_entry(name: [u8; 11], cluster: usize) -> DirectoryEntry {
        let mut entry = DirectoryEntry {
            name: name,
            attributes: 0x10,
            reserved: 0,
            creation_time_centiseconds: 0,
            creation_time: 0,
            creation_date: 0,
            last_accessed_date: 0,
            first_cluster_high: 0,
            modified_time: 0,
            modified_date: 0,
            first_cluster_low: 0,
            size: 0,
        };

        entry.set_cluster(cluster);
        entry
    }

    fn dot_name(parent: bool) -> [u8; 11] {
        let mut name = [b' '; 11];
        name[0] = b'.';
        if parent {
            name[1] = b'.';
        }

        name
    }

    fn is_dot_name(name: &[u8; 11]) -> bool {
        *name == Self::dot_name(false) || *name == Self::dot_name(true)
    }

    fn read_file(&mut self, file: &DirectoryEntry) -> Option<Region> {
        if file.attributes != 32 {
            return None;
        }

        let filesize = file.size as usize;
        if filesize == 0 {
            return Some(Region::new(0, 0));
        }

        let layout = Layout::array::<u8>(filesize).unwrap();
        let file_buffer = unsafe { alloc(layout) };

        if file_buffer.is_null() {
            return None;
        }

        let mut bytes_read: usize = 0;
        let mut cluster = file.get_cluster();
        if cluster == 0 {
            unsafe { dealloc(file_buffer, layout) };
            return None;
        }

        while bytes_read < filesize {
            match self.read_cluster(cluster) {
                Some(region) => {
                    let to_append = min(region.size, filesize - bytes_read);
                    let head = unsafe { file_buffer.add(bytes_read) };
                    let ptr = region.get_ptr::<u8>();

                    unsafe { core::ptr::copy_nonoverlapping(ptr, head, to_append) };
                    bytes_read += to_append;

                    let region_layout = region.construct_layout();
                    unsafe { dealloc(ptr, region_layout) };
                }
                None => {
                    unsafe { dealloc(file_buffer, layout) };
                    return None;
                }
            };

            if bytes_read == filesize {
                break;
            }

            cluster = match self.fat.next_cluster(cluster) {
                Some(n) => n,
                None => {
                    unsafe { dealloc(file_buffer, layout) };
                    return None;
                }
            };
        }

        let region = Region::from(file_buffer, filesize);

        Some(region)
    }

    fn write_existing_file(
        &mut self,
        file: &DirectoryEntry,
        offset: usize,
        bytes: &[u8],
    ) -> Option<usize> {
        if file.attributes != 32 {
            return None;
        }

        let filesize = file.size as usize;
        let end = offset.checked_add(bytes.len())?;
        if end > filesize {
            return None;
        }

        if bytes.is_empty() {
            return Some(0);
        }

        let cluster_size =
            (self.bs.bytes_per_sector as usize) * (self.bs.sectors_per_cluster as usize);
        let mut cluster = file.get_cluster();
        if cluster == 0 {
            return None;
        }

        let mut cluster_offset = offset;
        while cluster_offset >= cluster_size {
            cluster = self.fat.next_cluster(cluster)?;
            cluster_offset -= cluster_size;
        }

        let mut bytes_written = 0;
        while bytes_written < bytes.len() {
            let region = self.read_cluster(cluster)?;
            let cluster_bytes = region.as_slice_mut();
            let writable_bytes = min(bytes.len() - bytes_written, cluster_size - cluster_offset);

            unsafe {
                core::ptr::copy_nonoverlapping(
                    bytes.as_ptr().add(bytes_written),
                    cluster_bytes.as_mut_ptr().add(cluster_offset),
                    writable_bytes,
                );
            }

            let status = self.write_cluster(cluster, region.get_ptr::<u8>());
            let region_layout = region.construct_layout();
            unsafe { dealloc(region.get_ptr::<u8>(), region_layout) };

            if !status {
                return if bytes_written == 0 {
                    None
                } else {
                    Some(bytes_written)
                };
            }

            bytes_written += writable_bytes;
            if bytes_written == bytes.len() {
                break;
            }

            cluster = self.fat.next_cluster(cluster)?;
            cluster_offset = 0;
        }

        Some(bytes_written)
    }

    fn truncate_existing_file(
        &mut self,
        file: &mut DirectoryEntry,
        location: DirectoryEntryLocation,
        new_size: usize,
    ) -> Option<()> {
        *file = self.read_directory_entry(location)?;

        if file.attributes != 32 {
            return None;
        }

        let old_size = file.size as usize;
        if new_size > u32::MAX as usize {
            return None;
        }

        if new_size == old_size {
            return Some(());
        }

        if new_size < old_size {
            return self.shrink_existing_file(file, location, new_size);
        }

        self.grow_existing_file(file, location, old_size, new_size)
    }

    fn shrink_existing_file(
        &mut self,
        file: &mut DirectoryEntry,
        location: DirectoryEntryLocation,
        new_size: usize,
    ) -> Option<()> {
        let first_cluster = file.get_cluster();
        if new_size == 0 {
            if first_cluster != 0 {
                self.fat.free_chain(first_cluster)?;
                self.persist_fat()?;
            }

            file.set_cluster(0);
            file.size = 0;

            return self.persist_directory_entry(location, file);
        }

        if first_cluster == 0 {
            return None;
        }

        let cluster_size = self.cluster_size();
        let clusters_to_keep = (new_size + cluster_size - 1) / cluster_size;
        let mut last_kept_cluster = first_cluster;

        for _ in 1..clusters_to_keep {
            last_kept_cluster = self.fat.next_cluster(last_kept_cluster)?;
        }

        let first_freed_cluster = self.fat.next_cluster(last_kept_cluster);
        self.fat.set(last_kept_cluster, FAT_CLUSTER_EOF)?;

        if let Some(first_freed_cluster) = first_freed_cluster {
            self.fat.free_chain(first_freed_cluster)?;
        }

        self.persist_fat()?;

        file.size = new_size as u32;
        self.persist_directory_entry(location, file)
    }

    fn grow_existing_file(
        &mut self,
        file: &mut DirectoryEntry,
        location: DirectoryEntryLocation,
        old_size: usize,
        new_size: usize,
    ) -> Option<()> {
        let old_cluster_count = self.clusters_for_size(old_size);
        let new_cluster_count = self.clusters_for_size(new_size);
        let old_first_cluster = file.get_cluster();
        let mut new_chain_start = None;

        if new_cluster_count > old_cluster_count {
            let additional_clusters = new_cluster_count - old_cluster_count;
            let allocated_chain_start = self.fat.allocate_chain(additional_clusters)?;
            new_chain_start = Some(allocated_chain_start);

            if old_cluster_count == 0 {
                file.set_cluster(allocated_chain_start);
            } else {
                let last_old_cluster = self.cluster_at(old_first_cluster, old_cluster_count - 1)?;
                self.fat
                    .set(last_old_cluster, allocated_chain_start as u32)?;
            }
        }

        if self
            .zero_file_range(file.get_cluster(), old_size, new_size - old_size)
            .is_none()
        {
            self.rollback_growth(file, old_first_cluster, old_cluster_count, new_chain_start);
            return None;
        }

        if self.persist_fat().is_none() {
            self.rollback_growth(file, old_first_cluster, old_cluster_count, new_chain_start);
            return None;
        }

        file.size = new_size as u32;
        self.persist_directory_entry(location, file)
    }

    fn rollback_growth(
        &mut self,
        file: &mut DirectoryEntry,
        old_first_cluster: usize,
        old_cluster_count: usize,
        new_chain_start: Option<usize>,
    ) {
        if old_cluster_count == 0 {
            file.set_cluster(old_first_cluster);
        } else if let Some(last_old_cluster) =
            self.cluster_at(old_first_cluster, old_cluster_count - 1)
        {
            let _ = self.fat.set(last_old_cluster, FAT_CLUSTER_EOF);
        }

        if let Some(new_chain_start) = new_chain_start {
            let _ = self.fat.free_chain(new_chain_start);
        }
    }

    fn persist_directory_entry(
        &mut self,
        location: DirectoryEntryLocation,
        entry: &DirectoryEntry,
    ) -> Option<()> {
        let entry_size = core::mem::size_of::<DirectoryEntry>();
        let expected_offset = location.index.checked_mul(entry_size)?;

        if expected_offset != location.offset {
            return None;
        }

        let entry_end = location.offset.checked_add(entry_size)?;
        let region = self.read_cluster(location.cluster)?;

        if entry_end > region.size {
            let region_layout = region.construct_layout();
            unsafe { dealloc(region.get_ptr::<u8>(), region_layout) };
            return None;
        }

        unsafe {
            core::ptr::copy_nonoverlapping(
                entry as *const DirectoryEntry as *const u8,
                region.get_ptr::<u8>().add(location.offset),
                entry_size,
            );
        }

        let status = self.write_cluster(location.cluster, region.get_ptr::<u8>());
        let region_layout = region.construct_layout();
        unsafe { dealloc(region.get_ptr::<u8>(), region_layout) };

        status.then_some(())
    }

    fn read_directory_entry(&mut self, location: DirectoryEntryLocation) -> Option<DirectoryEntry> {
        let entry_size = core::mem::size_of::<DirectoryEntry>();
        let expected_offset = location.index.checked_mul(entry_size)?;

        if expected_offset != location.offset {
            return None;
        }

        let entry_end = location.offset.checked_add(entry_size)?;
        let region = self.read_cluster(location.cluster)?;

        if entry_end > region.size {
            let region_layout = region.construct_layout();
            unsafe { dealloc(region.get_ptr::<u8>(), region_layout) };

            return None;
        }

        let entry = unsafe {
            core::ptr::read_unaligned(
                region.get_ptr::<u8>().add(location.offset) as *const DirectoryEntry
            )
        };

        let region_layout = region.construct_layout();
        unsafe { dealloc(region.get_ptr::<u8>(), region_layout) };

        Some(entry)
    }

    fn persist_fat(&mut self) -> Option<()> {
        let sectors_per_fat = self.sectors_per_fat();

        for table_index in 0..self.bs.table_count as usize {
            let fat_start = self.fat_start_sector(table_index);
            let status = self
                .port
                .write_sectors(fat_start, sectors_per_fat, self.fat.as_ptr());

            if !status {
                return None;
            }
        }

        Some(())
    }

    fn read_directory_internal(&mut self, dir_cluster: usize) -> Option<Region> {
        self.read_cluster(dir_cluster)
    }

    fn clear_cluster(&mut self, cluster: usize) -> bool {
        let size = self.cluster_size();
        let layout = match Layout::array::<u8>(size) {
            Ok(layout) => layout,
            Err(_) => return false,
        };
        let buffer = unsafe { alloc(layout) };

        if buffer.is_null() {
            return false;
        }

        unsafe { core::ptr::write_bytes(buffer, 0, size) };
        let status = self.write_cluster(cluster, buffer);
        unsafe { dealloc(buffer, layout) };

        status
    }

    fn deallocate_region(region: &Region) {
        let layout = region.construct_layout();
        unsafe { dealloc(region.get_ptr::<u8>(), layout) };
    }

    fn get_sector(&self, cluster: usize) -> usize {
        let first_data_sector = self.bs.reserved_sector_count as usize
            + (self.bs.table_count as usize * self.bs_32.table_size as usize);

        first_data_sector + (cluster - 2) * self.bs.sectors_per_cluster as usize
    }

    fn cluster_size(&self) -> usize {
        (self.bs.bytes_per_sector as usize) * (self.bs.sectors_per_cluster as usize)
    }

    fn clusters_for_size(&self, size: usize) -> usize {
        if size == 0 {
            return 0;
        }

        let cluster_size = self.cluster_size();
        (size + cluster_size - 1) / cluster_size
    }

    fn cluster_at(&self, first_cluster: usize, index: usize) -> Option<usize> {
        let mut cluster = first_cluster;
        for _ in 0..index {
            cluster = self.fat.next_cluster(cluster)?;
        }

        Some(cluster)
    }

    fn zero_file_range(&mut self, first_cluster: usize, offset: usize, size: usize) -> Option<()> {
        if size == 0 {
            return Some(());
        }

        let cluster_size = self.cluster_size();
        let mut cluster = first_cluster;
        let mut cluster_offset = offset;
        while cluster_offset >= cluster_size {
            cluster = self.fat.next_cluster(cluster)?;
            cluster_offset -= cluster_size;
        }

        let mut bytes_zeroed = 0;
        while bytes_zeroed < size {
            let region = self.read_cluster(cluster)?;
            let zero_size = min(size - bytes_zeroed, cluster_size - cluster_offset);

            unsafe {
                core::ptr::write_bytes(region.get_ptr::<u8>().add(cluster_offset), 0, zero_size);
            }

            let status = self.write_cluster(cluster, region.get_ptr::<u8>());
            let region_layout = region.construct_layout();
            unsafe { dealloc(region.get_ptr::<u8>(), region_layout) };

            if !status {
                return None;
            }

            bytes_zeroed += zero_size;
            if bytes_zeroed == size {
                break;
            }

            cluster = self.fat.next_cluster(cluster)?;
            cluster_offset = 0;
        }

        Some(())
    }

    fn sectors_per_fat(&self) -> usize {
        if self.bs.table_size_16 != 0 {
            self.bs.table_size_16 as usize
        } else {
            self.bs_32.table_size as usize
        }
    }

    fn fat_start_sector(&self, table_index: usize) -> usize {
        self.bs.reserved_sector_count as usize + (table_index * self.sectors_per_fat())
    }

    fn read_cluster(&mut self, cluster: usize) -> Option<Region> {
        let dir_sector = self.get_sector(cluster);

        let num_sectors = self.bs.sectors_per_cluster as usize;
        let buffer_size = (self.bs.bytes_per_sector as usize) * num_sectors;
        let layout = Layout::array::<u8>(buffer_size).unwrap();

        let buffer = unsafe { alloc(layout) };
        if buffer.is_null() {
            log!(crate::io::LogType::FS, "Filesystem read buffer is null");
            panic!();
        }

        unsafe { core::ptr::write_bytes(buffer, 0, buffer_size) };

        let status = self.port.read(dir_sector, num_sectors, buffer);
        if !status {
            log!(crate::io::LogType::FS, "ERROR: Cannot cluster {}!", cluster);
            unsafe { dealloc(buffer, layout) };
            None
        } else {
            let region = Region::from(buffer, buffer_size);
            Some(region)
        }
    }

    fn write_cluster(&mut self, cluster: usize, buffer: *const u8) -> bool {
        let sector = self.get_sector(cluster);
        let num_sectors = self.bs.sectors_per_cluster as usize;

        self.port.write_sectors(sector, num_sectors, buffer)
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
        log!(crate::io::LogType::FS, "Failed to read FAT32 Boot Sector");
        None
    } else {
        let bs = unsafe { &*(buffer as *const FatBootSector) };
        let bytes_per_sector = bs.bytes_per_sector;

        let bs_size = core::mem::size_of::<FatBootSector>();
        let bs_32 = unsafe { &*(buffer.add(bs_size) as *const Fat32ExtendedBootSector) };
        let version = bs_32.fat_version;

        log!(
            crate::io::LogType::FS,
            "Bytes per sector: {}",
            bytes_per_sector
        );

        log!(crate::io::LogType::FS, "FAT version: {}", version);

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

    log!(
        crate::io::LogType::FS,
        "Reading fat with, start = 0x{:x}, sectors = 0x{:x}, buffer addr: 0x{:X}",
        fat_start,
        sectors_per_fat,
        buffer as usize
    );

    let status = port.read(fat_start, sectors_per_fat, buffer);
    if !status {
        log!(crate::io::LogType::FS, "Failed to read FAT");
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

// WARNING: We need to implement Send because of the raw FAT pointer...
unsafe impl Send for FatBuffer {}
unsafe impl Sync for FatBuffer {}

impl FatBuffer {
    pub fn new(fat_ptr: *mut u32, entries: usize) -> Self {
        Self {
            fat: fat_ptr,
            entries: entries,
        }
    }

    pub fn as_ptr(&self) -> *const u8 {
        self.fat as *const u8
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

    pub fn set(&mut self, cluster: usize, value: u32) -> Option<()> {
        if cluster >= self.entries {
            return None;
        }

        unsafe {
            *self.fat.add(cluster) = value;
        }

        Some(())
    }

    pub fn allocate_chain(&mut self, count: usize) -> Option<usize> {
        if count == 0 {
            return None;
        }

        let mut first_cluster = 0;
        let mut prev_cluster: Option<usize> = None;

        for _ in 0..count {
            let Some(cluster) = self.find_free_cluster() else {
                if first_cluster != 0 {
                    let _ = self.free_chain(first_cluster);
                }

                return None;
            };

            if self.set(cluster, FAT_CLUSTER_EOF).is_none() {
                if first_cluster != 0 {
                    let _ = self.free_chain(first_cluster);
                }

                return None;
            }

            if let Some(prev_cluster) = prev_cluster {
                if self.set(prev_cluster, cluster as u32).is_none() {
                    let _ = self.free_chain(first_cluster);
                    let _ = self.set(cluster, FAT_CLUSTER_FREE);

                    return None;
                }
            } else {
                first_cluster = cluster;
            }

            prev_cluster = Some(cluster);
        }

        Some(first_cluster)
    }

    pub fn free_chain(&mut self, mut cluster: usize) -> Option<()> {
        loop {
            if cluster >= self.entries {
                return None;
            }

            let next = self.next_cluster(cluster);
            self.set(cluster, FAT_CLUSTER_FREE)?;

            match next {
                Some(next) => cluster = next,
                None => return Some(()),
            }
        }
    }

    fn find_free_cluster(&self) -> Option<usize> {
        for cluster in 2..self.entries {
            let entry = unsafe { *self.fat.add(cluster) };
            if entry == FAT_CLUSTER_FREE {
                return Some(cluster);
            }
        }

        None
    }
}
