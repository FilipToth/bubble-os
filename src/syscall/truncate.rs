// syscall 11 - resize an open file descriptor to a new size

use crate::{arch::x86_64::registers::FullInterruptStackFrame, scheduling};

pub fn truncate(stack: &FullInterruptStackFrame) -> Option<usize> {
    let file_descriptor = stack.rdi;
    let size = stack.rsi;

    if scheduling::truncate_current_file_descriptor(file_descriptor, size).is_none() {
        return Some(0);
    }

    Some(1)
}
