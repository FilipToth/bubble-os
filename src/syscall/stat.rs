// syscall 22 - describe a file or directory by path

use alloc::format;

use crate::log;
use crate::{
    arch::x86_64::registers::FullInterruptStackFrame,
    scheduling,
    scheduling::process::Process,
    syscall::{Errno, SyscallResult},
};

pub fn stat(stack: &FullInterruptStackFrame) -> SyscallResult {
    let buffer_addr = stack.rdi;
    let buffer_size = stack.rsi;
    let stat_addr = stack.rdx;

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
                "Invalid string for stat syscall, rdi: 0x{:X}, rsi: 0x{:X}",
                buffer_addr, buffer_size
            );

            log!(crate::io::LogType::SYS, "{}\n{:?}", message, error);
            return Some(Err(Errno::Inval));
        }
    };

    if path.is_empty() {
        return Some(Err(Errno::Inval));
    }

    let Some(stat) = scheduling::stat_from_path(path) else {
        return Some(Err(Errno::NoEnt));
    };

    if Process::copy_slice_to_user(&page_table, stat_addr, &[stat]).is_none() {
        return Some(Err(Errno::Fault));
    }

    Some(Ok(0))
}
