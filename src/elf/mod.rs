use crate::mem::Region;

mod elf_parser;

pub fn parse(elf: Region) {
    elf_parser::parse(elf);
}