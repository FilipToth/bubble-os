// syscall 19 - move the program break by a signed increment

use crate::{arch::x86_64::registers::FullInterruptStackFrame, scheduling, syscall::SyscallResult};

pub fn sbrk(stack: &FullInterruptStackFrame) -> SyscallResult {
    // the increment is signed, a negative one gives heap pages back
    let increment = stack.rdi as isize;

    Some(scheduling::current_adjust_break(increment))
}
