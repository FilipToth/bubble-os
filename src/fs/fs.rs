use alloc::{
    boxed::Box,
    string::{String, ToString},
    vec::Vec,
};

use crate::mem::Region;

use super::fat_fs::FATDirectory;

pub trait Directory: Clone {
    fn name(&self) -> String;
}

#[derive(Clone)]
pub enum DirectoryKind {
    FATDirectory(FATDirectory),
}

impl DirectoryKind {
    pub fn name(&self) -> String {
        match self {
            DirectoryKind::FATDirectory(dir) => dir.name().clone()
        }
    }
}

pub trait File: Clone {
    fn name(&self) -> String;
    fn size(&self) -> usize;
}

pub struct DirectoryItems<A: File, B: Directory> {
    pub files: Vec<A>,
    pub directories: Vec<B>,
}

impl<A: File, B: Directory> DirectoryItems<A, B> {
    pub fn new(files: Vec<A>, directories: Vec<B>) -> Self {
        Self {
            files: files,
            directories: directories,
        }
    }
}

pub trait FileSystem {
    type FileType: File;
    type DirectoryType: Directory;

    fn root(&mut self) -> Self::DirectoryType;

    fn read_file(&mut self, file: &Self::FileType) -> Option<Region>;

    fn list_directory(
        &mut self,
        dir: &Self::DirectoryType,
    ) -> DirectoryItems<Self::FileType, Self::DirectoryType>;

    fn find_file(&mut self, path: &str) -> Option<Box<Self::FileType>> {
        let mut parts = path.split('/').peekable();
        let mut last = self.root();

        while let Some(next) = parts.next() {
            let entries = self.list_directory(&last);

            match entries.directories.iter().find(|d| d.name() == next) {
                Some(d) => last = d.clone(),
                None => {
                    // must be a final file, so if next is not last, i.e.
                    // there exists a last item, the path must not exist
                    if parts.peek().is_some() {
                        return None;
                    }

                    let file = entries.files.iter().find(|f| f.name() == next)?.clone();
                    return Some(Box::new(file));
                }
            };
        }

        None
    }

    fn find_directory(&mut self, path: &str) -> Option<Box<Self::DirectoryType>> {
        let mut parts = path.split('/').peekable();
        let mut last = self.root();

        if let Some(p) = parts.peek() {
            if p.is_empty() {
                parts.next();
            }
        }

        while let Some(next) = parts.next() {
            let entries = self.list_directory(&last);

            match entries.directories.iter().find(|d| d.name() == next) {
                Some(d) => last = d.clone(),
                None => return None,
            };
        }

        Some(Box::new(last))
    }
}

pub fn combine_path(p1: &str, p2: &str) -> String {
    p1.to_string() + "/" + p2
}
