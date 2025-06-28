use alloc::{string::String, vec::Vec};

use crate::mem::Region;

pub trait Directory {
    fn name(&self) -> String;
}

pub trait File {
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

    fn find(&mut self, path: String) {

    }
}
