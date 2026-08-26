// syscall 12 - create an empty regular file and return an open file descriptor

use alloc::format;

use crate::log;
use crate::{
    arch::x86_64::registers::FullInterruptStackFrame,
    scheduling,
    scheduling::process::Process,
    syscall::{Errno, SyscallResult},
};

pub fn create(stack: &FullInterruptStackFrame) -> SyscallResult {
    let buffer_addr = stack.rdi;
    let buffer_size = stack.rsi;

    let Some(page_table) = scheduling::get_current_process_page_table() else {
        return Some(Err(Errno::Srch));
    };

    let Some(buffer) = Process::copy_from_user(&page_table, buffer_addr, buffer_size) else {
        return Some(Err(Errno::Fault));
    };

    let path = match core::str::from_utf8(&buffer) {
        Ok(path) => path.trim(),
        Err(error) => {
            let message = format!(
                "Invalid string for create syscall, rdi: 0x{:X}, rsi: 0x{:X}",
                buffer_addr, buffer_size
            );

            log!(crate::io::LogType::SYS, "{}\n{:?}", message, error);
            return Some(Err(Errno::Inval));
        }
    };

    if path.is_empty() {
        return Some(Err(Errno::Inval));
    }

    // the filesystem only answers yes or no, so an existing file, a missing
    // parent directory and a full volume all arrive here the same way
    let Some(fd) = scheduling::curr_process_create_file(path, true, true) else {
        return Some(Err(Errno::NoEnt));
    };

    Some(Ok(fd))
}
