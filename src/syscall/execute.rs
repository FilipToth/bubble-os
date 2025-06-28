use alloc::format;

use crate::{
    arch::x86_64::registers::FullInterruptStackFrame, elf, fs::GLOBAL_FILESYSTEM, print, scheduling,
};

pub fn execute(stack: &FullInterruptStackFrame) -> Option<usize> {
    let buffer_addr = stack.rdi;
    let buffer_size = stack.rsi;

    let slice = unsafe { core::slice::from_raw_parts(buffer_addr as *const u8, buffer_size) };
    let filename = match core::str::from_utf8(slice) {
        Ok(f) => f,
        Err(e) => {
            let msg = format!(
                "Invalid string for execute syscall, rdi: 0x{:X}, rsi: 0x{:X}\n",
                buffer_addr, buffer_size
            );

            print!("{}\n{:?}\n", msg, e);
            return Some(0);
        }
    };

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
