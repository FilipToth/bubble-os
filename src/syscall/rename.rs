// syscall 25 - move a directory entry to another name

use alloc::format;

use crate::log;
use crate::{
    arch::x86_64::registers::FullInterruptStackFrame,
    scheduling,
    scheduling::process::Process,
    syscall::{Errno, SyscallResult},
};

/// Copies one path out of the caller and checks it is UTF-8.
///
/// ## Arguments
///
/// - `page_table` the calling process' page table
/// - `addr` the user pointer
/// - `size` its length in bytes
/// - `which` the name of the argument, for the log line on failure
fn copy_path(
    page_table: &crate::mem::paging::PageTable,
    addr: usize,
    size: usize,
    which: &str,
) -> Result<alloc::string::String, Errno> {
    let Some(buffer) = Process::copy_from_user(page_table, addr, size) else {
        return Err(Errno::Fault);
    };

    match core::str::from_utf8(&buffer) {
        Ok(path) => Ok(alloc::string::String::from(path.trim())),
        Err(error) => {
            let message = format!(
                "Invalid {} string for rename syscall, addr: 0x{:X}, size: 0x{:X}",
                which, addr, size
            );

            log!(crate::io::LogType::SYS, "{}\n{:?}", message, error);
            Err(Errno::Inval)
        }
    }
}

pub fn rename(stack: &FullInterruptStackFrame) -> SyscallResult {
    let old_addr = stack.rdi;
    let old_size = stack.rsi;
    let new_addr = stack.rdx;
    let new_size = stack.r10;

    let Some(page_table) = scheduling::get_current_process_page_table() else {
        return Some(Err(Errno::Srch));
    };

    // copied out before either is used, so a bad second pointer cannot leave
    // the first one half applied
    let old_path = match copy_path(&page_table, old_addr, old_size, "old") {
        Ok(path) => path,
        Err(errno) => return Some(Err(errno)),
    };

    let new_path = match copy_path(&page_table, new_addr, new_size, "new") {
        Ok(path) => path,
        Err(errno) => return Some(Err(errno)),
    };

    if !scheduling::curr_process_rename(&old_path, &new_path) {
        // the filesystem answers yes or no rather than saying why. NoEnt is
        // the common case by a distance: a missing source, or a destination
        // directory that does not exist
        return Some(Err(Errno::NoEnt));
    }

    Some(Ok(0))
}
