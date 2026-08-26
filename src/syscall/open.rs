// syscall 9 - open a regular file descriptor

use alloc::format;

use crate::log;
use crate::{
    arch::x86_64::registers::FullInterruptStackFrame,
    scheduling,
    scheduling::process::Process,
    syscall::{Errno, SyscallResult},
};

pub fn open(stack: &FullInterruptStackFrame) -> SyscallResult {
    let buffer_addr = stack.rdi;
    let buffer_size = stack.rsi;

    let Some(page_table) = scheduling::get_current_process_page_table() else {
        return Some(Err(Errno::Srch));
    };

    let Some(buffer) = Process::copy_from_user(&page_table, buffer_addr, buffer_size) else {
        return Some(Err(Errno::Fault));
    };

    let path = match core::str::from_utf8(&buffer) {
        Ok(p) => p.trim(),
        Err(e) => {
            let msg = format!(
                "Invalid string for open syscall, rdi: 0x{:X}, rsi: 0x{:X}\n",
                buffer_addr, buffer_size
            );

            log!(crate::io::LogType::SYS, "{}\n{:?}", msg, e);
            return Some(Err(Errno::Inval));
        }
    };

    // the lookup only reports whether it found the file, so a missing file
    // and an unreadable one both surface here as ENOENT
    let Some(fd) = scheduling::curr_process_open_file(path, true, true) else {
        return Some(Err(Errno::NoEnt));
    };

    Some(Ok(fd))
}
