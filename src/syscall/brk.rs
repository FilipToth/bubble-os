// syscall 18 - move the program break to an absolute address

use crate::{arch::x86_64::registers::FullInterruptStackFrame, scheduling};

pub fn brk(stack: &FullInterruptStackFrame) -> Option<usize> {
    let end_data_segment = stack.rdi;

    // zero is never a valid break, the heap always starts past the ELF
    // segments, so it doubles as the failure value
    match scheduling::current_set_break(end_data_segment) {
        Some(new_break) => Some(new_break),
        None => Some(0),
    }
}
