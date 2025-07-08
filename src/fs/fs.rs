use alloc::{
    boxed::Box,
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};
use spin::RwLock;

use crate::mem::Region;

pub type DirectoryItems = (Vec<Arc<dyn Directory>>, Vec<Arc<RwLock<dyn File>>>);

pub trait Directory: DirectoryClone + Send + Sync {
    fn name(&self) -> String;
    fn list_dir(&self) -> DirectoryItems;

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
        let (next, rest) = match path.find('/') {
            Some(pos) => {
                let (next, rest) = path.split_at(pos);
                (next, Some(&rest[1..]))
            }
            None => (path, None),
        };

        // resolve next
        let items = self.list_dir();
        let next = items.0.iter().find(|d| d.name() == next)?;

        match rest {
            Some(rest) => next.find_directory_recursive(rest),
            None => {
                // last part of the path
                Some(next.clone())
            }
        }
    }

    fn find_file_recursive(&self, path: &str) -> Option<Arc<RwLock<dyn File>>> {
        let (next, rest) = match path.find('/') {
            Some(pos) => {
                let (next, rest) = path.split_at(pos);
                (next, Some(&rest[1..]))
            }
            None => (path, None),
        };

        // resolve next
        let items = self.list_dir();
        match rest {
            Some(rest) => {
                let next = items.0.iter().find(|d| d.name() == next)?;
                next.find_file_recursive(rest)
            }
            None => {
                // last part of the path
                let file = items.1.iter().find(|f| {
                    let f_guard = f.read();
                    f_guard.name() == next
                })?;

                Some(file.clone())
            }
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
