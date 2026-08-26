// syscall 8 - change the current working directory

use alloc::format;

use crate::log;
use crate::{
    arch::x86_64::registers::FullInterruptStackFrame,
    scheduling,
    scheduling::process::Process,
    syscall::{Errno, SyscallResult},
};

pub fn cd(stack: &FullInterruptStackFrame) -> SyscallResult {
    let buffer_addr = stack.rdi;
    let buffer_size = stack.rsi;

    let Some(page_table) = scheduling::get_current_process_page_table() else {
        return Some(Err(Errno::Srch));
    };

    let Some(buffer) = Process::copy_from_user(&page_table, buffer_addr, buffer_size) else {
        return Some(Err(Errno::Fault));
    };

    let path = match core::str::from_utf8(&buffer) {
        Ok(f) => f,
        Err(e) => {
            let msg = format!(
                "Invalid string for change directory syscall, rdi: 0x{:X}, rsi: 0x{:X}\n",
                buffer_addr, buffer_size
            );

            log!(crate::io::LogType::SYS, "{}\n{:?}", msg, e);

            return Some(Err(Errno::Inval));
        }
    };

    let path = path.trim();
    if path.is_empty() {
        return Some(Err(Errno::Inval));
    }

    // a path that names a regular file fails the directory lookup, so this
    // covers ENOTDIR as well as a genuinely missing directory
    let Some(new_dir) = scheduling::find_directory_from_path(path) else {
        return Some(Err(Errno::NoEnt));
    };

    scheduling::change_cwd(new_dir);

    Some(Ok(0))
}
