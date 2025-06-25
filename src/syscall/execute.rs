use crate::{
    arch::x86_64::registers::FullInterruptStackFrame, elf, fs::GLOBAL_FILESYSTEM, scheduling,
};

pub fn execute(stack: &FullInterruptStackFrame) -> Option<usize> {
    let buffer_addr = stack.rdi;
    let buffer_size = stack.rsi;

    let slice = unsafe { core::slice::from_raw_parts(buffer_addr as *const u8, buffer_size) };
    let filename =
        core::str::from_utf8(slice).unwrap_or("Invalid ELF filename string for execute syscall\n");

    // check if file exists
    let mut fs = GLOBAL_FILESYSTEM.lock();
    let fs = fs.as_mut().unwrap();

    let file = match fs.get_entry_in_root(filename) {
        Some(f) => f,
        None => return Some(0),
    };

    let file = match fs.read_file(&file) {
        Some(f) => f,
        None => return Some(0),
    };

    let elf_entry = match elf::load(file) {
        Some(e) => e,
        None => return Some(0),
    };

    let pid = scheduling::deploy(elf_entry, true);
    Some(pid)
}
