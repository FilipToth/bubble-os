// syscall 15 - remove an empty directory

use alloc::format;

use crate::log;
use crate::{
    arch::x86_64::registers::FullInterruptStackFrame,
    scheduling,
    scheduling::process::Process,
    syscall::{Errno, SyscallResult},
};

pub fn rmdir(stack: &FullInterruptStackFrame) -> SyscallResult {
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
                "Invalid string for rmdir syscall, rdi: 0x{:X}, rsi: 0x{:X}",
                buffer_addr, buffer_size
            );

            log!(crate::io::LogType::SYS, "{}\n{:?}", message, error);
            return Some(Err(Errno::Inval));
        }
    };

    // a non empty directory is the interesting failure here, but the
    // filesystem layer does not distinguish it from a missing one yet
    if !scheduling::curr_process_remove_directory(path) {
        return Some(Err(Errno::NoEnt));
    }

    Some(Ok(0))
}
