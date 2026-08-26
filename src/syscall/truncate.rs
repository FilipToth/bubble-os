// syscall 11 - resize an open file descriptor to a new size

use crate::{
    arch::x86_64::registers::FullInterruptStackFrame,
    scheduling,
    syscall::{Errno, SyscallResult},
};

pub fn truncate(stack: &FullInterruptStackFrame) -> SyscallResult {
    let file_descriptor = stack.rdi;
    let size = stack.rsi;

    if scheduling::truncate_current_file_descriptor(file_descriptor, size).is_none() {
        return Some(Err(Errno::BadF));
    }

    Some(Ok(0))
}
