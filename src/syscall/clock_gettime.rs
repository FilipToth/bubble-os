// syscall 16 - read a clock into a user timespec

use crate::{
    arch::x86_64::registers::FullInterruptStackFrame,
    scheduling,
    scheduling::process::Process,
    syscall::{Errno, SyscallResult},
    time::{self, Timespec, CLOCK_MONOTONIC, CLOCK_REALTIME},
};

pub fn clock_gettime(stack: &FullInterruptStackFrame) -> SyscallResult {
    let clock_id = stack.rdi;
    let timespec_addr = stack.rsi;

    let timespec = match clock_id {
        CLOCK_REALTIME => time::realtime_timespec(),
        CLOCK_MONOTONIC => time::monotonic_timespec(),
        _ => return Some(Err(Errno::Inval)),
    };

    let Some(page_table) = scheduling::get_current_process_page_table() else {
        return Some(Err(Errno::Srch));
    };

    if Process::copy_slice_to_user(&page_table, timespec_addr, &[timespec]).is_none() {
        return Some(Err(Errno::Fault));
    }

    Some(Ok(0))
}
