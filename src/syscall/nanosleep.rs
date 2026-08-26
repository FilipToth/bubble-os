// syscall 17 - put the calling process to sleep for a duration

use core::mem::size_of;

use crate::{
    arch::x86_64::registers::FullInterruptStackFrame,
    scheduling,
    scheduling::process::Process,
    syscall::{self, Errno, SyscallResult},
    time::{self, Timespec, NANOSECONDS_PER_SECOND},
};

pub fn nanosleep(stack: &mut FullInterruptStackFrame) -> SyscallResult {
    let timespec_addr = stack.rdi;

    let Some(page_table) = scheduling::get_current_process_page_table() else {
        return Some(Err(Errno::Srch));
    };

    let Some(buffer) = Process::copy_from_user(&page_table, timespec_addr, size_of::<Timespec>())
    else {
        return Some(Err(Errno::Fault));
    };

    let timespec = unsafe { core::ptr::read_unaligned(buffer.as_ptr() as *const Timespec) };
    if timespec.tv_sec < 0
        || timespec.tv_nsec < 0
        || timespec.tv_nsec as u64 >= NANOSECONDS_PER_SECOND
    {
        return Some(Err(Errno::Inval));
    }

    let Some(ticks) = time::duration_to_ticks(timespec.tv_sec as u64, timespec.tv_nsec as u64)
    else {
        return Some(Err(Errno::Inval));
    };

    if ticks == 0 {
        return Some(Ok(0));
    }

    let deadline = time::current_ticks() + ticks;
    scheduling::sleep_current_until(deadline);

    // report success into the saved context before descheduling; the
    // process only resumes from this frame once the deadline has passed
    stack.rax = syscall::encode(Ok(0));
    scheduling::schedule(Some(stack));

    None
}
