use alloc::format;

use crate::{arch::x86_64::registers::FullInterruptStackFrame, elf, print, scheduling};

pub fn execute(stack: &FullInterruptStackFrame) -> Option<usize> {
    let buffer_addr = stack.rdi;
    let buffer_size = stack.rsi;

    let slice = unsafe { core::slice::from_raw_parts(buffer_addr as *const u8, buffer_size) };
    let path = match core::str::from_utf8(slice) {
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

    let cwd = scheduling::get_current_cwd();
    let file = cwd.find_file_recursive(path)?;

    // read file
    let region = {
        let file_guard = file.read();
        file_guard.read()?
    };

    let elf_entry = elf::load(region)?;
    let pid = scheduling::deploy(elf_entry, true);
    Some(pid)
}
