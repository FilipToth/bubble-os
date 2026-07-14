use alloc::{
    boxed::Box,
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};
use spin::RwLock;

use crate::{mem::Region, print};

pub type DirectoryItems = (Vec<Arc<dyn Directory>>, Vec<Arc<RwLock<dyn File>>>);

pub trait Directory: DirectoryClone + Send + Sync {
    fn name(&self) -> String;
    fn list_dir(&self) -> DirectoryItems;

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
        let items = self.list_dir();
        let directory = items.0.iter().find(|d| d.name() == name)?;
        Some(directory.clone())
    }

    fn find_file(&self, name: &str) -> Option<Arc<RwLock<dyn File>>> {
        let items = self.list_dir();
        let file = items.1.iter().find(|f| {
            let f_guard = f.read();
            f_guard.name() == name
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
    fn read(&self) -> Option<Region>;
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
