// syscall 3 - read bytes from stdin or a file descriptor

use crate::{
    arch::x86_64::registers::FullInterruptStackFrame, scheduling, scheduling::process::Process,
};

pub fn read(stack: &FullInterruptStackFrame) -> Option<usize> {
    let file_descriptor = stack.rdi;
    if file_descriptor >= 3 {
        let buffer_addr = stack.rsi;
        let buffer_size = stack.rdx;

        let Some(page_table) = scheduling::get_current_process_page_table() else {
            return Some(0);
        };

        if !Process::can_process_pointer(&page_table, buffer_addr, buffer_size, true) {
            return Some(0);
        }

        let Some(bytes) = scheduling::read_current_file_descriptor(file_descriptor, buffer_size)
        else {
            return Some(0);
        };

        if Process::copy_to_user(&page_table, buffer_addr, &bytes).is_none() {
            return Some(0);
        }

        return Some(bytes.len());
    }

    // input handling handled in PIT ISR,
    // and then unblocked in scheduler
    scheduling::block_current();

    // yield back to scheduler instead of
    // caller process
    scheduling::schedule(Some(stack));

    None
}
