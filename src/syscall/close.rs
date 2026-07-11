// syscall 10 - close an open file descriptor

use crate::{arch::x86_64::registers::FullInterruptStackFrame, scheduling};

pub fn close(stack: &FullInterruptStackFrame) -> Option<usize> {
    let fd = stack.rdi;
    let closed = scheduling::close_current_file_descriptor(fd);

    Some(closed as usize)
}
