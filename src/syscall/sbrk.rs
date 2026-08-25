// syscall 19 - move the program break by a signed increment

use crate::{arch::x86_64::registers::FullInterruptStackFrame, scheduling};

pub fn sbrk(stack: &FullInterruptStackFrame) -> Option<usize> {
    // the increment is signed, a negative one gives heap pages back
    let increment = stack.rdi as isize;

    match scheduling::current_adjust_break(increment) {
        Some(new_break) => Some(new_break),
        None => Some(0),
    }
}
