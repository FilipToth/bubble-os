// syscall 1 - exit the current process

use crate::{arch::x86_64::registers::FullInterruptStackFrame, scheduling};

pub fn exit(stack: &FullInterruptStackFrame) -> Option<usize> {
    let status = stack.rdi;

    scheduling::exit_current(status);
    scheduling::schedule(None);

    None
}
