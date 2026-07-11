// syscall 5 - yield execution to the scheduler

use crate::{arch::x86_64::registers::FullInterruptStackFrame, scheduling};

pub fn yld(stack: &FullInterruptStackFrame) -> Option<usize> {
    // yield back to scheduler instead of
    // caller process
    scheduling::schedule(Some(stack));
    None
}
