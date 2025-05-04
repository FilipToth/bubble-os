use crate::mem::Region;

mod loader;

pub fn load(elf: Region) {
    loader::load(elf);
}
