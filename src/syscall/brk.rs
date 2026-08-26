// syscall 18 - move the program break to an absolute address

use crate::{arch::x86_64::registers::FullInterruptStackFrame, scheduling, syscall::SyscallResult};

pub fn brk(stack: &FullInterruptStackFrame) -> SyscallResult {
    let end_data_segment = stack.rdi;

    Some(scheduling::current_set_break(end_data_segment))
}
