use alloc::{
    boxed::Box,
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};

use spin::RwLock;

use crate::mem::Region;

pub type DirectoryItems = (Vec<Arc<dyn Directory>>, Vec<Arc<RwLock<dyn File>>>);

/// File type and permission bits, laid out the way POSIX `st_mode` is.
///
/// Only the two type bits are ever set. Nothing here has an owner, so the
/// permission bits are filled in by the libc stub rather than the kernel.
pub const S_IFMT: u32 = 0o170_000;

/// `st_mode` type bits: a regular file.
pub const S_IFREG: u32 = 0o100_000;

/// `st_mode` type bits: a directory.
pub const S_IFDIR: u32 = 0o040_000;

/// `st_mode` type bits: a character device.
///
/// The standard streams report as this. stdio checks it to decide between
/// line and full buffering, so a terminal that claims to be a regular file
/// ends up withholding output until its buffer fills.
pub const S_IFCHR: u32 = 0o020_000;

/// Metadata about a file or directory.
///
/// This is the kernel's own layout, deliberately not a libc `struct stat`.
/// The libc porting stub copies these fields into whatever its own header
/// declares, so a libc that reorders or resizes its struct cannot silently
/// break the syscall ABI. Fields FAT cannot answer are left at zero by the
/// filesystem rather than invented here.
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct FileStat {
    /// A number that identifies the file within its filesystem.
    ///
    /// FAT has no inodes, so this is the first cluster of the file, which is
    /// stable for as long as the file exists. Empty files have no cluster and
    /// report zero.
    pub inode: u64,

    /// The file type, one of [`S_IFREG`] or [`S_IFDIR`].
    pub mode: u32,

    /// How many names refer to this file. FAT has no hard links, so this is
    /// always 1.
    pub links: u32,

    /// The size in bytes. Directories report zero, FAT does not record a size
    /// for them.
    pub size: u64,

    /// The cluster size of the filesystem, which is the unit reads and writes
    /// are most efficient in.
    pub block_size: u32,

    /// How many blocks the file occupies, rounded up to whole clusters.
    pub blocks: u32,

    /// Last access time, in seconds since the Unix epoch.
    ///
    /// FAT only records a date for this one, so it is always midnight.
    pub accessed_time: i64,

    /// Last modification time, in seconds since the Unix epoch.
    pub modified_time: i64,

    /// Creation time, in seconds since the Unix epoch.
    ///
    /// POSIX `st_ctime` means inode change time, which FAT does not record.
    /// The libc stub reports this in its place as the closest thing there is.
    pub created_time: i64,
}

impl FileStat {
    /// Whether this describes a directory.
    pub fn is_directory(&self) -> bool {
        self.mode & S_IFMT == S_IFDIR
    }
}

pub trait Directory: DirectoryClone + Send + Sync {
    fn name(&self) -> String;
    fn list_dir(&self) -> DirectoryItems;

    /// Metadata for this directory itself.
    fn stat(&self) -> Option<FileStat>;

    /// Creates an empty regular file directly inside this directory.
    ///
    /// ## Arguments
    ///
    /// - `name` the filename to create
    ///
    /// ## Returns
    /// The new file, or `None` when the name is invalid, already exists, or
    /// cannot be persisted.
    fn create_file(&self, name: &str) -> Option<Arc<RwLock<dyn File>>>;

    /// Creates an empty directory directly inside this directory.
    ///
    /// ## Arguments
    ///
    /// - `name` the directory name to create
    ///
    /// ## Returns
    /// `Some(())` when the directory was created, or `None` when the name is
    /// invalid, already exists, or cannot be persisted.
    fn create_directory(&self, name: &str) -> Option<()>;

    /// Removes a regular file directly inside this directory.
    ///
    /// ## Arguments
    ///
    /// - `name` the file name to remove
    ///
    /// ## Returns
    /// `Some(())` when the file was removed, or `None` when it does not exist
    /// or is not a regular file.
    fn unlink_file(&self, name: &str) -> Option<()>;

    /// Removes an empty directory directly inside this directory.
    ///
    /// ## Arguments
    ///
    /// - `name` the directory name to remove
    ///
    /// ## Returns
    /// `Some(())` when the directory was removed, or `None` when it does not
    /// exist, is not a directory, or is not empty.
    fn remove_directory(&self, name: &str) -> Option<()>;

    fn find_directory(&self, name: &str) -> Option<Arc<dyn Directory>> {
        // TODO: Use a method to only list subdirectories, so we save on performance
        // FAT filenames are case-insensitive
        let items = self.list_dir();
        let directory = items
            .0
            .iter()
            .find(|d| d.name().eq_ignore_ascii_case(name))?;

        Some(directory.clone())
    }

    fn find_file(&self, name: &str) -> Option<Arc<RwLock<dyn File>>> {
        let items = self.list_dir();
        let file = items.1.iter().find(|f| {
            let f_guard = f.read();
            f_guard.name().eq_ignore_ascii_case(name)
        })?;

        Some(file.clone())
    }

    fn find_directory_recursive(&self, path: &str) -> Option<Arc<dyn Directory>> {
        let components = normalize_path_components(path);
        self.find_directory_components(&components)
    }

    fn find_directory_components(&self, components: &[&str]) -> Option<Arc<dyn Directory>> {
        let (next, rest) = components.split_first()?;
        let next = self.find_directory(next)?;

        if rest.is_empty() {
            Some(next)
        } else {
            next.find_directory_components(rest)
        }
    }

    fn find_file_recursive(&self, path: &str) -> Option<Arc<RwLock<dyn File>>> {
        let components = normalize_path_components(path);
        self.find_file_components(&components)
    }

    fn find_file_components(&self, components: &[&str]) -> Option<Arc<RwLock<dyn File>>> {
        let (next, rest) = components.split_first()?;
        if rest.is_empty() {
            self.find_file(next)
        } else {
            let next = self.find_directory(next)?;
            next.find_file_components(rest)
        }
    }
}

pub trait DirectoryClone {
    fn clone_boxed<'a>(&self) -> Box<dyn 'a + Directory>
    where
        Self: 'a;
}

impl<T: Clone + Directory> DirectoryClone for T {
    fn clone_boxed<'a>(&self) -> Box<dyn 'a + Directory>
    where
        Self: 'a,
    {
        Box::new(T::clone(self))
    }
}

impl<'a> Clone for Box<dyn 'a + Directory> {
    fn clone(&self) -> Self {
        self.clone_boxed()
    }
}

pub trait File: FileClone + Send + Sync {
    fn name(&self) -> String;
    fn size(&self) -> usize;

    /// Reads the whole file into a freshly allocated region.
    ///
    /// The caller owns the region and has to free it. Only worth using when
    /// the entire file is wanted at once, like loading an ELF, otherwise
    /// [`File::read_at`] avoids buffering the parts nobody asked for.
    fn read(&self) -> Option<Region>;

    /// Metadata for this file.
    fn stat(&self) -> Option<FileStat>;

    /// Reads at most `buffer.len()` bytes starting at `offset`.
    ///
    /// ## Returns
    /// The number of bytes read. Zero means `offset` is at or past the end of
    /// the file, which is not a failure.
    fn read_at(&self, offset: usize, buffer: &mut [u8]) -> Option<usize>;

    fn write(&self, offset: usize, bytes: &[u8]) -> Option<usize>;
    fn truncate(&mut self, size: usize) -> Option<()>;
}

pub trait FileClone {
    fn clone_boxed<'a>(&self) -> Box<dyn 'a + File>
    where
        Self: 'a;
}

impl<T: Clone + File> FileClone for T {
    fn clone_boxed<'a>(&self) -> Box<dyn 'a + File>
    where
        Self: 'a,
    {
        Box::new(T::clone(self))
    }
}

impl<'a> Clone for Box<dyn 'a + File> {
    fn clone(&self) -> Self {
        self.clone_boxed()
    }
}

pub fn combine_path(p1: &str, p2: &str) -> String {
    p1.to_string() + "/" + p2
}

/// Normalizes a path into components.
///
/// Empty components and `.` are discarded, and `..` removes the previous
/// component when possible.
///
/// ## Arguments
///
/// - `path` the path to normalize
///
/// ## Returns
/// The normalized path components.
pub fn normalize_path_components(path: &str) -> Vec<&str> {
    let mut components = Vec::new();

    for component in path.split('/') {
        match component {
            "" | "." => {}
            ".." => match components.last() {
                Some(&"..") | None => components.push(component),
                Some(_) => {
                    components.pop();
                }
            },
            _ => components.push(component),
        }
    }

    components
}
