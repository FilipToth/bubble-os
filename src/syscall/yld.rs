// syscall 5 - yield execution to the scheduler

use crate::{arch::x86_64::registers::FullInterruptStackFrame, scheduling, syscall::SyscallResult};

pub fn yld(stack: &FullInterruptStackFrame) -> SyscallResult {
    // yield back to scheduler instead of
    // caller process
    scheduling::schedule(Some(stack));
    None
}
