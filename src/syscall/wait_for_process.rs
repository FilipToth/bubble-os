use crate::{arch::x86_64::registers::FullInterruptStackFrame, scheduling};

pub fn wait_for_process(stack: &FullInterruptStackFrame) -> Option<usize> {
    let pid = stack.rdi;
    scheduling::current_wait_for_process(pid);
    scheduling::schedule(Some(stack));

    None
}
