use crate::{mem::Region, scheduling::process::ProcessEntry};

mod loader;

pub fn load(elf: Region) -> Option<ProcessEntry> {
    loader::load(elf)
}
