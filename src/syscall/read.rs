use crate::{arch::x86_64::registers::FullInterruptStackFrame, scheduling};

pub fn read(stack: &FullInterruptStackFrame) {
    // input handling handled in PIT ISR,
    // and then unblocked in scheduler
    scheduling::block_current();

    // yield back to scheduler instead of
    // caller process
    scheduling::schedule(stack);
}
