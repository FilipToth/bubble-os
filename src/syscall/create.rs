// syscall 12 - create an empty regular file and return an open file descriptor

use alloc::format;

use crate::log;
use crate::{
    arch::x86_64::registers::FullInterruptStackFrame, scheduling, scheduling::process::Process,
};

pub fn create(stack: &FullInterruptStackFrame) -> Option<usize> {
    let buffer_addr = stack.rdi;
    let buffer_size = stack.rsi;

    let Some(page_table) = scheduling::get_current_process_page_table() else {
        return Some(0);
    };

    let Some(buffer) = Process::copy_from_user(&page_table, buffer_addr, buffer_size) else {
        return Some(0);
    };

    let path = match core::str::from_utf8(&buffer) {
        Ok(path) => path.trim(),
        Err(error) => {
            let message = format!(
                "Invalid string for create syscall, rdi: 0x{:X}, rsi: 0x{:X}",
                buffer_addr, buffer_size
            );

            log!(crate::io::LogType::SYS, "{}\n{:?}", message, error);
            return Some(0);
        }
    };

    if path.is_empty() {
        return Some(0);
    }

    // map a failed creation to fd 0, otherwise the dispatcher would leave
    // the syscall number in rax and userspace would see a phantom fd
    scheduling::curr_process_create_file(path, true, true).or(Some(0))
}
