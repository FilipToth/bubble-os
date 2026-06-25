use alloc::format;

use crate::log;
use crate::{
    arch::x86_64::registers::FullInterruptStackFrame, elf, scheduling, scheduling::process::Process,
};

pub fn execute(stack: &FullInterruptStackFrame) -> Option<usize> {
    let buffer_addr = stack.rdi;
    let buffer_size = stack.rsi;

    let Some(page_table) = scheduling::get_current_process_page_table() else {
        return Some(0);
    };

    let Some(buffer) = Process::copy_from_user(&page_table, buffer_addr, buffer_size) else {
        log!(crate::io::LogType::ERR, "Failed to copy user pointer");
        return Some(0);
    };

    let path = match core::str::from_utf8(&buffer) {
        Ok(f) => f,
        Err(e) => {
            let msg = format!(
                "Invalid string for execute syscall, rdi: 0x{:X}, rsi: 0x{:X}\n",
                buffer_addr, buffer_size
            );

            log!(crate::io::LogType::SYS, "{}\n{:?}", msg, e);
            return Some(0);
        }
    };

    let cwd = scheduling::get_current_cwd();
    let Some(file) = cwd.find_file_recursive(path) else {
        return Some(0);
    };

    // read file
    let region = {
        let file_guard = file.read();
        file_guard.read()?
    };

    let elf_entry = elf::load(region)?;
    let pid = scheduling::deploy(elf_entry, true);
    Some(pid)
}
