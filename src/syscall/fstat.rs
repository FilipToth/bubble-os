// syscall 21 - describe an open file descriptor

use crate::{
    arch::x86_64::registers::FullInterruptStackFrame,
    scheduling,
    scheduling::process::Process,
    syscall::{Errno, SyscallResult},
};

pub fn fstat(stack: &FullInterruptStackFrame) -> SyscallResult {
    let file_descriptor = stack.rdi;
    let stat_addr = stack.rsi;

    let Some(page_table) = scheduling::get_current_process_page_table() else {
        return Some(Err(Errno::Srch));
    };

    let Some(stat) = scheduling::stat_current_file_descriptor(file_descriptor) else {
        return Some(Err(Errno::BadF));
    };

    if Process::copy_slice_to_user(&page_table, stat_addr, &[stat]).is_none() {
        return Some(Err(Errno::Fault));
    }

    Some(Ok(0))
}
