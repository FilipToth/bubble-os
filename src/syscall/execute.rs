use alloc::format;

use crate::{
    arch::x86_64::registers::FullInterruptStackFrame,
    elf,
    fs::{fat_fs::FATFileSystem, fs::FileSystem},
    print, scheduling, with_fs,
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
    let file = with_fs!(FATFileSystem, fs, {
        let file = fs.find_file(filename)?;
        fs.read_file(&file)?
    });

    let elf_entry = elf::load(file)?;
    let pid = scheduling::deploy(elf_entry, true);
    Some(pid)
}
