// syscall 9 - open a regular file descriptor

use alloc::format;

use crate::log;
use crate::{
    arch::x86_64::registers::FullInterruptStackFrame, scheduling, scheduling::process::Process,
};

pub fn open(stack: &FullInterruptStackFrame) -> Option<usize> {
    let buffer_addr = stack.rdi;
    let buffer_size = stack.rsi;

    let Some(page_table) = scheduling::get_current_process_page_table() else {
        return Some(0);
    };

    let Some(buffer) = Process::copy_from_user(&page_table, buffer_addr, buffer_size) else {
        return Some(0);
    };

    let path = match core::str::from_utf8(&buffer) {
        Ok(p) => p.trim(),
        Err(e) => {
            let msg = format!(
                "Invalid string for open syscall, rdi: 0x{:X}, rsi: 0x{:X}\n",
                buffer_addr, buffer_size
            );

            log!(crate::io::LogType::SYS, "{}\n{:?}", msg, e);
            return Some(0);
        }
    };

    scheduling::curr_process_open_file(path, true, false)
}
